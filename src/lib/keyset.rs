/// Key vocabulary: named keys (N) and special modifier keys (S).

pub const NAMED_KEYS: &[&str] = &[
    "ESC", "TAB", "HOME", "END", "PGUP", "PGDN",
    "INS", "DEL", "BKSP",
    "UP", "LEFT", "RIGHT", "DOWN",
    "ENTER", "BACKSLASH", "QUOTE", "APOSTROPHE",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7",
    "F8", "F9", "F10", "F11", "F12",
    "KEYBOARD", "DRAWER", "SCROLL", "SPACE", "PASTE",
];

pub const SPECIAL_KEYS: &[&str] = &["CTRL", "ALT", "FN", "SHIFT"];

pub fn is_named_key(s: &str) -> bool {
    let upper = s.to_uppercase();
    NAMED_KEYS.iter().any(|k| *k == upper.as_str())
}

pub fn is_special_key(s: &str) -> bool {
    let upper = s.to_uppercase();
    SPECIAL_KEYS.iter().any(|k| *k == upper.as_str())
}

pub fn normalize_key_name(s: &str) -> String {
    let upper = s.to_uppercase();
    if is_named_key(s) || is_special_key(s) {
        upper
    } else {
        s.to_string()
    }
}

pub fn func_keys() -> &'static [&'static str] {
    &["ESC", "ALT", "CTRL", "SPACE", "ENTER"]
}

pub fn dir_keys() -> &'static [&'static str] {
    &["UP", "DOWN", "LEFT", "RIGHT"]
}
