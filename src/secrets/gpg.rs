use super::{
    archive::add,
    paths::{checked, read},
    process::capture,
    Files,
};
use crate::common::fs as filesystem;
use age::secrecy::{ExposeSecret, SecretVec};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inventory {
    pub groups: KeyGroups,
    pub revocations: Vec<String>,
}
fn valid_fingerprint(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}
fn command(home: &Path) -> Command {
    let mut command = Command::new("gpg");
    command
        .args(["--no-options", "--batch", "--homedir"])
        .arg(home)
        .env("GNUPGHOME", home);
    command
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SecretMaterial {
    Local,
    Stub,
    Token,
}

/// Primary fingerprint to all of its primary/subkey fingerprints and private material.
pub(super) type KeyGroups = BTreeMap<String, BTreeMap<String, SecretMaterial>>;

fn key_groups(bytes: &[u8], kind: &str) -> Result<KeyGroups> {
    let text = std::str::from_utf8(bytes).context("Invalid GPG key listing")?;
    let subkind = if kind == "sec" { "ssb" } else { "sub" };
    let mut groups = KeyGroups::new();
    let mut seen = BTreeSet::new();
    let mut current = None;
    let mut pending = None;
    for line in text.lines() {
        let fields: Vec<_> = line.split(':').collect();
        match fields[0] {
            "sec" | "pub" | "ssb" | "sub" => {
                if pending.is_some() {
                    bail!("GPG key fingerprint is missing");
                }
                let primary = fields[0] == kind;
                if !primary && fields[0] != subkind {
                    bail!("GPG backup must contain {kind} primary keys and {subkind} subkeys");
                }
                if !primary && current.is_none() {
                    bail!("GPG subkey has no primary key");
                }
                // GnuPG's colon field 15 marks missing material with '#', a token
                // serial for hardware keys, and '+' for local material with
                // --with-secret. Ordinary exported secret packets leave it empty.
                let material = if kind == "sec" {
                    match fields.get(14).copied().unwrap_or("") {
                        "" | "+" => SecretMaterial::Local,
                        "#" => SecretMaterial::Stub,
                        _ => SecretMaterial::Token,
                    }
                } else {
                    SecretMaterial::Local
                };
                pending = Some((primary, material));
            }
            "fpr" => {
                let Some((primary, material)) = pending.take() else {
                    continue;
                };
                let key = fields.get(9).context("GPG key fingerprint is missing")?;
                if !valid_fingerprint(key) || !seen.insert((*key).to_owned()) {
                    bail!("GPG key listing has an invalid or duplicate fingerprint");
                }
                if primary {
                    groups.insert(
                        (*key).to_owned(),
                        BTreeMap::from([((*key).to_owned(), material)]),
                    );
                    current = Some((*key).to_owned());
                } else {
                    groups
                        .get_mut(current.as_ref().context("GPG subkey has no primary key")?)
                        .context("GPG subkey has no primary key")?
                        .insert((*key).to_owned(), material);
                }
            }
            _ => (),
        }
    }
    if pending.is_some() {
        bail!("GPG key fingerprint is missing");
    }
    Ok(groups)
}

fn discover_keys(home: &Path) -> Result<KeyGroups> {
    let mut command = command(home);
    command.args([
        "--with-colons",
        "--with-secret",
        "--fingerprint",
        "--fingerprint",
        "--list-secret-keys",
    ]);
    key_groups(capture(command, None, "gpg")?.expose_secret(), "sec")
}

fn exportable_keys(inventory: &KeyGroups) -> KeyGroups {
    inventory.iter().filter_map(|(primary, components)| {
        if components.values().any(|material| *material == SecretMaterial::Local) {
            Some((primary.clone(), components.clone()))
        } else {
            eprintln!("Skipping GPG key {primary}: no exportable local private material (missing keys or hardware-token references).");
            None
        }
    }).collect()
}

fn validate_exported_groups(
    bytes: &[u8],
    kind: &str,
    expected: &KeyGroups,
    check_material: bool,
) -> Result<()> {
    let actual = key_groups(bytes, kind)?;
    if !actual.keys().eq(expected.keys()) {
        bail!("GPG backup fingerprint set mismatch; missing selected primary keys");
    }
    for (primary, components) in &actual {
        let expected = &expected[primary];
        if !components.keys().eq(expected.keys()) {
            bail!("GPG backup fingerprint set mismatch for {primary}; missing or unexpected primary keys or subkeys");
        }
        if kind == "sec" && check_material {
            // show-only leaves this field empty even for some dummy packets.
            // It proves structure, while successful export and stable inventory
            // protect the local-material selection. Reject explicit stubs too.
            for (key, material) in components {
                if expected[key] == SecretMaterial::Local && *material != SecretMaterial::Local {
                    bail!("GPG secret export is missing local private material for {key}");
                }
            }
        }
    }
    Ok(())
}

fn ownertrust(bytes: &[u8], expected: &BTreeSet<String>, filter: bool) -> Result<SecretVec<u8>> {
    let text = std::str::from_utf8(bytes).context("Invalid GPG ownertrust data")?;
    let mut output = String::new();
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        if filter && (line.starts_with('#') || line.is_empty()) {
            continue;
        }
        let fields: Vec<_> = line.split(':').collect();
        if filter && fields.first().is_some_and(|key| !expected.contains(*key)) {
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if fields.len() != 3
            || !expected.contains(fields[0])
            || fields[1].len() != 1
            || !matches!(fields[1].as_bytes()[0], b'1'..=b'6')
            || !fields[2].is_empty()
            || !seen.insert(fields[0])
        {
            bail!("GPG backup ownertrust contains an invalid, duplicate or unselected key");
        }
        output.push_str(line);
        output.push('\n');
    }
    Ok(SecretVec::new(output.into_bytes()))
}

pub(super) fn paths(inventory: &Inventory) -> Result<BTreeSet<String>> {
    if inventory.groups.is_empty() {
        bail!("GPG backup inventory is empty");
    }
    let mut seen = BTreeSet::new();
    for (primary, components) in &inventory.groups {
        if !valid_fingerprint(primary)
            || !components.contains_key(primary)
            || !components.values().any(|v| *v == SecretMaterial::Local)
        {
            bail!("Invalid GPG backup inventory");
        }
        for key in components.keys() {
            if !valid_fingerprint(key) || !seen.insert(key) {
                bail!("Invalid or duplicate GPG key fingerprint");
            }
        }
    }
    let mut paths = BTreeSet::from([
        "gpg/public.asc".into(),
        "gpg/secret.asc".into(),
        "gpg/ownertrust.txt".into(),
    ]);
    for name in &inventory.revocations {
        let fingerprint = name
            .strip_suffix(".rev")
            .context("Invalid GPG revocation filename")?;
        if !inventory.groups.contains_key(fingerprint)
            || !paths.insert(format!("gpg/revocations/{name}"))
        {
            bail!("Invalid or duplicate GPG revocation certificate");
        }
    }
    Ok(paths)
}

pub(super) fn export(
    home: &Path,
    files: &mut Files,
    warnings: &mut Vec<String>,
) -> Result<Option<Inventory>> {
    if !home.exists() {
        return Ok(None);
    }
    let inventory = discover_keys(home)?;
    let groups = exportable_keys(&inventory);
    if groups.is_empty() {
        return Ok(None);
    }
    for (primary, components) in &inventory {
        for (key, material) in components {
            if *material != SecretMaterial::Local {
                warnings.push(format!(
                    "GPG {primary}: private material for {key} is unavailable ({material:?})"
                ));
            }
        }
    }
    let selected = groups.keys().cloned().collect::<BTreeSet<_>>();
    let mut public = command(home);
    public.arg("--armor").arg("--export").args(&selected);
    let public = capture(public, None, "gpg")?;
    let mut secret = Command::new("gpg");
    // Passphrase entry belongs to the agent/pinentry; the export may have no TTY.
    secret
        .args(["--no-options", "--no-tty", "--homedir"])
        .arg(home)
        .env("GNUPGHOME", home)
        .args(["--armor", "--export-secret-keys"])
        .args(&selected);
    let secret = capture(secret, None, "gpg")?;
    let mut trust = command(home);
    trust.arg("--export-ownertrust");
    let trust = ownertrust(
        capture(trust, None, "gpg")?.expose_secret(),
        &selected,
        true,
    )?;
    if discover_keys(home)? != inventory {
        bail!("GPG inventory changed during export; retry; no backup was written");
    }
    add(files, "gpg/public.asc".into(), public)?;
    add(files, "gpg/secret.asc".into(), secret)?;
    add(files, "gpg/ownertrust.txt".into(), trust)?;
    let mut revocations = Vec::new();
    let directory = checked(&home.join("openpgp-revocs.d"))?;
    if directory.is_dir() {
        for primary in &selected {
            let name = format!("{primary}.rev");
            let path = directory.join(&name);
            if fs::symlink_metadata(&path).is_ok() {
                add(files, format!("gpg/revocations/{name}"), read(&path)?)?;
                revocations.push(name);
            }
        }
    }
    let inventory = Inventory {
        groups,
        revocations,
    };
    validate_at(&inventory, files, home, true)?;
    Ok(Some(inventory))
}

pub(super) fn validate(inventory: &Inventory, files: &Files) -> Result<()> {
    paths(inventory)?;
    let temporary = tempfile::Builder::new()
        .prefix("stool-secrets-check-")
        .tempdir_in(if cfg!(target_os = "macos") {
            std::path::PathBuf::from("/private/tmp")
        } else {
            std::env::temp_dir()
        })?;
    filesystem::set_mode(temporary.path(), 0o700)?;
    // GPG 2.2 marks show-only secret packets as unavailable in an empty
    // keyring. Compare complete packet fingerprints here; verify actual local
    // private material after import, before reporting successful restoration.
    let result = validate_at(inventory, files, temporary.path(), false);
    let mut cleanup = Command::new("gpgconf");
    let _ = cleanup
        .arg("--homedir")
        .arg(temporary.path())
        .args(["--kill", "all"])
        .output();
    result
}

fn validate_at(
    inventory: &Inventory,
    files: &Files,
    home: &Path,
    check_material: bool,
) -> Result<()> {
    for (kind, name) in [("pub", "gpg/public.asc"), ("sec", "gpg/secret.asc")] {
        let mut inspect = command(home);
        inspect.args([
            "--with-colons",
            "--fingerprint",
            "--fingerprint",
            "--import-options",
            "show-only",
            "--import",
        ]);
        let listing = capture(inspect, Some(files[name].expose_secret()), "gpg")?;
        validate_exported_groups(
            listing.expose_secret(),
            kind,
            &inventory.groups,
            check_material,
        )?;
    }
    ownertrust(
        files["gpg/ownertrust.txt"].expose_secret(),
        &inventory.groups.keys().cloned().collect(),
        false,
    )?;
    Ok(())
}

/// Refuse redirected keyring storage before importing any material. Existing
/// GPG agent sockets are allowed, but links anywhere in the keyring are not.
pub(super) fn preflight(target: &Path) -> Result<()> {
    fn inspect(path: &Path) -> Result<()> {
        checked(path)?;
        super::paths::outside_git(path)?;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("Inspect GPG restore directory"),
        };
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                inspect(&entry?.path())?;
            }
        }
        Ok(())
    }
    let home = target.join(".gnupg");
    inspect(&home)?;
    if home.exists() && !home.is_dir() {
        bail!("GPG restore target must be a directory");
    }
    for name in ["private-keys-v1.d", "openpgp-revocs.d", "public-keys.d"] {
        let path = home.join(name);
        if path.exists() && !path.is_dir() {
            bail!("GPG key storage must be a directory");
        }
    }
    for name in [
        "pubring.kbx",
        "pubring.gpg",
        "secring.gpg",
        "trustdb.gpg",
        "tofu.db",
    ] {
        let path = home.join(name);
        if path.exists() && !path.is_file() {
            bail!("GPG key database must be a regular file");
        }
    }
    Ok(())
}

