//! Pure GPUI keystroke → Carbon hot-key chord mapping (ANSI virtual key codes).
use gpui::Keystroke;

const CMD: u32 = 256;
const SHIFT: u32 = 512;
const OPTION: u32 = 2048;
const CONTROL: u32 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Chord {
    pub code: u32,
    pub modifiers: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChordError {
    NoModifier,
    Unsupported,
}

impl Chord {
    pub fn parse(source: &str) -> Result<Self, ChordError> {
        let keystroke = Keystroke::parse(source).map_err(|_| ChordError::Unsupported)?;
        let m = keystroke.modifiers;
        if m.function {
            return Err(ChordError::Unsupported);
        }
        let modifiers = [
            (m.platform, CMD),
            (m.shift, SHIFT),
            (m.alt, OPTION),
            (m.control, CONTROL),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |acc, (_, bit)| acc | bit);
        let code = key_code(&keystroke.key).ok_or(ChordError::Unsupported)?;
        if modifiers == 0 {
            return Err(ChordError::NoModifier);
        }
        Ok(Self { code, modifiers })
    }
}

fn key_code(key: &str) -> Option<u32> {
    const KEYS: &[(&str, u32)] = &[
        ("a", 0),
        ("s", 1),
        ("d", 2),
        ("f", 3),
        ("h", 4),
        ("g", 5),
        ("z", 6),
        ("x", 7),
        ("c", 8),
        ("v", 9),
        ("b", 11),
        ("q", 12),
        ("w", 13),
        ("e", 14),
        ("r", 15),
        ("y", 16),
        ("t", 17),
        ("1", 18),
        ("2", 19),
        ("3", 20),
        ("4", 21),
        ("6", 22),
        ("5", 23),
        ("=", 24),
        ("9", 25),
        ("7", 26),
        ("-", 27),
        ("8", 28),
        ("0", 29),
        ("]", 30),
        ("o", 31),
        ("u", 32),
        ("[", 33),
        ("i", 34),
        ("p", 35),
        ("enter", 36),
        ("l", 37),
        ("j", 38),
        ("'", 39),
        ("k", 40),
        (";", 41),
        ("\\", 42),
        (",", 43),
        ("/", 44),
        ("n", 45),
        ("m", 46),
        (".", 47),
        ("tab", 48),
        ("space", 49),
        ("`", 50),
        ("backspace", 51),
        ("escape", 53),
        ("left", 123),
        ("right", 124),
        ("down", 125),
        ("up", 126),
        ("f1", 122),
        ("f2", 120),
        ("f3", 99),
        ("f4", 118),
        ("f5", 96),
        ("f6", 97),
        ("f7", 98),
        ("f8", 100),
        ("f9", 101),
        ("f10", 109),
        ("f11", 103),
        ("f12", 111),
    ];
    KEYS.iter()
        .find(|(name, _)| *name == key)
        .map(|(_, code)| *code)
}

#[cfg(test)]
#[path = "../../../tests/unit/island_hotkey_tests.rs"]
mod tests;
