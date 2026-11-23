//! Key positions of the 65% and 108-key keyboards drawn in front of the cat, and the mapping
//! from Windows virtual-key codes to those keys.

/// Virtual-key code used for the numpad Enter key (Windows reports it as Enter + extended flag).
pub const VK_NUMPAD_ENTER: u32 = 0x10d;

/// Colour group of a key cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAccent {
    /// A normal letter or number key.
    Normal,
    /// A modifier or function key (slightly darker cap).
    Modifier,
    /// The orange Esc key.
    Orange,
}

/// One key on the drawn keyboard. Positions are in key units (1U = one letter key).
#[derive(Clone, Debug)]
pub struct KeySpec {
    /// Unique key id, e.g. "esc" or "np_dot".
    pub id: &'static str,
    /// Text printed on the key cap.
    pub label: &'static str,
    /// Text shown in the bubble when the key is pressed.
    pub bubble: &'static str,
    /// Exact vertical position in key units (usually equal to `row`).
    pub row_y: f32,
    /// Height in key units (1, or 2 for numpad + and Enter).
    pub row_span: f32,
    /// Left edge in key units.
    pub u_start: f32,
    /// Width in key units.
    pub u_width: f32,
    /// Cap colour group.
    pub accent: KeyAccent,
    /// Virtual-key codes that light up this key.
    pub vk_codes: &'static [u32],
}

/// Which keyboard is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardLayoutId {
    /// Compact 65% keyboard.
    Compact65,
    /// Full-size 108-key keyboard.
    Full108,
}

impl KeyboardLayoutId {
    /// Reads the layout from its settings value.
    ///
    /// @param value "65" or "108".
    /// @return The layout ("65" for anything unknown).
    pub fn from_setting(value: &str) -> KeyboardLayoutId {
        if value == "108" {
            KeyboardLayoutId::Full108
        } else {
            KeyboardLayoutId::Compact65
        }
    }
}

/// A complete keyboard: its size in key units and its keys.
pub struct KeyboardLayoutDef {
    /// Total width in key units.
    pub total_units: f32,
    /// Total height in key rows.
    pub total_rows: f32,
    /// All keys.
    pub keys: Vec<KeySpec>,
}

use std::collections::HashMap;
use std::sync::OnceLock;

/// Builds one key description. Used only to keep the tables below short and aligned.
///
/// @param id Unique key id.
/// @param label Text on the key cap.
/// @param bubble Text in the pop-up bubble.
/// @param _row Row index (kept so the tables read row by row; not stored).
/// @param row_y Exact vertical position in key units.
/// @param row_span Height in key units.
/// @param u_start Left edge in key units.
/// @param u_width Width in key units.
/// @param accent Cap colour group.
/// @param vk_codes Virtual-key codes for this key.
/// @return The key description.
#[allow(clippy::too_many_arguments)]
fn key(
    id: &'static str,
    label: &'static str,
    bubble: &'static str,
    _row: f32,
    row_y: f32,
    row_span: f32,
    u_start: f32,
    u_width: f32,
    accent: KeyAccent,
    vk_codes: &'static [u32],
) -> KeySpec {
    KeySpec {
        id,
        label,
        bubble,
        row_y,
        row_span,
        u_start,
        u_width,
        accent,
        vk_codes,
    }
}

