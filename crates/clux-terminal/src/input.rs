use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Convert a winit `KeyEvent` (with modifier state) into the VT byte sequence
/// that should be sent to a terminal emulator via `ConPTY` stdin.
///
/// Returns `None` when the key event does not map to any terminal input
/// (e.g. modifier-only presses, key releases, or unrecognised keys).
pub fn key_event_to_bytes(event: &KeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>> {
    // Only handle key presses (including repeat)
    if event.state != ElementState::Pressed {
        return None;
    }

    logical_key_to_bytes(&event.logical_key, modifiers)
}

/// Convert a logical key plus modifiers into VT bytes.
///
/// This is the core mapping logic, separated from `KeyEvent` so it can be
/// tested without constructing platform-specific event structs.
pub fn logical_key_to_bytes(key: &Key, modifiers: ModifiersState) -> Option<Vec<u8>> {
    let ctrl = modifiers.control_key();
    let alt = modifiers.alt_key();
    let shift = modifiers.shift_key();

    // --- Named / special keys ---
    if let Key::Named(named) = key {
        return named_key_to_bytes(*named, ctrl, alt, shift);
    }

    // --- Character keys ---
    if let Key::Character(ch) = key {
        let s: &str = ch.as_ref();
        return char_key_to_bytes(s, ctrl, alt);
    }

    None
}

/// Map a named (non-character) key to VT bytes.
fn named_key_to_bytes(key: NamedKey, ctrl: bool, alt: bool, shift: bool) -> Option<Vec<u8>> {
    // Simple keys without modifier awareness
    let simple: Option<&[u8]> = match key {
        NamedKey::Enter => Some(b"\r"),
        NamedKey::Tab if !shift => Some(b"\t"),
        NamedKey::Tab if shift => Some(b"\x1b[Z"), // Back-tab
        NamedKey::Backspace => Some(b"\x7f"),
        NamedKey::Escape => Some(b"\x1b"),
        NamedKey::Insert if !has_modifier(ctrl, alt, shift) => Some(b"\x1b[2~"),
        NamedKey::Delete if !has_modifier(ctrl, alt, shift) => Some(b"\x1b[3~"),
        NamedKey::PageUp if !has_modifier(ctrl, alt, shift) => Some(b"\x1b[5~"),
        NamedKey::PageDown if !has_modifier(ctrl, alt, shift) => Some(b"\x1b[6~"),
        _ => None,
    };

    if let Some(bytes) = simple {
        return Some(bytes.to_vec());
    }

    // Arrow keys with modifier support:  CSI 1 ; <mod> <final>
    if let Some(arrow_ch) = arrow_final(key) {
        return Some(csi_with_modifiers(arrow_ch, ctrl, alt, shift));
    }

    // Home / End with modifier support
    if let Some(final_ch) = home_end_final(key) {
        return Some(csi_with_modifiers(final_ch, ctrl, alt, shift));
    }

    // Tilde-style keys with modifiers (Insert, Delete, PageUp, PageDown)
    if let Some(tilde_num) = tilde_number(key) {
        return Some(tilde_with_modifiers(tilde_num, ctrl, alt, shift));
    }

    // Function keys
    if let Some(bytes) = function_key_bytes(key) {
        return Some(bytes);
    }

    None
}

/// Map a character key to VT bytes, handling Ctrl and Alt modifiers.
fn char_key_to_bytes(s: &str, ctrl: bool, alt: bool) -> Option<Vec<u8>> {
    if s.is_empty() {
        return None;
    }

    // Ctrl+letter -> ASCII control code
    if ctrl {
        let ch = s.chars().next()?;
        if let Some(code) = ctrl_code(ch) {
            return if alt {
                Some(vec![0x1b, code])
            } else {
                Some(vec![code])
            };
        }
    }

    // Alt+key -> ESC prefix + character bytes
    if alt {
        let mut bytes = vec![0x1b];
        bytes.extend_from_slice(s.as_bytes());
        return Some(bytes);
    }

    // Plain text -- let the caller (app.rs) use event.text for normal typing.
    // We return None here so that the caller can fall through to event.text.
    None
}

/// Compute the ASCII control code for Ctrl+<char>.
/// Works for a-z, @, \[, \\, \], ^, _
fn ctrl_code(ch: char) -> Option<u8> {
    match ch {
        'a'..='z' => Some(ch as u8 - b'a' + 1),
        'A'..='Z' => Some(ch as u8 - b'A' + 1),
        '@' | '2' => Some(0),    // Ctrl+@ / Ctrl+2  -> NUL
        '[' => Some(0x1b),       // Ctrl+[ -> ESC
        '\\' => Some(0x1c),      // Ctrl+\ -> FS
        ']' => Some(0x1d),       // Ctrl+] -> GS
        '^' | '6' => Some(0x1e), // Ctrl+^ / Ctrl+6 -> RS
        '_' | '-' => Some(0x1f), // Ctrl+_ / Ctrl+- -> US
        _ => None,
    }
}

// -------------------------------------------------------------------------
// Arrow / Home / End helpers
// -------------------------------------------------------------------------

fn arrow_final(key: NamedKey) -> Option<u8> {
    match key {
        NamedKey::ArrowUp => Some(b'A'),
        NamedKey::ArrowDown => Some(b'B'),
        NamedKey::ArrowRight => Some(b'C'),
        NamedKey::ArrowLeft => Some(b'D'),
        _ => None,
    }
}

fn home_end_final(key: NamedKey) -> Option<u8> {
    match key {
        NamedKey::Home => Some(b'H'),
        NamedKey::End => Some(b'F'),
        _ => None,
    }
}

/// Encode `CSI <final>` or `CSI 1 ; <mod> <final>` depending on modifiers.
fn csi_with_modifiers(final_byte: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    let mod_param = modifier_param(ctrl, alt, shift);
    if mod_param == 0 {
        vec![0x1b, b'[', final_byte]
    } else {
        format!("\x1b[1;{}{}", mod_param, final_byte as char).into_bytes()
    }
}

// -------------------------------------------------------------------------
// Tilde-style keys (Insert, Delete, PageUp, PageDown)
// -------------------------------------------------------------------------

fn tilde_number(key: NamedKey) -> Option<u8> {
    match key {
        NamedKey::Insert => Some(b'2'),
        NamedKey::Delete => Some(b'3'),
        NamedKey::PageUp => Some(b'5'),
        NamedKey::PageDown => Some(b'6'),
        _ => None,
    }
}

fn tilde_with_modifiers(num: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    let mod_param = modifier_param(ctrl, alt, shift);
    if mod_param == 0 {
        vec![0x1b, b'[', num, b'~']
    } else {
        format!("\x1b[{};{}~", num as char, mod_param).into_bytes()
    }
}

// -------------------------------------------------------------------------
// Function keys
// -------------------------------------------------------------------------

fn function_key_bytes(key: NamedKey) -> Option<Vec<u8>> {
    let seq: &[u8] = match key {
        NamedKey::F1 => b"\x1bOP",
        NamedKey::F2 => b"\x1bOQ",
        NamedKey::F3 => b"\x1bOR",
        NamedKey::F4 => b"\x1bOS",
        NamedKey::F5 => b"\x1b[15~",
        NamedKey::F6 => b"\x1b[17~",
        NamedKey::F7 => b"\x1b[18~",
        NamedKey::F8 => b"\x1b[19~",
        NamedKey::F9 => b"\x1b[20~",
        NamedKey::F10 => b"\x1b[21~",
        NamedKey::F11 => b"\x1b[23~",
        NamedKey::F12 => b"\x1b[24~",
        _ => return None,
    };
    Some(seq.to_vec())
}

// -------------------------------------------------------------------------
// Modifier parameter (xterm-style)
// -------------------------------------------------------------------------

/// Returns the xterm-style modifier parameter (2-8), or 0 for no modifiers.
///
/// The encoding is: `1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (ctrl ? 4 : 0)`.
fn modifier_param(ctrl: bool, alt: bool, shift: bool) -> u8 {
    let mut m: u8 = 0;
    if shift {
        m += 1;
    }
    if alt {
        m += 2;
    }
    if ctrl {
        m += 4;
    }
    if m == 0 { 0 } else { m + 1 }
}

fn has_modifier(ctrl: bool, alt: bool, shift: bool) -> bool {
    ctrl || alt || shift
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    /// Helper: build a `Key::Named` and call `logical_key_to_bytes`.
    fn named(key: NamedKey, mods: ModifiersState) -> Option<Vec<u8>> {
        logical_key_to_bytes(&Key::Named(key), mods)
    }

    /// Helper: build a `Key::Character` and call `logical_key_to_bytes`.
    fn character(c: &str, mods: ModifiersState) -> Option<Vec<u8>> {
        logical_key_to_bytes(&Key::Character(SmolStr::new(c)), mods)
    }

    fn no_mods() -> ModifiersState {
        ModifiersState::empty()
    }

    fn ctrl() -> ModifiersState {
        ModifiersState::CONTROL
    }

    fn alt() -> ModifiersState {
        ModifiersState::ALT
    }

    fn shift() -> ModifiersState {
        ModifiersState::SHIFT
    }

    // --- Arrow keys ---

    #[test]
    fn arrow_up() {
        assert_eq!(
            named(NamedKey::ArrowUp, no_mods()),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn arrow_down() {
        assert_eq!(
            named(NamedKey::ArrowDown, no_mods()),
            Some(b"\x1b[B".to_vec())
        );
    }

    #[test]
    fn arrow_right() {
        assert_eq!(
            named(NamedKey::ArrowRight, no_mods()),
            Some(b"\x1b[C".to_vec())
        );
    }

    #[test]
    fn arrow_left() {
        assert_eq!(
            named(NamedKey::ArrowLeft, no_mods()),
            Some(b"\x1b[D".to_vec())
        );
    }

    // --- Ctrl + arrow ---

    #[test]
    fn ctrl_arrow_up() {
        assert_eq!(
            named(NamedKey::ArrowUp, ctrl()),
            Some(b"\x1b[1;5A".to_vec())
        );
    }

    #[test]
    fn shift_arrow_right() {
        assert_eq!(
            named(NamedKey::ArrowRight, shift()),
            Some(b"\x1b[1;2C".to_vec())
        );
    }

    #[test]
    fn alt_arrow_left() {
        assert_eq!(
            named(NamedKey::ArrowLeft, alt()),
            Some(b"\x1b[1;3D".to_vec())
        );
    }

    // --- Home / End ---

    #[test]
    fn home_key() {
        assert_eq!(named(NamedKey::Home, no_mods()), Some(b"\x1b[H".to_vec()));
    }

    #[test]
    fn end_key() {
        assert_eq!(named(NamedKey::End, no_mods()), Some(b"\x1b[F".to_vec()));
    }

    // --- Page Up / Down ---

    #[test]
    fn page_up() {
        assert_eq!(
            named(NamedKey::PageUp, no_mods()),
            Some(b"\x1b[5~".to_vec())
        );
    }

    #[test]
    fn page_down() {
        assert_eq!(
            named(NamedKey::PageDown, no_mods()),
            Some(b"\x1b[6~".to_vec())
        );
    }

    // --- Insert / Delete ---

    #[test]
    fn insert_key() {
        assert_eq!(
            named(NamedKey::Insert, no_mods()),
            Some(b"\x1b[2~".to_vec())
        );
    }

    #[test]
    fn delete_key() {
        assert_eq!(
            named(NamedKey::Delete, no_mods()),
            Some(b"\x1b[3~".to_vec())
        );
    }

    // --- Simple keys ---

    #[test]
    fn enter_key() {
        assert_eq!(named(NamedKey::Enter, no_mods()), Some(b"\r".to_vec()));
    }

    #[test]
    fn tab_key() {
        assert_eq!(named(NamedKey::Tab, no_mods()), Some(b"\t".to_vec()));
    }

    #[test]
    fn shift_tab() {
        assert_eq!(named(NamedKey::Tab, shift()), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn backspace_key() {
        assert_eq!(
            named(NamedKey::Backspace, no_mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn escape_key() {
        assert_eq!(named(NamedKey::Escape, no_mods()), Some(b"\x1b".to_vec()));
    }

    // --- Function keys ---

    #[test]
    fn f1_key() {
        assert_eq!(named(NamedKey::F1, no_mods()), Some(b"\x1bOP".to_vec()));
    }

    #[test]
    fn f5_key() {
        assert_eq!(named(NamedKey::F5, no_mods()), Some(b"\x1b[15~".to_vec()));
    }

    #[test]
    fn f12_key() {
        assert_eq!(named(NamedKey::F12, no_mods()), Some(b"\x1b[24~".to_vec()));
    }

    // --- Ctrl + character ---

    #[test]
    fn ctrl_c() {
        assert_eq!(character("c", ctrl()), Some(vec![0x03]));
    }

    #[test]
    fn ctrl_d() {
        assert_eq!(character("d", ctrl()), Some(vec![0x04]));
    }

    #[test]
    fn ctrl_a() {
        assert_eq!(character("a", ctrl()), Some(vec![0x01]));
    }

    #[test]
    fn ctrl_z() {
        assert_eq!(character("z", ctrl()), Some(vec![0x1a]));
    }

    // --- Alt + character ---

    #[test]
    fn alt_a() {
        assert_eq!(character("a", alt()), Some(vec![0x1b, b'a']));
    }

    // --- Plain text returns None (caller uses event.text) ---

    #[test]
    fn plain_char_returns_none() {
        assert_eq!(character("x", no_mods()), None);
    }

    // --- Modifier parameter encoding ---

    #[test]
    fn modifier_param_values() {
        assert_eq!(modifier_param(false, false, false), 0);
        assert_eq!(modifier_param(false, false, true), 2); // shift
        assert_eq!(modifier_param(false, true, false), 3); // alt
        assert_eq!(modifier_param(true, false, false), 5); // ctrl
        assert_eq!(modifier_param(true, true, true), 8); // all
    }

    // --- Ctrl + Alt ---

    #[test]
    fn ctrl_alt_a() {
        let mods = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(character("a", mods), Some(vec![0x1b, 0x01]));
    }

    // --- Ctrl + Home ---

    #[test]
    fn ctrl_home() {
        assert_eq!(named(NamedKey::Home, ctrl()), Some(b"\x1b[1;5H".to_vec()));
    }

    // --- Shift + Delete (tilde with modifier) ---

    #[test]
    fn shift_delete() {
        assert_eq!(
            named(NamedKey::Delete, shift()),
            Some(b"\x1b[3;2~".to_vec())
        );
    }
}
