use std::fs;
use std::path::Path;

/// Atomically write canonical tekl source to `~/.termux/termux.tekl`.
///
/// The Termux fork reads this file directly and parses it natively, so no
/// `termux.properties` escaping or line merging is involved anymore.
///
/// Strategy: back up the old file to `*.tekl.bak`, write to a temp file, rename for
/// crash safety, then verify the contents.
pub fn write_extra_keys_file(path: &Path, tekl: &str) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let content = if tekl.ends_with('\n') {
        tekl.to_string()
    } else {
        format!("{tekl}\n")
    };

    let bak_path = path.with_extension("tekl.bak");
    if path.exists() {
        fs::copy(path, &bak_path)?;
    }

    let tmp_path = path.with_extension("tekl.tmp");
    fs::write(&tmp_path, &content)?;
    fs::rename(&tmp_path, path)?;

    // Verify read-back
    let read_back = fs::read_to_string(path)?;
    if read_back != content {
        if bak_path.exists() {
            fs::copy(&bak_path, path)?;
        }
        anyhow::bail!("write verification failed: {} does not match what was written", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tekl-writer-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("termux.tekl")
    }

    #[test]
    fn writes_file_and_creates_backup() {
        let path = temp_path("backup");
        write_extra_keys_file(&path, "ESC TAB;").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "ESC TAB;\n");

        write_extra_keys_file(&path, "ESC;").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "ESC;\n");
        assert_eq!(
            fs::read_to_string(path.with_extension("tekl.bak")).unwrap(),
            "ESC TAB;\n"
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keeps_trailing_newline_once() {
        let path = temp_path("newline");
        write_extra_keys_file(&path, "ESC\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "ESC\n");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
