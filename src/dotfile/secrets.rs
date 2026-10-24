use super::process;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct Secrets(Arc<Mutex<Cache>>);
#[derive(Default)]
struct Cache {
    values: BTreeMap<String, Value>,
    bw_session: Option<String>,
    bw_unlocked: bool,
}

impl Secrets {
    pub fn get(&self, provider: &str, args: &[String]) -> Result<Value> {
        let key = serde_json::to_string(&(provider, args))?;
        // Serialize unlock/read/cache so concurrent template evaluations cannot unlock twice.
        let mut cache = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Secret cache unavailable"))?;
        if let Some(value) = cache.values.get(&key) {
            return Ok(value.clone());
        }
        let first = args
            .first()
            .context("Secret function requires an item/reference")?;
        let mut input = None;
        let mut command = match provider {
            "onepassword" | "op" => {
                let mut command = Command::new("op");
                if first.starts_with("op://") {
                    command.args(["read", first, "--no-newline"]);
                } else {
                    command.args(["item", "get", first, "--format", "json"]);
                }
                command
            }
            "pass" | "gopass" => {
                let mut command = Command::new(provider);
                command.arg("show").args(args);
                command
            }
            "bitwarden" | "bw" => {
                if !cache.bw_unlocked
                    && cache.bw_session.is_none()
                    && std::env::var_os("BW_SESSION").is_none()
                {
                    let mut status = Command::new("bw");
                    status.arg("status");
                    let output = process::capture(status, None).context("Run bw status")?;
                    let value: Value =
                        serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
                    match value["status"].as_str() {
                        Some("unlocked") => {
                            cache.bw_unlocked = true;
                        }
                        Some("locked") => {
                            if std::env::var_os("BW_PASSWORD").is_none() {
                                bail!("Bitwarden is locked; unlock it or set BW_PASSWORD for this process");
                            }
                            let mut unlock = Command::new("bw");
                            unlock.args(["unlock", "--raw", "--passwordenv", "BW_PASSWORD"]);
                            let output = process::capture(unlock, None)?;
                            if !output.status.success() {
                                bail!("Bitwarden unlock failed");
                            }
                            cache.bw_session =
                                Some(String::from_utf8(output.stdout)?.trim().to_owned());
                        }
                        _ => bail!("Bitwarden is not logged in; authenticate with bw first"),
                    }
                }
                let mut command = Command::new("bw");
                command.arg("get");
                if args.len() == 1 {
                    command.arg("item").arg(first);
                } else {
                    command.args(args);
                }
                if let Some(session) = &cache.bw_session {
                    command.env("BW_SESSION", session);
                }
                command
            }
            "vault" => {
                let mut command = Command::new("vault");
                command.args(["kv", "get", "-format=json", first]);
                command
            }
            "keyring" => {
                let account = args
                    .get(1)
                    .context("keyring requires service and account")?;
                keyring_command(first, account)
            }
            "keepassxc" => {
                let entry = args
                    .get(1)
                    .context("keepassxc requires database and entry")?;
                let mut command = Command::new("keepassxc-cli");
                command.args(["show", "--show-protected", first, entry]);
                input = Some(
                    format!(
                        "{}\n",
                        std::env::var("STOOL_KEEPASS_PASSWORD")
                            .context("Set STOOL_KEEPASS_PASSWORD for KeePassXC")?
                    )
                    .into_bytes(),
                );
                command
            }
            "lastpass" => {
                let mut command = Command::new("lpass");
                command.args(["show", "--json", first]);
                command
            }
            "doppler" => {
                let mut command = Command::new("doppler");
                command.args(["secrets", "get", first, "--plain"]);
                command
            }
            "aws" => {
                let mut command = Command::new("aws");
                command.args([
                    "secretsmanager",
                    "get-secret-value",
                    "--secret-id",
                    first,
                    "--query",
                    "SecretString",
                    "--output",
                    "text",
                ]);
                command
            }
            "azure" => {
                let mut command = Command::new("az");
                command.args([
                    "keyvault", "secret", "show", "--id", first, "--query", "value", "--output",
                    "tsv",
                ]);
                command
            }
            "command" => {
                let mut command = Command::new(first);
                command.args(&args[1..]);
                command
            }
            _ => bail!("Unknown secret provider: {provider}"),
        };
        command.env("STOOL_SECRET_REQUEST", "1");
        let output = process::capture(command, input.as_deref()).with_context(|| {
            format!("Read secret using {provider}; install/authenticate its CLI first")
        })?;
        if !output.status.success() {
            bail!(
                "Secret provider {provider} failed ({}); provider output was withheld",
                output.status
            );
        }
        let text =
            String::from_utf8(output.stdout).context("Secret provider output must be UTF-8")?;
        let value = serde_json::from_str(&text)
            .unwrap_or_else(|_| Value::String(text.trim_end_matches(['\r', '\n']).to_owned()));
        cache.values.insert(key, value.clone());
        Ok(value)
    }
}

#[cfg(target_os = "macos")]
fn keyring_command(service: &str, account: &str) -> Command {
    let mut command = Command::new("security");
    command.args(["find-generic-password", "-s", service, "-a", account, "-w"]);
    command
}
#[cfg(all(unix, not(target_os = "macos")))]
fn keyring_command(service: &str, account: &str) -> Command {
    let mut command = Command::new("secret-tool");
    command.args(["lookup", "service", service, "account", account]);
    command
}
#[cfg(windows)]
fn keyring_command(service: &str, account: &str) -> Command {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; [Windows.Security.Credentials.PasswordVault,Windows.Security.Credentials,ContentType=WindowsRuntime] > $null; $v=New-Object Windows.Security.Credentials.PasswordVault; $c=$v.Retrieve($env:STOOL_KEYRING_SERVICE,$env:STOOL_KEYRING_ACCOUNT); $c.RetrievePassword(); [Console]::Write($c.Password)"]);
    command
        .env("STOOL_KEYRING_SERVICE", service)
        .env("STOOL_KEYRING_ACCOUNT", account);
    command
}
#[cfg(not(any(unix, windows)))]
fn keyring_command(_service: &str, _account: &str) -> Command {
    Command::new("secret-tool")
}
