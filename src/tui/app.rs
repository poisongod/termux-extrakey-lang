use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Clear};
use ratatui::Terminal;

use crate::lib::keys::{self, AstNode, KeyKind};
use crate::lib::keyset;
use crate::lib::behavior;
use crate::lib::id;
use crate::worker::script_runtime::ScriptHost;
use crate::tui::preview::{self, KeyCap};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ── Mode ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// 0x01 — 键盘浏览
    Normal,
    /// 0x02 — 主键编辑
    Edit,
    /// 0x03 — 弹出键编辑
    Popup,
}

impl Mode {
    fn as_hex(&self) -> &'static str {
        match self {
            Self::Normal => "0x01",
            Self::Edit => "0x02",
            Self::Popup => "0x03",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Normal => "键盘浏览",
            Self::Edit => "主键编辑",
            Self::Popup => "弹出键编辑",
        }
    }
}

// ── Edit panel ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditField {
    Key,
    Display,
    Macro,
    Popup,
}

impl EditField {
    fn label(&self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Display => "display",
            Self::Macro => "macro",
            Self::Popup => "popup",
        }
    }

    fn next(&self) -> Self {
        match self {
            Self::Key => Self::Display,
            Self::Display => Self::Macro,
            Self::Macro => Self::Popup,
            Self::Popup => Self::Key,
        }
    }

    fn prev(&self) -> Self {
        match self {
            Self::Key => Self::Popup,
            Self::Display => Self::Key,
            Self::Macro => Self::Display,
            Self::Popup => Self::Macro,
        }
    }
}

// ── App state ────────────────────────────────────────────────────────

pub struct App {
    rows: Vec<Vec<AstNode>>,
    cursor: (usize, usize),
    mode: Mode,
    selected: Option<(usize, usize)>,
    file_path: PathBuf,
    modified: bool,
    should_quit: bool,
    globals: Vec<keys::Global>,
    source: String,
    // ttoll dispatch table
    ttoll_table: HashMap<Option<String>, Vec<behavior::Rule>>,
    // ID registry (keyboard TUI specific)
    id_registry: id::IdRegistry,
    // Edit mode state
    edit_field: EditField,
    edit_buf: String,
    edit_cursor: usize,
    // Popup for insert menu
    show_insert_menu: bool,
    insert_menu_cursor: usize,
    // Status message
    status_msg: String,
    status_is_error: bool,
    // Undo/redo history (矩阵数据快照, 不含 UI 状态)
    undo_stack: Vec<Vec<Vec<AstNode>>>,
    redo_stack: Vec<Vec<Vec<AstNode>>>,
}

impl App {
    fn new(file_path: PathBuf, source: String) -> anyhow::Result<Self> {
        let result = keys::parse(&source)?;

        // Load tekl.ttoll dispatch rules
        let ttoll_path = crate::lib::user_dir::user_root().join("tekl.ttoll");
        let ttoll_source = fs::read_to_string(&ttoll_path).unwrap_or_default();
        let rules = behavior::parse_rules(&ttoll_source)
            .map_err(|e| anyhow::anyhow!("{}: {e}", ttoll_path.display()))?;
        let ttoll_table = behavior::build_table(&rules);

        // Build keyboard TUI ID registry — from built-in tui/*.rhai + user overrides
        let id_registry = id::IdRegistry::for_tui();

        Ok(Self {
            rows: result.rows,
            cursor: (0, 0),
            mode: Mode::Normal,
            selected: None,
            file_path,
            modified: false,
            should_quit: false,
            globals: result.globals,
            source,
            ttoll_table,
            id_registry,
            edit_field: EditField::Key,
            edit_buf: String::new(),
            edit_cursor: 0,
            show_insert_menu: false,
            insert_menu_cursor: 0,
            status_msg: String::new(),
            status_is_error: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    /// 修改矩阵前快照 (撤销用); 超过上限丢最旧。清空 redo。
    fn snapshot_before(&mut self) {
        self.undo_stack.push(self.rows.clone());
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.rows.clone());
            self.rows = prev;
            self.selected = None;
            self.clamp_cursor();
            self.modified = true;
            self.status_msg = "已撤销".into();
            self.status_is_error = false;
        } else {
            self.status_msg = "无可撤销".into();
            self.status_is_error = true;
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.rows.clone());
            self.rows = next;
            self.selected = None;
            self.clamp_cursor();
            self.modified = true;
            self.status_msg = "已重做".into();
            self.status_is_error = false;
        } else {
            self.status_msg = "无可重做".into();
            self.status_is_error = true;
        }
    }

