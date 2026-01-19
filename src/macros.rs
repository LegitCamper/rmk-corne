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

#[macro_export]
macro_rules! hrm {
    ($k: ident, $m: ident) => {
        KeyAction::TapHold(
            Action::Key(KeyCode::$k),
            Action::Modifier(ModifierCombination::$m),
            MorseProfile::new(Some(true), Some(MorseMode::PermissiveHold), Some(175), None),
        )
    };
}

// key or layer
#[macro_export]
macro_rules! kol {
    ($k: ident, $x: expr) => {
        KeyAction::TapHold(
            Action::Key(KeyCode::$k),
            Action::LayerOn($x),
            MorseProfile::new(None, None, Some(175), None),
        )
    };
}
