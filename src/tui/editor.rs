use std::fs;
use std::io;
use std::collections::HashMap;

use crossterm::event::{self, Event, KeyCode, KeyModifiers, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::lib::user_dir;
use crate::lib::id;
use crate::lib::behavior;
use crate::worker::script_runtime;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Mode {
    Normal,
    Insert,
    Command,
}

impl Mode {
    fn as_hex(&self) -> &'static str {
        match self {
            Mode::Normal => "0x01",
            Mode::Insert => "0x02",
            Mode::Command => "0x03",
        }
    }
}

/// 焦点分区 — 左 ID 表 / 右 可编辑内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Ids,
    Content,
}

struct EditorState {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_y: usize,
    mode: Mode,
    cmd_buf: String,
    modified: bool,
    status_msg: String,
    status_is_error: bool,
    show_help: bool,
    /// 当前焦点分区
    pane: Pane,
    /// ID 表选中行索引
    id_sel: usize,
    ttoll_table: HashMap<Option<String>, Vec<behavior::Rule>>,
    id_registry: id::IdRegistry,
    /// 热加载: 上次看到的文件修改时间
    ttoll_mtime: Option<std::time::SystemTime>,
    ids_mtime: Option<std::time::SystemTime>,
}

pub fn run_editor(file: &Option<String>) -> anyhow::Result<()> {
    let path = resolve_path(file);
    let content = fs::read_to_string(&path).unwrap_or_default();

    // Build ttoll editor ID registry — from built-in editor/*.rhai + user overrides
    let id_registry = id::IdRegistry::for_editor();

    // Load editor.ttoll dispatch rules
    let ttoll_path = user_dir::user_root().join("editor.ttoll");
    let ttoll_source = fs::read_to_string(&ttoll_path).unwrap_or_default();
    let rules = behavior::parse_rules(&ttoll_source);
    let ttoll_table = behavior::build_table(&rules);

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }

    let mut state = EditorState {
        lines,
        cursor_row: 0,
        cursor_col: 0,
        scroll_y: 0,
        mode: Mode::Normal,
        cmd_buf: String::new(),
        modified: false,
        status_msg: String::new(),
        status_is_error: false,
        pane: Pane::Content,
        id_sel: 0,
        show_help: false,
        ttoll_table,
        id_registry,
        ttoll_mtime: None,
        ids_mtime: None,
    };

    let result = event_loop(&mut terminal, &path, &mut state);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;

    result
}

fn resolve_path(file: &Option<String>) -> std::path::PathBuf {
    match file {
        Some(f) => std::path::PathBuf::from(f),
        None => user_dir::behavior_base_file(),
    }
}

// ── Event loop — 全部走 ttoll dispatch ──────────────────────────────

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &std::path::Path,
    state: &mut EditorState,
) -> anyhow::Result<()> {
    loop {
        draw(terminal, path, state)?;

        if let Event::Key(key) = event::read()? {
            // Ctrl+C always quits
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(());
            }

            // ── 分区导航 (输入层, 不经过 ttoll): ALT+LEFT/RIGHT ──
            if key.modifiers.contains(KeyModifiers::ALT) {
                match key.code {
                    KeyCode::Left => { state.pane = Pane::Ids; continue; }
                    KeyCode::Right => { state.pane = Pane::Content; continue; }
                    _ => {}
                }
            }

            // ── ID 表焦点: 只读浏览, 仅 ↑↓ 滚动 ──
            if state.pane == Pane::Ids {
                let count = state.id_registry.all_entries().len();
                match key.code {
                    KeyCode::Up => { state.id_sel = state.id_sel.saturating_sub(1); }
                    KeyCode::Down => {
                        if state.id_sel + 1 < count { state.id_sel += 1; }
                    }
                    _ => {}
                }
                continue;
            }

            // ── 内容焦点: 输入类按键不经过 ttoll；Normal 仅接受文法词汇 ──
            let should_quit = match state.mode {
                Mode::Insert => match key.code {
                    KeyCode::Char(c) => {
                        let row = state.cursor_row;
                        let col = state.cursor_col;
                        state.lines[row].insert(col, c);
                        state.cursor_col += 1;
                        state.modified = true;
                        false
                    }
                    KeyCode::Backspace => {
                        backspace(state);
                        false
                    }
                    KeyCode::Tab => {
                        insert_text(state, "    ");
                        false
                    }
                    _ => dispatch_named(state, path, crossterm_to_key_name(key)),
                },
                Mode::Command => match key.code {
                    KeyCode::Char(_) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        dispatch_named(state, path, crossterm_to_key_name(key))
                    }
                    KeyCode::Char(c) => {
                        state.cmd_buf.push(c);
                        false
                    }
                    KeyCode::Backspace => {
                        state.cmd_buf.pop();
                        false
                    }
                    _ => dispatch_named(state, path, crossterm_to_key_name(key)),
                },
                Mode::Normal => match crossterm_to_key_name(key) {
                    // 文法冻结: 只有 K ∪ D (及 CTRL+/ALT+ 和弦) 参与 ttoll 分派
                    Some(n) if behavior::is_known_key(&n) => dispatch_named(state, path, Some(n)),
                    _ => false,
                },
            };
            if should_quit {
                return Ok(());
            }

            // Auto-scroll
            if state.cursor_row >= state.scroll_y + 10 {
                state.scroll_y = state.cursor_row.saturating_sub(9);
            }
            if state.cursor_row < state.scroll_y {
                state.scroll_y = state.cursor_row;
            }
        }
    }
}

