use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::mode::Mode;

/// Render the status bar line.
///
/// Format: `[Mode: Browse] ~/path/file.tekl  [modified]  Ln 3/12`
pub fn render_status_bar(
    mode: Mode,
    file_path: &str,
    modified: bool,
    cursor: usize,
    rows_count: usize,
) -> Line<'static> {
    let mode_style = match mode {
        Mode::Browse => Style::default().fg(Color::Cyan),
        Mode::MainKey => Style::default().fg(Color::Green),
        Mode::PopupKey => Style::default().fg(Color::Magenta),
    };

    let mut spans = vec![
        Span::styled(format!("[{}]", mode.name()), mode_style),
        Span::raw(" "),
        Span::styled(file_path.to_string(), Style::default().fg(Color::White)),
    ];

    if modified {
        spans.push(Span::styled(" [+]", Style::default().fg(Color::Yellow)));
    }

    let pos = format!("  Ln {}/{}", cursor.saturating_add(1), rows_count);
    spans.push(Span::styled(pos, Style::default().fg(Color::DarkGray)));

    Line::from(spans)
}

/// Render the hint bar at the bottom.
///
/// Shows context-sensitive key hints for the current panel.
pub fn render_hint_bar(panel: &str) -> Line<'static> {
    let hints: Vec<(&str, &str)> = match panel {
        "browse" => vec![("↑↓", "move"), ("Enter", "edit"), ("q", "quit")],
        "edit" => vec![
            ("↑↓←→", "move"),
            ("Enter", "accept"),
            ("Esc", "back"),
        ],
        "popup" => vec![
            ("↑↓", "pick"),
            ("Enter", "select"),
            ("Esc", "close"),
        ],
        _ => vec![("Esc", "back")],
    };

    let spans: Vec<Span> = hints
        .into_iter()
        .flat_map(|(key, label)| {
            vec![
                Span::styled(
                    format!(" {key}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(":{label} "),
                    Style::default().fg(Color::DarkGray),
                ),
            ]
        })
        .collect();

    Line::from(spans)
}

/// Render mode indicator bar segments.
pub fn render_mode_bar(current_mode: Mode) -> Vec<Span<'static>> {
    let modes = [
        (Mode::Browse, " 0x01 "),
        (Mode::MainKey, " 0x02 "),
        (Mode::PopupKey, " 0x03 "),
    ];

    modes
        .iter()
        .map(|(m, label)| {
            let style = if *m == current_mode {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Span::styled(*label, style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bar_contains_mode() {
        let line = render_status_bar(Mode::Browse, "test.tekl", false, 0, 10);
        assert!(line.iter().any(|s| s.content.contains("Browse")));
    }

    #[test]
    fn status_bar_shows_modified() {
        let line = render_status_bar(Mode::MainKey, "f.tekl", true, 2, 5);
        let text: String = line.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[+]"));
    }

    #[test]
    fn hint_bar_browse() {
        let line = render_hint_bar("browse");
        let text: String = line.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("quit"));
    }

    #[test]
    fn mode_bar_active_highlighted() {
        let spans = render_mode_bar(Mode::MainKey);
        assert_eq!(spans.len(), 3);
        // 0x02 (MainKey) should have bold/bright style
        assert_eq!(spans[1].style, Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD));
    }
}