pub(super) fn restore(target: &Path, inventory: &Inventory, files: &Files) -> Result<()> {
    preflight(target)?;
    let home = checked(&target.join(".gnupg"))?;
    if !target.exists() {
        filesystem::private_dir(target)?;
    }
    filesystem::private_dir(&home)?;
    for (flag, name) in [
        ("--import", "gpg/public.asc"),
        ("--import", "gpg/secret.asc"),
        ("--import-ownertrust", "gpg/ownertrust.txt"),
    ] {
        let mut import = command(&home);
        import.arg(flag);
        capture(import, Some(files[name].expose_secret()), "gpg")?;
    }
    // Check that all locally exportable private components survived the import.
    let restored = discover_keys(&home)?;
    for (primary, components) in &inventory.groups {
        let actual = restored
            .get(primary)
            .context("Restored GPG primary key is missing")?;
        for (key, material) in components {
            if *material == SecretMaterial::Local && actual.get(key) != Some(&SecretMaterial::Local)
            {
                bail!("Restored GPG private component is missing: {key}");
            }
        }
    }
    Ok(())
}

pub(crate) fn available() -> Result<bool> {
    let output = match Command::new("gpg")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).context("Cannot execute gpg; repair PATH or executable permissions")
        }
    };
    if !output.status.success() || !output.stdout.starts_with(b"gpg (GnuPG") {
        bail!("gpg is present but unusable; repair it before restoring GPG keys");
    }
    Ok(true)
}
