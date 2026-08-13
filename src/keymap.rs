#[cfg(feature = "central")]
use rmk::types::{
    action::{Action, KeyAction},
    keycode::{HidKeyCode, KeyCode},
    modifier::ModifierCombination,
};
#[cfg(feature = "central")]
use rmk::{a, k, mo, to, wm};

pub(crate) const COL: usize = 12;
pub(crate) const ROW: usize = 4;
#[cfg(feature = "central")]
pub(crate) const NUM_LAYER: usize = 5;

#[cfg(feature = "central")]
pub(crate) const LAYER_PROFILE: u8 = 0;
#[cfg(feature = "central")]
pub(crate) const WIN_PROFILE: u8 = 1;

#[cfg(feature = "central")]
#[rustfmt::skip]
pub const fn get_default_keymap() -> [[[KeyAction; COL]; ROW]; NUM_LAYER] {
    [
        [ // base
            [k!(LAlt), k!(Q), k!(W), k!(E), k!(R), k!(T), k!(Y), k!(U), k!(I), k!(O), k!(P), to!(3)],
            [k!(LCtrl), k!(A), k!(S), k!(D), k!(F), k!(G), k!(H), k!(J), k!(K), k!(L), k!(Semicolon), k!(Quote)],
            [k!(LShift), k!(Z), k!(X), k!(C), k!(V), k!(B), k!(N), k!(M), k!(Comma), k!(Dot), k!(Slash), k!(LShift)],
            [na!(), na!(), na!(), k!(Backspace), kow!(Escape), kol!(Space, 1), kol!(Enter, 2), kow!(Tab), k!(Delete), na!(), na!(), na!()],
        ],
        [ // num
            [a!(Transparent), a!(Transparent),a!(Transparent), k!(LeftBracket), k!(RightBracket), k!(Grave), wm!(Grave, ModifierCombination::LSHIFT), wm!(LeftBracket, ModifierCombination::LSHIFT), wm!(RightBracket, ModifierCombination::LSHIFT), a!(Transparent), a!(Transparent), a!(Transparent)],  
            [k!(CapsLock),  k!(Kc1), k!(Kc2), k!(Kc3), k!(Kc4), k!(Kc5), k!(Kc6), k!(Kc7), k!(Kc8), k!(Kc9), k!(Kc0), a!(Transparent)], 
            [a!(Transparent), a!(Transparent), a!(Transparent), k!(Enter), k!(Minus), wm!(Minus, ModifierCombination::LSHIFT), k!(KpEqual), k!(KpPlus), a!(Transparent), a!(Transparent), k!(Backslash), a!(Transparent)], 
            [na!(), na!(), na!(), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), na!(), na!(), na!()], 
        ], 
        [ // nav
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(Home), k!(PageDown), k!(PageUp), k!(End), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(Left), k!(Down), k!(Up), k!(Right), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No)], 
            [na!(), na!(), na!(), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), na!(), na!(), na!()], 
        ],
        [ // gaming base
            [k!(Tab), k!(Q), k!(W), k!(E), k!(R), k!(T), k!(Y), k!(U), k!(I), k!(O), k!(P),to!(0)],
            [k!(LCtrl), k!(A), k!(S), k!(D), k!(F), k!(G), k!(H), k!(J), k!(K), k!(L), k!(No), k!(No)],
            [k!(LShift), k!(Z), k!(X), k!(C), k!(V), k!(B), k!(N), k!(M), k!(Comma), k!(Dot), k!(No),k!(No)],
            [na!(), na!(), na!(), k!(LAlt), mo!(4), k!(Space), k!(Enter), k!(Tab), k!(Delete), na!(), na!(), na!()],
        ],
        [ // gaming upper
            [k!(Escape), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)],
            [k!(CapsLock), k!(Kc1), k!(Kc2), k!(Kc3), k!(Kc4), k!(Kc5), k!(Kc6), k!(Kc7), k!(Kc8), k!(Kc9), k!(Kc0), a!(Transparent)],
            [a!(Transparent), k!(Kp6), k!(Kp7), k!(Kp8), k!(Kp9), k!(Kp0), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)],
            [na!(), na!(), na!(), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), na!(), na!(), na!()],
        ],
    ]
}
