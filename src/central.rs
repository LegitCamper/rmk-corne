#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod keymap;
mod vial;
use keymap::{COL, ROW};
use vial::{VIAL_KEYBOARD_DEF, VIAL_KEYBOARD_ID};

use bt_hci::controller::ExternalController;
use cyw43::aligned_bytes;
use cyw43_pio::PioSpi;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::flash::{Async, Flash};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, DMA_CH2, PIO0, USB};
use embassy_rp::pio::{self, Pio};
use embassy_rp::usb::{self, Driver};
use panic_probe as _;
use rmk::ble::{BleTransport, build_ble_stack};
use rmk::config::{
    BehaviorConfig, BleBatteryConfig, DeviceConfig, PositionalConfig, RmkConfig, StorageConfig,
    VialConfig,
};
use rmk::futures::future::join;
use rmk::host::HostService;
use rmk::keymap::KeymapData;
use rmk::processor::builtin::wpm::WpmProcessor;
use rmk::split::ble::central::scan_peripherals;
use rmk::split::central::run_peripheral_manager;
use rmk::usb::UsbTransport;
use rmk::watchdog::Rp2040Watchdog;
use rmk::{HostResources, initialize_keymap_and_storage, run_all};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>, embassy_rp::dma::InterruptHandler<DMA_CH1>, embassy_rp::dma::InterruptHandler<DMA_CH2>;
});

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>, cyw43::Cyw43439>,
) -> ! {
    runner.run().await
}

/// Total flash size on the Pico W.
const FLASH_SIZE: usize = 2 * 1024 * 1024;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // IMPORTANT: these firmware blobs are downloaded by build.rs into
    // ./cyw43-firmware -- see the "reset"/"usb_logging" note in the README
    // if you need to build offline.
    let fw = aligned_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
    let btfw = aligned_bytes!("../cyw43-firmware/43439A0_btfw.bin");
    let nvram = aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");

    // Fixed Pico W wiring: the CYW43439 wifi/BT chip sits behind PIO-driven SPI.
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        cyw43_pio::DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        embassy_rp::dma::Channel::new(p.DMA_CH0, Irqs),
        embassy_rp::dma::Channel::new(p.DMA_CH2, Irqs),
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (_net_device, bt_device, mut control, runner) =
        cyw43::new_with_bluetooth(state, pwr, spi, fw, btfw, nvram).await;
    spawner.spawn(cyw43_task(runner).unwrap());
    control.init(clm).await;

    let controller: ExternalController<_, 10> = ExternalController::new(bt_device);

    // Initialize usb driver
    let driver = Driver::new(p.USB, Irqs);

    // Use internal flash to emulate eeprom
    let flash = Flash::<_, Async, FLASH_SIZE>::new(p.FLASH, p.DMA_CH1, Irqs);

    // Keyboard config
    let keyboard_device_config = DeviceConfig {
        vid: 0x4c4b,
        pid: 0x4643,
        manufacturer: "LegitCamper",
        product_name: "RMK Keyboard",
        ..DeviceConfig::default()
    };
    let vial_config = VialConfig::new(VIAL_KEYBOARD_ID, VIAL_KEYBOARD_DEF, &[(0, 0), (0, 11)]);
    // The central is a USB-powered dongle with no battery of its own -- only
    // the split peripherals report battery level.
    let ble_battery_config = BleBatteryConfig::disabled();
    let storage_config = StorageConfig {
        start_addr: 0x100000, // Start from 1M
        num_sectors: 32,
        #[cfg(feature = "reset")]
        clear_storage: true,
        #[cfg(feature = "reset")]
        clear_layout: true,
        ..Default::default()
    };
    let rmk_config = RmkConfig {
        device_config: keyboard_device_config,
        vial_config,
        ble_battery_config,
        storage_config,
    };

    // Initialize keyboard stuffs
    // Initialize the storage and keymap
    let mut keymap_data = KeymapData::new(keymap::get_default_keymap());
    let mut behavior_config = BehaviorConfig::default();
    behavior_config.morse.enable_flow_tap = true;
    let per_key_config = PositionalConfig::default();
    let (keymap, mut storage) = initialize_keymap_and_storage(
        &mut keymap_data,
        flash,
        &storage_config,
        &mut behavior_config,
        &per_key_config,
    )
    .await;

    // Initialize the keyboard
    let mut keyboard = rmk::keyboard::Keyboard::new(&keymap);
    let host_ctx = rmk::host::KeyboardContext::new(&keymap);
    let mut host_service = HostService::new(&host_ctx, &rmk_config);

    // Read peripheral addresses from storage
    let peripheral_addrs = storage.read_peripheral_addresses::<2>().await;

    let mut host_resources = HostResources::new();
    let stack = build_ble_stack(controller, ble_addr(), &mut host_resources).await;

    let mut usb_transport = UsbTransport::new(driver, rmk_config.device_config);
    let mut ble_transport = BleTransport::new(&stack, rmk_config).await;
    let mut wpm_processor = WpmProcessor::new();
    let mut watchdog_runner =
        Rp2040Watchdog::default_runner(embassy_rp::watchdog::Watchdog::new(p.WATCHDOG));

    // Start
    join(
        run_all!(
            storage,
            usb_transport,
            ble_transport,
            wpm_processor,
            keyboard,
            host_service,
            watchdog_runner
        ),
        join(
            scan_peripherals(&stack, &peripheral_addrs),
            join(
                run_peripheral_manager::<ROW, COL, 0, 0, _>(0, &peripheral_addrs, &stack),
                run_peripheral_manager::<ROW, COL, 0, 6, _>(1, &peripheral_addrs, &stack),
            ),
        ),
    )
    .await;
}

fn ble_addr() -> [u8; 6] {
    [0x18, 0xe2, 0x21, 0x88, 0xc0, 0xc7]
}
