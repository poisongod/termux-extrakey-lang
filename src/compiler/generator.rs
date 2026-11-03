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
pub fn run_compile(
    file: &str,
    output: Option<&str>,
    reload: bool,
    strict: bool,
) -> anyhow::Result<()> {
    let source_path = PathBuf::from(file);
    let source = fs::read_to_string(&source_path)?;

    let result = lib::keys::compile_source(&source)?;
    report_warnings(&result.warnings, strict)?;

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

/// Validate a tekl file and print its warnings. Writes nothing and does not reload.
pub fn check(file: &str, strict: bool) -> anyhow::Result<()> {
    let source = fs::read_to_string(file)?;
    let result = lib::keys::compile_source(&source)?;
    report_warnings(&result.warnings, strict)?;

    eprintln!("ok: {} ({} rows)", file, result.rows.len());
    Ok(())
}

/// Print warnings, and in strict mode turn them into a hard failure.
fn report_warnings(warnings: &[String], strict: bool) -> anyhow::Result<()> {
    for w in warnings {
        eprintln!("warning: {w}");
    }
    if strict && !warnings.is_empty() {
        anyhow::bail!("strict: treated {} warning(s) as errors", warnings.len());
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

        run_compile(path.to_str().unwrap(), Some(path.to_str().unwrap()), false, false).unwrap();

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

        run_compile(src.to_str().unwrap(), Some(dst.to_str().unwrap()), false, false).unwrap();

        assert_eq!(fs::read_to_string(&dst).unwrap(), "ESC;\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_accepts_valid_file_without_writing() {
        let dir = temp_dir("check-valid");
        let src = dir.join("layout.tekl");
        let original = "# keep me\nESC; a b;\n";
        fs::write(&src, original).unwrap();

        check(src.to_str().unwrap(), false).unwrap();

        assert_eq!(fs::read_to_string(&src).unwrap(), original);
        assert!(!src.with_extension("tekl.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_reports_line_and_column() {
        let dir = temp_dir("check-invalid");
        let src = dir.join("broken.tekl");
        fs::write(&src, "ESC(\n").unwrap();

        let err = check(src.to_str().unwrap(), false).unwrap_err().to_string();

        assert!(err.contains("line 1"), "{err}");
        assert!(err.contains("column 5"), "{err}");
        // The `keys:` prefix must appear exactly once, not "keys: keys:".
        assert_eq!(err.matches("keys:").count(), 1, "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_turns_warnings_into_errors_before_writing() {
        let dir = temp_dir("strict");
        let src = dir.join("layout.tekl");
        // ESC alone is valid but warns about missing DRAWER/KEYBOARD.
        fs::write(&src, "ESC;\n").unwrap();

        let err = check(src.to_str().unwrap(), true).unwrap_err().to_string();
        assert!(err.contains("strict"), "{err}");

        let dst = dir.join("out.tekl");
        fs::write(&dst, "old\n").unwrap();
        let err = run_compile(src.to_str().unwrap(), Some(dst.to_str().unwrap()), false, true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("strict"), "{err}");
        assert_eq!(fs::read_to_string(&dst).unwrap(), "old\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
