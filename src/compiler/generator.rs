use std::fs;
use std::path::{Path, PathBuf};

use crate::lib;

/// Compile a tekl source file and write the canonical layout to
/// `~/.termux/termux.tekl` (or the path given with `--output`).
///
/// The Termux fork reads that file directly and parses tekl natively, so there is no
/// `termux.properties` layer, no escaping and no JSON intermediate anymore.
///
/// When the source is already the target file (the common `tekl reload
/// ~/.termux/termux.tekl` case), it is validated but left untouched so comments and
/// formatting survive.
pub fn run_compile(file: &str, output: Option<&str>, reload: bool) -> anyhow::Result<()> {
    let source_path = PathBuf::from(file);
    let source = fs::read_to_string(&source_path)?;

    let result = lib::keys::compile_source(&source)?;

    for w in &result.warnings {
        eprintln!("warning: {w}");
    }

    let extra_keys_path = resolve_extra_keys_path(output);

    if same_file(&source_path, &extra_keys_path) {
        eprintln!(
            "validated {}, left unchanged (it is already the layout file)",
            source_path.display()
        );
    } else {
        // Canonical tekl source: one row per line, rows terminated by `;`.
        let tekl = lib::keys::print_rows(&result.rows);
        crate::termux::writer::write_extra_keys_file(&extra_keys_path, &tekl)?;
        eprintln!("wrote tekl layout to {}", extra_keys_path.display());
    }

    if reload {
        crate::termux::reload::reload_settings()?;
    }

    Ok(())
}

fn resolve_extra_keys_path(output: Option<&str>) -> PathBuf {
    match output {
        Some(p) => PathBuf::from(p),
        None => {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".termux").join("termux.tekl")
        }
    }
}

/// True when both paths point at the same file. Falls back to a textual comparison
/// for paths that do not exist yet.
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tekl-generator-{}-{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn in_place_reload_keeps_comments() {
        let dir = temp_dir("in-place");
        let path = dir.join("termux.tekl");
        let original = "# my layout\nESC;\n# trailing comment\n";
        fs::write(&path, original).unwrap();

        run_compile(path.to_str().unwrap(), Some(path.to_str().unwrap()), false).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(!path.with_extension("tekl.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn separate_output_is_canonicalized() {
        let dir = temp_dir("separate");
        let src = dir.join("source.tekl");
        let dst = dir.join("out").join("termux.tekl");
        fs::write(&src, "# comment\nESC ;\n").unwrap();

        run_compile(src.to_str().unwrap(), Some(dst.to_str().unwrap()), false).unwrap();

        assert_eq!(fs::read_to_string(&dst).unwrap(), "ESC;\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
