//! Generate CLI completions and manuals in the project root.
#[allow(dead_code, unused_imports)]
#[path = "src/cli/mod.rs"]
mod cli;
#[allow(dead_code)]
#[path = "src/help.rs"]
mod help;

use clap::{Args, CommandFactory};
use std::{env, fs, path::Path};

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=src/cli");
    println!("cargo:rerun-if-changed=src/help.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let root = env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR");
    let completions = Path::new(&root).join("completions");
    let manuals = Path::new(&root).join("man-page");
    for directory in [&completions, &manuals] {
        clear_generated(directory)?;
    }
    let mut command = cli::Definition::<cli::BaseCommands>::command();
    if target_os == "macos" {
        command = command.subcommand(cli::apple::AppleCommand::augment_args(clap::Command::new(
            "apple",
        )));
    }
    for shell in [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Zsh,
        clap_complete::Shell::Fish,
        clap_complete::Shell::PowerShell,
        clap_complete::Shell::Elvish,
    ] {
        clap_complete::generate_to(shell, &mut command.clone(), "stool", &completions)?;
    }
    help::write_man(&command, "stool", &manuals)?;
    // Exercise this generator for each target OS in integration tests.
    println!(
        "cargo:rustc-env=STOOL_BUILD_SCRIPT={}",
        env::current_exe()?.display()
    );
    Ok(())
}

fn clear_generated(directory: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(directory)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Remove obsolete generated commands while preserving unrelated files.
        if entry.file_type()?.is_file() && (name.starts_with("stool") || name.starts_with("_stool"))
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