/// Insert 模式退格 — 输入层行为，不经过 ttoll。
fn backspace(state: &mut EditorState) {
    if state.cursor_col > 0 {
        state.lines[state.cursor_row].remove(state.cursor_col - 1);
        state.cursor_col -= 1;
        state.modified = true;
    } else if state.cursor_row > 0 {
        let prev_len = state.lines[state.cursor_row - 1].len();
        let cur = std::mem::take(&mut state.lines[state.cursor_row]);
        state.lines[state.cursor_row - 1].push_str(&cur);
        state.lines.remove(state.cursor_row);
        state.cursor_row -= 1;
        state.cursor_col = prev_len;
        state.modified = true;
    }
}

/// Insert 模式插入文本 — 输入层行为，不经过 ttoll。
fn insert_text(state: &mut EditorState, text: &str) {
    let row = state.cursor_row;
    let col = state.cursor_col;
    state.lines[row].insert_str(col, text);
    state.cursor_col += text.chars().count();
    state.modified = true;
}

/// 按 ttoll 规则分派一个键名；返回是否应退出编辑器。
fn dispatch_named(
    state: &mut EditorState,
    path: &std::path::Path,
    key_name: Option<String>,
) -> bool {
    let Some(name) = key_name else {
        return false;
    };
    let current_mode = state.mode.as_hex().to_string();
    // 和弦名 (CTRL+X) 走 dispatch_chord, 普通键走 dispatch
    let action_id = if name.contains('+') {
        let keys: Vec<String> = name.split('+').map(str::to_string).collect();
        behavior::dispatch_chord(&state.ttoll_table, &current_mode, &keys)
    } else {
        behavior::dispatch(&state.ttoll_table, &current_mode, &name)
    };
    if let Some(action_id) = action_id {
        execute_action(state, &action_id, path);
        return action_id == "0x00" && !state.modified;
    }
    false
}

/// Convert crossterm KeyEvent to DSL key name.
fn crossterm_to_key_name(key: KeyEvent) -> Option<String> {
    // CTRL+方向 → 和弦名 (文法 CHORD 形式)
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => Some("ESC".into()),
        KeyCode::Enter => Some("ENTER".into()),
        KeyCode::Tab => Some("TAB".into()),
        KeyCode::Backspace => Some("BKSP".into()),
        KeyCode::Up => Some(if ctrl { "CTRL+UP".into() } else { "UP".into() }),
        KeyCode::Down => Some(if ctrl { "CTRL+DOWN".into() } else { "DOWN".into() }),
        KeyCode::Left => Some(if ctrl { "CTRL+LEFT".into() } else { "LEFT".into() }),
        KeyCode::Right => Some(if ctrl { "CTRL+RIGHT".into() } else { "RIGHT".into() }),
        KeyCode::Char(c) => {
            // 大小写敏感 (vim 风格: i 与 I 是不同的键); CTRL 和弦统一大写修饰部分
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(format!("CTRL+{}", c.to_ascii_uppercase()))
            } else {
                Some(c.to_string())
            }
        }
        _ => None,
    }
}

// ── Action execution — 无硬编码：ID → action 由注册池提供（Rhai），此处仅回放宿主操作 ──

