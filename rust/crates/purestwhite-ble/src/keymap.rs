//! HID keyboard usages (page 0x07) as US QWERTY characters and special keys: the SDK's
//! `hidTranslate` (freeink-sdk/libs/network/BleKeyboardHost/src/HidKeymap.cpp).

/// A key that is not a character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum SpecialKey {
    #[default]
    None = 0,
    Enter,
    Backspace,
    Tab,
    Escape,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// Either Shift in a modifier byte.
const SHIFT: u8 = 0x02 | 0x20;

/// Unshifted ASCII for usages 0x00 to 0x38 (0: none).
const BASE: &[u8; 0x39] = b"\0\0\0\0abcdefghijklmnopqrstuvwxyz1234567890\0\0\0\0 -=[]\\\0;'`,./";
/// Shifted ASCII for the same usages.
const SHIFTED: &[u8; 0x39] = b"\0\0\0\0ABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()\0\0\0\0 _+{}|\0:\"~<>?";

/// The special key a usage is.
fn special(usage: u8) -> Option<SpecialKey> {
    let key = match usage {
        0x28 | 0x58 => SpecialKey::Enter,
        0x29 => SpecialKey::Escape,
        0x2A => SpecialKey::Backspace,
        0x2B => SpecialKey::Tab,
        0x4C => SpecialKey::Delete,
        0x4A => SpecialKey::Home,
        0x4B => SpecialKey::PageUp,
        0x4D => SpecialKey::End,
        0x4E => SpecialKey::PageDown,
        0x4F => SpecialKey::Right,
        0x50 => SpecialKey::Left,
        0x51 => SpecialKey::Down,
        0x52 => SpecialKey::Up,
        _ => return None,
    };
    Some(key)
}

/// `usage` with modifiers `mods`: its character (ASCII, 0 for none) and its special key, as
/// `hidTranslate` writes them.
#[must_use]
pub fn translate(usage: u8, mods: u8) -> (u8, SpecialKey) {
    if let Some(key) = special(usage) {
        return (0, key);
    }
    let table = if mods & SHIFT != 0 { SHIFTED } else { BASE };
    let ch = table.get(usize::from(usage)).copied().unwrap_or(0);
    (ch, SpecialKey::None)
}
