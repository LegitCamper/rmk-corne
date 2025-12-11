use rmk::types::action::KeyAction;
use rmk::types::modifier::ModifierCombination;
use rmk::{a, k, lt, mo, mt, wm};
pub(crate) const COL: usize = 12;
pub(crate) const ROW: usize = 4;
pub(crate) const NUM_LAYER: usize = 3;
#[rustfmt::skip]
pub const fn get_default_keymap() -> [[[KeyAction; COL]; ROW]; NUM_LAYER] {
    [
        [ // base
            [k!(No), k!(Q), k!(W), k!(E), k!(R), k!(T), k!(Y), k!(U), k!(I), k!(O), k!(P),mo!(3)],
            [k!(No), mt!(A, ModifierCombination::LALT), mt!(S, ModifierCombination::LGUI), mt!(D, ModifierCombination::LCTRL), mt!(F, ModifierCombination::LSHIFT), k!(G), k!(H), mt!(J, ModifierCombination::LSHIFT), mt!(K, ModifierCombination::LCTRL), mt!(L, ModifierCombination::LGUI), mt!(Semicolon, ModifierCombination::LALT), k!(Quote)],
            [k!(No), k!(Z), k!(X), k!(C), k!(V), k!(B), k!(N), k!(M), k!(Comma), k!(Dot), k!(Slash),k!(Backslash)],
            [k!(No), k!(No), k!(No), k!(Backspace), k!(Escape), lt!(1, Space), lt!(2, Enter), k!(Tab), k!(Delete), k!(No), a!(No),k!(No)],
        ],
        [ // num
            [a!(Transparent), a!(Transparent),a!(Transparent), k!(LeftBracket), k!(RightBracket), k!(Grave), wm!(Grave, ModifierCombination::LSHIFT), wm!(LeftBracket, ModifierCombination::LSHIFT), wm!(RightBracket, ModifierCombination::LSHIFT), a!(Transparent), a!(Transparent), a!(Transparent)],  
            [k!(CapsLock),  k!(Kp1), k!(Kp2), k!(Kp3), k!(Kp4), k!(Kp5), k!(Kp6), k!(Kp7), k!(Kp8), k!(Kp9), k!(Kp0), a!(Transparent)], 
            [a!(Transparent), a!(Transparent), a!(Transparent), k!(Enter), k!(Minus), wm!(Minus, ModifierCombination::LSHIFT), k!(KpEqual), k!(KpPlus), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent)], 
            [k!(No),  k!(No), k!(No), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), k!(No), k!(No), k!(No)], 
        ], 
        [ // nav
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(Home), k!(PageDown), k!(PageUp), k!(End), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(Left), k!(Down), k!(Up), k!(Right), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No), k!(No)], 
            [k!(No), k!(No), k!(No), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), a!(Transparent), k!(No), k!(No), k!(No)], 
        ],
    ]
}
