#![no_std]
#![no_main]

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::interrupt::{self, InterruptExt};
use embassy_nrf::mode::Async;
use embassy_nrf::peripherals::{RNG, SAADC, USBD};
use embassy_nrf::saadc::{self, AnyInput, Input as _, Saadc};
use embassy_nrf::{Peri, bind_interrupts, rng, usb};
use nrf_mpsl::Flash;
use nrf_sdc::mpsl::MultiprotocolServiceLayer;
use nrf_sdc::{self as sdc, mpsl};
use rand_chacha::ChaCha12Rng;
use rand_core::SeedableRng;
use rmk::ble::build_ble_stack;
use rmk::channel::EVENT_CHANNEL;
use rmk::config::StorageConfig;
use rmk::debounce::default_debouncer::DefaultDebouncer;
use rmk::futures::future::join;
use rmk::input_device::rotary_encoder::RotaryEncoder;
use rmk::matrix::Matrix;
use rmk::split::peripheral::run_rmk_split_peripheral;
use rmk::storage::new_storage_for_split_peripheral;
use rmk::{HostResources, run_devices};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod keymap;
use keymap::{COL, ROW};

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    SAADC => saadc::InterruptHandler;
    RNG => rng::InterruptHandler<RNG>;
    EGU0_SWI0 => nrf_sdc::mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => nrf_sdc::mpsl::ClockInterruptHandler, usb::vbus_detect::InterruptHandler;
    RADIO => nrf_sdc::mpsl::HighPrioInterruptHandler;
    TIMER0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
    RTC0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
});

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

/// How many outgoing L2CAP buffers per link
const L2CAP_TXQ: u8 = 3;

/// How many incoming L2CAP buffers per link
const L2CAP_RXQ: u8 = 3;

/// Size of L2CAP packets
const L2CAP_MTU: usize = 251;

fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<Async>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, nrf_sdc::Error> {
    sdc::Builder::new()?
        .support_adv()?
        .support_peripheral()?
        .support_dle_peripheral()?
        .support_phy_update_peripheral()?
        .support_le_2m_phy()?
        .peripheral_count(1)?
        .buffer_cfg(L2CAP_MTU as u16, L2CAP_MTU as u16, L2CAP_TXQ, L2CAP_RXQ)?
        .build(p, rng, mpsl, mem)
}

