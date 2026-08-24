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

use crate::lib::keys::{self, AstNode, KeyKind, ParseResult};
use crate::lib::keyset;
use crate::lib::behavior;
use crate::lib::id;
use crate::worker::script_runtime;
use std::collections::HashMap;

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

    fn color(&self) -> Color {
        match self {
            Self::Normal => Color::Green,
            Self::Edit => Color::Yellow,
            Self::Popup => Color::Cyan,
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
}

impl App {
    fn new(file_path: PathBuf, source: String) -> anyhow::Result<Self> {
        let result = keys::parse(&source)?;

        // Load tekl.ttoll dispatch rules
        let ttoll_path = crate::lib::user_dir::user_root().join("tekl.ttoll");
        let ttoll_source = fs::read_to_string(&ttoll_path).unwrap_or_default();
        let rules = behavior::parse_rules(&ttoll_source);
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
        })
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
            AstNode::Key { key, kind } => {
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
        for (i, row) in self.rows.iter().enumerate() {
            let entries: Vec<String> = row.iter().map(|n| keys::print_entry(n)).collect();
            lines.push(format!("{};", entries.join(" ")));
        }
        // Add globals
        for g in &self.globals {
            match &g.value {
                keys::GlobalValue::Number(n) => lines.push(format!("G {} = {};", g.key, n)),
                keys::GlobalValue::Ident(s) => lines.push(format!("G {} = {};", g.key, s)),
                keys::GlobalValue::Bool(b) => lines.push(format!("G {} = {};", g.key, b)),
            }
        }
        lines.join("\n")
    }

    fn insert_row_after(&mut self) {
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
        self.rows.swap(self.cursor.0, self.cursor.0 - 1);
        self.cursor.0 -= 1;
        self.modified = true;
    }

    fn move_row_down(&mut self) {
        if self.cursor.0 + 1 >= self.rows.len() {
            return;
        }
        self.rows.swap(self.cursor.0, self.cursor.0 + 1);
        self.cursor.0 += 1;
        self.modified = true;
    }

    fn insert_col_after(&mut self) {
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
        let row = &mut self.rows[self.cursor.0];
        row.swap(self.cursor.1, self.cursor.1 - 1);
        self.cursor.1 -= 1;
        self.modified = true;
    }

    fn move_col_right(&mut self) {
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

fn handle_event(app: &mut App, key: KeyEvent) {
    // Insert menu captures all input
    if app.show_insert_menu {
        handle_insert_menu(app, key);
        return;
    }

    // Edit/Popup mode: ESC and Enter go through ttoll dispatch, text input is internal
    if app.mode == Mode::Edit || app.mode == Mode::Popup {
        // ESC and Enter → ttoll dispatch
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
            let key_name = crossterm_to_key_name(key);
            if let Some(key_name) = key_name {
                let current_mode = app.mode.as_hex().to_string();
                if let Some(action_id) = behavior::dispatch(&app.ttoll_table, &current_mode, &key_name) {
                    execute_action(app, &action_id);
                    return;
                }
            }
        }
        // Chord dispatch (e.g., CTRL+C in edit mode)
        if let Some(chord) = crossterm_to_chord(key) {
            let current_mode = app.mode.as_hex().to_string();
            if let Some(action_id) = behavior::dispatch_chord(&app.ttoll_table, &current_mode, &chord) {
                execute_action(app, &action_id);
                return;
            }
        }
        // Text input is internal
        handle_edit(app, key);
        return;
    }

    // Normal mode: full ttoll dispatch
    // Try chord dispatch first (e.g., CTRL+C, CTRL+S)
    if let Some(chord) = crossterm_to_chord(key) {
        let current_mode = app.mode.as_hex().to_string();
        if let Some(action_id) = behavior::dispatch_chord(&app.ttoll_table, &current_mode, &chord) {
            execute_action(app, &action_id);
            return;
        }
    }

    // Single key dispatch
    let key_name = crossterm_to_key_name(key);
    if let Some(key_name) = key_name {
        let current_mode = app.mode.as_hex().to_string();
        if let Some(action_id) = behavior::dispatch(&app.ttoll_table, &current_mode, &key_name) {
            execute_action(app, &action_id);
        }
    }
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
            if ctrl { Some(format!("CTRL+{}", name)) }
            else if alt { Some(format!("ALT+{}", name)) }
            else { Some(name) }
        }
        _ => None,
    }
}