/// Builds the 62 keys of the compact 65% keyboard (15 units × 5 rows).
///
/// On this layout the number row also lights up for numpad digits and F1..F12, Esc also
/// covers the backtick, and Backspace also covers Delete.
///
/// @return The keys, row by row.
fn build_keys_65() -> Vec<KeySpec> {
    vec![
        key("esc", "ESC", "Esc", 0.0, 0.0, 1.0, 0.0, 1.0, KeyAccent::Orange, &[0x1b, 0xc0]),
        key("1", "1", "1", 0.0, 0.0, 1.0, 1.0, 1.0, KeyAccent::Normal, &[0x31, 0x61, 0x70]),
        key("2", "2", "2", 0.0, 0.0, 1.0, 2.0, 1.0, KeyAccent::Normal, &[0x32, 0x62, 0x71]),
        key("3", "3", "3", 0.0, 0.0, 1.0, 3.0, 1.0, KeyAccent::Normal, &[0x33, 0x63, 0x72]),
        key("4", "4", "4", 0.0, 0.0, 1.0, 4.0, 1.0, KeyAccent::Normal, &[0x34, 0x64, 0x73]),
        key("5", "5", "5", 0.0, 0.0, 1.0, 5.0, 1.0, KeyAccent::Normal, &[0x35, 0x65, 0x74]),
        key("6", "6", "6", 0.0, 0.0, 1.0, 6.0, 1.0, KeyAccent::Normal, &[0x36, 0x66, 0x75]),
        key("7", "7", "7", 0.0, 0.0, 1.0, 7.0, 1.0, KeyAccent::Normal, &[0x37, 0x67, 0x76]),
        key("8", "8", "8", 0.0, 0.0, 1.0, 8.0, 1.0, KeyAccent::Normal, &[0x38, 0x68, 0x77]),
        key("9", "9", "9", 0.0, 0.0, 1.0, 9.0, 1.0, KeyAccent::Normal, &[0x39, 0x69, 0x78]),
        key("0", "0", "0", 0.0, 0.0, 1.0, 10.0, 1.0, KeyAccent::Normal, &[0x30, 0x60, 0x79]),
        key("minus", "-", "-", 0.0, 0.0, 1.0, 11.0, 1.0, KeyAccent::Normal, &[0xbd, 0x6d, 0x7a]),
        key("equal", "=", "=", 0.0, 0.0, 1.0, 12.0, 1.0, KeyAccent::Normal, &[0xbb, 0x6b, 0x7b]),
        key("bs", "⌫", "⌫", 0.0, 0.0, 1.0, 13.0, 2.0, KeyAccent::Modifier, &[0x08, 0x2e]),
        key("tab", "↹", "Tab", 1.0, 1.0, 1.0, 0.0, 1.5, KeyAccent::Modifier, &[0x09]),
        key("q", "Q", "Q", 1.0, 1.0, 1.0, 1.5, 1.0, KeyAccent::Normal, &[0x51]),
        key("w", "W", "W", 1.0, 1.0, 1.0, 2.5, 1.0, KeyAccent::Normal, &[0x57]),
        key("e", "E", "E", 1.0, 1.0, 1.0, 3.5, 1.0, KeyAccent::Normal, &[0x45]),
        key("r", "R", "R", 1.0, 1.0, 1.0, 4.5, 1.0, KeyAccent::Normal, &[0x52]),
        key("t", "T", "T", 1.0, 1.0, 1.0, 5.5, 1.0, KeyAccent::Normal, &[0x54]),
        key("y", "Y", "Y", 1.0, 1.0, 1.0, 6.5, 1.0, KeyAccent::Normal, &[0x59]),
        key("u", "U", "U", 1.0, 1.0, 1.0, 7.5, 1.0, KeyAccent::Normal, &[0x55]),
        key("i", "I", "I", 1.0, 1.0, 1.0, 8.5, 1.0, KeyAccent::Normal, &[0x49]),
        key("o", "O", "O", 1.0, 1.0, 1.0, 9.5, 1.0, KeyAccent::Normal, &[0x4f]),
        key("p", "P", "P", 1.0, 1.0, 1.0, 10.5, 1.0, KeyAccent::Normal, &[0x50]),
        key("lb", "[", "[", 1.0, 1.0, 1.0, 11.5, 1.0, KeyAccent::Normal, &[0xdb]),
        key("rb", "]", "]", 1.0, 1.0, 1.0, 12.5, 1.0, KeyAccent::Normal, &[0xdd]),
        key("bslash", "\\", "\\", 1.0, 1.0, 1.0, 13.5, 1.5, KeyAccent::Modifier, &[0xdc, 0x2d, 0x24]),
        key("caps", "⇪", "Caps", 2.0, 2.0, 1.0, 0.0, 1.75, KeyAccent::Modifier, &[0x14]),
        key("a", "A", "A", 2.0, 2.0, 1.0, 1.75, 1.0, KeyAccent::Normal, &[0x41]),
        key("s", "S", "S", 2.0, 2.0, 1.0, 2.75, 1.0, KeyAccent::Normal, &[0x53]),
        key("d", "D", "D", 2.0, 2.0, 1.0, 3.75, 1.0, KeyAccent::Normal, &[0x44]),
        key("f", "F", "F", 2.0, 2.0, 1.0, 4.75, 1.0, KeyAccent::Normal, &[0x46]),
        key("g", "G", "G", 2.0, 2.0, 1.0, 5.75, 1.0, KeyAccent::Normal, &[0x47]),
        key("h", "H", "H", 2.0, 2.0, 1.0, 6.75, 1.0, KeyAccent::Normal, &[0x48]),
        key("j", "J", "J", 2.0, 2.0, 1.0, 7.75, 1.0, KeyAccent::Normal, &[0x4a]),
        key("k", "K", "K", 2.0, 2.0, 1.0, 8.75, 1.0, KeyAccent::Normal, &[0x4b]),
        key("l", "L", "L", 2.0, 2.0, 1.0, 9.75, 1.0, KeyAccent::Normal, &[0x4c]),
        key("semi", ";", ";", 2.0, 2.0, 1.0, 10.75, 1.0, KeyAccent::Normal, &[0xba]),
        key("quote", "'", "'", 2.0, 2.0, 1.0, 11.75, 1.0, KeyAccent::Normal, &[0xde]),
        key("enter", "↵", "↵", 2.0, 2.0, 1.0, 12.75, 2.25, KeyAccent::Modifier, &[0x0d, VK_NUMPAD_ENTER]),
        key("lshift", "⇧", "⇧", 3.0, 3.0, 1.0, 0.0, 2.25, KeyAccent::Modifier, &[0xa0, 0x10]),
        key("z", "Z", "Z", 3.0, 3.0, 1.0, 2.25, 1.0, KeyAccent::Normal, &[0x5a]),
        key("x", "X", "X", 3.0, 3.0, 1.0, 3.25, 1.0, KeyAccent::Normal, &[0x58]),
        key("c", "C", "C", 3.0, 3.0, 1.0, 4.25, 1.0, KeyAccent::Normal, &[0x43]),
        key("v", "V", "V", 3.0, 3.0, 1.0, 5.25, 1.0, KeyAccent::Normal, &[0x56]),
        key("b", "B", "B", 3.0, 3.0, 1.0, 6.25, 1.0, KeyAccent::Normal, &[0x42]),
        key("n", "N", "N", 3.0, 3.0, 1.0, 7.25, 1.0, KeyAccent::Normal, &[0x4e]),
        key("m", "M", "M", 3.0, 3.0, 1.0, 8.25, 1.0, KeyAccent::Normal, &[0x4d]),
        key("comma", ",", ",", 3.0, 3.0, 1.0, 9.25, 1.0, KeyAccent::Normal, &[0xbc]),
        key("dot", ".", ".", 3.0, 3.0, 1.0, 10.25, 1.0, KeyAccent::Normal, &[0xbe, 0x6e]),
        key("slash", "/", "/", 3.0, 3.0, 1.0, 11.25, 1.0, KeyAccent::Normal, &[0xbf, 0x6f]),
        key("rshift", "⇧", "⇧", 3.0, 3.0, 1.0, 12.25, 2.75, KeyAccent::Orange, &[0xa1]),
        key("lctrl", "Ctrl", "Ctrl", 4.0, 4.0, 1.0, 0.0, 1.25, KeyAccent::Modifier, &[0xa2, 0x11]),
        key("lwin", "Win", "Win", 4.0, 4.0, 1.0, 1.25, 1.25, KeyAccent::Modifier, &[0x5b, 0x5c]),
        key("lalt", "Alt", "Alt", 4.0, 4.0, 1.0, 2.5, 1.25, KeyAccent::Modifier, &[0xa4, 0x12]),
        key("space", "␣", "␣", 4.0, 4.0, 1.0, 3.75, 6.25, KeyAccent::Normal, &[0x20]),
        key("ralt", "Alt", "Alt", 4.0, 4.0, 1.0, 10.0, 1.0, KeyAccent::Modifier, &[0xa5, 0xa3]),
        key("left", "←", "←", 4.0, 4.0, 1.0, 11.0, 1.0, KeyAccent::Modifier, &[0x25, 0x23]),
        key("down", "↓", "↓", 4.0, 4.0, 1.0, 12.0, 1.0, KeyAccent::Modifier, &[0x28, 0x22]),
        key("up", "↑", "↑", 4.0, 4.0, 1.0, 13.0, 1.0, KeyAccent::Modifier, &[0x26, 0x21]),
        key("right", "→", "→", 4.0, 4.0, 1.0, 14.0, 1.0, KeyAccent::Modifier, &[0x27]),
    ]
}

