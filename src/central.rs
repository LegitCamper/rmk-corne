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
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_nrf::interrupt::{InterruptExt, Priority};
use embassy_nrf::mode::Async;
use embassy_nrf::peripherals::{
    NVMC, PPI_CH17, PPI_CH18, PPI_CH19, PPI_CH20, PPI_CH21, PPI_CH22, PPI_CH23, PPI_CH24, PPI_CH25,
    PPI_CH26, PPI_CH27, PPI_CH28, PPI_CH29, PPI_CH30, PPI_CH31, RNG, SPI3, USBD,
};
use embassy_nrf::saadc::{self};
use embassy_nrf::usb::Driver;
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::{Peri, interrupt};
use embassy_nrf::{bind_interrupts, rng, spim, usb};
use nrf_mpsl::Flash;
use nrf_sdc::mpsl::MultiprotocolServiceLayer;
use nrf_sdc::{self as sdc, SoftdeviceController, mpsl};
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
use rmk::{
    DefaultPacketPool, HostResources, Stack, initialize_encoder_keymap_and_storage, run_rmk,
};
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

static mut ARENA: [u8; 5 * 1024] = [0; 5 * 1024];

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

/// How many outgoing L2CAP buffers per link
const L2CAP_TXQ: u8 = 3;

/// How many incoming L2CAP buffers per link
const L2CAP_RXQ: u8 = 3;

/// Size of L2CAP packets
const L2CAP_MTU: usize = 251;

fn build_sdc<const N: usize>(
    p: nrf_sdc::Peripherals<'static>,
    rng: &'static mut rng::Rng<Async>,
    mpsl: &'static MultiprotocolServiceLayer,
    mem: &'static mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'static>, nrf_sdc::Error> {
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

static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();

struct MpslPeris {
    rtc: Peri<'static, embassy_nrf::peripherals::RTC0>,
    timer: Peri<'static, embassy_nrf::peripherals::TIMER0>,
    timer1: Peri<'static, embassy_nrf::peripherals::TEMP>,
    ch19: Peri<'static, PPI_CH19>,
    ch30: Peri<'static, PPI_CH30>,
    ch31: Peri<'static, PPI_CH31>,
}

struct SdcPeris {
    ppi17: Peri<'static, PPI_CH17>,
    ppi18: Peri<'static, PPI_CH18>,
    ppi20: Peri<'static, PPI_CH20>,
    ppi21: Peri<'static, PPI_CH21>,
    ppi22: Peri<'static, PPI_CH22>,
    ppi23: Peri<'static, PPI_CH23>,
    ppi24: Peri<'static, PPI_CH24>,
    ppi25: Peri<'static, PPI_CH25>,
    ppi26: Peri<'static, PPI_CH26>,
    ppi27: Peri<'static, PPI_CH27>,
    ppi28: Peri<'static, PPI_CH28>,
    ppi29: Peri<'static, PPI_CH29>,
}

#[embassy_executor::task]
async fn run_med(
    mpsl_peris: MpslPeris,
    sdc_peris: SdcPeris,
    rng: Peri<'static, RNG>,
    nvmc: Peri<'static, NVMC>,
    usbd: Peri<'static, USBD>,
) -> ! {
    let mpsl_p = mpsl::Peripherals::new(
        mpsl_peris.rtc,
        mpsl_peris.timer,
        mpsl_peris.timer1,
        mpsl_peris.ch19,
        mpsl_peris.ch30,
        mpsl_peris.ch31,
    );
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
    let sdc_p = sdc::Peripherals::new(
        sdc_peris.ppi17,
        sdc_peris.ppi18,
        sdc_peris.ppi20,
        sdc_peris.ppi21,
        sdc_peris.ppi22,
        sdc_peris.ppi23,
        sdc_peris.ppi24,
        sdc_peris.ppi25,
        sdc_peris.ppi26,
        sdc_peris.ppi27,
        sdc_peris.ppi28,
        sdc_peris.ppi29,
    );
    static RNG: StaticCell<rng::Rng<'static, Async>> = StaticCell::new();
    let rng = RNG.init(rng::Rng::new(rng, Irqs));
    let mut rng_gen = ChaCha12Rng::from_rng(&mut *rng).unwrap();
    static SDC_MEM: StaticCell<sdc::Mem<15472>> = StaticCell::new();
    let sdc_mem = SDC_MEM.init(sdc::Mem::<15472>::new());
    let sdc = unwrap!(build_sdc(sdc_p, rng, mpsl, sdc_mem));
    let mut host_resources = HostResources::new();
    let stack = build_ble_stack(sdc, ble_addr(), &mut rng_gen, &mut host_resources).await;

    // Initialize usb driver
    let driver = Driver::new(usbd, Irqs, HardwareVbusDetect::new(Irqs));

    // Initialize flash
    let flash = Flash::take(mpsl, nvmc);

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

    // Start
    join3(
        mpsl.run(),
        keyboard.run(),
        join4(
            scan_peripherals(&stack, &peripheral_addrs),
            run_peripheral_manager::<ROW, COL, 0, 0, _>(0, &peripheral_addrs, &stack),
            run_peripheral_manager::<ROW, COL, 0, 6, _>(1, &peripheral_addrs, &stack),
            run_rmk(&keymap, driver, &stack, &mut storage, rmk_config),
        ),
    )
    .await;

    loop {}
}

#[embassy_executor::task]
async fn run_low(pins: ProspectorPins) {
    let (display, _backlight_pin, framebuffer) = create_display(pins).await;
    prospector::run(display, framebuffer).await
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

    let mpsl_peris = MpslPeris {
        rtc: p.RTC0,
        timer: p.TIMER0,
        timer1: p.TEMP,
        ch19: p.PPI_CH19,
        ch30: p.PPI_CH30,
        ch31: p.PPI_CH31,
    };

    let sdc_peris = SdcPeris {
        ppi17: p.PPI_CH17,
        ppi18: p.PPI_CH18,
        ppi20: p.PPI_CH20,
        ppi21: p.PPI_CH21,
        ppi22: p.PPI_CH22,
        ppi23: p.PPI_CH23,
        ppi24: p.PPI_CH24,
        ppi25: p.PPI_CH25,
        ppi26: p.PPI_CH26,
        ppi27: p.PPI_CH27,
        ppi28: p.PPI_CH28,
        ppi29: p.PPI_CH29,
    };

    // Medium-priority executor: EGU0_SWI0, priority level 7
    interrupt::EGU0_SWI0.set_priority(Priority::P7);
    let spawner = EXECUTOR_MED.start(interrupt::EGU0_SWI0);
    spawner
        .spawn(run_med(mpsl_peris, sdc_peris, p.RNG, p.NVMC, p.USBD))
        .unwrap();

    let prospector_pins = ProspectorPins {
        spi: p.SPI3,
        dc: p.P1_12,
        sck: p.P1_13,
        cs: p.P1_14,
        mosi: p.P1_15,
        bl: p.P1_11,
        rst: p.P0_29,
    };

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(run_low(prospector_pins)).unwrap();
    });
}