/// Extract chord key set from a crossterm KeyEvent (e.g., CTRL+C → ["CTRL", "C"]).
fn crossterm_to_chord(key: KeyEvent) -> Option<Vec<String>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    if !ctrl && !alt {
        return None;
    }

    let mut chord = Vec::new();
    if ctrl { chord.push("CTRL".to_string()); }
    if alt { chord.push("ALT".to_string()); }

    match key.code {
        KeyCode::Char(c) => {
            chord.push(c.to_uppercase().to_string());
            Some(chord)
        }
        KeyCode::Esc => {
            chord.push("ESC".to_string());
            Some(chord)
        }
        KeyCode::Enter => {
            chord.push("ENTER".to_string());
            Some(chord)
        }
        KeyCode::Up => {
            chord.push("UP".to_string());
            Some(chord)
        }
        KeyCode::Down => {
            chord.push("DOWN".to_string());
            Some(chord)
        }
        KeyCode::Left => {
            chord.push("LEFT".to_string());
            Some(chord)
        }
        KeyCode::Right => {
            chord.push("RIGHT".to_string());
            Some(chord)
        }
        _ => None,
    }
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
/// 无硬编码：ID → action 由注册池提供（Rhai），此处仅回放宿主操作。
fn execute_action(app: &mut App, action_id: &str) {
    if !app.id_registry.is_registered(action_id) {
        app.status_msg = format!("未知 ID: {}", action_id);
        app.status_is_error = true;
        return;
    }

    let ops = match app.id_registry.eval_action(action_id) {
        Ok(ops) => ops,
        Err(e) => {
            app.status_msg = format!("action 错误: {e}");
            app.status_is_error = true;
            return;
        }
    };

    for op in &ops {
        apply_host_op(app, op);
    }
}

/// 宿主操作回放 — HostOp → App 状态变更。
fn apply_host_op(app: &mut App, op: &script_runtime::HostOp) {
    use script_runtime::HostOp;
    match op {
        // ── 系统 ──
        HostOp::Exit => { app.should_quit = true; }
        HostOp::SetMode(n) => match *n {
            0x01 => { app.mode = Mode::Normal; app.status_msg.clear(); }
            0x02 => { app.enter_edit_mode(); }
            0x03 => { app.enter_popup_mode(); }
            _ => {}
        },
        // ── 焦点 ──
        HostOp::Focus(dir) => match dir.as_str() {
            "up" => { if app.cursor.0 > 0 { app.cursor.0 -= 1; app.clamp_cursor(); } }
            "down" => { if app.cursor.0 + 1 < app.row_count() { app.cursor.0 += 1; app.clamp_cursor(); } }
            "left" => { if app.cursor.1 > 0 { app.cursor.1 -= 1; } }
            "right" => { if app.cursor.1 + 1 < app.col_count() { app.cursor.1 += 1; } }
            _ => {}
        },
        // ── 行列 ──
        HostOp::AddRow(pos) => match pos.as_str() {
            "before" => app.insert_row_before(),
            "after" => app.insert_row_after(),
            _ => {}
        },
        HostOp::AddCol(pos) => match pos.as_str() {
            "before" => app.insert_col_before(),
            "after" => app.insert_col_after(),
            _ => {}
        },
        HostOp::DelRow => { app.delete_row(); }
        HostOp::DelCol => { app.delete_col(); }
        HostOp::SwapRow => { app.move_row_up(); }
        HostOp::SwapCol => { app.move_col_left(); }
        // ── 选中 ──
        HostOp::SelectKey => {
            if app.selected == Some(app.cursor) { app.selected = None; }
            else { app.selected = Some(app.cursor); }
        }
        HostOp::SwapKey => { app.swap_with_selected(); }
        // ── 面板/输入 ──
        HostOp::ShowMatrix(_) => { app.show_insert_menu = true; app.insert_menu_cursor = 0; }
        HostOp::InputValue(kind) => match kind.as_str() {
            "key" => { app.save_file(); }
            "popup" => { app.delete_key(); }
            _ => {}
        },
        HostOp::InputName(kind) => match kind.as_str() {
            "key" | "popup" => { app.add_key_after(); }
            _ => {}
        },
        HostOp::Apply => { app.apply_edit(); app.mode = Mode::Normal; app.status_msg.clear(); }
        // ── 编辑器原语在布局 TUI 中无对应状态, 忽略 ──
        _ => {}
    }
}

