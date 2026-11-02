use std::fs;
use std::path::Path;

const EXTRA_KEYS_PREFIX: &str = "extra-keys = ";

/// 转义 `java.util.Properties` 值。
///
/// Termux 用 `java.util.Properties` 读取 `termux.properties`，反斜杠是转义字符，
/// 所以 tekl 文本里的每个 `\` 都要写成 `\\`，否则字面反斜杠会被吞掉。
pub fn escape_value(value: &str) -> String {
    value.replace('\\', "\\\\")
}

/// Atomically write tekl source to the `extra-keys` property of termux.properties.
///
/// Strategy: read existing file, replace or append `extra-keys = ...` line,
/// write to temp file, rename for crash safety.
pub fn write_properties(path: &Path, tekl: &str) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let escaped = escape_value(tekl);
    let new_line = format!("{EXTRA_KEYS_PREFIX}{escaped}");

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let replaced = lines.iter_mut().any(|line| {
        if line.starts_with(EXTRA_KEYS_PREFIX) {
            *line = new_line.clone();
            true
        } else {
            false
        }
    });

    if !replaced {
        lines.push(new_line);
    }

    let new_content = lines.join("\n") + "\n";

    // Atomic write: write to temp, then rename
    let bak_path = path.with_extension("tekl.bak");
    if path.exists() {
        fs::copy(path, &bak_path)?;
    }

    let tmp_path = path.with_extension("tekl.tmp");
    fs::write(&tmp_path, &new_content)?;

    fs::rename(&tmp_path, path)?;

    // Verify read-back
    let read_back = fs::read_to_string(path)?;
    if !read_back.contains(&escaped) {
        // Restore from backup
        if bak_path.exists() {
            fs::copy(&bak_path, path)?;
        }
        anyhow::bail!("write verification failed: extra-keys not found in written file");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_backslash_for_properties() {
        assert_eq!(escape_value("ESC"), "ESC");
        assert_eq!(escape_value(r#"BACKSLASH."|""#), r#"BACKSLASH."|""#);
        assert_eq!(escape_value(r#""\""#), r#""\\""#);
    }
}