    fn filename(&self) -> &str {
        self.file_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("untitled.tekl")
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn col_count(&self) -> usize {
        self.rows
            .get(self.cursor.0)
            .map(|r| r.len())
            .unwrap_or(0)
    }

    fn clamp_cursor(&mut self) {
        if self.rows.is_empty() {
            self.cursor = (0, 0);
            return;
        }
        let max_row = self.rows.len() - 1;
        if self.cursor.0 > max_row {
            self.cursor.0 = max_row;
        }
        let max_col = self.rows[self.cursor.0].len().saturating_sub(1);
        if self.cursor.1 > max_col {
            self.cursor.1 = max_col;
        }
    }

    fn current_node(&self) -> Option<&AstNode> {
        self.rows.get(self.cursor.0)?.get(self.cursor.1)
    }

    fn display_name(node: &AstNode) -> String {
        match node {
            AstNode::Key { key, kind } => match kind {
                KeyKind::Literal => format!("'{}'", key),
                _ => key.clone(),
            },
            AstNode::Combo { node, inner } => {
                format!("{}({})", Self::display_name(node), Self::display_name(inner))
            }
            AstNode::Label { node, label } => {
                if label.is_empty() {
                    Self::display_name(node)
                } else {
                    format!("{}:{}", Self::display_name(node), label)
                }
            }
            AstNode::Popup { main, popup_node } => {
                format!("{}.{}", Self::display_name(main), Self::display_name(popup_node))
            }
        }
    }

    fn enter_edit_mode(&mut self) {
        if let Some(node) = self.current_node().cloned() {
            self.mode = Mode::Edit;
            self.edit_field = EditField::Key;
            self.fill_edit_buf(&node);
            self.status_msg.clear();
        }
    }

    fn enter_popup_mode(&mut self) {
        if let Some(node) = self.current_node().cloned() {
            self.mode = Mode::Popup;
            self.edit_field = EditField::Popup;
            self.fill_edit_buf(&node);
            self.status_msg.clear();
        }
    }

    fn fill_edit_buf(&mut self, node: &AstNode) {
        match node {
            AstNode::Key { key, kind: _ } => {
                self.edit_buf = match self.edit_field {
                    EditField::Key => key.clone(),
                    EditField::Display => {
                        // Check if there's a label parent
                        String::new()
                    }
                    EditField::Macro => String::new(),
                    EditField::Popup => String::new(),
                };
            }
            AstNode::Label { node: inner, label } => {
                self.edit_buf = match self.edit_field {
                    EditField::Key => Self::display_name(inner),
                    EditField::Display => label.clone(),
                    EditField::Macro => String::new(),
                    EditField::Popup => String::new(),
                };
            }
            AstNode::Popup { main: _, popup_node } => {
                self.edit_buf = match self.edit_field {
                    EditField::Popup => Self::display_name(popup_node),
                    _ => Self::display_name(node),
                };
            }
            AstNode::Combo { .. } => {
                self.edit_buf = Self::display_name(node);
            }
        }
        self.edit_cursor = self.edit_buf.len();
    }

    fn apply_edit(&mut self) {
        let new_val = self.edit_buf.trim().to_string();
        if new_val.is_empty() {
            return;
        }
        self.snapshot_before();

        let row = &mut self.rows[self.cursor.0];
        let node = &mut row[self.cursor.1];

        match self.edit_field {
            EditField::Key => {
                let kind = if new_val.starts_with('\'') && new_val.ends_with('\'') && new_val.len() > 2 {
                    KeyKind::Literal
                } else if keyset::is_named_key(&new_val) || keyset::is_special_key(&new_val) {
                    KeyKind::Named
                } else {
                    KeyKind::Literal
                };
                let key_val = if kind == KeyKind::Literal && new_val.starts_with('\'') && new_val.ends_with('\'') {
                    new_val[1..new_val.len()-1].to_string()
                } else {
                    new_val.clone()
                };
                *node = AstNode::Key { key: key_val, kind };
            }
            EditField::Display => {
                // Wrap in Label
                let inner = node.clone();
                *node = AstNode::Label {
                    node: Box::new(inner),
                    label: new_val,
                };
            }
            EditField::Macro => {
                // For now, just store as display
                let inner = node.clone();
                *node = AstNode::Label {
                    node: Box::new(inner),
                    label: new_val,
                };
            }
            EditField::Popup => {
                // Create Popup node
                let main = node.clone();
                let parts: Vec<&str> = new_val.split('.').collect();
                let popup_key = if parts.len() > 1 { parts[1] } else { parts[0] };
                let popup_node = AstNode::Key {
                    key: popup_key.to_string(),
                    kind: KeyKind::Named,
                };
                *node = AstNode::Popup {
                    main: Box::new(main),
                    popup_node: Box::new(popup_node),
                };
            }
        }

        self.modified = true;
        self.status_msg = "已编辑".into();
        self.status_is_error = false;
    }

    fn save_file(&mut self) {
        let content = self.to_source();
        match fs::write(&self.file_path, &content) {
            Ok(()) => {
                self.modified = false;
                self.status_msg = format!("已保存: {}", self.file_path.display());
                self.status_is_error = false;
            }
            Err(e) => {
                self.status_msg = format!("保存失败: {}", e);
                self.status_is_error = true;
            }
        }
    }

    fn to_source(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for (_i, row) in self.rows.iter().enumerate() {
            let entries: Vec<String> = row.iter().map(|n| keys::print_entry(n)).collect();
            lines.push(format!("{};", entries.join(" ")));
        }
        // Add globals
        for g in &self.globals {
            lines.push(g.to_source());
        }
        lines.join("\n")
    }

    fn insert_row_after(&mut self) {
        self.snapshot_before();
        let new_row = vec![AstNode::Key {
            key: "SPACE".into(),
            kind: KeyKind::Named,
        }];
        self.rows.insert(self.cursor.0 + 1, new_row);
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        self.modified = true;
        self.status_msg = "已插入行".into();
        self.status_is_error = false;
    }

    fn insert_row_before(&mut self) {
        self.snapshot_before();
        let new_row = vec![AstNode::Key {
            key: "SPACE".into(),
            kind: KeyKind::Named,
        }];
        self.rows.insert(self.cursor.0, new_row);
        self.cursor.1 = 0;
        self.modified = true;
        self.status_msg = "已插入行".into();
        self.status_is_error = false;
    }

    fn delete_row(&mut self) {
        if self.rows.len() <= 1 {
            self.status_msg = "不能删除唯一行".into();
            self.status_is_error = true;
            return;
        }
        self.snapshot_before();
        self.rows.remove(self.cursor.0);
        self.clamp_cursor();
        self.modified = true;
        self.status_msg = "已删除行".into();
        self.status_is_error = false;
    }

    fn move_row_up(&mut self) {
        if self.cursor.0 == 0 {
            return;
        }
        self.snapshot_before();
        self.rows.swap(self.cursor.0, self.cursor.0 - 1);
        self.cursor.0 -= 1;
        self.modified = true;
    }

    fn move_row_down(&mut self) {
        if self.cursor.0 + 1 >= self.rows.len() {
            return;
        }
        self.snapshot_before();
        self.rows.swap(self.cursor.0, self.cursor.0 + 1);
        self.cursor.0 += 1;
        self.modified = true;
    }

    fn insert_col_after(&mut self) {
        self.snapshot_before();
        let new_key = AstNode::Key {
            key: "SPACE".into(),
            kind: KeyKind::Named,
        };
        let row = &mut self.rows[self.cursor.0];
        let insert_at = self.cursor.1 + 1;
        row.insert(insert_at, new_key);
        self.cursor.1 = insert_at;
        self.modified = true;
        self.status_msg = "已插入列".into();
        self.status_is_error = false;
    }

    fn insert_col_before(&mut self) {
        self.snapshot_before();
        let new_key = AstNode::Key {
            key: "SPACE".into(),
            kind: KeyKind::Named,
        };
        let row = &mut self.rows[self.cursor.0];
        row.insert(self.cursor.1, new_key);
        self.modified = true;
        self.status_msg = "已插入列".into();
        self.status_is_error = false;
    }

    fn delete_col(&mut self) {
        self.snapshot_before();
        let row = &mut self.rows[self.cursor.0];
        if row.len() <= 1 {
            self.status_msg = "不能删除唯一列".into();
            self.status_is_error = true;
            return;
        }
        row.remove(self.cursor.1);
        self.clamp_cursor();
        self.modified = true;
        self.status_msg = "已删除列".into();
        self.status_is_error = false;
    }

    fn move_col_left(&mut self) {
        if self.cursor.1 == 0 {
            return;
        }
        self.snapshot_before();
        let row = &mut self.rows[self.cursor.0];
        row.swap(self.cursor.1, self.cursor.1 - 1);
        self.cursor.1 -= 1;
        self.modified = true;
    }

    fn move_col_right(&mut self) {
        self.snapshot_before();
        let row = &mut self.rows[self.cursor.0];
        if self.cursor.1 + 1 >= row.len() {
            return;
        }
        row.swap(self.cursor.1, self.cursor.1 + 1);
        self.cursor.1 += 1;
        self.modified = true;
    }

    fn swap_with_selected(&mut self) {
        if let Some(sel) = self.selected {
            if sel != self.cursor {
                self.snapshot_before();
                let sr = sel.0;
                let sc = sel.1;
                let cr = self.cursor.0;
                let cc = self.cursor.1;

                // Swap within same row
                if sr == cr {
                    let row = &mut self.rows[sr];
                    row.swap(sc, cc);
                } else {
                    // Swap across rows: exchange the nodes
                    let node_a = self.rows[sr][sc].clone();
                    let node_b = self.rows[cr][cc].clone();
                    self.rows[sr][sc] = node_b;
                    self.rows[cr][cc] = node_a;
                }
                self.selected = None;
                self.modified = true;
                self.status_msg = "已交换".into();
                self.status_is_error = false;
            }
        }
    }

    fn delete_key(&mut self) {
        self.snapshot_before();
        let row = &mut self.rows[self.cursor.0];
        if row.len() <= 1 {
            self.status_msg = "不能删除唯一键".into();
            self.status_is_error = true;
            return;
        }
        row.remove(self.cursor.1);
        self.clamp_cursor();
        self.modified = true;
        self.status_msg = "已删除键".into();
        self.status_is_error = false;
    }

    fn add_key_after(&mut self) {
        self.snapshot_before();
        let new_key = AstNode::Key {
            key: "a".into(),
            kind: KeyKind::Literal,
        };
        let row = &mut self.rows[self.cursor.0];
        let insert_at = self.cursor.1 + 1;
        row.insert(insert_at, new_key);
        self.cursor.1 = insert_at;
        self.modified = true;
        self.status_msg = "已插入键".into();
        self.status_is_error = false;
    }
}

// ── Event handling ───────────────────────────────────────────────────

fn handle_event(ctx: &TuiCtx, key: KeyEvent) {
    // 系统级快捷 — 输入层硬编码, 不入 ttoll 文法 (Ctrl+C/S 字符和弦不在 K∪D)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => {
                ctx.app.lock().unwrap().should_quit = true;
                return;
            }
            KeyCode::Char('s') => {
                ctx.app.lock().unwrap().save_file();
                return;
            }
            _ => {}
        }
    }

    // 阶段一: 短暂持锁完成路由决策, 产出待执行动作
    let pending = {
        let mut app = ctx.app.lock().unwrap();
        handle_event_inner(&mut app, key)
    };
    // 阶段二: 无锁状态下直接执行 Rhai 动作 (引擎内部自行加锁)
    if let Some(action_id) = pending {
        execute_action(ctx, &action_id);
    }
}

