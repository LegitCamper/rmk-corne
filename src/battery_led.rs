use embassy_nrf::gpio::Output;
use embassy_time::Duration;
use rmk::core_traits::Runnable;
use rmk::event::BatteryStatusEvent;
use rmk::macros::processor;
use rmk::processor::PollingProcessor;
use rmk::types::battery::BatteryStatus;

/// How long the LED stays lit for each blink, regardless of battery level.
const LED_ON_DURATION: Duration = Duration::from_secs(5);

/// While the battery is fine (or unknown) the LED just stays off. Any battery
/// update wakes the controller immediately anyway, so this can be long.
const IDLE_INTERVAL: Duration = Duration::from_secs(60);

/// Blink tiers as (max battery % for this tier, time the LED stays off between blinks),
/// ordered from most to least severe. Below 35% the LED starts blinking, and every
/// further 10% drop makes it blink faster.
const BLINK_TIERS: [(u8, Duration); 4] = [
    (4, Duration::from_secs(1)),   // < 5%:  ~6s cycle
    (14, Duration::from_secs(3)),  // < 15%: ~8s cycle
    (24, Duration::from_secs(10)), // < 25%: ~15s cycle
    (34, Duration::from_secs(25)), // < 35%: ~30s cycle
];

/// Blinks this half's onboard LED faster the lower its own battery gets, so
/// it's obvious which half is actually dying instead of guessing from an
/// aggregate indicator on the central.
///
/// Uses a hand-rolled `Runnable`/`PollingProcessor` (marked with
/// `#[::rmk::macros::runnable_generated]`) instead of the `#[processor]`
/// macro's `poll_interval` because the blink rate is dynamic, not fixed --
/// see `rmk::processor::DeadlineProcessor`'s docs for this exact pattern.
#[processor(subscribe = [BatteryStatusEvent])]
#[::rmk::macros::runnable_generated]
pub struct BatteryLowLedController<'d> {
    led: Output<'d>,
    low_active: bool,
    led_on: bool,
    battery: Option<u8>,
}

impl<'d> BatteryLowLedController<'d> {
    pub fn new(led: Output<'d>, low_active: bool) -> Self {
        Self {
            led,
            low_active,
            led_on: false,
            battery: None,
        }
    }

    async fn on_battery_status_event(&mut self, event: BatteryStatusEvent) {
        self.battery = match *event {
            BatteryStatus::Available { level, .. } => level,
            BatteryStatus::Unavailable => None,
        };
    }

    fn set_led(&mut self, on: bool) {
        if on == self.low_active {
            self.led.set_low();
        } else {
            self.led.set_high();
        }
        self.led_on = on;
    }

    /// Off-time for the current battery level, or `None` if no blinking is needed.
    fn off_duration(&self) -> Option<Duration> {
        let level = self.battery?;
        BLINK_TIERS
            .iter()
            .find(|(max, _)| level <= *max)
            .map(|(_, d)| *d)
    }
}

impl PollingProcessor for BatteryLowLedController<'_> {
    fn interval(&self) -> Duration {
        if self.led_on {
            LED_ON_DURATION
        } else {
            self.off_duration().unwrap_or(IDLE_INTERVAL)
        }
    }

    async fn update(&mut self) {
        if self.led_on {
            self.set_led(false);
        } else if self.off_duration().is_some() {
            self.set_led(true);
        }
    }
}

impl Runnable for BatteryLowLedController<'_> {
    async fn run(&mut self) -> ! {
        self.polling_loop().await
    }
}