fn handle_insert_menu(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.show_insert_menu = false;
        }
        KeyCode::Up => {
            if app.insert_menu_cursor > 0 {
                app.insert_menu_cursor -= 1;
            }
        }
        KeyCode::Down => {
            if app.insert_menu_cursor < 5 {
                app.insert_menu_cursor += 1;
            }
        }
        KeyCode::Enter => {
            app.show_insert_menu = false;
            match app.insert_menu_cursor {
                0 => app.insert_row_after(),
                1 => app.insert_row_before(),
                2 => app.insert_col_after(),
                3 => app.insert_col_before(),
                4 => app.add_key_after(),
                5 => app.delete_key(),
                _ => {}
            }
        }
        _ => {}
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
    let size = frame.size();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(4),   // keyboard preview
            Constraint::Length(1), // hints
        ])
        .split(size);

    render_status_bar(frame, app, main_chunks[0], size);
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

fn render_status_bar(frame: &mut ratatui::Frame, app: &App, area: Rect, _size: Rect) {
    let mode_bg = match app.mode {
        Mode::Normal => Color::Rgb(40, 80, 40),
        Mode::Edit => Color::Rgb(80, 70, 30),
        Mode::Popup => Color::Rgb(40, 70, 80),
    };
    let mode_fg = match app.mode {
        Mode::Normal => Color::Rgb(120, 220, 120),
        Mode::Edit => Color::Rgb(220, 200, 80),
        Mode::Popup => Color::Rgb(100, 200, 220),
    };

    let modified = if app.modified { " ●" } else { "" };
    let filename = app.filename();

    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.mode.as_hex()),
            Style::default().fg(Color::Black).bg(mode_bg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", app.mode.label()),
            Style::default().fg(mode_fg).bg(mode_bg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(Color::Rgb(80, 80, 100)).bg(Color::Rgb(20, 20, 30))),
        Span::styled(
            filename,
            Style::default().fg(Color::Rgb(200, 200, 220)).bg(Color::Rgb(20, 20, 30)),
        ),
        Span::styled(
            modified,
            Style::default().fg(Color::Rgb(255, 180, 60)).bg(Color::Rgb(20, 20, 30)),
        ),
    ];

    if !app.status_msg.is_empty() {
        let msg_style = if app.status_is_error {
            Style::default().fg(Color::Rgb(255, 100, 100)).bg(Color::Rgb(20, 20, 30))
        } else {
            Style::default().fg(Color::Rgb(100, 220, 120)).bg(Color::Rgb(20, 20, 30))
        };
        spans.push(Span::styled(" │ ", Style::default().fg(Color::Rgb(80, 80, 100)).bg(Color::Rgb(20, 20, 30))));
        spans.push(Span::styled(format!(" {} ", app.status_msg), msg_style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Rgb(20, 20, 30))),
        area,
    );
}

