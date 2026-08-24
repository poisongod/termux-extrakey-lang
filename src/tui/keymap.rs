use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Normalize a crossterm key event into a DSL key name.
///
/// Returns the uppercase key name as used in the layout DSL, or `None` for
/// unhandled/composed events.
pub fn normalize_key(event: KeyEvent) -> Option<String> {
    let mods = event.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);

    // Skip composed/ime events
    if mods.contains(KeyModifiers::CONTROL) && mods.contains(KeyModifiers::ALT) {
        return None;
    }

    match event.code {
        KeyCode::Esc => Some("ESC".into()),
        KeyCode::Enter => Some("ENTER".into()),
        KeyCode::Tab => Some("TAB".into()),
        KeyCode::Backspace => Some("BKSP".into()),
        KeyCode::Delete => Some("DEL".into()),
        KeyCode::Insert => Some("INS".into()),
        KeyCode::Home => Some("HOME".into()),
        KeyCode::End => Some("END".into()),
        KeyCode::PageUp => Some("PGUP".into()),
        KeyCode::PageDown => Some("PGDN".into()),
        KeyCode::Up => Some("UP".into()),
        KeyCode::Down => Some("DOWN".into()),
        KeyCode::Left => Some("LEFT".into()),
        KeyCode::Right => Some("RIGHT".into()),
        KeyCode::F(n) => Some(format!("F{}", n)),
        KeyCode::Char(c) => {
            let s = c.to_uppercase().to_string();
            if ctrl || alt {
                // Build modifier-prefixed name: e.g. CTRL+A
                let mut prefix = String::new();
                if ctrl {
                    prefix.push_str("CTRL+SHIFT+");
                }
                if alt {
                    prefix.push_str("ALT+");
                }
                // Remove trailing "+" from ctrl case if no alt
                if prefix.ends_with('+') {
                    prefix.pop();
                }
                Some(format!("{}+{}", prefix, s))
            } else {
                Some(s)
            }
        }
        KeyCode::BackTab => Some("SHIFT+TAB".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn arrow_keys() {
        assert_eq!(normalize_key(key(KeyCode::Up, KeyModifiers::NONE)), Some("UP".into()));
        assert_eq!(normalize_key(key(KeyCode::Down, KeyModifiers::NONE)), Some("DOWN".into()));
        assert_eq!(normalize_key(key(KeyCode::Left, KeyModifiers::NONE)), Some("LEFT".into()));
        assert_eq!(normalize_key(key(KeyCode::Right, KeyModifiers::NONE)), Some("RIGHT".into()));
    }

    #[test]
    fn special_keys() {
        assert_eq!(normalize_key(key(KeyCode::Esc, KeyModifiers::NONE)), Some("ESC".into()));
        assert_eq!(normalize_key(key(KeyCode::Enter, KeyModifiers::NONE)), Some("ENTER".into()));
        assert_eq!(normalize_key(key(KeyCode::Tab, KeyModifiers::NONE)), Some("TAB".into()));
    }

    #[test]
    fn function_keys() {
        assert_eq!(normalize_key(key(KeyCode::F(1), KeyModifiers::NONE)), Some("F1".into()));
        assert_eq!(normalize_key(key(KeyCode::F(12), KeyModifiers::NONE)), Some("F12".into()));
    }

    #[test]
    fn ctrl_char() {
        let e = key(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let n = normalize_key(e).unwrap();
        assert!(n.contains("A"));
    }

    #[test]
    fn plain_char() {
        assert_eq!(normalize_key(key(KeyCode::Char('x'), KeyModifiers::NONE)), Some("X".into()));
    }
}