/// Initializes the SAADC peripheral in single-ended mode on the given pin.
fn init_adc(adc_pin: AnyInput, adc: Peri<'static, SAADC>) -> Saadc<'static, 1> {
    // Then we initialize the ADC. We are only using one channel in this example.
    let config = saadc::Config::default();
    let channel_cfg = saadc::ChannelConfig::single_ended(adc_pin.degrade_saadc());
    interrupt::SAADC.set_priority(interrupt::Priority::P3);

    saadc::Saadc::new(adc, Irqs, config, [channel_cfg])
}

fn ble_addr() -> [u8; 6] {
    let ficr = embassy_nrf::pac::FICR;
    let high = u64::from(ficr.deviceid(1).read());
    let addr = high << 32 | u64::from(ficr.deviceid(0).read());
    let addr = addr | 0x0000_c000_0000_0000;
    unwrap!(addr.to_le_bytes()[..6].try_into())
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello RMK BLE!");
    // Initialize the peripherals and nrf-sdc controller
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.dcdc.reg0_voltage = Some(embassy_nrf::config::Reg0Voltage::_3V3);
    nrf_config.dcdc.reg0 = true;
    nrf_config.dcdc.reg1 = true;
    let p = embassy_nrf::init(nrf_config);
    let mpsl_p =
        mpsl::Peripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);
    let lfclk_cfg = mpsl::raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl::raw::MPSL_CLOCK_LF_SRC_RC as u8,
        rc_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_CTIV as u8,
        rc_temp_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
        accuracy_ppm: mpsl::raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
        skip_wait_lfclk_started: mpsl::raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
    };
    static MPSL: StaticCell<MultiprotocolServiceLayer> = StaticCell::new();
    static SESSION_MEM: StaticCell<mpsl::SessionMem<1>> = StaticCell::new();
    let mpsl = MPSL.init(unwrap!(mpsl::MultiprotocolServiceLayer::with_timeslots(
        mpsl_p,
        Irqs,
        lfclk_cfg,
        SESSION_MEM.init(mpsl::SessionMem::new())
    )));
    spawner.must_spawn(mpsl_task(&*mpsl));
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
        p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );
    let mut rng = rng::Rng::new(p.RNG, Irqs);
    let mut rng_generator = ChaCha12Rng::from_rng(&mut rng).unwrap();
    let mut sdc_mem = sdc::Mem::<4624>::new();
    let sdc = unwrap!(build_sdc(sdc_p, &mut rng, mpsl, &mut sdc_mem));

    let mut resources = HostResources::new();
    let stack = build_ble_stack(sdc, ble_addr(), &mut rng_generator, &mut resources).await;

    // Initialize the ADC. We are only using one channel for detecting battery level
    let adc_pin = p.P0_05.degrade_saadc();
    let saadc = init_adc(adc_pin, p.SAADC);
    // Wait for ADC calibration.
    saadc.calibrate().await;

    #[cfg(feature = "peripheral_left")]
    let (row_pins, col_pins) = {
        let row_0 = Input::new(p.P1_00, Pull::Down);
        let row_1 = Input::new(p.P0_11, Pull::Down);
        let row_2 = Input::new(p.P1_04, Pull::Down);
        let row_3 = Input::new(p.P1_06, Pull::Down);

        let col_0 = Output::new(p.P0_10, Level::Low, OutputDrive::Standard);
        let col_1 = Output::new(p.P0_09, Level::Low, OutputDrive::Standard);
        let col_2 = Output::new(p.P1_11, Level::Low, OutputDrive::Standard);
        let col_3 = Output::new(p.P1_13, Level::Low, OutputDrive::Standard);
        let col_4 = Output::new(p.P1_15, Level::Low, OutputDrive::Standard);
        let col_5 = Output::new(p.P0_02, Level::Low, OutputDrive::Standard);

        (
            [row_0, row_1, row_2, row_3],
            [col_0, col_1, col_2, col_3, col_4, col_5],
        )
    };
    #[cfg(not(feature = "peripheral_left"))] // peripheral_right
    let (row_pins, col_pins) = {
        let row_0 = Input::new(p.P1_13, Pull::Down);
        let row_1 = Input::new(p.P1_11, Pull::Down);
        let row_2 = Input::new(p.P0_10, Pull::Down);
        let row_3 = Input::new(p.P0_09, Pull::Down);

        let col_0 = Output::new(p.P0_22, Level::Low, OutputDrive::Standard);
        let col_1 = Output::new(p.P0_24, Level::Low, OutputDrive::Standard);
        let col_2 = Output::new(p.P1_00, Level::Low, OutputDrive::Standard);
        let col_3 = Output::new(p.P0_11, Level::Low, OutputDrive::Standard);
        let col_4 = Output::new(p.P1_04, Level::Low, OutputDrive::Standard);
        let col_5 = Output::new(p.P1_06, Level::Low, OutputDrive::Standard);

        (
            [row_0, row_1, row_2, row_3],
            [col_0, col_1, col_2, col_3, col_4, col_5],
        )
    };

    // Initialize flash
    // nRF52840's bootloader starts from 0xF4000(976K)
    let storage_config = StorageConfig {
        start_addr: 0xA0000, // 640K
        num_sectors: 32,     // 128K
        ..Default::default()
    };
    let flash = Flash::take(mpsl, p.NVMC);
    let mut storage = new_storage_for_split_peripheral(flash, storage_config).await;

    // Initialize the peripheral matrix
    let debouncer = DefaultDebouncer::new();
    let mut matrix = Matrix::<_, _, _, ROW, { COL / 2 }, true>::new(row_pins, col_pins, debouncer);

    // Start
    join(
        run_devices! (
            (matrix) => EVENT_CHANNEL, // Peripheral uses EVENT_CHANNEL to send events to central
        ),
        #[cfg(feature = "peripheral_left")]
        run_rmk_split_peripheral(0, &stack, &mut storage),
        #[cfg(not(feature = "peripheral_left"))]
        run_rmk_split_peripheral(1, &stack, &mut storage),
    )
    .await;
}
