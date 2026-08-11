#[cfg(any(feature = "peripheral_left", feature = "peripheral_right"))]
macro_rules! config_matrix_pins_nrf {
    (peripherals: $p:ident, input: [$($in_pin:ident), *], output: [$($out_pin:ident), +]) => {
        {
            let mut output_pins = [$(Output::new($p.$out_pin, embassy_nrf::gpio::Level::Low, embassy_nrf::gpio::OutputDrive::Standard)), +];
            let input_pins = [$(Input::new($p.$in_pin, embassy_nrf::gpio::Pull::Down)), +];
            output_pins.iter_mut().for_each(|p| {
                p.set_low();
            });
            (input_pins, output_pins)
        }
    };
}

// position doesn't exist on the pcb (as opposed to k!(No), which is a real key mapped to nothing)
#[macro_export]
macro_rules! na {
    () => {
        k!(No)
    };
}

// home row mods
macro_rules! hrm {
    ($k: ident, $m: ident) => {
        KeyAction::TapHold(
            Action::Key(KeyCode::Hid(HidKeyCode::$k)),
            Action::Modifier(ModifierCombination::$m),
            crate::keymap::HRM_PROFILE,
        )
    };
}

// key or layer
#[macro_export]
macro_rules! kol {
    ($k: ident, $x: expr) => {
        KeyAction::TapHold(
            Action::Key(KeyCode::Hid(HidKeyCode::$k)),
            Action::LayerOn($x),
            crate::keymap::LAYER_PROFILE,
        )
    };
}
