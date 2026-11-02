use crate::lib;

/// Compile a tekl source file and write the layout to the `extra-keys` property.
///
/// The value written is tekl text (single line, rows separated by `;`), not JSON. The Termux app
/// parses it natively.
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

    // 一行的 tekl 文本，行之间用 `;` 分隔；换行只用于源码可读性。
    let tekl = lib::keys::print_rows(&result.rows).replace('\n', " ");

    // Write to termux.properties
    let props_path = resolve_properties_path(output)?;
    crate::termux::writer::write_properties(&props_path, &tekl)?;
    eprintln!("wrote extra-keys (tekl) to {}", props_path.display());

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

fn resolve_properties_path(output: &Option<String>) -> anyhow::Result<std::path::PathBuf> {
    match output.as_deref() {
        Some(p) => Ok(std::path::PathBuf::from(p)),
        None => {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            Ok(home.join(".termux").join("termux.properties"))
        }
    }
}
