#[cfg(target_os = "macos")]
mod apple;
mod cli;
mod common;
mod dotfile;
mod help;
mod init;
mod secrets;

use clap::Parser;
use cli::Cli;

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        #[cfg(target_os = "macos")]
        cli::Commands::Apple(cmd) => apple::run(&cmd, cli.verbose)?,
        cli::Commands::Base(command) => match command {
            cli::BaseCommands::Init(cmd) => init::run(&cmd, cli.verbose)?,
            cli::BaseCommands::Dotfile(cmd) => dotfile::run(&cmd, cli.verbose)?,
            cli::BaseCommands::Secrets(cmd) => secrets::run(&cmd)?,
            cli::BaseCommands::Completions { shell } => help::completions(shell),
            cli::BaseCommands::Man { output } => help::man(output.as_deref())?,
        },
    }
    Ok(())
}
