#![no_std]
#![no_main]
#![allow(static_mut_refs)]

extern crate alloc;

#[macro_use]
mod macros;

mod keymap;
use keymap::{COL, NUM_LAYER, ROW};
use talc::{ClaimOnOom, Span, Talc, Talck};

mod prospector;
use crate::prospector::display::{ProspectorPins, create_display};

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_nrf::gpio::Output;
use embassy_nrf::mode::Async;
use embassy_nrf::peripherals::{RNG, SPI3, USBD};
use embassy_nrf::saadc::{self};
use embassy_nrf::usb::Driver;
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::{bind_interrupts, rng, spim, usb};
use nrf_mpsl::Flash;
use nrf_sdc::mpsl::MultiprotocolServiceLayer;
use nrf_sdc::{self as sdc, mpsl};
use rand_chacha::ChaCha12Rng;
use rand_core::SeedableRng;
use rmk::ble::build_ble_stack;
use rmk::config::{BehaviorConfig, DeviceConfig, PositionalConfig, RmkConfig, StorageConfig};
use rmk::controller::EventController as _;
use rmk::controller::led_indicator::KeyboardIndicatorController;
use rmk::futures::future::{join, join3, join4};
use rmk::input_device::Runnable;
use rmk::keyboard::Keyboard;
use rmk::split::ble::central::{read_peripheral_addresses, scan_peripherals};
use rmk::split::central::run_peripheral_manager;
use rmk::types::action::EncoderAction;
use rmk::{HostResources, initialize_encoder_keymap_and_storage, run_rmk};
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

static mut ARENA: [u8; 25 * 1024] = [0; 25 * 1024];

#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> =
    Talc::new(unsafe { ClaimOnOom::new(Span::from_array(core::ptr::addr_of!(ARENA).cast_mut())) })
        .lock();

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    SAADC => saadc::InterruptHandler;
    RNG => rng::InterruptHandler<RNG>;
    EGU0_SWI0 => nrf_sdc::mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => nrf_sdc::mpsl::ClockInterruptHandler, usb::vbus_detect::InterruptHandler;
    RADIO => nrf_sdc::mpsl::HighPrioInterruptHandler;
    TIMER0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
    RTC0 => nrf_sdc::mpsl::HighPrioInterruptHandler;
    SPIM3 => spim::InterruptHandler<SPI3>;
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
        .support_scan()?
        .support_central()?
        .support_adv()?
        .support_peripheral()?
        .support_dle_peripheral()?
        .support_dle_central()?
        .support_phy_update_central()?
        .support_phy_update_peripheral()?
        .support_le_2m_phy()?
        .central_count(2)? // The number of peripherals
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
    spawner.must_spawn(mpsl_task(&*mpsl));
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
        p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );
    let mut rng = rng::Rng::new(p.RNG, Irqs);
    let mut rng_gen = ChaCha12Rng::from_rng(&mut rng).unwrap();
    let mut sdc_mem = sdc::Mem::<15472>::new();
    let sdc = unwrap!(build_sdc(sdc_p, &mut rng, mpsl, &mut sdc_mem));
    let mut host_resources = HostResources::new();
    let stack = build_ble_stack(sdc, ble_addr(), &mut rng_gen, &mut host_resources).await;

    // Initialize usb driver
    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));

    // Initialize flash
    let flash = Flash::take(mpsl, p.NVMC);

    // Keyboard config
    let keyboard_device_config = DeviceConfig {
        vid: 0x4c4b,
        pid: 0x4643,
        manufacturer: "LegitCamper",
        product_name: "RMK Keyboard",
        serial_number: "na",
    };
    let storage_config = StorageConfig {
        start_addr: 0xA0000,
        num_sectors: 6,
        #[cfg(feature = "reset")]
        clear_storage: true,
        #[cfg(feature = "reset")]
        clear_layout: true,
        ..Default::default()
    };
    let rmk_config = RmkConfig {
        device_config: keyboard_device_config,
        storage_config,
        ..Default::default()
    };

    // Initialize keyboard stuffs
    // Initialize the storage and keymap
    let mut default_keymap = keymap::get_default_keymap();
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

    // Initialize the matrix and keyboard
    let mut keyboard = Keyboard::new(&keymap);

    // Read peripheral address from storage
    let peripheral_addrs =
        read_peripheral_addresses::<2, _, ROW, COL, NUM_LAYER, 0>(&mut storage).await;

    // create prospector display
    let (display, _backlight_pin, framebuffer) = create_display(ProspectorPins {
        spi: p.SPI3,
        dc: p.P1_12,
        sck: p.P1_13,
        cs: p.P1_14,
        mosi: p.P1_15,
        bl: p.P1_11,
        rst: p.P0_29,
    })
    .await;

    // Start
    join(
        // keyboard.run(),
        join4(
            scan_peripherals(&stack, &peripheral_addrs),
            run_peripheral_manager::<ROW, COL, 0, 0, _>(0, &peripheral_addrs, &stack),
            run_peripheral_manager::<ROW, COL, 0, 6, _>(1, &peripheral_addrs, &stack),
            run_rmk(&keymap, driver, &stack, &mut storage, rmk_config),
        ),
        prospector::run(display, framebuffer),
    )
    .await;
}