fn execute_action(state: &mut EditorState, action_id: &str, path: &std::path::Path) {
    if !state.id_registry.is_registered(action_id) {
        state.status_msg = format!("未知 ID: {}", action_id);
        state.status_is_error = true;
        return;
    }

    let ops = match state.id_registry.eval_action(action_id) {
        Ok(ops) => ops,
        Err(e) => {
            state.status_msg = format!("action 错误: {e}");
            state.status_is_error = true;
            return;
        }
    };

    for op in &ops {
        apply_host_op(state, op, path);
    }
}

/// 宿主操作回放 — HostOp → EditorState 状态变更。
fn apply_host_op(state: &mut EditorState, op: &script_runtime::HostOp, path: &std::path::Path) {
    use script_runtime::HostOp;
    match op {
        // ── 系统/模式 ──
        HostOp::Exit => {
            if state.modified {
                state.status_msg = "已修改，用 :q! 强制退出".into();
                state.status_is_error = true;
            }
        }
        HostOp::SetMode(n) => match *n {
            // 模式: 0x01=Normal 0x02=Insert 0x03=Command
            0x01 => { state.mode = Mode::Normal; state.status_msg.clear(); }
            0x02 => { state.mode = Mode::Insert; state.status_msg.clear(); }
            0x03 => { state.mode = Mode::Command; state.cmd_buf.clear(); state.status_msg.clear(); }
            _ => {}
        },
        // ── 光标 ──
        HostOp::Cursor(dir) => match dir.as_str() {
            "left" => { if state.cursor_col > 0 { state.cursor_col -= 1; } }
            "right" => {
                if state.cursor_col < state.lines[state.cursor_row].len() {
                    state.cursor_col += 1;
                }
            }
            "down" => {
                if state.cursor_row + 1 < state.lines.len() {
                    state.cursor_row += 1;
                    clamp_col(state);
                }
            }
            "up" => {
                if state.cursor_row > 0 {
                    state.cursor_row -= 1;
                    clamp_col(state);
                }
            }
            _ => {}
        },
        // ── 跳转 ──
        HostOp::Jump(target) => match target.as_str() {
            "top" => { state.cursor_row = 0; state.cursor_col = 0; }
            "bottom" => {
                state.cursor_row = state.lines.len().saturating_sub(1);
                state.cursor_col = 0;
                clamp_col(state);
            }
            _ => {}
        },
        // ── 编辑 ──
        HostOp::DeleteChar => {
            if !state.lines[state.cursor_row].is_empty() {
                state.lines[state.cursor_row].remove(state.cursor_col);
                clamp_col(state);
                state.modified = true;
            }
        }
        HostOp::NewLine(pos) => match pos.as_str() {
            "below" => {
                let insert_at = state.cursor_row + 1;
                state.lines.insert(insert_at, String::new());
                state.cursor_row = insert_at;
                state.cursor_col = 0;
                state.mode = Mode::Insert;
                state.modified = true;
            }
            "above" => {
                state.lines.insert(state.cursor_row, String::new());
                state.mode = Mode::Insert;
                state.modified = true;
            }
            _ => {}
        },
        HostOp::DeleteLine => {
            state.lines.remove(state.cursor_row);
            if state.lines.is_empty() {
                state.lines.push(String::new());
            }
            clamp_row(state);
            clamp_col(state);
            state.modified = true;
        }
        HostOp::InsertText(text) => {
            if text == "\n" {
                let rest = state.lines[state.cursor_row][state.cursor_col..].to_string();
                state.lines[state.cursor_row].truncate(state.cursor_col);
                state.lines.insert(state.cursor_row + 1, rest);
                state.cursor_row += 1;
                state.cursor_col = 0;
            } else {
                state.lines[state.cursor_row].insert_str(state.cursor_col, text);
                state.cursor_col += text.chars().count();
            }
            state.modified = true;
        }
        HostOp::Backspace => {
            if state.cursor_col > 0 {
                state.lines[state.cursor_row].remove(state.cursor_col - 1);
                state.cursor_col -= 1;
                state.modified = true;
            } else if state.cursor_row > 0 {
                let prev_len = state.lines[state.cursor_row - 1].len();
                let cur = std::mem::take(&mut state.lines[state.cursor_row]);
                state.lines[state.cursor_row - 1].push_str(&cur);
                state.lines.remove(state.cursor_row);
                state.cursor_row -= 1;
                state.cursor_col = prev_len;
                state.modified = true;
            }
        }
        // ── 面板 ──
        HostOp::TogglePanel(kind) => match kind.as_str() {
            "id" => { /* ID 表常驻左栏, 此操作为兼容占位 */ }
            "help" => { state.show_help = !state.show_help; }
            _ => {}
        },
        // ── 命令/保存 ──
        HostOp::ExecCmd => {
            if state.mode == Mode::Command {
                execute_command(state, path);
            }
        }
        HostOp::Save(validate) => {
            let content = state.lines.join("\n");
            if *validate {
                if let Some(e) = validate_ttoll(&content) {
                    state.status_msg = format!("语法错误: {}", e);
                    state.status_is_error = true;
                    return;
                }
            }
            let _ = fs::write(path, &content);
            state.modified = false;
            state.status_msg = format!("已保存: {}", path.display());
            state.status_is_error = false;
        }
        HostOp::CancelCmd => {
            state.mode = Mode::Normal;
            state.cmd_buf.clear();
        }
        // ── 矩阵原语在 ttoll 编辑器中无对应状态, 忽略 ──
        _ => {}
    }
}