fn handle_event_inner(app: &mut App, key: KeyEvent) -> Option<String> {
    // Insert menu captures all input
    if app.show_insert_menu {
        return handle_insert_menu(app, key);
    }

    // Edit/Popup mode: ESC and Enter go through ttoll dispatch, text input is internal
    if app.mode == Mode::Edit || app.mode == Mode::Popup {
        // ESC and Enter → ttoll dispatch
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
            let key_name = crossterm_to_key_name(key);
            if let Some(key_name) = key_name {
                let current_mode = app.mode.as_hex().to_string();
                if let Some(action_id) = behavior::dispatch(&app.ttoll_table, &current_mode, &key_name) {
                    return Some(action_id);
                }
            }
        }
        // Chord dispatch (e.g., CTRL+C in edit mode)
        if let Some(chord) = crossterm_to_chord(key) {
            let current_mode = app.mode.as_hex().to_string();
            if let Some(action_id) = behavior::dispatch_chord(&app.ttoll_table, &current_mode, &chord) {
                return Some(action_id);
            }
        }
        // Text input is internal
        handle_edit(app, key);
        return None;
    }

    // Normal mode: full ttoll dispatch
    // Try chord dispatch first (e.g., CTRL+C, CTRL+S)
    if let Some(chord) = crossterm_to_chord(key) {
        let current_mode = app.mode.as_hex().to_string();
        if let Some(action_id) = behavior::dispatch_chord(&app.ttoll_table, &current_mode, &chord) {
            return Some(action_id);
        }
    }

    // Single key dispatch
    let key_name = crossterm_to_key_name(key);
    if let Some(key_name) = key_name {
        let current_mode = app.mode.as_hex().to_string();
        if let Some(action_id) = behavior::dispatch(&app.ttoll_table, &current_mode, &key_name) {
            return Some(action_id);
        }
    }
    None
}

