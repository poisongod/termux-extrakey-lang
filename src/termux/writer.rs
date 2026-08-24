use std::fs;
use std::path::Path;

const EXTRA_KEYS_PREFIX: &str = "extra-keys = ";

/// Atomically write extra-keys JSON to termux.properties.
///
/// Strategy: read existing file, replace or append `extra-keys = ...` line,
/// write to temp file, rename for crash safety.
pub fn write_properties(path: &Path, json: &str) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let new_line = format!("{EXTRA_KEYS_PREFIX}{json}");

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
    if !read_back.contains(json) {
        // Restore from backup
        if bak_path.exists() {
            fs::copy(&bak_path, path)?;
        }
        anyhow::bail!("write verification failed: extra-keys not found in written file");
    }

    Ok(())
}
