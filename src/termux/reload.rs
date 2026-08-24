use std::process::Command;

/// Trigger `termux-reload-settings` to apply the new keyboard layout.
pub fn reload_settings() -> anyhow::Result<()> {
    let output = Command::new("termux-reload-settings").output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("termux-reload-settings: {stderr}");
        }
        // Not fatal — the settings file was written, just not reloaded
    }

    Ok(())
}
