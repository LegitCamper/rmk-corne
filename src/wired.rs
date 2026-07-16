//! Fully wired split: halves are linked over a wired serial connection
//! instead of BLE, and the central half (peripheral_left) plugs directly
//! into USB instead of going through a BLE dongle.
//!
//! The two halves share a single data wire (nice!nano pin "D2", nRF
//! `P0.17`) plus VCC/GND, so the link is half-duplex: both TXD and RXD are
//! pinned to the same GPIO. That means every half also hears its own bytes
//! echoed straight back on RX, which [`HalfDuplexUart`] drains away so it's
//! never mistaken for a message from the other half. Because both ends can
//! drive the shared line, a small series resistor (a few hundred ohms) on
//! the data line at each end is strongly recommended to limit current if
//! both sides ever happen to transmit at once.

use defmt::info;
use embassy_executor::Spawner;
use embassy_nrf::buffered_uarte::{self, BufferedUarte};
use embassy_nrf::gpio::{Input, Output};
use embassy_nrf::peripherals::{P0_17, PPI_CH0, PPI_CH1, PPI_GROUP0, TIMER1, UARTE0, USBD};
use embassy_nrf::{Peri, bind_interrupts, uarte, usb};
use embedded_io_async::{ErrorType, Read, Write};
use rmk::channel::EVENT_CHANNEL;
#[cfg(feature = "peripheral_left")]
use rmk::config::StorageConfig;
use rmk::debounce::default_debouncer::DefaultDebouncer;
use rmk::matrix::Matrix;
use rmk::run_devices;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use crate::keymap::{COL, ROW};

#[cfg(feature = "peripheral_left")]
use embassy_embedded_hal::adapter::BlockingAsync;
#[cfg(feature = "peripheral_left")]
use embassy_nrf::nvmc::Nvmc;
#[cfg(feature = "peripheral_left")]
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
#[cfg(feature = "peripheral_left")]
use rmk::config::{BehaviorConfig, DeviceConfig, PositionalConfig, RmkConfig};
#[cfg(feature = "peripheral_left")]
use rmk::futures::future::join4;
#[cfg(feature = "peripheral_left")]
use rmk::input_device::Runnable;
#[cfg(feature = "peripheral_left")]
use rmk::keyboard::Keyboard;
#[cfg(feature = "peripheral_left")]
use rmk::split::central::run_peripheral_manager;
#[cfg(feature = "peripheral_left")]
use rmk::types::action::EncoderAction;
#[cfg(feature = "peripheral_left")]
use rmk::{initialize_encoder_keymap_and_storage, run_rmk};
#[cfg(feature = "peripheral_left")]
use crate::keymap::NUM_LAYER;

#[cfg(not(feature = "peripheral_left"))]
use rmk::futures::future::join;
#[cfg(not(feature = "peripheral_left"))]
use rmk::split::peripheral::run_rmk_split_peripheral;

bind_interrupts!(struct Irqs {
    UARTE0 => buffered_uarte::InterruptHandler<UARTE0>;
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

const UART_BUF_LEN: usize = 128;

/// Wraps [`BufferedUarte`] to drain the self-echo produced by sharing one
/// physical pin for both TXD and RXD. See the module docs for why this is
/// necessary.
struct HalfDuplexUart<'d> {
    inner: BufferedUarte<'d>,
}

impl<'d> ErrorType for HalfDuplexUart<'d> {
    type Error = buffered_uarte::Error;
}

impl<'d> Read for HalfDuplexUart<'d> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.inner.read(buf).await
    }
}

impl<'d> Write for HalfDuplexUart<'d> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let n = self.inner.write(buf).await?;
        // Discard exactly the bytes we just sent, since they loop straight
        // back onto RX on a shared single-wire link.
        let mut discarded = 0;
        let mut echo = [0u8; 32];
        while discarded < n {
            let want = (n - discarded).min(echo.len());
            discarded += self.inner.read(&mut echo[..want]).await?;
        }
        Ok(n)
    }
}

fn init_uart(
    uarte: Peri<'static, UARTE0>,
    timer: Peri<'static, TIMER1>,
    ppi_ch0: Peri<'static, PPI_CH0>,
    ppi_ch1: Peri<'static, PPI_CH1>,
    ppi_group0: Peri<'static, PPI_GROUP0>,
    data_pin: Peri<'static, P0_17>,
) -> HalfDuplexUart<'static> {
    static RX_BUF: StaticCell<[u8; UART_BUF_LEN]> = StaticCell::new();
    static TX_BUF: StaticCell<[u8; UART_BUF_LEN]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0u8; UART_BUF_LEN])[..];
    let tx_buf = &mut TX_BUF.init([0u8; UART_BUF_LEN])[..];

    // SAFETY: rxd and txd are configured on the same physical GPIO on
    // purpose, since the wired link between halves is a single shared data
    // line. Only ever driven by this one BufferedUarte instance.
    let rxd = unsafe { data_pin.clone_unchecked() };
    let inner = BufferedUarte::new(
        uarte,
        timer,
        ppi_ch0,
        ppi_ch1,
        ppi_group0,
        rxd,
        data_pin,
        Irqs,
        uarte::Config::default(),
        rx_buf,
        tx_buf,
    );
    HalfDuplexUart { inner }
}

