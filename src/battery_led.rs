use defmt::unwrap;
use embassy_nrf::gpio::Output;
use embassy_time::Duration;
use rmk::channel::{CONTROLLER_CHANNEL, ControllerSub};
use rmk::controller::{Controller, PollingController};
use rmk::event::ControllerEvent;

/// Number of split peripherals whose battery level feeds this LED.
const NUM_PERIPHERALS: usize = 2;

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

/// Blinks an LED on the central faster the lower the (lowest reported) peripheral
/// battery level gets, using battery levels relayed from the split peripherals over BLE.
pub struct BatteryLowLedController<'d> {
    led: Output<'d>,
    low_active: bool,
    sub: ControllerSub,
    battery: [Option<u8>; NUM_PERIPHERALS],
    led_on: bool,
}

impl<'d> BatteryLowLedController<'d> {
    pub fn new(led: Output<'d>, low_active: bool) -> Self {
        Self {
            led,
            low_active,
            sub: unwrap!(CONTROLLER_CHANNEL.subscriber()),
            battery: [None; NUM_PERIPHERALS],
            led_on: false,
        }
    }

    fn set_led(&mut self, on: bool) {
        if on == self.low_active {
            self.led.set_low();
        } else {
            self.led.set_high();
        }
        self.led_on = on;
    }

    fn lowest_battery(&self) -> Option<u8> {
        self.battery.iter().flatten().copied().min()
    }

    /// Off-time for the current lowest battery level, or `None` if no blinking is needed.
    fn off_duration(&self) -> Option<Duration> {
        let level = self.lowest_battery()?;
        BLINK_TIERS
            .iter()
            .find(|(max, _)| level <= *max)
            .map(|(_, d)| *d)
    }
}

impl<'d> Controller for BatteryLowLedController<'d> {
    type Event = ControllerEvent;

    async fn process_event(&mut self, event: Self::Event) {
        match event {
            ControllerEvent::SplitPeripheralBattery(id, level) => {
                if let Some(slot) = self.battery.get_mut(id) {
                    *slot = Some(level);
                }
            }
            // Peripheral disconnected: drop its stale battery reading.
            ControllerEvent::SplitPeripheral(id, false) => {
                if let Some(slot) = self.battery.get_mut(id) {
                    *slot = None;
                }
            }
            _ => (),
        }
    }

    async fn next_message(&mut self) -> Self::Event {
        self.sub.next_message_pure().await
    }
}

impl<'d> PollingController for BatteryLowLedController<'d> {
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