fn render_preview(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    if app.rows.is_empty() {
        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(" ⌨ ", Style::default().fg(Color::Rgb(100, 180, 255)).add_modifier(Modifier::BOLD)),
                Span::styled("键盘", Style::default().fg(Color::Rgb(180, 180, 200)).add_modifier(Modifier::BOLD)),
            ]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
            .style(Style::default().bg(Color::Rgb(15, 15, 25)));

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("空布局")
                .style(Style::default().fg(Color::Rgb(80, 80, 100)))
                .alignment(ratatui::layout::Alignment::Center),
            inner,
        );
        return;
    }

    let total_rows = app.rows.len();
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ⌨ ", Style::default().fg(Color::Rgb(100, 180, 255)).add_modifier(Modifier::BOLD)),
            Span::styled("键盘", Style::default().fg(Color::Rgb(180, 180, 200)).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" {}/{}", app.cursor.0 + 1, total_rows),
                Style::default().fg(Color::Rgb(120, 120, 140)),
            ),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
        .style(Style::default().bg(Color::Rgb(15, 15, 25)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let available_width = inner.width as usize;

    for (row_idx, row) in app.rows.iter().enumerate() {
        let y = inner.y + row_idx as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let total_display_len: usize = row
            .iter()
            .map(|node| {
                let name = App::display_name(node);
                name.len().max(4)
            })
            .sum();
        let total_gaps = row.len().saturating_sub(1);
        let usable = available_width.saturating_sub(total_gaps);

        let mut x_offset: u16 = 0;

        for (col_idx, node) in row.iter().enumerate() {
            let name = App::display_name(node);
            let base_len = name.len().max(4);

            let cell_width = if total_display_len > 0 {
                ((base_len as f64 / total_display_len as f64) * usable as f64).round() as usize
            } else {
                base_len
            };
            let cell_width = cell_width.max(4);

            let cell_rect = Rect {
                x: inner.x + x_offset,
                y,
                width: cell_width as u16,
                height: 1,
            };

            let is_cursor = app.cursor == (row_idx, col_idx);
            let is_selected = app.selected == Some((row_idx, col_idx));

            let (fg, bg, bold) = if is_cursor {
                match app.mode {
                    Mode::Normal => (Color::Rgb(15, 15, 25), Color::Rgb(220, 220, 240), true),
                    Mode::Edit => (Color::Rgb(15, 15, 25), Color::Rgb(220, 200, 80), true),
                    Mode::Popup => (Color::Rgb(15, 15, 25), Color::Rgb(100, 200, 220), true),
                }
            } else if is_selected {
                (Color::Rgb(15, 15, 25), Color::Rgb(100, 220, 120), true)
            } else {
                match node {
                    AstNode::Key { kind: KeyKind::Special, .. } => (Color::Rgb(200, 120, 255), Color::Rgb(30, 20, 40), false),
                    AstNode::Key { kind: KeyKind::Named, .. } => (Color::Rgb(200, 200, 220), Color::Rgb(30, 30, 45), false),
                    AstNode::Key { kind: KeyKind::Literal, .. } => (Color::Rgb(120, 180, 255), Color::Rgb(20, 25, 40), false),
                    AstNode::Combo { .. } => (Color::Rgb(255, 220, 100), Color::Rgb(40, 35, 20), false),
                    AstNode::Popup { .. } => (Color::Rgb(100, 200, 220), Color::Rgb(20, 35, 40), false),
                    AstNode::Label { .. } => (Color::Rgb(120, 220, 140), Color::Rgb(20, 35, 25), false),
                }
            };

            let mut style = Style::default().fg(fg).bg(bg);
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }

            let truncated: String = name.chars().take(cell_width).collect();
            frame.render_widget(
                Paragraph::new(truncated).style(style).alignment(ratatui::layout::Alignment::Center),
                cell_rect,
            );

            x_offset += cell_width as u16 + 1;
        }
    }
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
                ("i", "菜单"),
                ("Ctrl+S", "保存"),
                ("Esc", "退出"),
            ],
            Mode::Edit | Mode::Popup => vec![
                ("Enter", "确认"),
                ("Esc", "取消"),
            ],
        }
    };

    let mut spans = vec![Span::styled(" ", Style::default().bg(Color::Rgb(25, 25, 35)))];
    for (key, label) in &hints {
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default().fg(Color::Rgb(255, 220, 100)).bg(Color::Rgb(40, 40, 55)).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{} ", label),
            Style::default().fg(Color::Rgb(140, 140, 160)).bg(Color::Rgb(25, 25, 35)),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Rgb(25, 25, 35))),
        area,
    );
}