/// Convert crossterm KeyEvent to DSL key name for ttoll dispatch.
fn crossterm_to_key_name(key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Esc => Some("ESC".into()),
        KeyCode::Enter => Some("ENTER".into()),
        KeyCode::Char(' ') => Some("SPACE".into()),
        KeyCode::Up => Some("UP".into()),
        KeyCode::Down => Some("DOWN".into()),
        KeyCode::Left => Some("LEFT".into()),
        KeyCode::Right => Some("RIGHT".into()),
        KeyCode::Char(c) => {
            let name = c.to_uppercase().to_string();
            if !behavior::is_known_key(&name) {
                return None;
            }
            if ctrl { Some(format!("CTRL+{}", name)) }
            else if alt { Some(format!("ALT+{}", name)) }
            else { Some(name) }
        }
        _ => None,
    }
}

/// Extract chord key set from a crossterm KeyEvent (e.g., CTRL+SPACE → ["CTRL", "SPACE"]).
/// 只接受文法键 (K ∪ D)；`CTRL+C` 这类字符和弦不产生候选项。
fn crossterm_to_chord(key: KeyEvent) -> Option<Vec<String>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    if !ctrl && !alt {
        return None;
    }

    let mut chord = Vec::new();
    if ctrl { chord.push("CTRL".to_string()); }
    if alt { chord.push("ALT".to_string()); }

    let tail: String = match key.code {
        KeyCode::Char(c) => c.to_uppercase().to_string(),
        KeyCode::Esc => "ESC".to_string(),
        KeyCode::Enter => "ENTER".to_string(),
        KeyCode::Up => "UP".to_string(),
        KeyCode::Down => "DOWN".to_string(),
        KeyCode::Left => "LEFT".to_string(),
        KeyCode::Right => "RIGHT".to_string(),
        _ => return None,
    };
    if !behavior::is_known_key(&tail) {
        return None;
    }
    chord.push(tail);
    Some(chord)
}

/// Execute a built-in action by ID.
///
/// ID registry (from id.txt):
///   0x00-0x04  系统动作
///   0x10-0x13  焦点移动
///   0x20-0x27  行列增删
///   0x30-0x31  选中与交换
///   0x40-0x42  主键编辑
///   0x50-0x53  弹出键编辑
///
/// 无硬编码：ID → run 闭包由注册池提供（.rhai），此处直接执行，
/// 插件通过引擎内注册的原语读写 TUI 状态。
fn execute_action(ctx: &TuiCtx, action_id: &str) {
    if !ctx.registry.is_registered(action_id) {
        let mut app = ctx.app.lock().unwrap();
        app.status_msg = format!("未知 ID: {}", action_id);
        app.status_is_error = true;
        return;
    }

    let run = match ctx.registry.action_fn(action_id) {
        Some(r) => r.clone(),
        None => return,
    };

    if let Err(e) = ctx.host.call(&run) {
        let mut app = ctx.app.lock().unwrap();
        app.status_msg = format!("action 错误: {e}");
        app.status_is_error = true;
    }
}

/// 插入菜单 — 导航是输入层 (Enter 选中项则映射为 action ID, 交 dispatch→rhai 执行)。
fn handle_insert_menu(app: &mut App, key: KeyEvent) -> Option<String> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.show_insert_menu = false;
            None
        }
        KeyCode::Up => {
            if app.insert_menu_cursor > 0 {
                app.insert_menu_cursor -= 1;
            }
            None
        }
        KeyCode::Down => {
            if app.insert_menu_cursor < 5 {
                app.insert_menu_cursor += 1;
            }
            None
        }
        KeyCode::Enter => {
            app.show_insert_menu = false;
            match app.insert_menu_cursor {
                0 => Some("0x21".into()), // 行后增行
                1 => Some("0x20".into()), // 行前增行
                2 => Some("0x23".into()), // 列后增列
                3 => Some("0x22".into()), // 列前增列
                4 => Some("0x25".into()), // 删除当前键列
                _ => None,
            }
        }
        _ => None,
    }
}

