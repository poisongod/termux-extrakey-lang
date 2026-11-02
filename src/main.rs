mod lib;
mod compiler;
mod tui;
mod worker;
mod termux;

use clap::{Parser, Subcommand};
use std::path::Path;

#[derive(Parser)]
#[command(name = "tekl", version, about = "Termux Extra key Language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Edit .ttoll (behavior rules) or .tekl (keyboard layout) files
    Edit {
        /// File to edit: .ttoll → behavior editor, .tekl → keyboard TUI
        file: String,
    },
    /// Compile .tekl file and update termux.properties
    Reload {
        /// Source .tekl file
        file: String,
        /// Reload termux settings after write
        #[arg(long)]
        no_reload: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 首次运行补齐 ~/.termux 模板（Release 包里模板位于可执行文件同目录）
    if !matches!(&cli.command, Some(Commands::Reload { .. })) {
        lib::user_dir::provision_user_dir();
    }

    match cli.command {
        None => {
            // Default: launch TUI with default tekl file
            tui::app::run()
        }
        Some(Commands::Edit { file }) => {
            let path = Path::new(&file);
            match path.extension().and_then(|e| e.to_str()) {
                Some("ttoll") => tui::editor::run_editor(&Some(file)),
                Some("tekl") => tui::app::run_with_file(&file),
                _ => {
                    eprintln!("error: unsupported file type '{file}'");
                    eprintln!("  .ttoll → behavior rule editor");
                    eprintln!("  .tekl  → keyboard layout TUI");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Reload { file, no_reload }) => {
            compiler::generator::run_compile(&Some(file), &None, false, !no_reload)
        }
    }
}