fn render_edit_panel(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let (title, accent) = if app.mode == Mode::Edit {
        (" 编辑主键 ", Color::Rgb(220, 200, 80))
    } else {
        (" 编辑弹出键 ", Color::Rgb(100, 200, 220))
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ✎ ", Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            Span::styled(title, Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));

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
                Span::styled(" 现值 ", Style::default().fg(Color::Rgb(100, 100, 120)).add_modifier(Modifier::BOLD)),
                Span::styled(&current_dsl, Style::default().fg(Color::Rgb(160, 160, 180))),
            ])),
            Rect { x: inner.x, y: y0, width: inner.width, height: 1 },
        );
    }

    // Edit input
    let y1 = y0 + 1;
    if y1 < inner.y + inner.height {
        let cursor_char = "▏";
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" 输入 ", Style::default().fg(Color::Rgb(100, 100, 120)).add_modifier(Modifier::BOLD)),
                Span::styled(&app.edit_buf, Style::default().fg(Color::Rgb(220, 220, 240)).add_modifier(Modifier::BOLD)),
                Span::styled(cursor_char, Style::default().fg(accent)),
            ])),
            Rect { x: inner.x, y: y1, width: inner.width, height: 1 },
        );
    }

    // Syntax hint
    let y2 = y1 + 2;
    if y2 < inner.y + inner.height {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" 语法: A | A:B | A(B) | A.B", Style::default().fg(Color::Rgb(80, 80, 100))),
            ])),
            Rect { x: inner.x, y: y2, width: inner.width, height: 1 },
        );
    }
}

fn render_insert_menu(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ◆ ", Style::default().fg(Color::Rgb(120, 220, 140)).add_modifier(Modifier::BOLD)),
            Span::styled("插入", Style::default().fg(Color::Rgb(120, 220, 140)).add_modifier(Modifier::BOLD)),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(80, 160, 100)))
        .style(Style::default().bg(Color::Rgb(20, 25, 30)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items = [
        ("+", "行后"),
        ("+", "行前"),
        ("+", "列后"),
        ("+", "列前"),
        ("×", "删除"),
    ];

    for (i, (icon, label)) in items.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let is_selected = app.insert_menu_cursor == i;
        let (fg, bg) = if is_selected {
            (Color::Rgb(20, 25, 30), Color::Rgb(120, 220, 140))
        } else {
            (Color::Rgb(160, 160, 180), Color::Rgb(20, 25, 30))
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", if is_selected { "▶" } else { " " }),
                    Style::default().fg(fg).bg(bg),
                ),
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(Color::Rgb(100, 200, 120)).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    label.to_string(),
                    Style::default().fg(fg).bg(bg),
                ),
            ])),
            Rect { x: inner.x, y, width: inner.width, height: 1 },
        );
    }
}

// ── Public entry point ───────────────────────────────────────────────

pub fn run() -> anyhow::Result<()> {
    let file_path = crate::lib::user_dir::default_layout_file();
    let source = if file_path.exists() {
        fs::read_to_string(&file_path)?
    } else {
        String::new()
    };

    let mut app = App::new(file_path, source)?;
    run_app(&mut app)
}

/// Launch TUI with a specific .tekl file.
pub fn run_with_file(path: &str) -> anyhow::Result<()> {
    let file_path = std::path::PathBuf::from(path);
    let source = if file_path.exists() {
        fs::read_to_string(&file_path)?
    } else {
        String::new()
    };

    let mut app = App::new(file_path, source)?;
    run_app(&mut app)
}

fn run_app(app: &mut App) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(50);
    loop {
        terminal.draw(|f| render_frame(f, app))?;

        if app.should_quit {
            break;
        }

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                handle_event(app, key);
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
}
