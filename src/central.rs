#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod keymap;
mod search_led;
use keymap::{COL, ROW};
use search_led::SearchingLedController;

use defmt::{info, unwrap};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::mode::Async;
use embassy_nrf::peripherals::{RNG, USBD};
use embassy_nrf::usb::Driver;
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::{bind_interrupts, rng, usb};
use nrf_mpsl::Flash;
use nrf_sdc::mpsl::MultiprotocolServiceLayer;
use nrf_sdc::{self as sdc, mpsl};
use panic_probe as _;
use rmk::ble::BleTransport;
use rmk::config::{
    BehaviorConfig, BleBatteryConfig, DeviceConfig, LockConfig, PositionalConfig, RmkConfig,
    StorageConfig,
};
use rmk::host::HostService;
use rmk::split::PeripheralMatrixConfig;
use rmk::types::morse::{MorseMode, MorseProfile};
use rmk::usb::UsbTransport;
use rmk::watchdog::Nrf52Watchdog;
use rmk::{KeymapData, initialize_keymap_and_storage, run_all};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
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

/// Unlike the peripherals' `build_sdc` (peripheral-only role), the central
/// also scans for and connects to both split peripherals as a BLE central,
/// on top of advertising itself to the host -- hence the extra
/// support_scan/support_central/central_count(2) and the much larger SDC
/// memory buffer below.
fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<Async>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, nrf_sdc::Error> {
    sdc::Builder::new()?
        .support_scan()
        .support_central()
        .support_adv()
        .support_peripheral()
        .support_dle_peripheral()
        .support_dle_central()
        .support_phy_update_central()
        .support_phy_update_peripheral()
        .support_le_2m_phy()
        .central_count(2)? // The number of split peripherals
        .peripheral_count(1)?
        .buffer_cfg(L2CAP_MTU as u16, L2CAP_MTU as u16, L2CAP_TXQ, L2CAP_RXQ)?
        .build(p, rng, mpsl, mem)
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
    spawner.spawn(mpsl_task(&*mpsl).unwrap());
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
        p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );
    let mut rng = rng::Rng::new(p.RNG, Irqs);
    // Central needs a much bigger SDC buffer than a peripheral: it juggles
    // both an advertising/GATT link to the host and two scan+central links
    // to the split halves at once.
    let mut sdc_mem = sdc::Mem::<15472>::new();
    let sdc = unwrap!(build_sdc(sdc_p, &mut rng, mpsl, &mut sdc_mem));

    // Initialize usb driver. The central is a plain XIAO BLE dongle -- no
    // matrix of its own, just USB<->BLE bridging for the two split halves.
    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));

    // Use internal flash to emulate eeprom
    let flash = Flash::take(mpsl, p.NVMC);

    // Keyboard config
    let keyboard_device_config = DeviceConfig {
        vid: 0x4c4b,
        pid: 0x4643,
        manufacturer: "LegitCamper",
        product_name: "RMK Keyboard",
        ..DeviceConfig::default()
    };
    // Rynk's physical-presence unlock gate: no unlock keys configured, so
    // dangerous ops (e.g. config writes) stay permanently locked.
    let lock_config = LockConfig::default();
    // The central is a USB-powered dongle with no battery of its own -- only
    // the split peripherals report battery level.
    let ble_battery_config = BleBatteryConfig::disabled();
    let storage_config = StorageConfig {
        start_addr: 0xA0000, // 640K, same layout as the peripherals
        num_sectors: 32,     // 128K
        #[cfg(feature = "reset")]
        clear_storage: true,
        #[cfg(feature = "reset")]
        clear_layout: true,
        ..Default::default()
    };
    let rmk_config = RmkConfig {
        device_config: keyboard_device_config,
        lock_config,
        layout_blob: &[],
        ble_battery_config,
        storage_config,
    };

    // Initialize keyboard stuffs
    // Initialize the storage and keymap
    let mut keymap_data = KeymapData::new(keymap::get_default_keymap());
    let mut behavior_config = BehaviorConfig::default();
    behavior_config.morse.enable_flow_tap = true;
    // Registered in the same order as `keymap::HRM_PROFILE`/`LAYER_PROFILE`,
    // which `hrm!`/`kol!` in macros.rs reference by index.
    behavior_config
        .morse
        .profiles
        .push(MorseProfile::new(
            Some(true),
            Some(MorseMode::PermissiveHold),
            Some(175),
            None,
        ))
        .unwrap();
    behavior_config
        .morse
        .profiles
        .push(MorseProfile::new(None, None, Some(175), None))
        .unwrap();
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
    let host_service = HostService::new(&keymap, &rmk_config);

    let mut usb_transport =
        UsbTransport::new(driver, rmk_config.device_config).with_host_service(&host_service);
    // Two peripheral halves, both 4x6 at column offset 0/6 within the full
    // 4x12 keymap -- matches `PeripheralLeft`/`PeripheralRight`'s own matrix.
    let mut ble_transport = BleTransport::new(
        sdc,
        ble_addr(),
        rmk_config,
        [
            PeripheralMatrixConfig {
                rows: ROW as u8,
                cols: (COL / 2) as u8,
                row_offset: 0,
                col_offset: 0,
            },
            PeripheralMatrixConfig {
                rows: ROW as u8,
                cols: (COL / 2) as u8,
                row_offset: 0,
                col_offset: (COL / 2) as u8,
            },
        ],
    )
    .with_host_service(&host_service);
    let mut wpm_processor = rmk::processor::builtin::wpm::WpmProcessor::new();
    let mut watchdog_runner = Nrf52Watchdog::default_runner(p.WDT);

    // Blinks blue while still looking for a split peripheral (at boot, or
    // if a half dies/goes out of range later), off once both are connected.
    let search_led_pin = Output::new(p.P0_06, Level::High, OutputDrive::Standard);
    let mut search_led = SearchingLedController::new(search_led_pin, true);

    // Start. Peripheral scanning/pairing now happens inside `ble_transport`
    // itself -- no separate scan/manager tasks needed.
    run_all!(
        storage,
        usb_transport,
        ble_transport,
        wpm_processor,
        keyboard,
        watchdog_runner,
        search_led
    )
    .await;
}
