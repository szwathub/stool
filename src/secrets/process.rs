//! Bounded subprocess capture with borrowed secret input.
use super::LIMIT;
use age::secrecy::{ExposeSecret, SecretVec};
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Feed borrowed secret input without leaving an unzeroized writer-thread clone.
pub(super) fn capture(
    mut command: Command,
    input: Option<&[u8]>,
    executable: &str,
) -> Result<SecretVec<u8>> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("STOOL_AGE_PASSPHRASE");
    let mut child = command
        .spawn()
        .with_context(|| format!("Start {executable}; install it and make it available on PATH"))?;
    let output = std::thread::scope(|scope| -> Result<_> {
        let writer = input.map(|bytes| {
            let mut stdin = child.stdin.take().unwrap();
            scope.spawn(move || stdin.write_all(bytes))
        });
        let output = child.wait_with_output()?;
        if let Some(writer) = writer {
            let result = writer
                .join()
                .map_err(|_| anyhow::anyhow!("Key input writer failed"))?;
            if output.status.success() {
                result?;
            }
        }
        Ok(output)
    })?;
    let stdout = SecretVec::new(output.stdout);
    let _stderr = SecretVec::new(output.stderr);
    if !output.status.success() {
        bail!("{executable} operation failed ({}); check the selected keys, agent and original key passphrases",output.status);
    }
    if stdout.expose_secret().len() > LIMIT as usize {
        bail!("Key command output exceeds the size limit");
    }
    Ok(stdout)
}
