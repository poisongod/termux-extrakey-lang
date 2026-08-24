//! 用户主目录 (~/.termux) 规范。
//!
//! 布局:
//!   ~/.termux/termux.tekl       键盘布局源码 (普通用户)
//!   ~/.termux/tekl.ttoll        布局 TUI 行为规则   ┐
//!   ~/.termux/editor.ttoll      编辑器行为规则       ├ 插件开发者层
//!   ~/.termux/id/<ID>.rhai      用户 ID 插件 (出厂为空) ┘
//!
//! 仓库内 keys.layout / behavior.base 是"出厂模板"，首次运行时复制到主目录。

use std::fs;
use std::path::PathBuf;

pub fn user_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/data/data/com.termux/files/home"))
        .join(".termux")
}

pub fn default_layout_file() -> PathBuf {
    user_root().join("termux.tekl")
}

pub fn behavior_base_file() -> PathBuf {
    user_root().join("ui").join("init.ttoll")
}

/// 用户 ID 插件目录 (~/.termux/id/)，文件名即 ID: <0xHH>.rhai
pub fn id_dir() -> PathBuf {
    user_root().join("id")
}

fn bundled_behavior() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("behavior.base")
}

fn bundled_layout() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("keys.layout")
}

pub fn behavior_base_content() -> String {
    let f = behavior_base_file();
    if f.exists() {
        fs::read_to_string(&f).unwrap_or_default()
    } else {
        let bundled = bundled_behavior();
        fs::read_to_string(&bundled).unwrap_or_default()
    }
}

/// 首次运行: 补齐主目录模板。返回新建的文件列表。
pub fn provision_user_dir() -> Vec<PathBuf> {
    let mut created = Vec::new();

    let _ = fs::create_dir_all(user_root().join("ui"));
    let _ = fs::create_dir_all(id_dir());

    let behavior_file = behavior_base_file();
    if !behavior_file.exists() {
        let bundled = bundled_behavior();
        if bundled.exists() {
            let _ = fs::copy(&bundled, &behavior_file);
            created.push(behavior_file);
        }
    }

    let layout_file = default_layout_file();
    if !layout_file.exists() {
        let bundled = bundled_layout();
        if bundled.exists() {
            let _ = fs::copy(&bundled, &layout_file);
            created.push(layout_file);
        }
    }

    created
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_root_ends_with_dot_termux() {
        let root = user_root();
        assert!(root.to_string_lossy().ends_with(".termux"));
    }

    #[test]
    fn test_default_layout_file_path() {
        let f = default_layout_file();
        assert!(f.to_string_lossy().ends_with("termux.tekl"));
    }

    #[test]
    fn test_behavior_base_file_path() {
        let f = behavior_base_file();
        assert!(f.to_string_lossy().ends_with("init.ttoll"));
    }

    #[test]
    fn test_id_dir_path() {
        let d = id_dir();
        assert!(d.to_string_lossy().ends_with("id"));
    }
}
