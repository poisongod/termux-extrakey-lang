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
use crate::worker::script_runtime::{self, ScriptHost};
use std::sync::{Arc, Mutex};

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
    /// 退出信号 — 由 0x00 的 run 闭包置位
    should_quit: bool,
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
        should_quit: false,
        show_help: false,
        ttoll_table,
        id_registry,
        ttoll_mtime: None,
        ids_mtime: None,
    };

    let registry = state.id_registry.clone();
    let lib = std::mem::take(&mut state.id_registry.lib);
    let shared = Arc::new(Mutex::new(state));
    let host = ScriptHost { engine: ed_engine(shared.clone(), path.clone()).engine, lib };
    let ctx = EdCtx { state: shared, host, registry, path: path.clone() };

    let result = event_loop(&mut terminal, &ctx);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;

    result
}

/// 编辑器运行上下文。
struct EdCtx {
    state: Arc<Mutex<EditorState>>,
    host: ScriptHost,
    registry: id::IdRegistry,
    path: std::path::PathBuf,
}

/// 构建编辑器引擎 — 细粒度原语捕获状态句柄，插件逻辑直接读写状态。
fn ed_engine(handle: Arc<Mutex<EditorState>>, path: std::path::PathBuf) -> ScriptHost {
    let mut engine = rhai::Engine::new();
    engine.register_type_with_name::<Arc<Mutex<EditorState>>>("Ed");

    macro_rules! prim0 {
        ($name:literal, $f:expr) => {{
            let h = handle.clone();
            let g = move |h: Arc<Mutex<EditorState>>| $f(h);
            engine.register_fn($name, move || g(h.clone()));
        }};
    }
    macro_rules! prim1s {
        ($name:literal, $arg:ident, $f:expr) => {{
            let h = handle.clone();
            let g = move |h: Arc<Mutex<EditorState>>, $arg: &str| $f(h, $arg);
            engine.register_fn($name, move |$arg: &str| g(h.clone(), $arg));
        }};
    }

    // ── 读原语 ──
    prim0!("cursor_row", |h: Arc<Mutex<EditorState>>| h.lock().unwrap().cursor_row as i64);
    prim0!("cursor_col", |h: Arc<Mutex<EditorState>>| h.lock().unwrap().cursor_col as i64);
    prim0!("row_count", |h: Arc<Mutex<EditorState>>| h.lock().unwrap().lines.len() as i64);
    prim0!("line_len", |h: Arc<Mutex<EditorState>>| {
        let s = h.lock().unwrap();
        s.lines[s.cursor_row].len() as i64
    });

    // ── 光标/编辑原语 ──
    {
        let h = handle.clone();
        engine.register_fn("set_cursor", move |r: i64, c: i64| {
            let mut s = h.lock().unwrap();
            let n = s.lines.len();
            if r >= 0 && (r as usize) < n {
                s.cursor_row = r as usize;
                let len = s.lines[s.cursor_row].len();
                s.cursor_col = (c.max(0) as usize).min(len);
            }
        });
    }
    prim1s!("insert_text", t, |h: Arc<Mutex<EditorState>>, t: &str| {
        insert_text(&mut h.lock().unwrap(), t);
    });
    prim0!("backspace", |h: Arc<Mutex<EditorState>>| backspace(&mut h.lock().unwrap()));
    prim0!("delete_char", |h: Arc<Mutex<EditorState>>| {
        let mut s = h.lock().unwrap();
        let (r, c) = { let g = h.lock().unwrap(); (g.cursor_row, g.cursor_col) };
        let mut s = h.lock().unwrap();
        if !s.lines[r].is_empty() && c < s.lines[r].len() {
            s.lines[r].remove(c);
            clamp_col(&mut s);
            s.modified = true;
        }
    });
    prim0!("delete_line", |h: Arc<Mutex<EditorState>>| {
        let mut s = h.lock().unwrap();
        if s.lines.len() > 1 {
            let r = s.cursor_row;
            s.lines.remove(r);
            clamp_row(&mut s);
            clamp_col(&mut s);
            s.modified = true;
        }
    });
    // 仅插入空行; 进入插入模式的决策属于插件逻辑 (见 <ID>.rhai)
    prim1s!("new_line", pos, |h: Arc<Mutex<EditorState>>, pos: &str| {
        let mut s = h.lock().unwrap();
        if pos == "below" {
            let at = s.cursor_row + 1;
            s.lines.insert(at, String::new());
            s.cursor_row = at;
            s.cursor_col = 0;
        } else {
            let at = s.cursor_row;
            s.lines.insert(at, String::new());
            s.cursor_col = 0;
        }
        s.modified = true;
    });

    // ── 模式/面板/命令/文件 ──
    {
        let h = handle.clone();
        engine.register_fn("set_mode", move |n: i64| {
            let mut s = h.lock().unwrap();
            match n {
                0x01 => {
                    s.mode = Mode::Normal;
                    s.status_msg.clear();
                }
                0x02 => s.mode = Mode::Insert,
                0x03 => {
                    s.mode = Mode::Command;
                    s.cmd_buf.clear();
                }
                _ => {}
            }
        });
    }
    prim1s!("toggle_panel", k, |h: Arc<Mutex<EditorState>>, k: &str| {
        let mut s = h.lock().unwrap();
        if k == "help" {
            s.show_help = !s.show_help;
        }
    });
    {
        let h = handle.clone();
        let p = path.clone();
        engine.register_fn("exec_cmd", move || {
            let mut s = h.lock().unwrap();
            if s.mode == Mode::Command {
                execute_command(&mut s, &p);
            }
        });
    }
    prim0!("cancel_cmd", |h: Arc<Mutex<EditorState>>| {
        let mut s = h.lock().unwrap();
        s.mode = Mode::Normal;
        s.cmd_buf.clear();
    });
    {
        let h = handle.clone();
        let p = path.clone();
        engine.register_fn("save", move |validate: bool| {
            let mut s = h.lock().unwrap();
            let content = s.lines.join("\n");
            if validate {
                if let Some(e) = validate_ttoll(&content) {
                    s.status_msg = format!("语法错误: {}", e);
                    s.status_is_error = true;
                    return;
                }
            }
            let _ = fs::write(&p, &content);
            s.modified = false;
            s.status_msg = format!("已保存: {}", p.display());
            s.status_is_error = false;
        });
    }
    // ── 系统 ──
    prim0!("exit", |h: Arc<Mutex<EditorState>>| {
        let mut s = h.lock().unwrap();
        if s.modified {
            s.status_msg = "已修改，用 :q! 强制退出".into();
            s.status_is_error = true;
        } else {
            s.should_quit = true;
        }
    });

    ScriptHost { engine, lib: rhai::AST::empty() }
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
    ctx: &EdCtx,
) -> anyhow::Result<()> {
    loop {
        {
            let g = ctx.state.lock().unwrap();
            draw(terminal, &ctx.path, &g)?;
        }

        if let Event::Key(key) = event::read()? {
            // Ctrl+C always quits
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(());
            }

            // ── 阶段一: 短暂持锁完成路由与输入层处理, 产出待分派键名 ──
            enum Next { Handled, Dispatch(String) }
            let next = {
                let mut s = ctx.state.lock().unwrap();

                // 分区导航 (输入层, 不经过 ttoll): ALT+LEFT/RIGHT
                if key.modifiers.contains(KeyModifiers::ALT) {
                    match key.code {
                        KeyCode::Left => { s.pane = Pane::Ids; Next::Handled }
                        KeyCode::Right => { s.pane = Pane::Content; Next::Handled }
                        _ => Next::Dispatch(String::new()), // 落入下方常规路由
                    }
                } else if s.pane == Pane::Ids {
                    // ID 表焦点: 只读浏览, 仅 ↑↓ 滚动
                    let count = s.id_registry.all_entries().len();
                    match key.code {
                        KeyCode::Up => { s.id_sel = s.id_sel.saturating_sub(1); }
                        KeyCode::Down => {
                            if s.id_sel + 1 < count { s.id_sel += 1; }
                        }
                        _ => {}
                    }
                    Next::Handled
                } else {
                    match s.mode {
                        Mode::Insert => match key.code {
                            KeyCode::Char(c) => {
                                let row = s.cursor_row;
                                let col = s.cursor_col;
                                s.lines[row].insert(col, c);
                                s.cursor_col += 1;
                                s.modified = true;
                                Next::Handled
                            }
                            KeyCode::Backspace => {
                                backspace(&mut s);
                                Next::Handled
                            }
                            KeyCode::Tab => {
                                insert_text(&mut s, "    ");
                                Next::Handled
                            }
                            _ => match crossterm_to_key_name(key) {
                                Some(n) => Next::Dispatch(n),
                                None => Next::Handled,
                            },
                        },
                        Mode::Command => match key.code {
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                s.cmd_buf.push(c);
                                Next::Handled
                            }
                            KeyCode::Backspace => {
                                s.cmd_buf.pop();
                                Next::Handled
                            }
                            _ => match crossterm_to_key_name(key) {
                                Some(n) => Next::Dispatch(n),
                                None => Next::Handled,
                            },
                        },
                        Mode::Normal => match crossterm_to_key_name(key) {
                            // 文法冻结: 只有 K ∪ D (及 CTRL+/ALT+ 和弦) 参与 ttoll 分派
                            Some(n) if behavior::is_known_key(&n) => Next::Dispatch(n),
                            _ => Next::Handled,
                        },
                    }
                }
            };

            // ── 阶段二: 无锁状态下直接执行 Rhai 动作 (引擎内部自行加锁) ──
            if let Next::Dispatch(name) = next {
                dispatch_execute(ctx, &name);
            }

            // Auto-scroll + 退出检查 (短暂持锁)
            {
                let mut s = ctx.state.lock().unwrap();
                if s.cursor_row >= s.scroll_y + 10 {
                    s.scroll_y = s.cursor_row.saturating_sub(9);
                }
                if s.cursor_row < s.scroll_y {
                    s.scroll_y = s.cursor_row;
                }
                if s.should_quit {
                    return Ok(());
                }
            }
        }
    }
}

/// 按 ttoll 规则分派并直接执行 Rhai 动作。
fn dispatch_execute(ctx: &EdCtx, name: &str) {
    let action_id = {
        let s = ctx.state.lock().unwrap();
        let current_mode = s.mode.as_hex().to_string();
        if name.contains('+') {
            let keys: Vec<String> = name.split('+').map(str::to_string).collect();
            behavior::dispatch_chord(&s.ttoll_table, &current_mode, &keys)
        } else {
            behavior::dispatch(&s.ttoll_table, &current_mode, name)
        }
    };
    let Some(action_id) = action_id else { return };

    if !ctx.registry.is_registered(&action_id) {
        let mut s = ctx.state.lock().unwrap();
        s.status_msg = format!("未知 ID: {}", action_id);
        s.status_is_error = true;
        return;
    }

    if let Some(run) = ctx.registry.action_fn(&action_id).cloned() {
        if let Err(e) = ctx.host.call(&run) {
            let mut s = ctx.state.lock().unwrap();
            s.status_msg = format!("action 错误: {e}");
            s.status_is_error = true;
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