fn handle_edit(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Tab => {
            app.edit_field = app.edit_field.next();
            if let Some(node) = app.current_node().cloned() {
                app.fill_edit_buf(&node);
            }
        }
        KeyCode::BackTab => {
            app.edit_field = app.edit_field.prev();
            if let Some(node) = app.current_node().cloned() {
                app.fill_edit_buf(&node);
            }
        }
        KeyCode::Left => {
            if app.edit_cursor > 0 {
                app.edit_cursor -= 1;
            }
        }
        KeyCode::Right => {
            if app.edit_cursor < app.edit_buf.len() {
                app.edit_cursor += 1;
            }
        }
        KeyCode::Home => {
            app.edit_cursor = 0;
        }
        KeyCode::End => {
            app.edit_cursor = app.edit_buf.len();
        }
        KeyCode::Backspace => {
            if app.edit_cursor > 0 {
                app.edit_cursor -= 1;
                app.edit_buf.remove(app.edit_cursor);
            }
        }
        KeyCode::Delete => {
            if app.edit_cursor < app.edit_buf.len() {
                app.edit_buf.remove(app.edit_cursor);
            }
        }
        KeyCode::Char(c) => {
            app.edit_buf.insert(app.edit_cursor, c);
            app.edit_cursor += 1;
        }
        _ => {}
    }
}

// ── Rendering ────────────────────────────────────────────────────────

fn render_frame(frame: &mut ratatui::Frame, app: &App) {
    let size = frame.area();

    // 预览高度动态贴合键帽总高(陪边框), 不预留空白区
    let avail_h = size.height.saturating_sub(2);
    let n_rows = app.rows.len() as u16;
    let preview_h = if app.rows.is_empty() {
        4
    } else {
        let cap_h = if avail_h >= n_rows * 3 + 2 {
            3
        } else if avail_h >= n_rows * 2 + 2 {
            2
        } else {
            1
        };
        (n_rows * cap_h + 2).min(avail_h)
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Length(preview_h), // keyboard preview
            Constraint::Length(1), // hints
        ])
        .split(size);

    render_status_bar(frame, app, main_chunks[0]);
    render_preview(frame, app, main_chunks[1]);
    render_hints(frame, app, main_chunks[2]);

    // Overlay edit panel
    if app.mode != Mode::Normal {
        let popup_area = centered_rect(60, 40, size);
        frame.render_widget(Clear, popup_area);
        render_edit_panel(frame, app, popup_area);
    }

    // Overlay insert menu
    if app.show_insert_menu {
        let popup_area = centered_rect(30, 40, size);
        frame.render_widget(Clear, popup_area);
        render_insert_menu(frame, app, popup_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_status_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let modified = if app.modified { " [+]" } else { "" };
    let filename = app.filename();

    let mut spans = vec![
        Span::styled(
            format!(" {} {} ", app.mode.as_hex(), app.mode.label()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" | {} {}", filename, modified)),
    ];

    if !app.status_msg.is_empty() {
        let prefix = if app.status_is_error { " ! " } else { " " };
        let style = if app.status_is_error {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(format!(" | {} {} ", prefix, app.status_msg), style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_preview(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    if app.rows.is_empty() {
        let block = Block::default().title(" 键盘 ").borders(Borders::ALL);

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("空布局").alignment(ratatui::layout::Alignment::Center),
            inner,
        );
        return;
    }

    let total_rows = app.rows.len();
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                format!(" 键盘 {}/{}", app.cursor.0 + 1, total_rows),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let rows: Vec<Vec<KeyCap>> = app
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            row.iter()
                .enumerate()
                .map(|(c, node)| {
                    // 预览标签按模式只看一边: 0x01/0x02 主键值, 0x03 弹出键值
                    let name = match node {
                        AstNode::Popup { main, popup_node } => match app.mode {
                            Mode::Popup => App::display_name(popup_node.as_ref()),
                            _ => App::display_name(main),
                        },
                        _ => App::display_name(node),
                    };
                    let is_cursor = app.cursor == (r, c);
                    let is_selected = app.selected == Some((r, c));
                    node_keycap(&name, node, is_cursor, is_selected, app.mode)
                })
                .collect()
        })
        .collect();

    preview::render_keyboard(&rows, inner, frame.buffer_mut());
}

/// 按节点语义给定键帽配色 — 黑白灰 + 反白高亮, 无彩色。
fn node_keycap(name: &str, node: &AstNode, is_cursor: bool, is_selected: bool, mode: Mode) -> KeyCap {
    let _ = mode;
    let _ = node;
    let pal: (Color, Color, Color, Color) = if is_cursor {
        (Color::White, Color::White, Color::White, Color::Black)
    } else if is_selected {
        (Color::White, Color::White, Color::White, Color::Black)
    } else {
        (Color::Reset, Color::Reset, Color::Reset, Color::Reset)
    };
    let (top, body, dark, text) = pal;
    let mut cap = KeyCap::new(name, top, body, dark, text);
    cap.bold = is_cursor;
    cap
}

fn render_hints(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let hints = if app.show_insert_menu {
        vec![
            ("↑↓", "选择"),
            ("Enter", "确认"),
            ("Esc", "取消"),
        ]
    } else {
        match app.mode {
            Mode::Normal => vec![
                ("↑↓←→", "移动"),
                ("Enter", "编辑"),
                ("Space", "选中"),
                ("Ctrl+Space", "菜单"),
                ("Ctrl+S", "保存"),
                ("Esc", "退出"),
            ],
            Mode::Edit | Mode::Popup => vec![
                ("Enter", "确认"),
                ("Esc", "取消"),
            ],
        }
    };

    let mut spans = vec![Span::raw(" ")];
    for (key, label) in &hints {
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!("{} ", label), Style::default()));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_edit_panel(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let title = if app.mode == Mode::Edit {
        " 编辑主键 "
    } else {
        " 编辑弹出键 "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Show DSL input line
    let current_dsl = app.current_node().map(|n| App::display_name(n)).unwrap_or_default();

    // Current value
    let y0 = inner.y;
    if y0 < inner.y + inner.height {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" 现值 ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&current_dsl),
            ])),
            Rect { x: inner.x, y: y0, width: inner.width, height: 1 },
        );
    }

    // Edit input
    let y1 = y0 + 1;
    if y1 < inner.y + inner.height {
        let cursor_char = "|";
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" 输入 ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&app.edit_buf),
                Span::styled(cursor_char, Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)),
            ])),
            Rect { x: inner.x, y: y1, width: inner.width, height: 1 },
        );
    }

    // Syntax hint
    let y2 = y1 + 2;
    if y2 < inner.y + inner.height {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" 语法: A | A:B | A(B) | A.B"),
            ])),
            Rect { x: inner.x, y: y2, width: inner.width, height: 1 },
        );
    }
}