/// Execute the command buffer.
fn execute_command(state: &mut EditorState, path: &std::path::Path) {
    let cmd = state.cmd_buf.trim().to_string();
    state.cmd_buf.clear();
    match cmd.as_str() {
        "w" => {
            let content = state.lines.join("\n");
            let err = validate_ttoll(&content);
            if let Some(e) = err {
                state.status_msg = format!("语法错误: {}", e);
                state.status_is_error = true;
            } else {
                let _ = fs::write(path, &content);
                state.modified = false;
                state.status_msg = format!("已保存: {}", path.display());
                state.status_is_error = false;
            }
        }
        "w!" => {
            let content = state.lines.join("\n");
            let _ = fs::write(path, &content);
            state.modified = false;
            state.status_msg = format!("已保存: {}", path.display());
            state.status_is_error = false;
        }
        "wq" => {
            let content = state.lines.join("\n");
            let err = validate_ttoll(&content);
            if let Some(e) = err {
                state.status_msg = format!("语法错误: {}", e);
                state.status_is_error = true;
            } else {
                let _ = fs::write(path, &content);
                state.modified = false;
            }
        }
        "q!" => { state.modified = false; }
        "q" => {
            if !state.modified {
                // Will exit on next loop
            } else {
                state.status_msg = "已修改，用 :q! 强制退出".into();
                state.status_is_error = true;
            }
        }
        "ids" => {
            let entries = state.id_registry.all_entries();
            let ids: Vec<String> = entries.iter()
                .map(|e| format!("{} {}", e.id, e.desc))
                .collect();
            state.status_msg = ids.join(" │ ");
            state.status_is_error = false;
        }
        "help" => { state.show_help = !state.show_help; }
        "set ids" => { state.pane = Pane::Ids; }
        "hide ids" => { state.pane = Pane::Content; }
        _ => {
            state.status_msg = format!("未知命令: {} (:help)", cmd);
            state.status_is_error = true;
        }
    }
    state.mode = Mode::Normal;
}

/// Validate ttoll syntax. Returns Some(error) if invalid.
fn validate_ttoll(content: &str) -> Option<String> {
    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("(*") {
            continue;
        }
        if behavior::tokenize_rule(line).is_none() {
            return Some(format!("行 {}: 语法错误", i + 1));
        }
    }
    None
}

fn clamp_row(state: &mut EditorState) {
    if state.cursor_row >= state.lines.len() {
        state.cursor_row = state.lines.len().saturating_sub(1);
    }
}

fn clamp_col(state: &mut EditorState) {
    let len = state.lines.get(state.cursor_row).map_or(0, |l| l.len());
    if state.cursor_col > len {
        state.cursor_col = len;
    }
}

// ── Drawing ──────────────────────────────────────────────────────────

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &std::path::Path,
    state: &EditorState,
) -> anyhow::Result<()> {
    terminal.draw(|f| {
        let size = f.area();

        // 顶部分区: [ID 表 | 可编辑内容] + 底部状态栏
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(size);

        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(20)])
            .split(main_chunks[0]);

        // 内容区: 帮助开启时在右栏底部叠加
        let (content_area, help_area) = if state.show_help {
            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(12)])
                .split(panes[1]);
            (right[0], Some(right[1]))
        } else {
            (panes[1], None)
        };

        draw_id_panel(f, panes[0], state);
        draw_edit_area(f, content_area, state);
        if let Some(h) = help_area {
            draw_help_panel(f, h);
        }

        draw_status_bar(f, main_chunks[1], path, state);
    })?;
    Ok(())
}

