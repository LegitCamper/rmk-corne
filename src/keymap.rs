#[cfg(not(any(feature = "peripheral_left", feature = "peripheral_right")))]
use rmk::types::{
    action::{Action, KeyAction, MorseMode, MorseProfile},
    keycode::KeyCode,
    modifier::ModifierCombination,
};
#[cfg(not(any(feature = "peripheral_left", feature = "peripheral_right")))]
use rmk::{a, k, mo, to, wm};

#[cfg(not(any(feature = "peripheral_left", feature = "peripheral_right")))]
use crate::{hrm, kol};

pub(crate) const COL: usize = 12;
pub(crate) const ROW: usize = 4;
#[cfg(not(any(feature = "peripheral_left", feature = "peripheral_right")))]
pub(crate) const NUM_LAYER: usize = 5;

#[cfg(not(any(feature = "peripheral_left", feature = "peripheral_right")))]
#[rustfmt::skip]
pub const fn get_default_keymap() -> [[[KeyAction; COL]; ROW]; NUM_LAYER] {
    [
        [ // base
            [k!(No), k!(Q), k!(W), k!(E), k!(R), k!(T), k!(Y), k!(U), k!(I), k!(O), k!(P), to!(3)],
            [k!(No), hrm!(A, LALT), hrm!(S, LGUI), hrm!(D, LCTRL), hrm!(F, LSHIFT), k!(G), k!(H), hrm!(J, LSHIFT), hrm!(K, LCTRL), hrm!(L, LGUI), hrm!(Semicolon, LALT), k!(Quote)],
            [k!(No), k!(Z), k!(X), k!(C), k!(V), k!(B), k!(N), k!(M), k!(Comma), k!(Dot), k!(Slash),k!(Backslash)],
            [k!(No), k!(No), k!(No), k!(Backspace), k!(Escape), kol!(1, Space), kol!(2, Enter), k!(Tab), k!(Delete), k!(No), a!(No), k!(No)],
        ],
        [ // num
            [a!(Transparent), a!(Transparent),a!(Transparent), k!(LeftBracket), k!(RightBracket), k!(Grave), wm!(Grave, ModifierCombination::LSHIFT), wm!(LeftBracket, ModifierCombination::LSHIFT), wm!(RightBracket, ModifierCombination::LSHIFT), a!(Transparent), a!(Transparent), a!(Transparent)],  
            [k!(CapsLock),  k!(Kc1), k!(Kc2), k!(Kc3), k!(Kc4), k!(Kc5), k!(Kc6), k!(Kc7), k!(Kc8), k!(Kc9), k!(Kc0), a!(Transparent)], 
            [a!(Transparent), a!(Transparent), a!(Transparent), k!(Enter), k!(Minus), wm!(Minus, ModifierCombination::LSHIFT), k!(KpEqual), k!(KpPlus), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)], 
            [k!(No),  k!(No), k!(No), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), k!(No), k!(No), k!(No)], 
        ], 
        [ // nav
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(Home), k!(PageDown), k!(PageUp), k!(End), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(Left), k!(Down), k!(Up), k!(Right), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), k!(No), k!(No), k!(No)], 
        ],
        [ // gaming base
            [k!(Tab), k!(Q), k!(W), k!(E), k!(R), k!(T), k!(Y), k!(U), k!(I), k!(O), k!(P),to!(0)],
            [k!(LCtrl), k!(A), k!(S), k!(D), k!(F), k!(G), k!(H), k!(J), k!(K), k!(L), k!(No), k!(No)],
            [k!(LShift), k!(Z), k!(X), k!(C), k!(V), k!(B), k!(N), k!(M), k!(Comma), k!(Dot), k!(No),k!(No)],
            [k!(No), k!(No), k!(No), k!(LAlt), mo!(4), k!(Space), kol!(2, Enter), k!(Tab), k!(Delete), k!(No), k!(No), k!(No)],
        ],
        [ // gaming upper
            [k!(Escape), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)],
            [k!(CapsLock), k!(Kp1), k!(Kp2), k!(Kp3), k!(Kp4), k!(Kp5), k!(Kp6), k!(Kp7), k!(Kp8), k!(Kp9), k!(Kp0), a!(Transparent)],
            [a!(Transparent), k!(Kp6), k!(Kp7), k!(Kp8), k!(Kp9), k!(Kp0), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)],
            [a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)],
        ],
    ]
}