fn render_insert_menu(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(" 插入 ", Style::default().add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items = [
        ("+", "行后"),
        ("+", "行前"),
        ("+", "列后"),
        ("+", "列前"),
        ("x", "删除"),
    ];

    for (i, (icon, label)) in items.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let is_selected = app.insert_menu_cursor == i;
        let (fg, bg) = if is_selected {
            (Color::Black, Color::White)
        } else {
            (Color::Reset, Color::Reset)
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", if is_selected { ">" } else { " " }), Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{} ", icon), Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)),
                Span::styled(label.to_string(), Style::default().fg(fg).bg(bg)),
            ])),
            Rect { x: inner.x, y, width: inner.width, height: 1 },
        );
    }
}

// ── Public entry point ───────────────────────────────────────────────

/// TUI 运行上下文: 共享状态 + 直接执行引擎 + 注册池。
struct TuiCtx {
    app: Arc<Mutex<App>>,
    host: ScriptHost,
    registry: id::IdRegistry,
}

pub fn run() -> anyhow::Result<()> {
    let file_path = crate::lib::user_dir::default_layout_file();
    let source = if file_path.exists() {
        fs::read_to_string(&file_path)?
    } else {
        String::new()
    };

    let app = App::new(file_path, source)?;
    run_app(app)
}

/// Launch TUI with a specific .tekl file.
pub fn run_with_file(path: &str) -> anyhow::Result<()> {
    let file_path = std::path::PathBuf::from(path);
    let source = if file_path.exists() {
        fs::read_to_string(&file_path)?
    } else {
        String::new()
    };

    let app = App::new(file_path, source)?;
    run_app(app)
}