/// Categorize an ID by its hex value.
fn id_category(id: &str) -> &'static str {
    match id {
        "0x00" => "系统",
        "0x01" | "0x02" | "0x03" => "模式",
        _ => "编辑",
    }
}

fn draw_id_panel(f: &mut ratatui::Frame, area: Rect, state: &EditorState) {
    let focused = state.pane == Pane::Ids;
    let block = Block::default()
        .title(if focused { " ▶ ID 表 " } else { " ID 表 " })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { Color::Yellow } else { Color::Cyan }));

    let entries = state.id_registry.all_entries();
    let mut lines: Vec<Line> = Vec::new();

    let current_rule = state.lines.get(state.cursor_row).map(|s| s.as_str()).unwrap_or("");
    let current_action = extract_action_id(current_rule);

    // 可见行计数(不含空行/分类头)用于选中高亮
    let mut visible_idx: usize = 0;
    let mut last_category = "";
    for (i, e) in entries.iter().enumerate() {
        let category = id_category(&e.id);

        if category != last_category {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("── {} ──", category),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
            )));
            last_category = category;
        }

        let is_sel = focused && i == state.id_sel;
        let is_current = current_action.as_deref() == Some(e.id.as_str());
        let style = if is_sel {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", e.id), style.add_modifier(Modifier::BOLD)),
            Span::styled(e.desc.clone(), style),
        ]));
        visible_idx += 1;
    }
    let _ = visible_idx;

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn extract_action_id(line: &str) -> Option<String> {
    let s = line.trim();
    if s.is_empty() || s.starts_with("(*") { return None; }
    let gt_pos = s.find('>')?;
    let action = s[gt_pos + 1..].trim().to_string();
    if action.starts_with("0x") { Some(action) } else { None }
}

fn draw_help_panel(f: &mut ratatui::Frame, area: Rect) {
    let block = Block::default()
        .title(" 按键帮助 (Ctrl+H 切换) ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));

    let helps = vec![
        Line::from(Span::styled("── ttoll 语法 ──", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" 0x61,h>0x63", Style::default().fg(Color::Magenta)), Span::raw("  模式规则")]),
        Line::from(vec![Span::styled(" CTRL+C>0x60", Style::default().fg(Color::Green)), Span::raw("  全局规则")]),
        Line::from(vec![Span::styled(" (* 注释 *)", Style::default().fg(Color::DarkGray)), Span::raw("  注释")]),
        Line::from(""),
        Line::from(Span::styled("── 模式 ──", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" 0x61", Style::default().fg(Color::Cyan)), Span::raw("  Normal 模式")]),
        Line::from(vec![Span::styled(" 0x62", Style::default().fg(Color::Cyan)), Span::raw("  Insert 模式")]),
        Line::from(vec![Span::styled(" 0x63", Style::default().fg(Color::Cyan)), Span::raw("  Command 模式")]),
        Line::from(""),
        Line::from(Span::styled("── 命令 ──", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" :w", Style::default().fg(Color::Cyan)), Span::raw("      保存")]),
        Line::from(vec![Span::styled(" :wq", Style::default().fg(Color::Cyan)), Span::raw("     保存退出")]),
        Line::from(vec![Span::styled(" :q!", Style::default().fg(Color::Cyan)), Span::raw("    强制退出")]),
        Line::from(vec![Span::styled(" :ids", Style::default().fg(Color::Cyan)), Span::raw("     ID表")]),
        Line::from(vec![Span::styled(" :help", Style::default().fg(Color::Cyan)), Span::raw("    帮助")]),
    ];

    let paragraph = Paragraph::new(helps).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_edit_area(f: &mut ratatui::Frame, area: Rect, state: &EditorState) {
    let title = match state.mode {
        Mode::Normal => " 编辑 ttoll (Normal) ",
        Mode::Insert => " 编辑 ttoll (Insert) ",
        Mode::Command => " 编辑 ttoll (Command) ",
    };

    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_height = inner.height as usize;
    let line_no_width = format!("{}", state.lines.len()).len().max(3);

    for i in 0..visible_height {
        let line_idx = state.scroll_y + i;
        if line_idx >= state.lines.len() { break; }

        let y = inner.y + i as u16;
        let line_no = format!("{:>width$} ", line_idx + 1, width = line_no_width);
        let content = &state.lines[line_idx];

        let spans = if line_idx == state.cursor_row && state.mode != Mode::Normal {
            let (before, ch, after) = split_at_byte(content, state.cursor_col);
            let mut new_spans = vec![Span::styled(line_no, Style::default().fg(Color::DarkGray))];
            if content.trim().starts_with("(*") || content.trim().is_empty() {
                new_spans.push(Span::styled(before.to_string(), Style::default().fg(Color::DarkGray)));
            } else if let Some(parts) = highlight_ttoll_line(before) {
                new_spans.extend(parts);
            } else {
                new_spans.push(Span::raw(before.to_string()));
            }
            if let Some(c) = ch {
                new_spans.push(Span::styled(c.to_string(), Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD)));
            }
            new_spans.push(Span::raw(after.to_string()));
            new_spans
        } else {
            let mut spans = vec![Span::styled(line_no, Style::default().fg(Color::DarkGray))];
            if content.trim().starts_with("(*") || content.trim().is_empty() {
                spans.push(Span::styled(content.clone(), Style::default().fg(Color::DarkGray)));
            } else if let Some(highlighted) = highlight_ttoll_line(content) {
                spans.extend(highlighted);
            } else {
                spans.push(Span::raw(content.clone()));
            }
            spans
        };

        f.render_widget(Paragraph::new(Line::from(spans)), Rect { x: inner.x, y, width: inner.width, height: 1 });
    }
}

