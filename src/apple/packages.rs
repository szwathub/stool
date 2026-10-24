use super::homebrew::{self, Environment};
use super::process::{self, Executor};
use crate::cli::apple::brew::InstallArgs;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

pub(super) fn install(
    selected: Option<&Path>,
    args: &InstallArgs,
    environment: &Environment,
    executor: &mut impl Executor,
    report: &mut impl Write,
    verbose: bool,
) -> Result<()> {
    let file = match &args.file {
        Some(file) => file.to_owned(),
        None => environment
            .home
            .as_ref()
            .context("Cannot find HOME; pass --file to select a Brewfile")?
            .join(".Brewfile"),
    };
    let file = fs::canonicalize(&file).with_context(|| {
        format!(
            "Read Brewfile {}; deploy ~/.Brewfile with stool dotfile apply or pass --file",
            file.display()
        )
    })?;
    if !file.is_file() {
        bail!("Brewfile must be a regular file: {}", file.display());
    }
    let contents = fs::read(&file).with_context(|| format!("Read Brewfile {}", file.display()))?;
    if !args.options.dry_run && !environment.terminal && !args.options.non_interactive {
        bail!("Package installation needs an interactive terminal; rerun in a terminal or explicitly use --non-interactive after preparing privileges");
    }
    let brew = homebrew::ensure(
        selected,
        &args.options,
        environment,
        executor,
        report,
        verbose,
    )?;
    if args.options.dry_run {
        writeln!(
            report,
            "Brewfile (not evaluated): {}\n{}",
            file.display(),
            String::from_utf8_lossy(&contents)
        )?;
        writeln!(
            report,
            "Would run {} bundle install --no-upgrade --file {}",
            brew.executable.display(),
            file.display()
        )?;
        return Ok(());
    }
    writeln!(report, "Installing packages from {}", file.display())?;
    let mut command = process::command(&brew.executable);
    command
        .args(["bundle", "install", "--no-upgrade", "--file"])
        .arg(&file)
        .env("HOMEBREW_BUNDLE_NO_UPGRADE", "1")
        // Inherited automation settings must not turn explicit installation into cleanup or an upgrade.
        .env_remove("HOMEBREW_BUNDLE_INSTALL_CLEANUP");
    process::run(
        executor,
        command,
        args.options.non_interactive,
        "Install Brewfile packages",
    )?;
    writeln!(
        report,
        "Brewfile installation completed: {}",
        file.display()
    )?;
    Ok(())
}