/// Builds the 108 keys of the full-size keyboard (22.7 units × 6.25 rows), with the
/// navigation cluster and the numpad.
///
/// @return The keys, row by row.
fn build_keys_108() -> Vec<KeySpec> {
    vec![
        key("esc", "Esc", "Esc", 0.0, 0.0, 1.0, 0.0, 1.0, KeyAccent::Orange, &[0x1b]),
        key("f1", "F1", "F1", 0.0, 0.0, 1.0, 2.0, 1.0, KeyAccent::Normal, &[0x70]),
        key("f2", "F2", "F2", 0.0, 0.0, 1.0, 3.0, 1.0, KeyAccent::Normal, &[0x71]),
        key("f3", "F3", "F3", 0.0, 0.0, 1.0, 4.0, 1.0, KeyAccent::Normal, &[0x72]),
        key("f4", "F4", "F4", 0.0, 0.0, 1.0, 5.0, 1.0, KeyAccent::Normal, &[0x73]),
        key("f5", "F5", "F5", 0.0, 0.0, 1.0, 6.5, 1.0, KeyAccent::Modifier, &[0x74]),
        key("f6", "F6", "F6", 0.0, 0.0, 1.0, 7.5, 1.0, KeyAccent::Modifier, &[0x75]),
        key("f7", "F7", "F7", 0.0, 0.0, 1.0, 8.5, 1.0, KeyAccent::Modifier, &[0x76]),
        key("f8", "F8", "F8", 0.0, 0.0, 1.0, 9.5, 1.0, KeyAccent::Modifier, &[0x77]),
        key("f9", "F9", "F9", 0.0, 0.0, 1.0, 11.0, 1.0, KeyAccent::Normal, &[0x78]),
        key("f10", "F10", "F10", 0.0, 0.0, 1.0, 12.0, 1.0, KeyAccent::Normal, &[0x79]),
        key("f11", "F11", "F11", 0.0, 0.0, 1.0, 13.0, 1.0, KeyAccent::Normal, &[0x7a]),
        key("f12", "F12", "F12", 0.0, 0.0, 1.0, 14.0, 1.0, KeyAccent::Normal, &[0x7b]),
        key("prt", "Prt", "PrtSc", 0.0, 0.0, 1.0, 15.35, 1.0, KeyAccent::Modifier, &[0x2c]),
        key("scr", "Scr", "ScrLk", 0.0, 0.0, 1.0, 16.35, 1.0, KeyAccent::Modifier, &[0x91]),
        key("pau", "Pau", "Pause", 0.0, 0.0, 1.0, 17.35, 1.0, KeyAccent::Modifier, &[0x13]),
        key("m_mute", "M1", "Mute", 0.0, 0.0, 1.0, 18.7, 1.0, KeyAccent::Modifier, &[0xad]),
        key("m_vdown", "M2", "Vol-", 0.0, 0.0, 1.0, 19.7, 1.0, KeyAccent::Modifier, &[0xae]),
        key("m_vup", "M3", "Vol+", 0.0, 0.0, 1.0, 20.7, 1.0, KeyAccent::Modifier, &[0xaf]),
        key("m_calc", "M4", "Calc", 0.0, 0.0, 1.0, 21.7, 1.0, KeyAccent::Modifier, &[0xb7, 0xb3]),
        key("grave", "`", "`", 1.0, 1.25, 1.0, 0.0, 1.0, KeyAccent::Normal, &[0xc0]),
        key("1", "1", "1", 1.0, 1.25, 1.0, 1.0, 1.0, KeyAccent::Normal, &[0x31]),
        key("2", "2", "2", 1.0, 1.25, 1.0, 2.0, 1.0, KeyAccent::Normal, &[0x32]),
        key("3", "3", "3", 1.0, 1.25, 1.0, 3.0, 1.0, KeyAccent::Normal, &[0x33]),
        key("4", "4", "4", 1.0, 1.25, 1.0, 4.0, 1.0, KeyAccent::Normal, &[0x34]),
        key("5", "5", "5", 1.0, 1.25, 1.0, 5.0, 1.0, KeyAccent::Normal, &[0x35]),
        key("6", "6", "6", 1.0, 1.25, 1.0, 6.0, 1.0, KeyAccent::Normal, &[0x36]),
        key("7", "7", "7", 1.0, 1.25, 1.0, 7.0, 1.0, KeyAccent::Normal, &[0x37]),
        key("8", "8", "8", 1.0, 1.25, 1.0, 8.0, 1.0, KeyAccent::Normal, &[0x38]),
        key("9", "9", "9", 1.0, 1.25, 1.0, 9.0, 1.0, KeyAccent::Normal, &[0x39]),
        key("0", "0", "0", 1.0, 1.25, 1.0, 10.0, 1.0, KeyAccent::Normal, &[0x30]),
        key("minus", "-", "-", 1.0, 1.25, 1.0, 11.0, 1.0, KeyAccent::Normal, &[0xbd]),
        key("equal", "=", "=", 1.0, 1.25, 1.0, 12.0, 1.0, KeyAccent::Normal, &[0xbb]),
        key("bs", "⌫", "⌫", 1.0, 1.25, 1.0, 13.0, 2.0, KeyAccent::Modifier, &[0x08]),
        key("ins", "Ins", "Ins", 1.0, 1.25, 1.0, 15.35, 1.0, KeyAccent::Modifier, &[0x2d]),
        key("home", "Hm", "Home", 1.0, 1.25, 1.0, 16.35, 1.0, KeyAccent::Modifier, &[0x24]),
        key("pgup", "PU", "PgUp", 1.0, 1.25, 1.0, 17.35, 1.0, KeyAccent::Modifier, &[0x21]),
        key("numlk", "Num", "Num", 1.0, 1.25, 1.0, 18.7, 1.0, KeyAccent::Modifier, &[0x90]),
        key("np_div", "/", "Num/", 1.0, 1.25, 1.0, 19.7, 1.0, KeyAccent::Modifier, &[0x6f]),
        key("np_mul", "*", "Num*", 1.0, 1.25, 1.0, 20.7, 1.0, KeyAccent::Modifier, &[0x6a]),
        key("np_sub", "-", "Num-", 1.0, 1.25, 1.0, 21.7, 1.0, KeyAccent::Modifier, &[0x6d]),
        key("tab", "↹", "Tab", 2.0, 2.25, 1.0, 0.0, 1.5, KeyAccent::Modifier, &[0x09]),
        key("q", "Q", "Q", 2.0, 2.25, 1.0, 1.5, 1.0, KeyAccent::Normal, &[0x51]),
        key("w", "W", "W", 2.0, 2.25, 1.0, 2.5, 1.0, KeyAccent::Normal, &[0x57]),
        key("e", "E", "E", 2.0, 2.25, 1.0, 3.5, 1.0, KeyAccent::Normal, &[0x45]),
        key("r", "R", "R", 2.0, 2.25, 1.0, 4.5, 1.0, KeyAccent::Normal, &[0x52]),
        key("t", "T", "T", 2.0, 2.25, 1.0, 5.5, 1.0, KeyAccent::Normal, &[0x54]),
        key("y", "Y", "Y", 2.0, 2.25, 1.0, 6.5, 1.0, KeyAccent::Normal, &[0x59]),
        key("u", "U", "U", 2.0, 2.25, 1.0, 7.5, 1.0, KeyAccent::Normal, &[0x55]),
        key("i", "I", "I", 2.0, 2.25, 1.0, 8.5, 1.0, KeyAccent::Normal, &[0x49]),
        key("o", "O", "O", 2.0, 2.25, 1.0, 9.5, 1.0, KeyAccent::Normal, &[0x4f]),
        key("p", "P", "P", 2.0, 2.25, 1.0, 10.5, 1.0, KeyAccent::Normal, &[0x50]),
        key("lb", "[", "[", 2.0, 2.25, 1.0, 11.5, 1.0, KeyAccent::Normal, &[0xdb]),
        key("rb", "]", "]", 2.0, 2.25, 1.0, 12.5, 1.0, KeyAccent::Normal, &[0xdd]),
        key("bslash", "\\", "\\", 2.0, 2.25, 1.0, 13.5, 1.5, KeyAccent::Modifier, &[0xdc]),
        key("del", "Del", "Del", 2.0, 2.25, 1.0, 15.35, 1.0, KeyAccent::Modifier, &[0x2e]),
        key("end", "End", "End", 2.0, 2.25, 1.0, 16.35, 1.0, KeyAccent::Modifier, &[0x23]),
        key("pgdn", "PD", "PgDn", 2.0, 2.25, 1.0, 17.35, 1.0, KeyAccent::Modifier, &[0x22]),
        key("np_7", "7", "Num7", 2.0, 2.25, 1.0, 18.7, 1.0, KeyAccent::Normal, &[0x67]),
        key("np_8", "8", "Num8", 2.0, 2.25, 1.0, 19.7, 1.0, KeyAccent::Normal, &[0x68]),
        key("np_9", "9", "Num9", 2.0, 2.25, 1.0, 20.7, 1.0, KeyAccent::Normal, &[0x69]),
        key("np_add", "+", "Num+", 2.0, 2.25, 2.0, 21.7, 1.0, KeyAccent::Modifier, &[0x6b]),
        key("caps", "⇪", "Caps", 3.0, 3.25, 1.0, 0.0, 1.75, KeyAccent::Modifier, &[0x14]),
        key("a", "A", "A", 3.0, 3.25, 1.0, 1.75, 1.0, KeyAccent::Normal, &[0x41]),
        key("s", "S", "S", 3.0, 3.25, 1.0, 2.75, 1.0, KeyAccent::Normal, &[0x53]),
        key("d", "D", "D", 3.0, 3.25, 1.0, 3.75, 1.0, KeyAccent::Normal, &[0x44]),
        key("f", "F", "F", 3.0, 3.25, 1.0, 4.75, 1.0, KeyAccent::Normal, &[0x46]),
        key("g", "G", "G", 3.0, 3.25, 1.0, 5.75, 1.0, KeyAccent::Normal, &[0x47]),
        key("h", "H", "H", 3.0, 3.25, 1.0, 6.75, 1.0, KeyAccent::Normal, &[0x48]),
        key("j", "J", "J", 3.0, 3.25, 1.0, 7.75, 1.0, KeyAccent::Normal, &[0x4a]),
        key("k", "K", "K", 3.0, 3.25, 1.0, 8.75, 1.0, KeyAccent::Normal, &[0x4b]),
        key("l", "L", "L", 3.0, 3.25, 1.0, 9.75, 1.0, KeyAccent::Normal, &[0x4c]),
        key("semi", ";", ";", 3.0, 3.25, 1.0, 10.75, 1.0, KeyAccent::Normal, &[0xba]),
        key("quote", "'", "'", 3.0, 3.25, 1.0, 11.75, 1.0, KeyAccent::Normal, &[0xde]),
        key("enter", "↵", "↵", 3.0, 3.25, 1.0, 12.75, 2.25, KeyAccent::Modifier, &[0x0d]),
        key("np_4", "4", "Num4", 3.0, 3.25, 1.0, 18.7, 1.0, KeyAccent::Normal, &[0x64]),
        key("np_5", "5", "Num5", 3.0, 3.25, 1.0, 19.7, 1.0, KeyAccent::Normal, &[0x65]),
        key("np_6", "6", "Num6", 3.0, 3.25, 1.0, 20.7, 1.0, KeyAccent::Normal, &[0x66]),
        key("lshift", "⇧", "⇧", 4.0, 4.25, 1.0, 0.0, 2.25, KeyAccent::Modifier, &[0xa0, 0x10]),
        key("z", "Z", "Z", 4.0, 4.25, 1.0, 2.25, 1.0, KeyAccent::Normal, &[0x5a]),
        key("x", "X", "X", 4.0, 4.25, 1.0, 3.25, 1.0, KeyAccent::Normal, &[0x58]),
        key("c", "C", "C", 4.0, 4.25, 1.0, 4.25, 1.0, KeyAccent::Normal, &[0x43]),
        key("v", "V", "V", 4.0, 4.25, 1.0, 5.25, 1.0, KeyAccent::Normal, &[0x56]),
        key("b", "B", "B", 4.0, 4.25, 1.0, 6.25, 1.0, KeyAccent::Normal, &[0x42]),
        key("n", "N", "N", 4.0, 4.25, 1.0, 7.25, 1.0, KeyAccent::Normal, &[0x4e]),
        key("m", "M", "M", 4.0, 4.25, 1.0, 8.25, 1.0, KeyAccent::Normal, &[0x4d]),
        key("comma", ",", ",", 4.0, 4.25, 1.0, 9.25, 1.0, KeyAccent::Normal, &[0xbc]),
        key("dot", ".", ".", 4.0, 4.25, 1.0, 10.25, 1.0, KeyAccent::Normal, &[0xbe]),
        key("slash", "/", "/", 4.0, 4.25, 1.0, 11.25, 1.0, KeyAccent::Normal, &[0xbf]),
        key("rshift", "⇧", "⇧", 4.0, 4.25, 1.0, 12.25, 2.75, KeyAccent::Orange, &[0xa1]),
        key("up", "↑", "↑", 4.0, 4.25, 1.0, 16.35, 1.0, KeyAccent::Modifier, &[0x26]),
        key("np_1", "1", "Num1", 4.0, 4.25, 1.0, 18.7, 1.0, KeyAccent::Normal, &[0x61]),
        key("np_2", "2", "Num2", 4.0, 4.25, 1.0, 19.7, 1.0, KeyAccent::Normal, &[0x62]),
        key("np_3", "3", "Num3", 4.0, 4.25, 1.0, 20.7, 1.0, KeyAccent::Normal, &[0x63]),
        key("np_ent", "↵", "Num↵", 4.0, 4.25, 2.0, 21.7, 1.0, KeyAccent::Orange, &[VK_NUMPAD_ENTER]),
        key("lctrl", "Ctrl", "Ctrl", 5.0, 5.25, 1.0, 0.0, 1.25, KeyAccent::Modifier, &[0xa2, 0x11]),
        key("lwin", "Win", "Win", 5.0, 5.25, 1.0, 1.25, 1.25, KeyAccent::Modifier, &[0x5b]),
        key("lalt", "Alt", "Alt", 5.0, 5.25, 1.0, 2.5, 1.25, KeyAccent::Modifier, &[0xa4, 0x12]),
        key("space", "␣", "␣", 5.0, 5.25, 1.0, 3.75, 6.25, KeyAccent::Normal, &[0x20]),
        key("ralt", "Alt", "Alt", 5.0, 5.25, 1.0, 10.0, 1.25, KeyAccent::Modifier, &[0xa5]),
        key("rwin", "Win", "Win", 5.0, 5.25, 1.0, 11.25, 1.25, KeyAccent::Modifier, &[0x5c]),
        key("menu", "▤", "Menu", 5.0, 5.25, 1.0, 12.5, 1.25, KeyAccent::Modifier, &[0x5d]),
        key("rctrl", "Ctrl", "Ctrl", 5.0, 5.25, 1.0, 13.75, 1.25, KeyAccent::Modifier, &[0xa3]),
        key("left", "←", "←", 5.0, 5.25, 1.0, 15.35, 1.0, KeyAccent::Modifier, &[0x25]),
        key("down", "↓", "↓", 5.0, 5.25, 1.0, 16.35, 1.0, KeyAccent::Modifier, &[0x28]),
        key("right", "→", "→", 5.0, 5.25, 1.0, 17.35, 1.0, KeyAccent::Modifier, &[0x27]),
        key("np_0", "0", "Num0", 5.0, 5.25, 1.0, 18.7, 2.0, KeyAccent::Normal, &[0x60]),
        key("np_dot", ".", "Num.", 5.0, 5.25, 1.0, 20.7, 1.0, KeyAccent::Normal, &[0x6e]),
    ]
}

