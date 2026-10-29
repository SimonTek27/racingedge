//! Bridge between Bevy's [`KeyCode`] and the virtual-key codes the simulator
//! stores in [`KeyboardMapping`](crate::simulator::input::KeyboardMapping).
//!
//! The ported input model uses the platform virtual-key numbers ksengine reads
//! (`b'W' as i32`, `0x26` for the up arrow, …), so it stays usable from the C
//! API and the headless server, which have no Bevy keyboard. The game binary
//! reads Bevy's `ButtonInput<KeyCode>`, so the two need one translation table —
//! this module, shared by the drive loop and the rebind screen.

use bevy::prelude::KeyCode;

/// The supported key set, as `(Bevy key, virtual-key code)`.
///
/// Letters and digits use their ASCII codes, as Win32 does; the rest are the
/// documented `VK_*` values also named by
/// [`key_name`](crate::simulator::input::key_name).
pub const KEY_TABLE: &[(KeyCode, i32)] = &[
    (KeyCode::KeyA, b'A' as i32),
    (KeyCode::KeyB, b'B' as i32),
    (KeyCode::KeyC, b'C' as i32),
    (KeyCode::KeyD, b'D' as i32),
    (KeyCode::KeyE, b'E' as i32),
    (KeyCode::KeyF, b'F' as i32),
    (KeyCode::KeyG, b'G' as i32),
    (KeyCode::KeyH, b'H' as i32),
    (KeyCode::KeyI, b'I' as i32),
    (KeyCode::KeyJ, b'J' as i32),
    (KeyCode::KeyK, b'K' as i32),
    (KeyCode::KeyL, b'L' as i32),
    (KeyCode::KeyM, b'M' as i32),
    (KeyCode::KeyN, b'N' as i32),
    (KeyCode::KeyO, b'O' as i32),
    (KeyCode::KeyP, b'P' as i32),
    (KeyCode::KeyQ, b'Q' as i32),
    (KeyCode::KeyR, b'R' as i32),
    (KeyCode::KeyS, b'S' as i32),
    (KeyCode::KeyT, b'T' as i32),
    (KeyCode::KeyU, b'U' as i32),
    (KeyCode::KeyV, b'V' as i32),
    (KeyCode::KeyW, b'W' as i32),
    (KeyCode::KeyX, b'X' as i32),
    (KeyCode::KeyY, b'Y' as i32),
    (KeyCode::KeyZ, b'Z' as i32),
    (KeyCode::Digit0, b'0' as i32),
    (KeyCode::Digit1, b'1' as i32),
    (KeyCode::Digit2, b'2' as i32),
    (KeyCode::Digit3, b'3' as i32),
    (KeyCode::Digit4, b'4' as i32),
    (KeyCode::Digit5, b'5' as i32),
    (KeyCode::Digit6, b'6' as i32),
    (KeyCode::Digit7, b'7' as i32),
    (KeyCode::Digit8, b'8' as i32),
    (KeyCode::Digit9, b'9' as i32),
    (KeyCode::F1, 0x70),
    (KeyCode::F2, 0x71),
    (KeyCode::F3, 0x72),
    (KeyCode::F4, 0x73),
    (KeyCode::F5, 0x74),
    (KeyCode::F6, 0x75),
    (KeyCode::F7, 0x76),
    (KeyCode::F8, 0x77),
    (KeyCode::F9, 0x78),
    (KeyCode::F10, 0x79),
    (KeyCode::F11, 0x7A),
    (KeyCode::F12, 0x7B),
    (KeyCode::ArrowLeft, 0x25),
    (KeyCode::ArrowUp, 0x26),
    (KeyCode::ArrowRight, 0x27),
    (KeyCode::ArrowDown, 0x28),
    (KeyCode::Space, 0x20),
    (KeyCode::Enter, 0x0D),
    (KeyCode::Escape, 0x1B),
    (KeyCode::Tab, 0x09),
    (KeyCode::Backspace, 0x08),
    // The left/right pairs share a `VK_*` code, so only the left variants are
    // listed: a rebind stores one code per action, and a duplicate would make
    // the reverse lookup ambiguous.
    (KeyCode::ShiftLeft, 0x10),
    (KeyCode::ControlLeft, 0x11),
    (KeyCode::AltLeft, 0x12),
    (KeyCode::Home, 0x24),
    (KeyCode::End, 0x23),
    (KeyCode::PageUp, 0x21),
    (KeyCode::PageDown, 0x22),
    (KeyCode::Insert, 0x2D),
    (KeyCode::Delete, 0x2E),
];

/// The virtual-key code for a Bevy key, or `None` when it has no counterpart
/// (a media key, a scancode without a VK, …).
pub fn vk_from_key_code(code: KeyCode) -> Option<i32> {
    KEY_TABLE
        .iter()
        .find(|(table_code, _)| *table_code == code)
        .map(|(_, vk)| *vk)
}

/// The Bevy key for a virtual-key code, or `None` when the binding is zero
/// (unbound) or outside the supported set.
pub fn key_code_from_vk(vk: i32) -> Option<KeyCode> {
    KEY_TABLE
        .iter()
        .find(|(_, table_vk)| *table_vk == vk)
        .map(|(code, _)| *code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_key_round_trips() {
        for (code, vk) in KEY_TABLE {
            assert_eq!(vk_from_key_code(*code), Some(*vk), "{code:?}");
            assert_eq!(key_code_from_vk(*vk), Some(*code), "VK{vk}");
        }
    }

    #[test]
    fn letters_and_digits_use_ascii_codes() {
        assert_eq!(vk_from_key_code(KeyCode::KeyW), Some(b'W' as i32));
        assert_eq!(vk_from_key_code(KeyCode::Digit5), Some(b'5' as i32));
        assert_eq!(key_code_from_vk(b'A' as i32), Some(KeyCode::KeyA));
    }

    #[test]
    fn special_keys_use_the_documented_codes() {
        assert_eq!(vk_from_key_code(KeyCode::Space), Some(0x20));
        assert_eq!(vk_from_key_code(KeyCode::ArrowUp), Some(0x26));
        assert_eq!(vk_from_key_code(KeyCode::Escape), Some(0x1B));
        assert_eq!(vk_from_key_code(KeyCode::F2), Some(0x71));
        assert_eq!(key_code_from_vk(0x28), Some(KeyCode::ArrowDown));
        assert_eq!(key_code_from_vk(0x7B), Some(KeyCode::F12));
    }

    #[test]
    fn unbound_and_unknown_codes_have_no_key() {
        assert_eq!(key_code_from_vk(0), None);
        assert_eq!(key_code_from_vk(-1), None);
        assert_eq!(key_code_from_vk(0xFF), None);
        assert_eq!(vk_from_key_code(KeyCode::Numpad0), None);
    }

    #[test]
    fn the_lookup_table_has_no_duplicate_virtual_keys() {
        let mut vks: Vec<i32> = KEY_TABLE.iter().map(|(_, vk)| *vk).collect();
        let total = vks.len();
        vks.sort_unstable();
        vks.dedup();
        assert_eq!(vks.len(), total);
    }
}
