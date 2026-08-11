use embassy_nrf::gpio::Output;
use rmk::event::PeripheralConnectedEvent;
use rmk::macros::processor;

/// Number of split peripherals the central expects.
const NUM_PERIPHERALS: usize = 2;

/// Blinks blue while any split peripheral isn't connected -- still
/// scanning at boot, or a half died/went out of range later -- off once
/// both are connected. Lets you glance at the dongle to tell a half is
/// missing instead of only noticing when keystrokes from that half stop.
#[processor(subscribe = [PeripheralConnectedEvent], poll_interval = 250)]
pub struct SearchingLedController<'d> {
    led: Output<'d>,
    low_active: bool,
    connected: [bool; NUM_PERIPHERALS],
    led_on: bool,
}

impl<'d> SearchingLedController<'d> {
    pub fn new(led: Output<'d>, low_active: bool) -> Self {
        Self {
            led,
            low_active,
            connected: [false; NUM_PERIPHERALS],
            led_on: false,
        }
    }

    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        if let Some(slot) = self.connected.get_mut(event.id) {
            *slot = event.connected;
        }
        if self.all_connected() {
            self.set_led(false);
        }
    }

    fn all_connected(&self) -> bool {
        self.connected.iter().all(|&c| c)
    }

    fn set_led(&mut self, on: bool) {
        if on == self.low_active {
            self.led.set_low();
        } else {
            self.led.set_high();
        }
        self.led_on = on;
    }

    async fn poll(&mut self) {
        if self.all_connected() {
            return;
        }
        let next = !self.led_on;
        self.set_led(next);
    }
}
