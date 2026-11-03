use crate::lib;

/// Compile a tekl source file and write the canonical layout to
/// `~/.termux/termux.tekl` (or the path given with `--output`).
///
/// The Termux fork reads that file directly and parses tekl natively, so there is no
/// `termux.properties` layer, no escaping and no JSON intermediate anymore.
pub fn run_compile(
    file: &Option<String>,
    output: &Option<String>,
    _watch: bool,
    reload: bool,
) -> anyhow::Result<()> {
    let layout_path = resolve_layout_file(file)?;
    let source = std::fs::read_to_string(&layout_path)?;

    let result = lib::keys::compile_source(&source)?;

    for w in &result.warnings {
        eprintln!("warning: {w}");
    }

    // Canonical tekl source: one row per line, rows terminated by `;`.
    let tekl = lib::keys::print_rows(&result.rows);

    let extra_keys_path = resolve_extra_keys_path(output);
    crate::termux::writer::write_extra_keys_file(&extra_keys_path, &tekl)?;
    eprintln!("wrote tekl layout to {}", extra_keys_path.display());

    if reload {
        crate::termux::reload::reload_settings()?;
    }

    Ok(())
}

fn resolve_layout_file(file: &Option<String>) -> anyhow::Result<std::path::PathBuf> {
    match file.as_deref() {
        Some(p) => Ok(std::path::PathBuf::from(p)),
        None => {
            let default = lib::user_dir::default_layout_file();
            if default.exists() {
                Ok(default)
            } else {
                eprintln!("error: no tekl file and default {} missing", default.display());
                std::process::exit(1);
            }
        }
    }
}

fn resolve_extra_keys_path(output: &Option<String>) -> std::path::PathBuf {
    match output.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            home.join(".termux").join("termux.tekl")
        }
    }
}