/// 构建 TUI 引擎 — 细粒度原语捕获状态句柄，插件逻辑直接读写状态。
fn tui_engine(handle: Arc<Mutex<App>>) -> ScriptHost {
    let mut engine = rhai::Engine::new();
    engine.register_type_with_name::<Arc<Mutex<App>>>("Tui");

    macro_rules! prim0 {
        ($name:literal, $f:expr) => {{
            let h = handle.clone();
            let g = move |h: Arc<Mutex<App>>| $f(h);
            engine.register_fn($name, move || g(h.clone()));
        }};
    }
    macro_rules! prim1s {
        ($name:literal, $arg:ident, $f:expr) => {{
            let h = handle.clone();
            let g = move |h: Arc<Mutex<App>>, $arg: &str| $f(h, $arg);
            engine.register_fn($name, move |$arg: &str| g(h.clone(), $arg));
        }};
    }

    // ── 读原语 (让 .rhai 能写守卫与组合) ──
    prim0!("cursor_row", |h: Arc<Mutex<App>>| h.lock().unwrap().cursor.0 as i64);
    prim0!("cursor_col", |h: Arc<Mutex<App>>| h.lock().unwrap().cursor.1 as i64);
    prim0!("row_count", |h: Arc<Mutex<App>>| h.lock().unwrap().row_count() as i64);
    prim0!("col_count", |h: Arc<Mutex<App>>| h.lock().unwrap().col_count() as i64);
    prim0!("selected_row", |h: Arc<Mutex<App>>| h
        .lock()
        .unwrap()
        .selected
        .map(|s| s.0 as i64)
        .unwrap_or(-1));
    prim0!("selected_col", |h: Arc<Mutex<App>>| h
        .lock()
        .unwrap()
        .selected
        .map(|s| s.1 as i64)
        .unwrap_or(-1));

    // ── 写原语 ──
    {
        let h = handle.clone();
        engine.register_fn("set_cursor", move |r: i64, c: i64| {
            let mut a = h.lock().unwrap();
            a.cursor = (r.max(0) as usize, c.max(0) as usize);
            a.clamp_cursor();
        });
    }
    {
        let h = handle.clone();
        engine.register_fn("set_selected", move |r: i64, c: i64| {
            let mut a = h.lock().unwrap();
            a.selected = Some((r.max(0) as usize, c.max(0) as usize));
        });
    }
    prim0!("clear_selection", |h: Arc<Mutex<App>>| h.lock().unwrap().selected = None);

    // ── 系统与模式 ──
    prim0!("exit", |h: Arc<Mutex<App>>| h.lock().unwrap().should_quit = true);
    prim1s!("set_mode", n, |h: Arc<Mutex<App>>, n: &str| {
        let mut a = h.lock().unwrap();
        match n {
            "0x01" => {
                a.mode = Mode::Normal;
                a.status_msg.clear();
            }
            "0x02" => a.enter_edit_mode(),
            "0x03" => a.enter_popup_mode(),
            _ => {}
        }
    });
    // 数字形式 (兼容 set_mode(0x02))
    {
        let h = handle.clone();
        engine.register_fn("set_mode", move |n: i64| {
            let mut a = h.lock().unwrap();
            match n {
                0x01 => {
                    a.mode = Mode::Normal;
                    a.status_msg.clear();
                }
                0x02 => a.enter_edit_mode(),
                0x03 => a.enter_popup_mode(),
                _ => {}
            }
        });
    }

    // ── 粗粒度便捷原语 (仅保留原子数据操作; 策略/守卫在 rhai 组合) ──
    prim1s!("add_row", p, |h: Arc<Mutex<App>>, p: &str| {
        let mut a = h.lock().unwrap();
        match p {
            "before" => a.insert_row_before(),
            _ => a.insert_row_after(),
        }
    });
    prim1s!("add_col", p, |h: Arc<Mutex<App>>, p: &str| {
        let mut a = h.lock().unwrap();
        match p {
            "before" => a.insert_col_before(),
            _ => a.insert_col_after(),
        }
    });
    prim0!("del_row", |h: Arc<Mutex<App>>| h.lock().unwrap().delete_row());
    prim0!("del_col", |h: Arc<Mutex<App>>| h.lock().unwrap().delete_col());
    prim0!("swap_row", |h: Arc<Mutex<App>>| h.lock().unwrap().move_row_up());
    prim0!("swap_col", |h: Arc<Mutex<App>>| h.lock().unwrap().move_col_left());
    prim0!("swap_key", |h: Arc<Mutex<App>>| h.lock().unwrap().swap_with_selected());
    prim1s!("show_matrix", k, |h: Arc<Mutex<App>>, _k: &str| {
        let mut a = h.lock().unwrap();
        a.show_insert_menu = true;
        a.insert_menu_cursor = 0;
    });
    prim1s!("input_value", k, |h: Arc<Mutex<App>>, k: &str| {
        let mut a = h.lock().unwrap();
        match k {
            "key" => a.save_file(),
            _ => a.delete_key(),
        }
    });
    prim1s!("input_name", k, |h: Arc<Mutex<App>>, _k: &str| {
        let mut a = h.lock().unwrap();
        a.add_key_after()
    });
    prim0!("apply_edit", |h: Arc<Mutex<App>>| h.lock().unwrap().apply_edit());
    prim0!("save_file", |h: Arc<Mutex<App>>| h.lock().unwrap().save_file());
    prim0!("undo", |h: Arc<Mutex<App>>| h.lock().unwrap().undo());
    prim0!("redo", |h: Arc<Mutex<App>>| h.lock().unwrap().redo());

    // 按坐标改写键文本 (dsl 语义同主键编辑的 Key 字段); 负值 = 错误码
    {
        let h = handle.clone();
        engine.register_fn("set_key", move |r: i64, c: i64, dsl: &str| -> i64 {
            let mut a = h.lock().unwrap();
            if r < 0 || c < 0 {
                return -1;
            }
            let row_len = a.rows.get(r as usize).map(|row| row.len()).unwrap_or(0);
            if c as usize >= row_len {
                return -2;
            }
            let new_val = dsl.trim().to_string();
            if new_val.is_empty() {
                return -3;
            }
            let kind = if new_val.starts_with('\'') && new_val.ends_with('\'') && new_val.len() > 2 {
                KeyKind::Literal
            } else if keyset::is_named_key(&new_val) || keyset::is_special_key(&new_val) {
                KeyKind::Named
            } else {
                KeyKind::Literal
            };
            let key_val = if kind == KeyKind::Literal && new_val.starts_with('\'') && new_val.ends_with('\'') {
                new_val[1..new_val.len() - 1].to_string()
            } else {
                new_val.clone()
            };
            a.snapshot_before();
            let row = &mut a.rows[r as usize];
            row[c as usize] = AstNode::Key { key: key_val, kind };
            a.modified = true;
            1
        });
    }

    // 读取键数据 — 让插件能按坐标读取当前键 (守卫/逻辑可直接用, 不再只靠光标)
    {
        let h = handle.clone();
        engine.register_fn("key_text", move |r: i64, c: i64| {
            let a = h.lock().unwrap();
            if r < 0 || c < 0 {
                return String::new();
            }
            match a.rows.get(r as usize).and_then(|row| row.get(c as usize)) {
                Some(n) => App::display_name(n),
                None => String::new(),
            }
        });
    }

    ScriptHost { engine, lib: rhai::AST::empty() }
}