/// Both layouts plus their virtual-key lookup tables, built once on first use.
struct Layouts {
    compact: KeyboardLayoutDef,
    full: KeyboardLayoutDef,
    compact_by_vk: HashMap<u32, usize>,
    full_by_vk: HashMap<u32, usize>,
}

static LAYOUTS: OnceLock<Layouts> = OnceLock::new();

/// Maps every virtual-key code of a layout to the index of its key. Later keys win, as in the
/// original lookup.
///
/// @param keys The layout's keys.
/// @return Virtual-key code to key index.
fn build_vk_lookup(keys: &[KeySpec]) -> HashMap<u32, usize> {
    let mut lookup = HashMap::new();
    for (index, spec) in keys.iter().enumerate() {
        for vk in spec.vk_codes {
            lookup.insert(*vk, index);
        }
    }
    lookup
}

/// Returns both layouts, building them on first use.
///
/// @return The layouts.
fn layouts() -> &'static Layouts {
    LAYOUTS.get_or_init(|| {
        let compact_keys = build_keys_65();
        let full_keys = build_keys_108();
        let compact_by_vk = build_vk_lookup(&compact_keys);
        let full_by_vk = build_vk_lookup(&full_keys);
        Layouts {
            compact: KeyboardLayoutDef {
                total_units: 15.0,
                total_rows: 5.0,
                keys: compact_keys,
            },
            full: KeyboardLayoutDef {
                total_units: 22.7,
                total_rows: 6.25,
                keys: full_keys,
            },
            compact_by_vk,
            full_by_vk,
        }
    })
}

