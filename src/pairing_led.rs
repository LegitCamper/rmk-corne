use embassy_nrf::gpio::Output;
use rmk::event::CentralConnectedEvent;
use rmk::macros::processor;

/// Number of on/off blinks of the green "alive" flash at boot, before
/// switching to the blue "still pairing" blink.
const ALIVE_BLINKS: u8 = 3;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// Boot-time "hey, I'm alive" flash on the green channel.
    Alive,
    /// Blinking blue while advertising/trying to (re)connect to the central.
    Pairing,
    /// Connected -- both channels off.
    Connected,
}

/// Drives two channels of this half's onboard RGB LED as a status indicator:
/// a quick green flash at boot ("I'm alive"), then a blue blink while
/// advertising/reconnecting to the central, off once connected.
#[processor(subscribe = [CentralConnectedEvent], poll_interval = 200)]
pub struct PairingLedController<'d> {
    green: Output<'d>,
    blue: Output<'d>,
    low_active: bool,
    phase: Phase,
    alive_ticks_remaining: u8,
    led_on: bool,
}

impl<'d> PairingLedController<'d> {
    pub fn new(green: Output<'d>, blue: Output<'d>, low_active: bool) -> Self {
        Self {
            green,
            blue,
            low_active,
            phase: Phase::Alive,
            // Each blink is one on + one off tick.
            alive_ticks_remaining: ALIVE_BLINKS * 2,
            led_on: false,
        }
    }

    async fn on_central_connected_event(&mut self, event: CentralConnectedEvent) {
        if event.connected {
            self.phase = Phase::Connected;
            self.led_on = false;
            self.set(false, false);
        } else if self.phase != Phase::Alive {
            // Don't cut the boot "alive" flash short; only take over once
            // we're past it (initial pairing, or a later reconnect attempt).
            self.phase = Phase::Pairing;
        }
    }

    fn set(&mut self, green_on: bool, blue_on: bool) {
        Self::drive(&mut self.green, green_on, self.low_active);
        Self::drive(&mut self.blue, blue_on, self.low_active);
    }

    fn drive(led: &mut Output<'d>, on: bool, low_active: bool) {
        if on == low_active {
            led.set_low();
        } else {
            led.set_high();
        }
    }

    async fn poll(&mut self) {
        match self.phase {
            Phase::Alive => {
                self.led_on = !self.led_on;
                self.set(self.led_on, false);
                self.alive_ticks_remaining = self.alive_ticks_remaining.saturating_sub(1);
                if self.alive_ticks_remaining == 0 {
                    self.phase = Phase::Pairing;
                    self.led_on = false;
                    self.set(false, false);
                }
            }
            Phase::Pairing => {
                self.led_on = !self.led_on;
                self.set(false, self.led_on);
            }
            Phase::Connected => {}
        }
    }
}
