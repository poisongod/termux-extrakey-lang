//! 用户主目录 (~/.termux) 规范。
//!
//! 布局:
//!   ~/.termux/termux.tekl       键盘布局源码 (普通用户)
//!   ~/.termux/tekl.ttoll        布局 TUI 行为规则   ┐
//!   ~/.termux/editor.ttoll      编辑器行为规则       ├ 插件开发者层
//!   ~/.termux/id/<ID>.rhai      用户 ID 插件 (出厂为空) ┘
//!
//! 仓库内 termux.tekl / behavior.base 是"出厂模板"，首次运行时复制到主目录。
//! 发布版把模板放在可执行文件同目录（或 `$PREFIX/share/tekl/`），见 `bundled_asset`。

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

/// 出厂资源查找顺序：
/// 1. `$TEKL_ASSET_DIR`（显式覆盖）
/// 2. 可执行文件同目录（Release 包解压即用）
/// 3. `../share/tekl/`（`$PREFIX/bin/tekl` → `$PREFIX/share/tekl/`）
/// 4. 编译期源码目录（开发期 `cargo run` / `cargo build`）
fn bundled_asset(name: &str) -> PathBuf {
    if let Some(dir) = std::env::var_os("TEKL_ASSET_DIR") {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return candidate;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            let beside = bin_dir.join(name);
            if beside.exists() {
                return beside;
            }
            if let Some(prefix) = bin_dir.parent() {
                let shared = prefix.join("share").join("tekl").join(name);
                if shared.exists() {
                    return shared;
                }
            }
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn bundled_behavior() -> PathBuf {
    bundled_asset("behavior.base")
}

fn bundled_layout() -> PathBuf {
    bundled_asset("termux.tekl")
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

    #[test]
    fn test_bundled_layout_name() {
        assert!(bundled_layout().to_string_lossy().ends_with("termux.tekl"));
    }

    #[test]
    fn test_bundled_behavior_name() {
        assert!(bundled_behavior().to_string_lossy().ends_with("behavior.base"));
    }
}
