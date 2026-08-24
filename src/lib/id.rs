//! ID 注册池 — 按消费者隔离, 一 ID 一 .rhai 文件。
//!
//! 架构约定:
//!   * 一个 ID = 一个 .rhai 文件, 文件名即 ID (`0x00.rhai` 只含 0x00)
//!   * 文件内容 = 该 ID 的注册信息 (desc) + 实际实现 (run 闭包)
//!   * 内置 ID 通过编译期清单嵌入二进制 (include_str!), Core 不耦合任何具体 ID
//!   * 用户扩展: ~/.termux/id/<ID>.rhai, 只添加不覆盖内置
//!   * .ttoll 规则通过十六进制 ID 接线
//!
//! 单文件格式 (最后表达式为返回值):
//!
//!   // 退出/返回
//!   #{
//!       desc: "退出/返回",
//!       run: || exit()
//!   }

use crate::worker::script_runtime;
use rhai::{Engine, FnPtr};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── 内置 ID 清单 (编译期嵌入) ────────────────────────────────────────
// 增删一个内置 ID = 增删一个文件 + 清单一行。Core 逻辑零改动。

/// 布局 TUI 内置 ID (src/id/tui/)
pub const TUI_IDS: &[(&str, &str)] = &[
    ("0x00", include_str!("../id/tui/0x00.rhai")),
    ("0x01", include_str!("../id/tui/0x01.rhai")),
    ("0x02", include_str!("../id/tui/0x02.rhai")),
    ("0x03", include_str!("../id/tui/0x03.rhai")),
    ("0x04", include_str!("../id/tui/0x04.rhai")),
    ("0x10", include_str!("../id/tui/0x10.rhai")),
    ("0x11", include_str!("../id/tui/0x11.rhai")),
    ("0x12", include_str!("../id/tui/0x12.rhai")),
    ("0x13", include_str!("../id/tui/0x13.rhai")),
    ("0x20", include_str!("../id/tui/0x20.rhai")),
    ("0x21", include_str!("../id/tui/0x21.rhai")),
    ("0x22", include_str!("../id/tui/0x22.rhai")),
    ("0x23", include_str!("../id/tui/0x23.rhai")),
    ("0x24", include_str!("../id/tui/0x24.rhai")),
    ("0x25", include_str!("../id/tui/0x25.rhai")),
    ("0x26", include_str!("../id/tui/0x26.rhai")),
    ("0x27", include_str!("../id/tui/0x27.rhai")),
    ("0x30", include_str!("../id/tui/0x30.rhai")),
    ("0x31", include_str!("../id/tui/0x31.rhai")),
    ("0x40", include_str!("../id/tui/0x40.rhai")),
    ("0x41", include_str!("../id/tui/0x41.rhai")),
    ("0x42", include_str!("../id/tui/0x42.rhai")),
    ("0x50", include_str!("../id/tui/0x50.rhai")),
    ("0x51", include_str!("../id/tui/0x51.rhai")),
    ("0x53", include_str!("../id/tui/0x53.rhai")),
];

/// ttoll 编辑器内置 ID (src/id/editor/)
/// 0x00 全局等同（退出）；自有 ID 从 0x01 起，模式: 0x01=Normal 0x02=Insert 0x03=Command
pub const EDITOR_IDS: &[(&str, &str)] = &[
    ("0x00", include_str!("../id/editor/0x00.rhai")),
    ("0x01", include_str!("../id/editor/0x01.rhai")),
    ("0x02", include_str!("../id/editor/0x02.rhai")),
    ("0x03", include_str!("../id/editor/0x03.rhai")),
    ("0x04", include_str!("../id/editor/0x04.rhai")),
    ("0x05", include_str!("../id/editor/0x05.rhai")),
    ("0x06", include_str!("../id/editor/0x06.rhai")),
    ("0x07", include_str!("../id/editor/0x07.rhai")),
    ("0x08", include_str!("../id/editor/0x08.rhai")),
    ("0x09", include_str!("../id/editor/0x09.rhai")),
    ("0x0A", include_str!("../id/editor/0x0A.rhai")),
    ("0x0B", include_str!("../id/editor/0x0B.rhai")),
    ("0x0C", include_str!("../id/editor/0x0C.rhai")),
    ("0x0D", include_str!("../id/editor/0x0D.rhai")),
    ("0x0E", include_str!("../id/editor/0x0E.rhai")),
    ("0x0F", include_str!("../id/editor/0x0F.rhai")),
    ("0x10", include_str!("../id/editor/0x10.rhai")),
    ("0x11", include_str!("../id/editor/0x11.rhai")),
    ("0x12", include_str!("../id/editor/0x12.rhai")),
    ("0x13", include_str!("../id/editor/0x13.rhai")),
    ("0x14", include_str!("../id/editor/0x14.rhai")),
    ("0x15", include_str!("../id/editor/0x15.rhai")),
    ("0x16", include_str!("../id/editor/0x16.rhai")),
    ("0x17", include_str!("../id/editor/0x17.rhai")),
];

