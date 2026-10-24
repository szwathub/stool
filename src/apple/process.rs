use anyhow::{bail, Context, Result};
use std::process::{Command, ExitStatus, Output, Stdio};

pub(super) trait Executor {
    fn capture(&mut self, command: Command) -> Result<Output>;
    fn run(&mut self, command: Command) -> Result<ExitStatus>;
}

pub(super) struct System;
impl Executor for System {
    fn capture(&mut self, mut command: Command) -> Result<Output> {
        command
            .stdin(Stdio::null())
            .output()
            .context("Start executable")
    }
    fn run(&mut self, mut command: Command) -> Result<ExitStatus> {
        command.status().context("Start executable")
    }
}

pub(super) fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.env("HOMEBREW_NO_AUTO_UPDATE", "1");
    command
}

pub(super) fn capture(
    executor: &mut impl Executor,
    command: Command,
    label: &str,
) -> Result<Output> {
    let output = executor
        .capture(command)
        .with_context(|| label.to_owned())?;
    if !output.status.success() {
        bail!(
            "{label} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

pub(super) fn run(
    executor: &mut impl Executor,
    mut command: Command,
    non_interactive: bool,
    label: &str,
) -> Result<()> {
    command.stdin(if non_interactive {
        Stdio::null()
    } else {
        Stdio::inherit()
    });
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    command
        .env_remove("INTERACTIVE")
        .env_remove("NONINTERACTIVE");
    if non_interactive {
        command.env("NONINTERACTIVE", "1");
    }
    let status = executor.run(command).with_context(|| label.to_owned())?;
    if !status.success() {
        bail!("{label} failed ({status}); completed installations are retained, fix the error above and retry");
    }
    Ok(())
}
