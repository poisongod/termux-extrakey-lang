use crate::lib;

/// Compile a tekl source file to extra-keys JSON and write to termux.properties.
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

    let json_str = lib::keys::stringify_json_rows(&result.json);

    // Write to termux.properties
    let props_path = resolve_properties_path(output)?;
    crate::termux::writer::write_properties(&props_path, &json_str)?;
    eprintln!("wrote extra-keys to {}", props_path.display());

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
