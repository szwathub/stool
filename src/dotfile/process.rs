use super::storage::Paths;
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Output, Stdio};

/// Drain stdout while feeding stdin so large filters cannot deadlock on pipe capacity.
pub fn capture(mut command: Command, input: Option<&[u8]>) -> Result<Output> {
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().context("Start configured executable")?;
    let writer = input.map(|bytes| {
        let bytes = bytes.to_owned();
        let mut stdin = child.stdin.take().unwrap();
        std::thread::spawn(move || stdin.write_all(&bytes))
    });
    let output = child.wait_with_output()?;
    if let Some(writer) = writer {
        let result = writer
            .join()
            .map_err(|_| anyhow::anyhow!("Input writer failed"))?;
        if output.status.success() {
            result?;
        }
    }
    Ok(output)
}

pub fn script(
    paths: &Paths,
    name: &str,
    bytes: &[u8],
    input: Option<&[u8]>,
    transform: bool,
) -> Result<Vec<u8>> {
    let temporary = tempfile::Builder::new().prefix("stool-script-").tempdir()?;
    let path = temporary.path().join(if name.ends_with(".ps1") {
        "script.ps1"
    } else {
        "script"
    });
    filesystem::atomic_write(&path, bytes, 0o700)?;
    let first = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let mut argv = if let Some(shebang) = first.strip_prefix(b"#!") {
        shell_words::split(std::str::from_utf8(shebang)?.trim())?
    } else if name.ends_with(".ps1") || cfg!(windows) {
        vec![
            "powershell.exe".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
        ]
    } else {
        vec!["sh".into()]
    };
    if argv.is_empty() {
        bail!("Script has an empty interpreter: {name}");
    }
    if argv.first().is_some_and(|arg| arg.ends_with("/env"))
        && argv.get(1).is_some_and(|arg| arg == "-S")
    {
        argv.remove(1);
    }
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .arg(&path)
        .env("STOOL_SOURCE_DIR", &paths.source)
        .env("STOOL_DEST_DIR", &paths.destination)
        .env("STOOL_HOME", &paths.home)
        .env("STOOL_OS", std::env::consts::OS)
        .env("STOOL_ARCH", std::env::consts::ARCH)
        .env(
            "STOOL_EXECUTABLE",
            std::env::current_exe().context("Locate stool executable for hook")?,
        )
        .current_dir(if transform {
            temporary.path()
        } else {
            paths.destination.as_path()
        });
    if transform {
        let output = capture(command, input)?;
        if !output.status.success() {
            bail!("Transform script {name} failed ({})", output.status);
        }
        Ok(output.stdout)
    } else {
        let status = command
            .stdin(Stdio::null())
            .status()
            .with_context(|| format!("Run script {name}"))?;
        if !status.success() {
            bail!("Script {name} failed ({status}); it was not marked as successful");
        }
        Ok(Vec::new())
    }
}