/// Returns the keys of the 65% keyboard.
///
/// @return The keys.
pub fn keys_65() -> &'static [KeySpec] {
    &layouts().compact.keys
}

/// Returns the keys of the 108-key keyboard.
///
/// @return The keys.
pub fn keys_108() -> &'static [KeySpec] {
    &layouts().full.keys
}

/// Returns a complete keyboard layout.
///
/// @param id Which layout.
/// @return The layout.
pub fn layout(id: KeyboardLayoutId) -> &'static KeyboardLayoutDef {
    match id {
        KeyboardLayoutId::Compact65 => &layouts().compact,
        KeyboardLayoutId::Full108 => &layouts().full,
    }
}

/// Finds the drawn key that a virtual-key code lights up.
///
/// On the 65% keyboard, F1..F24 fall back to the number row (F1 → "1", F12 and above → "=").
///
/// @param vk The virtual-key code.
/// @param id Which layout.
/// @return The key, or None when the code has no key on this layout.
pub fn resolve_key_by_vk(vk: u32, id: KeyboardLayoutId) -> Option<&'static KeySpec> {
    let all = layouts();
    let (definition, lookup) = match id {
        KeyboardLayoutId::Compact65 => (&all.compact, &all.compact_by_vk),
        KeyboardLayoutId::Full108 => (&all.full, &all.full_by_vk),
    };
    if let Some(index) = lookup.get(&vk) {
        return definition.keys.get(*index);
    }
    if id == KeyboardLayoutId::Compact65 && (0x70..=0x87).contains(&vk) {
        let index = (vk as i64 - 0x6f).clamp(1, 12) as usize;
        return definition.keys.get(index).or_else(|| definition.keys.first());
    }
    None
}