#[derive(Clone)]
pub struct IdEntry {
    pub id: String,
    /// 描述 — TUI ID 面板显示用
    pub desc: String,
    /// 操作逻辑 — Rhai 闭包, 调用时记录宿主操作序列
    pub run: FnPtr,
}

/// Per-consumer ID registry.
#[derive(Clone)]
pub struct IdRegistry {
    entries: Vec<IdEntry>,
    /// 全部已加载插件文件的编译产物 (run 闭包的函数体所在 lib)
    lib: rhai::AST,
}

impl IdRegistry {
    pub fn new() -> Self {
        Self { entries: Vec::new(), lib: rhai::AST::empty() }
    }

    /// TUI 注册池 — 内置清单 + 用户 ~/.termux/id/ 扩展。
    pub fn for_tui() -> Self {
        let mut reg = Self::new();
        reg.load_builtin(TUI_IDS);
        reg.load_user_ids();
        reg
    }

    /// 编辑器注册池 — 内置清单 + 用户扩展。
    pub fn for_editor() -> Self {
        let mut reg = Self::new();
        reg.load_builtin(EDITOR_IDS);
        reg.load_user_ids();
        reg
    }

    fn load_builtin(&mut self, manifest: &[(&str, &str)]) {
        for (id, src) in manifest {
            self.load_single(id, src);
        }
    }

    /// 加载用户 ID 插件 (~/.termux/id/<ID>.rhai)。
    /// 文件名即 ID；只添加新 ID，不覆盖内置条目。
    fn load_user_ids(&mut self) {
        let dir = crate::lib::user_dir::user_root().join("id");
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("rhai") {
                    continue;
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if !is_valid_id(stem) {
                    continue;
                }
                if self.is_registered(stem) {
                    eprintln!("警告: ID {} 已内置, 忽略用户插件 {}", stem, path.display());
                    continue;
                }
                match fs::read_to_string(&path) {
                    Ok(content) => self.load_single(stem, &content),
                    Err(e) => eprintln!("读取用户插件失败 {}: {e}", path.display()),
                }
            }
        }
    }

    /// 加载单个 ID 插件源码并注册。
    pub fn load_single(&mut self, id: &str, src: &str) {
        let engine = Engine::new();
        let ast = match engine.compile(src) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("ID {id} 插件编译失败: {e}");
                return;
            }
        };
        let map: rhai::Map = match engine.eval_ast(&ast) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ID {id} 插件求值失败: {e}");
                return;
            }
        };
        let desc = map.get("desc").map(|d| d.to_string()).unwrap_or_default();
        let run = match map.get("run").and_then(|d| d.clone().try_cast::<FnPtr>()) {
            Some(f) => f,
            None => {
                eprintln!("ID {id} 缺少操作逻辑 (run), 跳过");
                return;
            }
        };
        self.lib = self.lib.merge(&ast);
        self.register(id, &desc, run);
    }

    /// 注册一个 ID (描述 + Rhai 操作逻辑闭包)。
    pub fn register(&mut self, id: &str, desc: &str, run: FnPtr) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.desc = desc.to_string();
            entry.run = run;
        } else {
            self.entries.push(IdEntry {
                id: id.to_string(),
                desc: desc.to_string(),
                run,
            });
        }
    }

    /// 注销一个 ID。
    pub fn unregister(&mut self, id: &str) -> bool {
        let len = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < len
    }

    pub fn id_desc(&self, id: &str) -> Option<&str> {
        self.entries.iter().find(|e| e.id == id).map(|e| e.desc.as_str())
    }

    pub fn is_registered(&self, id: &str) -> bool {
        self.entries.iter().any(|e| e.id == id)
    }

    pub fn all_entries(&self) -> &[IdEntry] {
        &self.entries
    }

    pub fn to_map(&self) -> HashMap<&str, &str> {
        self.entries.iter().map(|e| (e.id.as_str(), e.desc.as_str())).collect()
    }

    /// 调用某个 ID 的操作逻辑，返回按序宿主操作（由消费者回放到自身状态）。
    pub fn eval_action(&self, id: &str) -> anyhow::Result<Vec<script_runtime::HostOp>> {
        let entry = self.entries.iter().find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("未注册的 ID: {id}"))?;
        script_runtime::call_action(&entry.run, &self.lib)
    }
}

