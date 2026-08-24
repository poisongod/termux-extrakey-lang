use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::mode::Mode;

#[derive(Debug, Clone)]
pub struct KeyCell {
    pub text: String,
    pub width: u16,
}

impl KeyCell {
    pub fn new(text: &str) -> Self {
        let w = (text.len() as u16).max(3) + 2;
        KeyCell { text: text.to_string(), width: w }
    }
}

pub fn compute_column_widths(keys: &[KeyCell], total_width: u16) -> Vec<u16> {
    if keys.is_empty() {
        return vec![];
    }
    let raw_sum: u16 = keys.iter().map(|k| k.width).sum();
    if raw_sum == 0 {
        let even = total_width / keys.len() as u16;
        return vec![even; keys.len()];
    }
    keys.iter()
        .map(|k| {
            let w = (k.width as u32 * total_width as u32 / raw_sum as u32) as u16;
            w.max(3)
        })
        .collect()
}

pub fn render_keyboard(
    rows: &[Vec<String>],
    cursor: (usize, usize),
    mode: Mode,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height == 0 || area.width == 0 || rows.is_empty() {
        return;
    }

    let row_count = rows.len() as u16;
    let row_height = (area.height / row_count).max(1);

    for (row_idx, row) in rows.iter().enumerate() {
        let y = area.y + row_idx as u16 * row_height;
        if y + row_height > area.y + area.height {
            break;
        }

        let cells: Vec<KeyCell> = row.iter().map(|k| KeyCell::new(k)).collect();
        let widths = compute_column_widths(&cells, area.width);

        let mut x = area.x;
        for (col_idx, (cell, &cw)) in cells.iter().zip(&widths).enumerate() {
            if x >= area.x + area.width {
                break;
            }
            let cell_w = cw.min(area.x + area.width - x);

            let is_cursor = row_idx == cursor.0 && col_idx == cursor.1;
            let style = if is_cursor {
                match mode {
                    Mode::MainKey => Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    Mode::PopupKey => Style::default()
                        .fg(Color::Black)
                        .bg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                    Mode::Browse => Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                }
            } else {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            };

            // Fill cell background
            let fill = " ".repeat(cell_w as usize);
            for cy in y..y + row_height {
                buf.set_string(x, cy, &fill, style);
            }

            // Center key text in cell
            let text_w = cell.text.len() as u16;
            let offset = if text_w < cell_w { (cell_w - text_w) / 2 } else { 0 };
            let display = if text_w > cell_w {
                &cell.text[..cell_w as usize]
            } else {
                &cell.text
            };
            let text_y = y + row_height / 2;
            if text_y < area.y + area.height {
                buf.set_string(x + offset, text_y, display, style);
            }

            x += cell_w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_cell_min_width() {
        let c = KeyCell::new("a");
        assert_eq!(c.width, 5); // max(1,3) + 2 = 5
    }

    #[test]
    fn compute_column_widths_basic() {
        let keys = vec![KeyCell::new("ESC"), KeyCell::new("a")];
        let widths = compute_column_widths(&keys, 40);
        assert_eq!(widths.len(), 2);
        assert_eq!(widths.iter().sum::<u16>(), 40);
    }

    #[test]
    fn compute_column_widths_empty() {
        assert!(compute_column_widths(&[], 40).is_empty());
    }
}
