use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Output, Stdio};

const DOMAIN: &str = "com.apple.dock";
const KEY: &str = "mineffect";
const VALUE: &str = "suck";

pub fn init(dry_run: bool, verbose: bool) -> Result<()> {
    configure(dry_run, verbose, defaults, &mut std::io::stdout().lock())
}

fn defaults(arguments: &[&str]) -> Result<Output> {
    Command::new("/usr/bin/defaults")
        .args(arguments)
        // Missing-key diagnostics are recognized narrowly; other errors must stop initialization.
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .output()
        .context("Start /usr/bin/defaults")
}

fn read(execute: &mut impl FnMut(&[&str]) -> Result<Output>) -> Result<Option<String>> {
    let output = execute(&["read", DOMAIN, KEY])
        .with_context(|| format!("Read macOS preference {DOMAIN} {KEY}"))?;
    if output.status.success() {
        let value = String::from_utf8(output.stdout).context("Decode macOS preference as UTF-8")?;
        // Remove the tool's single line terminator, preserving whitespace in the actual value.
        return Ok(Some(value.strip_suffix('\n').unwrap_or(&value).to_owned()));
    }
    let error = String::from_utf8_lossy(&output.stderr);
    let missing = format!("The domain/default pair of ({DOMAIN}, {KEY}) does not exist");
    if output.status.code() == Some(1) && error.lines().any(|line| line == missing) {
        return Ok(None);
    }
    bail!(
        "Read macOS preference {DOMAIN} {KEY} failed ({}): {}",
        output.status,
        error.trim()
    );
}

fn configure(
    dry_run: bool,
    verbose: bool,
    mut execute: impl FnMut(&[&str]) -> Result<Output>,
    report: &mut impl Write,
) -> Result<()> {
    let current = read(&mut execute)?;
    if verbose {
        writeln!(
            report,
            "[VERBOSE] macOS preference {DOMAIN} {KEY}: {current:?}"
        )?;
    }
    if current.as_deref() == Some(VALUE) {
        writeln!(report, "Unchanged: {DOMAIN} {KEY} = {VALUE}")?;
        return Ok(());
    }
    let previous = current.as_deref().unwrap_or("<unset>");
    if dry_run {
        writeln!(
            report,
            "Would set: {DOMAIN} {KEY}: {previous:?} -> {VALUE:?}"
        )?;
        return Ok(());
    }
    let output = execute(&["write", DOMAIN, KEY, "-string", VALUE])
        .with_context(|| format!("Write macOS preference {DOMAIN} {KEY}"))?;
    if !output.status.success() {
        bail!(
            "Write macOS preference {DOMAIN} {KEY} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let saved = read(&mut execute).context("Verify saved macOS preference")?;
    if saved.as_deref() != Some(VALUE) {
        bail!("macOS preference {DOMAIN} {KEY} did not retain {VALUE:?}; found {saved:?}");
    }
    writeln!(report, "Updated: {DOMAIN} {KEY}: {previous:?} -> {VALUE:?}")?;
    writeln!(
        report,
        "If the animation has not changed, restart Dock or log out and back in."
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/apple/preferences.rs"]
mod tests;