/// Returns the text to show in the bubble for a pressed key.
///
/// @param vk The virtual-key code.
/// @param spec The key it resolved to, if any.
/// @return The bubble text (empty when there is nothing sensible to show).
pub fn format_bubble_for_vk(vk: u32, spec: Option<&KeySpec>) -> String {
    if let Some(found) = spec {
        if !found.bubble.is_empty() {
            return found.bubble.to_string();
        }
    }
    if (0x70..=0x7b).contains(&vk) {
        return format!("F{}", vk - 0x6f);
    }
    let text = match vk {
        VK_NUMPAD_ENTER => "Num↵",
        0xc0 => "`",
        0x2e => "Del",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "PgUp",
        0x22 => "PgDn",
        _ => "",
    };
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_have_expected_key_counts() {
        assert_eq!(keys_65().len(), 62);
        assert_eq!(keys_108().len(), 108);
    }

    #[test]
    fn escape_resolves_on_both_layouts() {
        assert_eq!(resolve_key_by_vk(0x1b, KeyboardLayoutId::Compact65).unwrap().id, "esc");
        assert_eq!(resolve_key_by_vk(0x1b, KeyboardLayoutId::Full108).unwrap().id, "esc");
    }

    #[test]
    fn numpad_enter_has_its_own_key_on_108() {
        let spec = resolve_key_by_vk(VK_NUMPAD_ENTER, KeyboardLayoutId::Full108).unwrap();
        assert_eq!(spec.id, "np_ent");
        assert_eq!(spec.row_span, 2.0);
        let compact = resolve_key_by_vk(VK_NUMPAD_ENTER, KeyboardLayoutId::Compact65).unwrap();
        assert_eq!(compact.id, "enter");
    }

    #[test]
    fn function_keys_map_to_number_row_on_65() {
        assert_eq!(resolve_key_by_vk(0x74, KeyboardLayoutId::Compact65).unwrap().id, "5");
        assert_eq!(resolve_key_by_vk(0x86, KeyboardLayoutId::Compact65).unwrap().id, "equal");
        assert_eq!(resolve_key_by_vk(0x74, KeyboardLayoutId::Full108).unwrap().id, "f5");
    }

    #[test]
    fn bubble_text_falls_back_for_unknown_keys() {
        assert_eq!(format_bubble_for_vk(0x7a, None), "F11");
        assert_eq!(format_bubble_for_vk(0x2e, None), "Del");
        assert_eq!(format_bubble_for_vk(0x99, None), "");
    }
}
