use crate::lib::behavior;

/// Current TUI mode — maps 1:1 to ttoll mode states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 0x01 — 键盘浏览
    Browse,
    /// 0x02 — 主键编辑
    MainKey,
    /// 0x03 — 弹出键编辑
    PopupKey,
}

pub const MODE_0x01: &str = "0x01";
pub const MODE_0x02: &str = "0x02";
pub const MODE_0x03: &str = "0x03";

impl Mode {
    /// Return the hex constant for this mode.
    pub fn as_hex(&self) -> &'static str {
        match self {
            Mode::Browse => MODE_0x01,
            Mode::MainKey => MODE_0x02,
            Mode::PopupKey => MODE_0x03,
        }
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Browse => "Browse",
            Mode::MainKey => "MainKey",
            Mode::PopupKey => "PopupKey",
        }
    }

    /// Mode ordinal (0, 1, 2).
    pub fn ordinal(&self) -> u8 {
        *self as u8
    }
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Browse
    }
}

/// Transition to the next mode based on a ttoll action token.
///
/// Mode-targeting actions (`0x01`..`0x03`) switch to that mode.
/// Action IDs that are mode hex but outside 0x01–0x03 are ignored (stay).
/// Non-mode action strings are no-ops (return `None`).
pub fn transition(_current: Mode, action: &str) -> Option<Mode> {
    match action {
        behavior::MODE_0x01 => Some(Mode::Browse),
        behavior::MODE_0x02 => Some(Mode::MainKey),
        behavior::MODE_0x03 => Some(Mode::PopupKey),
        _ => None,
    }
}

/// Current mode name as a static string.
pub fn current_mode_name(mode: Mode) -> &'static str {
    mode.name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_defaults() {
        assert_eq!(Mode::default(), Mode::Browse);
        assert_eq!(Mode::Browse.ordinal(), 0);
    }

    #[test]
    fn transition_to_main_key() {
        assert_eq!(transition(Mode::Browse, behavior::MODE_0x02), Some(Mode::MainKey));
    }

    #[test]
    fn transition_to_popup() {
        assert_eq!(transition(Mode::MainKey, behavior::MODE_0x03), Some(Mode::PopupKey));
    }

    #[test]
    fn transition_noop_on_non_mode() {
        assert_eq!(transition(Mode::Browse, "ESC"), None);
        assert_eq!(transition(Mode::Browse, "0x09"), None);
    }

    #[test]
    fn names() {
        assert_eq!(Mode::Browse.name(), "Browse");
        assert_eq!(Mode::MainKey.as_hex(), "0x02");
        assert_eq!(Mode::PopupKey.as_hex(), "0x03");
    }
}
