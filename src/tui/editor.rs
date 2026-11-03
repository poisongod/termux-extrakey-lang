use std::fs;
use std::io;
use std::collections::{HashMap, HashSet};

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
use crate::worker::script_runtime::ScriptHost;
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
    /// 撤销/重做历史 (行快照)
    undo_stack: Vec<Vec<String>>,
    redo_stack: Vec<Vec<String>>,
}

pub fn run_editor(file: &Option<String>) -> anyhow::Result<()> {
    let path = resolve_path(file);
    let content = fs::read_to_string(&path).unwrap_or_default();

    // Build ttoll editor ID registry — from built-in editor/*.rhai + user overrides
    let id_registry = id::IdRegistry::for_editor();

    // Load editor.ttoll dispatch rules
    let ttoll_path = user_dir::user_root().join("editor.ttoll");
    let ttoll_source = fs::read_to_string(&ttoll_path).unwrap_or_default();
    let rules = behavior::parse_rules(&ttoll_source)
        .map_err(|e| anyhow::anyhow!("{}: {e}", ttoll_path.display()))?;
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
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
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
    prim1s!("line_text", row, |h: Arc<Mutex<EditorState>>, row: &str| {
        let s = h.lock().unwrap();
        if let Ok(r) = row.parse::<usize>() {
            if let Some(l) = s.lines.get(r) {
                return l.clone();
            }
        }
        String::new()
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
        let r = s.cursor_row;
        let c = s.cursor_col;
        if !s.lines[r].is_empty() && c < s.lines[r].len() {
            lines_snapshot(&mut s);
            s.lines[r].remove(c);
            clamp_col(&mut s);
            s.modified = true;
        }
    });
    prim0!("delete_line", |h: Arc<Mutex<EditorState>>| {
        let mut s = h.lock().unwrap();
        if s.lines.len() > 1 {
            lines_snapshot(&mut s);
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
        lines_snapshot(&mut s);
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

    // 在光标处拆分当前行为两行 (0x0B 换行)
    prim0!("split_line", |h: Arc<Mutex<EditorState>>| {
        let mut s = h.lock().unwrap();
        lines_snapshot(&mut s);
        let r = s.cursor_row;
        let c = s.cursor_col;
        let rest = s.lines[r].split_off(c);
        s.lines.insert(r + 1, rest);
        s.cursor_row = r + 1;
        s.cursor_col = 0;
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
        match k {
            "help" => s.show_help = !s.show_help,
            "id" => s.pane = if s.pane == Pane::Ids { Pane::Content } else { Pane::Ids },
            _ => {}
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
                if let Some(e) = validate_ttoll(&content, &registered_ids(&s.id_registry)) {
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
    // 系统管理
    prim0!("undo", |h: Arc<Mutex<EditorState>>| undo_edit(&mut h.lock().unwrap()));
    prim0!("redo", |h: Arc<Mutex<EditorState>>| redo_edit(&mut h.lock().unwrap()));
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
        None => user_dir::tui_rules_file(),
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
                    // ID 表: ↑↓ 滚动, Enter 跳到当前 ID 对应的接线行
                    let count = s.id_registry.all_entries().len();
                    match key.code {
                        KeyCode::Up => { s.id_sel = s.id_sel.saturating_sub(1); }
                        KeyCode::Down => {
                            if s.id_sel + 1 < count { s.id_sel += 1; }
                        }
                        KeyCode::Enter => {
                            let sel: String = s.id_registry
                                .all_entries()
                                .get(s.id_sel)
                                .map(|e| e.id.clone())
                                .unwrap_or_default();
                            if !sel.is_empty() {
                                if let Some(at) = s.lines.iter().position(|l| {
                                    extract_action_id(l).as_deref() == Some(sel.as_str())
                                }) {
                                    s.cursor_row = at;
                                    s.cursor_col = s.lines[at].len();
                                } else {
                                    s.status_msg = format!("{} 尚未接线, 用 :map 生成", sel);
                                    s.status_is_error = false;
                                }
                            }
                            s.pane = Pane::Content;
                        }
                        _ => {}
                    }
                    Next::Handled
                } else {
                    match s.mode {
                        Mode::Insert => match key.code {
                            KeyCode::Char(c) => {
                                lines_snapshot(&mut s);
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

/// 修改前行快照 (撤销用); 上限 100, 并清空 redo。
fn lines_snapshot(state: &mut EditorState) {
    state.undo_stack.push(state.lines.clone());
    if state.undo_stack.len() > 100 {
        state.undo_stack.remove(0);
    }
    state.redo_stack.clear();
}

fn undo_edit(state: &mut EditorState) {
    if let Some(prev) = state.undo_stack.pop() {
        state.redo_stack.push(state.lines.clone());
        state.lines = prev;
        clamp_row(state);
        clamp_col(state);
        state.modified = true;
        state.status_msg = "已撤销".into();
        state.status_is_error = false;
    } else {
        state.status_msg = "无可撤销".into();
        state.status_is_error = true;
    }
}

fn redo_edit(state: &mut EditorState) {
    if let Some(next) = state.redo_stack.pop() {
        state.undo_stack.push(state.lines.clone());
        state.lines = next;
        clamp_row(state);
        clamp_col(state);
        state.modified = true;
        state.status_msg = "已重做".into();
        state.status_is_error = false;
    } else {
        state.status_msg = "无可重做".into();
        state.status_is_error = true;
    }
}

/// Insert 模式退格 — 输入层行为，不经过 ttoll。
fn backspace(state: &mut EditorState) {
    if state.cursor_col == 0 && state.cursor_row == 0 {
        return;
    }
    lines_snapshot(state);
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
    if text.is_empty() {
        return;
    }
    lines_snapshot(state);
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
            // 大小写敏感 (vim 风格: i 与 I 是不同的键); CTRL 和弦只对文法键生成
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let up = c.to_ascii_uppercase().to_string();
                if behavior::is_known_key(&up) {
                    Some(format!("CTRL+{up}"))
                } else {
                    None
                }
            } else {
                Some(c.to_string())
            }
        }
        _ => None,
    }
}

/// Execute the command buffer.
/// 命令名 == 目标动作; 生成规则用 :map / :tmpl 只需字母+数字, 不必打符号。
fn execute_command(state: &mut EditorState, path: &std::path::Path) {
    let cmd = state.cmd_buf.trim().to_string();
    state.cmd_buf.clear();

    let mut parts = cmd.split_whitespace();
    let base = parts.next().unwrap_or("").to_string();

    let stay_insert = base == "tmpl" || base == "map";

    match base.as_str() {
        // ── 文件操作 ──
        "w" | "s" => {
            let content = state.lines.join("\n");
            let err = validate_ttoll(&content, &registered_ids(&state.id_registry));
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
        "w!" | "s!" => {
            let content = state.lines.join("\n");
            let _ = fs::write(path, &content);
            state.modified = false;
            state.status_msg = format!("已保存: {}", path.display());
            state.status_is_error = false;
        }
        "wq" | "x" => {
            let content = state.lines.join("\n");
            let err = validate_ttoll(&content, &registered_ids(&state.id_registry));
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
            state.status_msg = ids.join(" | ");
            state.status_is_error = false;
        }
        "help" | "h" => { state.show_help = !state.show_help; }
        "set ids" => { state.pane = Pane::Ids; }
        "hide ids" => { state.pane = Pane::Content; }

        // ── 行编辑 ──
        "del" => {
            if state.lines.len() > 1 {
                lines_snapshot(state);
                state.lines.remove(state.cursor_row);
                clamp_row(state);
                clamp_col(state);
                state.modified = true;
                state.status_msg = "已删除当前行".into();
                state.status_is_error = false;
            } else {
                state.status_msg = "不能删除最后一行".into();
                state.status_is_error = true;
            }
        }

        // ── 生成合法规则行 (免打 , > 0x 等符号) ──
        // tmpl → 0x01,UP>0x10
        // map <MODE hex> <KEY|Chord> [ID hex] → 0x<MODE>,<KEY>>0x<ID>
        "tmpl" | "map" => {
            let mode_hex = normalize_hex(parts.next().unwrap_or("01"));
            let key = parts.next().unwrap_or("UP").to_uppercase();
            let id_hex = normalize_hex(parts.next().unwrap_or("10"));
            let line = format!("0x{},{}>0x{}", mode_hex, key, id_hex);
            lines_snapshot(state);
            let at = state.cursor_row + 1;
            state.lines.insert(at, line.clone());
            state.cursor_row = at;
            // 光标停在 ID 起始处, 进入 Insert 直接改写
            state.cursor_col = line.rfind('>').unwrap_or(0) + 1;
            state.mode = Mode::Insert;
            state.modified = true;
            state.status_msg = format!("已插入: {}", line);
            state.status_is_error = false;
        }
        _ => {
            state.status_msg = format!("未知命令: {} (:help)", cmd);
            state.status_is_error = true;
        }
    }

    if !stay_insert {
        state.mode = Mode::Normal;
    }
}

/// 把 "01" / "0x01" 归一为 "01"; 非法则原样返回 (让生成结果可见可改)。
fn normalize_hex(h: &str) -> String {
    h.strip_prefix("0x").unwrap_or(h).to_uppercase()
}

/// Validate ttoll syntax and action IDs before writing.
fn validate_ttoll(content: &str, ids: &HashSet<String>) -> Option<String> {
    behavior::check_action_ids(content, ids)
        .err()
        .map(|e| e.to_string())
}

/// Action IDs registered for the editor consumer.
fn registered_ids(reg: &id::IdRegistry) -> HashSet<String> {
    reg.all_entries().iter().map(|e| e.id.clone()).collect()
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
                .constraints([Constraint::Min(6), Constraint::Length(24)])
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
        .title(if focused { "> ID 表 " } else { " ID 表 " })
        .borders(Borders::ALL)
        .border_style(if focused { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() });

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
                format!("-- {} --", category),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            last_category = category;
        }

        let is_sel = focused && i == state.id_sel;
        let is_current = current_action.as_deref() == Some(e.id.as_str());
        let style = if is_sel || is_current {
            Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
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
        .borders(Borders::ALL);

    let helps = vec![
        Line::from(Span::styled("-- ttoll 语法 --", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" 0x01,ENTER>0x07", Style::default()), Span::raw("  模式规则")]),
        Line::from(vec![Span::styled(" CTRL+SPACE>0x00", Style::default()), Span::raw("  全局规则")]),
        Line::from(vec![Span::styled(" (* 注释 *)", Style::default()), Span::raw("  注释")]),
        Line::from(""),
        Line::from(Span::styled("-- 模式 --", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" 0x01", Style::default()), Span::raw("  Normal 模式")]),
        Line::from(vec![Span::styled(" 0x02", Style::default()), Span::raw("  Insert 模式")]),
        Line::from(vec![Span::styled(" 0x03", Style::default()), Span::raw("  Command 模式")]),
        Line::from(""),
        Line::from(Span::styled("-- 命令 --", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" :w/:w!", Style::default()), Span::raw("     保存/强制")]),
        Line::from(vec![Span::styled(" :wq/:q/:q!", Style::default()), Span::raw("    退出")]),
        Line::from(vec![Span::styled(" :map 01 UP 10", Style::default()), Span::raw(" 生成规则")]),
        Line::from(vec![Span::styled(" :tmpl", Style::default()), Span::raw("     插入模板")]),
        Line::from(vec![Span::styled(" :del", Style::default()), Span::raw("      删行")]),
        Line::from(vec![Span::styled(" :ids/:help/:hide ids", Style::default()), Span::raw(" 面板")]),
        Line::from(vec![Span::styled(" :s/:x/:h", Style::default()), Span::raw("    别名")]),
        Line::from(""),
        Line::from(Span::styled("-- 快捷键 (Normal) --", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" Ctrl+UP/DOWN", Style::default()), Span::raw("  首/末行  Ctrl+ENTER 保存")]),
        Line::from(vec![Span::styled("  Ctrl+ESC 保存退出", Style::default()), Span::raw("   Alt+ENTER 帮助  Alt+Space 面板")]),
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
            let mut new_spans = vec![Span::styled(line_no, Style::default())];
            if content.trim().starts_with("(*") || content.trim().is_empty() {
                new_spans.push(Span::styled(before.to_string(), Style::default()));
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
            let mut spans = vec![Span::styled(line_no, Style::default())];
            if content.trim().starts_with("(*") || content.trim().is_empty() {
                spans.push(Span::styled(content.clone(), Style::default()));
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
        spans.push(Span::styled(mode.to_string(), Style::default().add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(","));
        spans.push(Span::styled(keys.to_string(), Style::default()));
    } else {
        spans.push(Span::styled(lhs.to_string(), Style::default()));
    }

    spans.push(Span::styled(">", Style::default().add_modifier(Modifier::BOLD)));

    let action = rhs.trim();
    spans.push(Span::styled(action.to_string(), Style::default().add_modifier(Modifier::BOLD)));

    Some(spans)
}

fn draw_status_bar(f: &mut ratatui::Frame, area: Rect, path: &std::path::Path, state: &EditorState) {
    let mode_str = match state.mode {
        Mode::Normal => " NORMAL ",
        Mode::Insert => " INSERT ",
        Mode::Command => " COMMAND ",
    };

    let mod_str = if state.modified { " [+]" } else { "" };
    let file_str = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().to_string());
    let pos = format!("{}/{}:{}", state.cursor_row + 1, state.lines.len(), state.cursor_col);

    let cmd_display = if state.mode == Mode::Command {
        format!(":{}", state.cmd_buf)
    } else if !state.status_msg.is_empty() {
        state.status_msg.clone()
    } else {
        "? 帮助 | : 命令".to_string()
    };

    let mut spans = vec![
        Span::styled(mode_str, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!(" {}{}", file_str, mod_str)),
    ];

    let used = mode_str.len() + file_str.len() + mod_str.len() + cmd_display.len() + pos.len() + 4;
    let width = area.width as usize;
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }

    spans.push(Span::raw(format!(" {} ", pos)));
    spans.push(Span::styled(&cmd_display, Style::default().add_modifier(Modifier::BOLD)));

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

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn validate_accepts_registered_action_id() {
        assert!(validate_ttoll("0x01,ESC>0x02\n", &ids(&["0x01", "0x02"])).is_none());
    }

    #[test]
    fn validate_rejects_unknown_action_id() {
        let err = validate_ttoll("0x01,ESC>0xFF\n", &ids(&["0x00", "0x01"])).unwrap();
        assert!(err.contains("unknown action id '0xFF'"), "{err}");
    }

    #[test]
    fn validate_reports_syntax_line() {
        let err = validate_ttoll("0x01,ESC>0x02\n0x01,NOPE>0x02\n", &ids(&["0x02"])).unwrap();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("unknown key 'NOPE'"), "{err}");
    }

    #[test]
    fn validate_skips_id_check_when_registry_is_empty() {
        // 插件加载失败时不应阻塞用户的语法保存，但键法仍要校验。
        assert!(validate_ttoll("0x01,ESC>0xFF\n", &HashSet::new()).is_none());
        assert!(validate_ttoll("0x01,NOPE>0x02\n", &HashSet::new()).is_some());
    }

    #[test]
    fn registered_ids_cover_the_builtin_editor_registry() {
        let reg = id::IdRegistry::for_editor();
        let set = registered_ids(&reg);
        assert_eq!(set.len(), reg.all_entries().len());
        assert!(set.contains("0x00"));
        assert!(set.contains("0x01"));
    }
}