fn highlight_ttoll_line(line: &str) -> Option<Vec<Span<'static>>> {
    let s = line.trim();
    if s.is_empty() || s.starts_with("(*") { return None; }

    let gt_pos = s.find('>')?;
    let lhs = &s[..gt_pos];
    let rhs = &s[gt_pos + 1..];

    let mut spans = Vec::new();

    if let Some(comma_pos) = lhs.find(',') {
        let mode = &lhs[..comma_pos];
        let keys = &lhs[comma_pos + 1..];
        spans.push(Span::styled(mode.to_string(), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(",", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(keys.to_string(), Style::default().fg(Color::Green)));
    } else {
        spans.push(Span::styled(lhs.to_string(), Style::default().fg(Color::Green)));
    }

    spans.push(Span::styled(">", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

    let action = rhs.trim();
    let action_color = if action.starts_with("0x") { Color::Cyan } else { Color::White };
    spans.push(Span::styled(action.to_string(), Style::default().fg(action_color).add_modifier(Modifier::BOLD)));

    Some(spans)
}

fn draw_status_bar(f: &mut ratatui::Frame, area: Rect, path: &std::path::Path, state: &EditorState) {
    let (mode_str, mode_bg) = match state.mode {
        Mode::Normal => (" NORMAL ", Color::Green),
        Mode::Insert => (" INSERT ", Color::Yellow),
        Mode::Command => (" COMMAND ", Color::Cyan),
    };

    let mod_str = if state.modified { " [+]" } else { "" };
    let file_str = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().to_string());
    let pos = format!("{}/{}:{}", state.cursor_row + 1, state.lines.len(), state.cursor_col);

    let cmd_display = if state.mode == Mode::Command {
        format!(":{}", state.cmd_buf)
    } else if !state.status_msg.is_empty() {
        state.status_msg.clone()
    } else {
        "? 帮助 │ : 命令".to_string()
    };

    let msg_color = if state.status_is_error { Color::Red } else { Color::White };

    let mut spans = vec![
        Span::styled(mode_str, Style::default().fg(Color::Black).bg(mode_bg).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}{}", file_str, mod_str), Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 60))),
    ];

    let used = mode_str.len() + file_str.len() + mod_str.len() + cmd_display.len() + pos.len() + 4;
    let width = area.width as usize;
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }

    spans.push(Span::styled(format!(" {} ", pos), Style::default().fg(Color::DarkGray).bg(Color::Rgb(40, 40, 60))));
    spans.push(Span::styled(&cmd_display, Style::default().fg(msg_color).bg(Color::Rgb(40, 40, 60))));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn split_at_byte(s: &str, byte_idx: usize) -> (&str, Option<char>, &str) {
    if byte_idx == 0 { return (s, None, ""); }
    if byte_idx >= s.len() { return (s, None, ""); }
    let (before, after) = s.split_at(byte_idx);
    let ch = after.chars().next();
    let rest = &after[ch.map_or(0, |c| c.len_utf8())..];
    (before, ch, rest)
}