fn run_app(mut app: App) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let registry = app.id_registry.clone();
    let lib = std::mem::take(&mut app.id_registry.lib);
    let shared = Arc::new(Mutex::new(app));
    let host = ScriptHost { engine: tui_engine(shared.clone()).engine, lib };
    let ctx = TuiCtx { app: shared, host, registry };

    let tick_rate = Duration::from_millis(50);
    loop {
        {
            let g = ctx.app.lock().unwrap();
            terminal.draw(|f| render_frame(f, &g))?;
        }

        if ctx.app.lock().unwrap().should_quit {
            break;
        }

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                handle_event(&ctx, key);
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_hex_labels() {
        assert_eq!(Mode::Normal.as_hex(), "0x01");
        assert_eq!(Mode::Edit.as_hex(), "0x02");
        assert_eq!(Mode::Popup.as_hex(), "0x03");
        assert_eq!(Mode::Normal.label(), "键盘浏览");
        assert_eq!(Mode::Edit.label(), "主键编辑");
        assert_eq!(Mode::Popup.label(), "弹出键编辑");
    }

    #[test]
    fn edit_field_cycle() {
        assert_eq!(EditField::Key.next(), EditField::Display);
        assert_eq!(EditField::Display.next(), EditField::Macro);
        assert_eq!(EditField::Macro.next(), EditField::Popup);
        assert_eq!(EditField::Popup.next(), EditField::Key);
        assert_eq!(EditField::Key.prev(), EditField::Popup);
        assert_eq!(EditField::Popup.prev(), EditField::Macro);
    }

    #[test]
    fn app_new_parses_source() {
        let source = "ESC; a b c;".to_string();
        let app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.cursor, (0, 0));
        assert!(!app.modified);
        assert!(!app.should_quit);
    }

    #[test]
    fn app_display_name() {
        let key = AstNode::Key { key: "ESC".into(), kind: KeyKind::Named };
        assert_eq!(App::display_name(&key), "ESC");

        let lit = AstNode::Key { key: "a".into(), kind: KeyKind::Literal };
        assert_eq!(App::display_name(&lit), "'a'");

        let combo = AstNode::Combo {
            node: Box::new(AstNode::Key { key: "CTRL".into(), kind: KeyKind::Special }),
            inner: Box::new(AstNode::Key { key: "a".into(), kind: KeyKind::Literal }),
        };
        assert_eq!(App::display_name(&combo), "CTRL('a')");

        let label = AstNode::Label {
            node: Box::new(AstNode::Key { key: "a".into(), kind: KeyKind::Literal }),
            label: "test".into(),
        };
        assert_eq!(App::display_name(&label), "'a':test");

        let popup = AstNode::Popup {
            main: Box::new(AstNode::Key { key: "a".into(), kind: KeyKind::Literal }),
            popup_node: Box::new(AstNode::Key { key: "b".into(), kind: KeyKind::Named }),
        };
        assert_eq!(App::display_name(&popup), "'a'.b");
    }

    #[test]
    fn app_to_source_roundtrip() {
        let source = "ESC; a b;".to_string();
        let app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        let output = app.to_source();
        assert!(output.contains("ESC"));
        assert!(output.contains("a b"));
    }

    #[test]
    fn app_to_source_preserves_globals() {
        let source = "height = 40\ntheme = dark\nESC;".to_string();
        let app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        let output = app.to_source();

        assert!(output.contains("height = 40"));
        assert!(output.contains("theme = dark"));
        assert!(!output.contains("G height"));

        let reparsed = keys::parse(&output).unwrap();
        assert_eq!(reparsed.rows.len(), 1);
        assert_eq!(reparsed.globals.len(), 2);
    }

    #[test]
    fn app_insert_delete_row() {
        let source = "ESC; a b;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        assert_eq!(app.rows.len(), 2);
        app.insert_row_after();
        assert_eq!(app.rows.len(), 3);
        assert!(app.modified);
        app.delete_row();
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn app_insert_delete_col() {
        let source = "a b c;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        assert_eq!(app.rows[0].len(), 3);
        app.insert_col_after();
        assert_eq!(app.rows[0].len(), 4);
        app.delete_col();
        assert_eq!(app.rows[0].len(), 3);
    }

    #[test]
    fn app_move_row() {
        let source = "a; b;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        assert_eq!(App::display_name(&app.rows[0][0]), "a");
        assert_eq!(App::display_name(&app.rows[1][0]), "b");
        app.move_row_down();
        assert_eq!(App::display_name(&app.rows[0][0]), "b");
        assert_eq!(App::display_name(&app.rows[1][0]), "a");
        app.move_row_up();
        assert_eq!(App::display_name(&app.rows[0][0]), "a");
        assert_eq!(App::display_name(&app.rows[1][0]), "b");
    }

    #[test]
    fn app_move_col() {
        let source = "a b;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        assert_eq!(App::display_name(&app.rows[0][0]), "a");
        assert_eq!(App::display_name(&app.rows[0][1]), "b");
        app.move_col_right();
        assert_eq!(App::display_name(&app.rows[0][0]), "b");
        assert_eq!(App::display_name(&app.rows[0][1]), "a");
        app.move_col_left();
        assert_eq!(App::display_name(&app.rows[0][0]), "a");
        assert_eq!(App::display_name(&app.rows[0][1]), "b");
    }

    #[test]
    fn app_select_and_swap() {
        let source = "a b;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        app.selected = Some((0, 0));
        app.cursor = (0, 1);
        app.swap_with_selected();
        assert_eq!(App::display_name(&app.rows[0][0]), "b");
        assert_eq!(App::display_name(&app.rows[0][1]), "a");
        assert!(app.selected.is_none());
    }

    #[test]
    fn app_enter_edit_mode() {
        let source = "a;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        app.enter_edit_mode();
        assert_eq!(app.mode, Mode::Edit);
        assert_eq!(app.edit_field, EditField::Key);
    }

    #[test]
    fn app_enter_popup_mode() {
        let source = "a;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        app.enter_popup_mode();
        assert_eq!(app.mode, Mode::Popup);
        assert_eq!(app.edit_field, EditField::Popup);
    }

    #[test]
    fn app_apply_edit_key() {
        let source = "a;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        app.enter_edit_mode();
        app.edit_buf = "ESC".into();
        app.edit_cursor = 3;
        app.apply_edit();
        app.mode = Mode::Normal; // Simulate the mode change that handle_edit does
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(App::display_name(&app.rows[0][0]), "ESC");
        assert!(app.modified);
    }

    #[test]
    fn app_undo_redo_row() {
        let source = "a; b;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        app.insert_row_after();
        assert_eq!(app.rows.len(), 3);
        app.undo();
        assert_eq!(app.rows.len(), 2, "undo 应恢复 2 行");
        app.redo();
        assert_eq!(app.rows.len(), 3, "redo 应重新插入行");
    }

    #[test]
    fn app_undo_redo_key_text() {
        let source = "a b;".to_string();
        let mut app = App::new(PathBuf::from("test.tekl"), source).unwrap();
        app.snapshot_before();
        app.rows[0][0] = AstNode::Key { key: "ESC".into(), kind: KeyKind::Named };
        app.undo();
        assert_eq!(App::display_name(&app.rows[0][0]), "a");
        app.redo();
        assert_eq!(App::display_name(&app.rows[0][0]), "ESC");
    }
}
