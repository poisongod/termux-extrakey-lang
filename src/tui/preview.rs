use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

/// 键间水平缝隙(列)
pub const KEY_GAP: u16 = 1;

/// 一个键帽的外观定义 — 由调用方按语义给定颜色，本模块仅负责绘制。
///
/// 立体键帽由三色组成:
///   top   — 键帽上部亮带(高光)
///   body  — 键帽主体
///   dark  — 键帽底部暗影(制造凸起感)
#[derive(Debug, Clone)]
pub struct KeyCap {
    /// 键面文字
    pub label: String,
    /// 上部高光
    pub top: Color,
    /// 主体
    pub body: Color,
    /// 底部暗影
    pub dark: Color,
    /// 键面文字色
    pub text: Color,
    /// 加粗(按压/选中反馈)
    pub bold: bool,
}

impl KeyCap {
    pub fn new(label: impl Into<String>, top: Color, body: Color, dark: Color, text: Color) -> Self {
        KeyCap { label: label.into(), top, body, dark, text, bold: false }
    }

    /// 按压高亮 — 键帽整体提亮、文字反白加粗。
    pub fn pressed(mut self, top: Color, body: Color, dark: Color, text: Color) -> Self {
        self.top = top;
        self.body = body;
        self.dark = dark;
        self.text = text;
        self.bold = true;
        self
    }
}

/// 按比例把一行键帽铺到可用宽度。宽度以字符数为权重(label 短键不小于 3)。
///
/// `total_width` 含键间缝隙 `gap` 占用的列; 返回值之和 + gap*(n-1) ≈ total_width。
pub fn layout_key_row(labels: &[&str], total_width: u16, gap: u16) -> Vec<u16> {
    if labels.is_empty() || total_width == 0 {
        return vec![];
    }
    let n = labels.len() as u16;
    let usable = total_width.saturating_sub(gap.saturating_mul(n.saturating_sub(1)));
    let weights: Vec<u16> = labels
        .iter()
        .map(|l| (l.chars().count() as u16).max(3) + 2)
        .collect();
    let sum: u32 = weights.iter().map(|&w| w as u32).sum();

    let mut widths: Vec<u16> = if sum == 0 {
        vec![usable / n; n as usize]
    } else {
        weights
            .iter()
            .map(|w| (((*w as u32 * usable as u32) / sum) as u16).max(3))
            .collect()
    };

    // 校正尾键, 使总和精确等于可用宽度
    let now: i32 = widths.iter().map(|&w| w as i32).sum();
    let diff = usable as i32 - now;
    if let Some(last) = widths.last_mut() {
        *last = (*last as i32 + diff).max(1) as u16;
    }
    widths
}

/// 渲染一整片键盘。
///
/// 键帽高度自适应: 竖直空间充足则每键行 3 行(高光条+主体+暗影), 次之 2 行(高光条+主体),
/// 不足则 1 行扁平主体。仅绘制 `area` 内的键; 越界/空输入安全返回。
pub fn render_keyboard(rows: &[Vec<KeyCap>], area: Rect, buf: &mut Buffer) {
    if rows.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    let n_rows = rows.len() as u16;
    let cap_h = if area.height >= n_rows * 3 {
        3
    } else if area.height >= n_rows * 2 {
        2
    } else {
        1
    };

    for (r, row) in rows.iter().enumerate() {
        let y = area.y + r as u16 * cap_h;
        if y + cap_h > area.y + area.height {
            break;
        }

        let labels: Vec<&str> = row.iter().map(|k| k.label.as_str()).collect();
        let widths = layout_key_row(&labels, area.width, KEY_GAP);

        let mut x = area.x;
        for (key, &w) in row.iter().zip(&widths) {
            let avail = w.min(area.x + area.width - x);
            if avail == 0 {
                break;
            }
            draw_key_cap(buf, x, y, avail, cap_h, key);
            x += avail + KEY_GAP;
        }
    }
}

/// 绘制单个键帽(纯颜色块, 无块字符/图标)。
/// 高度自适应: `h>=3` 高光条+主体+暗影; `h==2` 高光条+主体; 否则扁平主体。
fn draw_key_cap(buf: &mut Buffer, x: u16, y: u16, w: u16, h: u16, key: &KeyCap) {
    let base = Style::default().add_modifier(if key.bold { Modifier::BOLD } else { Modifier::empty() });

    if h >= 3 {
        fill_row(buf, x, y, w, key.top, base);
        fill_row(buf, x, y + 1, w, key.body, base);
        put_label(buf, x, y + 1, w, key, base);
        fill_row(buf, x, y + 2, w, key.dark, base);
    } else if h == 2 {
        fill_row(buf, x, y, w, key.top, base);
        fill_row(buf, x, y + 1, w, key.body, base);
        put_label(buf, x, y + 1, w, key, base);
    } else {
        fill_row(buf, x, y, w, key.body, base);
        put_label(buf, x, y, w, key, base);
    }
}

fn fill_row(buf: &mut Buffer, x: u16, y: u16, w: u16, color: Color, base: Style) {
    let style = base.bg(color);
    for cx in x..x + w {
        buf.set_stringn(cx, y, " ", 1, style);
    }
}

fn put_label(buf: &mut Buffer, x: u16, y: u16, w: u16, key: &KeyCap, base: Style) {
    let t = truncate(&key.label, w);
    let tw = t.chars().count() as u16;
    let tx = x + w.saturating_sub(tw) / 2;
    buf.set_stringn(tx, y, &t, tw.into(), base.fg(key.text).bg(key.body));
}

/// 按可视宽度截断标签(以字符计; 过宽则用两个点收尾)。
fn truncate(label: &str, width: u16) -> String {
    let w = width as usize;
    let mut chars: Vec<char> = label.chars().collect();
    if chars.len() <= w {
        return label.to_string();
    }
    if w == 0 {
        return String::new();
    }
    if w <= 2 {
        chars.truncate(w);
        return chars.into_iter().collect();
    }
    chars.truncate(w - 2);
    let mut s: String = chars.into_iter().collect();
    s.push_str("..");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_row_sums_to_usable() {
        let labels = ["ESC", "A:A", "CAPS", "SPACE"];
        let widths = layout_key_row(&labels, 40, KEY_GAP);
        let sum: u16 = widths.iter().sum();
        let expect = 40 - KEY_GAP * (labels.len() as u16 - 1);
        assert_eq!(sum, expect);
        assert!(widths.iter().all(|&w| w >= 3));
    }

    #[test]
    fn layout_row_empty() {
        assert!(layout_key_row(&[], 40, KEY_GAP).is_empty());
        assert!(layout_key_row(&["A"], 0, KEY_GAP).is_empty());
    }

    #[test]
    fn layout_row_wide_unsqueezes_narrow() {
        // 单键独占全部可用宽度
        let w = layout_key_row(&["Enter"], 30, KEY_GAP);
        assert_eq!(w, vec![30]);
    }

    #[test]
    fn truncate_ellipsis() {
        assert_eq!(truncate("LONGKEY", 4), "LO..");
        assert_eq!(truncate("short", 6), "short");
        assert_eq!(truncate("x", 1), "x");
        assert_eq!(truncate("", 4), "");
    }

    #[test]
    fn pressed_highlights() {
        let cap = KeyCap::new("A", Color::Red, Color::Blue, Color::Black, Color::White)
            .pressed(Color::White, Color::White, Color::Gray, Color::Black);
        assert!(cap.bold);
        assert_eq!(cap.text, Color::Black);
    }
}