fn nrf_config() -> embassy_nrf::config::Config {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.dcdc.reg0_voltage = Some(embassy_nrf::config::Reg0Voltage::_3V3);
    nrf_config.dcdc.reg0 = true;
    nrf_config.dcdc.reg1 = true;
    nrf_config
}

#[cfg(feature = "peripheral_left")]
fn storage_config() -> StorageConfig {
    StorageConfig {
        start_addr: 0xA0000,
        num_sectors: 32,
        #[cfg(feature = "reset")]
        clear_storage: true,
        #[cfg(feature = "reset")]
        clear_layout: true,
        ..Default::default()
    }
}

/// The wired central: hosts USB directly and aggregates its own local
/// matrix with the wired peripheral's matrix over the shared serial link.
#[cfg(feature = "peripheral_left")]
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("RMK start! (wired central)");
    let p = embassy_nrf::init(nrf_config());

    let driver = usb::Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));

    let (row_pins, col_pins) = config_matrix_pins_nrf!(peripherals: p,
        input: [P0_22, P0_24, P1_00, P0_11],
        output:  [P0_31, P0_29, P0_02, P1_15, P1_13, P1_11]);

    let serial = init_uart(p.UARTE0, p.TIMER1, p.PPI_CH0, p.PPI_CH1, p.PPI_GROUP0, p.P0_17);

    let flash = BlockingAsync::new(Nvmc::new(p.NVMC));
    let storage_config = storage_config();

    let keyboard_device_config = DeviceConfig {
        vid: 0x4c4b,
        pid: 0x4643,
        manufacturer: "LegitCamper",
        product_name: "RMK Keyboard",
        serial_number: "na",
    };
    let rmk_config = RmkConfig {
        device_config: keyboard_device_config,
        storage_config,
        ..Default::default()
    };

    let mut default_keymap = crate::keymap::get_default_keymap();
    let mut behavior_config = BehaviorConfig::default();
    behavior_config.morse.enable_flow_tap = true;
    let mut key_config = PositionalConfig::default();
    let mut encoder_config = [{
        EncoderAction::default();
        [] as [EncoderAction; 0]
    }; NUM_LAYER];
    let (keymap, mut storage) = initialize_encoder_keymap_and_storage::<_, ROW, COL, NUM_LAYER, 0>(
        &mut default_keymap,
        &mut encoder_config,
        flash,
        &storage_config,
        &mut behavior_config,
        &mut key_config,
    )
    .await;

    let mut keyboard = Keyboard::new(&keymap);
    let debouncer = DefaultDebouncer::new();
    let mut matrix = Matrix::<_, _, _, ROW, { COL / 2 }, true>::new(row_pins, col_pins, debouncer);

    join4(
        run_devices! (
            (matrix) => EVENT_CHANNEL, // Local (left) matrix
        ),
        keyboard.run(),
        run_peripheral_manager::<ROW, COL, 0, { COL / 2 }, _>(1, serial),
        run_rmk(&keymap, driver, &mut storage, rmk_config),
    )
    .await;
}

/// The wired peripheral: no USB, no BLE. Just scans its own matrix and
/// forwards key events to the wired central over the shared serial link.
#[cfg(not(feature = "peripheral_left"))]
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("RMK start! (wired peripheral)");
    let p = embassy_nrf::init(nrf_config());

    let (row_pins, col_pins) = config_matrix_pins_nrf!(peripherals: p,
        input: [P0_22, P0_24, P1_00, P0_11],
        output:  [P1_11, P1_13, P1_15, P0_02, P0_29, P0_31]);

    let serial = init_uart(p.UARTE0, p.TIMER1, p.PPI_CH0, p.PPI_CH1, p.PPI_GROUP0, p.P0_17);

    // Unlike the BLE peripheral, the wired peripheral has no peer address or
    // other state to persist, so it needs no flash storage of its own.
    let debouncer = DefaultDebouncer::new();
    let mut matrix = Matrix::<_, _, _, ROW, { COL / 2 }, true>::new(row_pins, col_pins, debouncer);

    join(
        run_devices! (
            (matrix) => EVENT_CHANNEL,
        ),
        run_rmk_split_peripheral(serial),
    )
    .await;
}