impl Default for IdRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for IdRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdRegistry")
            .field("ids", &self.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>())
            .finish()
    }
}

/// 合法 ID: 0x + 两位十六进制
fn is_valid_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 4
        && b[0] == b'0'
        && (b[1] == b'x' || b[1] == b'X')
        && b[2..].iter().all(|c| c.is_ascii_hexdigit())
}

/// 用户插件目录
pub fn user_ids_dir() -> PathBuf {
    crate::lib::user_dir::user_root().join("id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::script_runtime::HostOp;

    const SAMPLE: &str = r#"
// 测试
#{
    desc: "测试动作",
    run: || { save(true); exit() }
}
"#;

    #[test]
    fn load_single_extracts_desc_and_run() {
        let mut reg = IdRegistry::new();
        reg.load_single("0x99", SAMPLE);
        assert!(reg.is_registered("0x99"));
        assert_eq!(reg.id_desc("0x99"), Some("测试动作"));
        assert_eq!(
            reg.eval_action("0x99").unwrap(),
            vec![HostOp::Save(true), HostOp::Exit]
        );
    }

    #[test]
    fn missing_run_skipped() {
        let mut reg = IdRegistry::new();
        reg.load_single("0x01", r#"#{ desc: "无实现" }"#);
        assert!(!reg.is_registered("0x01"));
    }

    #[test]
    fn invalid_source_skipped() {
        let mut reg = IdRegistry::new();
        reg.load_single("0x01", "这是非法 rhai !!!");
        assert!(!reg.is_registered("0x01"));
    }

    #[test]
    fn builtin_tui_manifest_loads() {
        let reg = IdRegistry::for_tui();
        assert_eq!(reg.all_entries().len(), TUI_IDS.len());
        assert_eq!(reg.id_desc("0x00"), Some("退出/返回"));
        assert_eq!(reg.eval_action("0x00").unwrap(), vec![HostOp::Exit]);
    }

    #[test]
    fn builtin_editor_manifest_loads() {
        let reg = IdRegistry::for_editor();
        assert_eq!(reg.all_entries().len(), EDITOR_IDS.len());
        // 场景序: 0x01-0x06 光标/跳转, 0x07 进入插入模式
        assert_eq!(reg.id_desc("0x01"), Some("光标左移"));
        assert_eq!(reg.eval_action("0x00").unwrap(), vec![HostOp::Exit]);
        // 组合操作: 保存退出
        assert_eq!(
            reg.eval_action("0x17").unwrap(),
            vec![HostOp::Save(true), HostOp::Exit]
        );
    }

    #[test]
    fn registries_are_isolated() {
        let tui = IdRegistry::for_tui();
        let ed = IdRegistry::for_editor();
        // 0x00 全局等同; 其余编号互不共享、语义独立
        assert!(tui.is_registered("0x00") && ed.is_registered("0x00"));
        assert!(tui.is_registered("0x40") && !ed.is_registered("0x40"));
        assert!(ed.is_registered("0x15") && !tui.is_registered("0x15"));
        assert_eq!(tui.id_desc("0x10"), Some("焦点上移"));
        assert_eq!(ed.id_desc("0x10"), Some("上方新建行"));
    }

    #[test]
    fn eval_action_unregistered_fails() {
        let reg = IdRegistry::new();
        assert!(reg.eval_action("0xEE").is_err());
    }

    #[test]
    fn valid_id_check() {
        assert!(is_valid_id("0x00"));
        assert!(is_valid_id("0xFF"));
        assert!(!is_valid_id("00"));
        assert!(!is_valid_id("0x001"));
        assert!(!is_valid_id("xx00"));
    }
}
