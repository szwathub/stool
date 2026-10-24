mod homebrew;
mod packages;
mod preferences;
mod process;

use crate::cli::apple::{AppleCommand, Commands};
use anyhow::Result;

pub fn run(command: &AppleCommand, verbose: bool) -> Result<()> {
    match &command.command {
        Commands::Init(args) => preferences::init(args.dry_run, verbose),
        Commands::Brew(command) => {
            use crate::cli::apple::brew::Commands;
            let environment = homebrew::Environment::current()?;
            let mut executor = process::System;
            let mut report = std::io::stdout().lock();
            match &command.command {
                Commands::Init(args) => homebrew::ensure(
                    command.prefix.as_deref(),
                    &args.options,
                    &environment,
                    &mut executor,
                    &mut report,
                    verbose,
                )
                .map(|_| ()),
                Commands::Install(args) => packages::install(
                    command.prefix.as_deref(),
                    args,
                    &environment,
                    &mut executor,
                    &mut report,
                    verbose,
                ),
            }
        }
    }
}

/// Share Homebrew discovery and package installation with top-level initialization.
pub(crate) fn bootstrap(
    file: Option<&std::path::Path>,
    dry_run: bool,
    non_interactive: bool,
    verbose: bool,
) -> Result<std::path::PathBuf> {
    let environment = homebrew::Environment::current()?;
    let mut executor = process::System;
    let mut report = std::io::stdout().lock();
    let options = crate::cli::apple::brew::Options {
        dry_run,
        non_interactive,
    };
    if let Some(file) = file {
        let args = crate::cli::apple::brew::InstallArgs {
            file: Some(file.to_owned()),
            options,
        };
        packages::install(
            None,
            &args,
            &environment,
            &mut executor,
            &mut report,
            verbose,
        )?;
    }
    // Read back the installation using the same discovery rules.
    homebrew::ensure(
        None,
        &crate::cli::apple::brew::Options {
            dry_run,
            non_interactive,
        },
        &environment,
        &mut executor,
        &mut report,
        verbose,
    )
    .map(|brew| brew.prefix)
}

pub(crate) fn initialize_preferences(verbose: bool) -> Result<()> {
    preferences::init(false, verbose)
}

#[cfg(test)]
#[path = "../../tests/unit/apple/brew.rs"]
mod brew_tests;
