use crate::common::gpg::{fake_gpg, private_write};
use crate::common::*;

const A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const PRIVATE: &[u8] = include_bytes!("../fixtures/offline-keys/test_ed25519");
const PUBLIC: &[u8] = include_bytes!("../fixtures/offline-keys/test_ed25519.pub");
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    password: PathBuf,
}
impl Fixture {
    fn new(ssh: bool) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = git_path(&fs::canonicalize(temporary.path()).unwrap());
        let home = root.join("source home");
        let bin = root.join("tools on PATH");
        fs::create_dir_all(home.join(".gnupg")).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::copy(
            fake_gpg(),
            bin.join(if cfg!(windows) { "gpg.exe" } else { "gpg" }),
        )
        .unwrap();
        let password = root.join("private password");
        private_write(&password, b"offline test password\n");
        write(
            &home.join(".stool/dotfile/config.toml"),
            "malformed and unrelated dotfile configuration",
        );
        if ssh {
            private_write(&home.join(".ssh/custom identity"), PRIVATE);
            private_write(&home.join(".ssh/custom identity.pub"), PUBLIC);
        }
        Self {
            _temp: temporary,
            root,
            home,
            bin,
            password,
        }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_stool"));
        c.arg("secrets")
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("GNUPGHOME", self.home.join(".gnupg"))
            .env("STOOL_TEST_GPG_ROOT", &self.root)
            .env("PATH", &self.bin)
            .env_remove("STOOL_AGE_PASSPHRASE");
        c
    }
    fn backup(&self, extra: &[&str]) -> PathBuf {
        let file = self.root.join("keys.tar.age");
        success(
            self.command()
                .arg("backup")
                .args(extra)
                .arg("--output")
                .arg(&file)
                .arg("--passphrase-file")
                .arg(&self.password)
                .output()
                .unwrap(),
        );
        file
    }
    fn restore(&self, file: &Path, target: &Path, extra: &[&str]) -> Output {
        self.command()
            .arg("restore")
            .args(extra)
            .arg("--input")
            .arg(file)
            .arg("--output")
            .arg(target)
            .arg("--passphrase-file")
            .arg(&self.password)
            .output()
            .unwrap()
    }
}
#[test]
fn default_all_is_standalone_encrypted_and_restores_both_categories() {
    let f = Fixture::new(true);
    let file = f.backup(&[]);
    let cipher = fs::read(&file).unwrap();
    let payload = decrypt_age_fixture("offline test password", &cipher);
    let mut archive = tar::Archive::new(payload.as_slice());
    let mut entries = archive.entries().unwrap();
    let mut metadata = entries.next().unwrap().unwrap();
    assert_eq!(
        metadata.path().unwrap().as_ref(),
        Path::new("manifest.json")
    );
    let manifest: serde_json::Value = serde_json::from_reader(&mut metadata).unwrap();
    assert_eq!(manifest["format"], "stool-key-backup");
    assert_eq!(manifest["version"], 1);
    for secret in [
        b"BEGIN OPENSSH PRIVATE KEY".as_slice(),
        b"STOOL-FAKE-SECRET",
        b"manifest.json",
        A.as_bytes(),
        b"stool-test-only",
    ] {
        assert!(!cipher.windows(secret.len()).any(|part| part == secret));
    }
    let target = f.root.join("restored user");
    success(f.restore(&file, &target, &[]));
    assert_eq!(
        fs::read(target.join(".ssh/custom identity")).unwrap(),
        PRIVATE
    );
    assert_eq!(
        fs::read(target.join(".ssh/custom identity.pub")).unwrap(),
        PUBLIC
    );
    assert_eq!(
        fs::read_to_string(f.root.join("imports"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    success(f.restore(&file, &target, &[]));
    assert!(!target.join(".stool").exists());
}
#[test]
fn backup_defaults_to_current_directory_and_restore_to_current_home() {
    let f = Fixture::new(true);
    success(
        f.command()
            .args(["backup", "--ssh", "--passphrase-file"])
            .arg(&f.password)
            .output()
            .unwrap(),
    );
    let files = fs::read_dir(&f.root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|v| v == "age"))
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let name = files[0].file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("stool-keys-") && name.ends_with(".tar.age"));
    let destination = f.root.join("new default home");
    fs::create_dir(&destination).unwrap();
    success(
        f.command()
            .env("HOME", &destination)
            .env("USERPROFILE", &destination)
            .args(["restore", "--input"])
            .arg(&files[0])
            .arg("--passphrase-file")
            .arg(&f.password)
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read(destination.join(".ssh/custom identity")).unwrap(),
        PRIVATE
    );
}
#[test]
fn explicit_source_directory_overrides_the_current_user_keyring() {
    let f = Fixture::new(false);
    let input = f.root.join("another user");
    private_write(&input.join(".ssh/another"), PRIVATE);
    let file = f.root.join("another.tar.age");
    success(
        f.command()
            .args(["backup", "--ssh", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&file)
            .arg("--passphrase-file")
            .arg(&f.password)
            .output()
            .unwrap(),
    );
    let target = f.root.join("restored");
    success(f.restore(&file, &target, &[]));
    assert_eq!(fs::read(target.join(".ssh/another")).unwrap(), PRIVATE);
    assert!(target.join(".ssh/another.pub").is_file());
}
#[test]
fn category_selection_only_restores_the_requested_keys() {
    let f = Fixture::new(true);
    let file = f.backup(&[]);
    let target = f.root.join("ssh only");
    success(f.restore(&file, &target, &["--ssh"]));
    assert!(!target.join(".gnupg").exists());
    assert!(!f.root.join("imports").exists());
    let target = f.root.join("gpg only");
    success(f.restore(&file, &target, &["--gpg"]));
    assert!(!target.join(".ssh").exists());
}
#[test]
fn categories_conflict_and_restore_requires_input_before_any_io() {
    let f = Fixture::new(false);
    for command in ["backup", "restore"] {
        failure(
            f.command()
                .arg(command)
                .args(["--gpg", "--ssh"])
                .output()
                .unwrap(),
            "cannot be used with",
        );
        failure(
            f.command()
                .arg(command)
                .args(["--all", "--ssh"])
                .output()
                .unwrap(),
            "cannot be used with",
        );
    }
    failure(f.command().arg("restore").output().unwrap(), "--input");
    assert!(!f.root.join("events").exists());
}
#[test]
fn backup_never_overwrites_an_existing_file_and_rejects_git_workspaces() {
    let f = Fixture::new(true);
    let file = f.backup(&["--ssh"]);
    let before = fs::read(&file).unwrap();
    failure(
        f.command()
            .args(["backup", "--output"])
            .arg(&file)
            .output()
            .unwrap(),
        "already exists",
    );
    assert_eq!(fs::read(&file).unwrap(), before);
    let repository = f.root.join("git workspace");
    fs::create_dir_all(repository.join(".git")).unwrap();
    failure(
        f.command()
            .current_dir(&repository)
            .arg("backup")
            .output()
            .unwrap(),
        "outside Git workspaces",
    );
    failure(
        f.restore(&file, &repository, &["--ssh"]),
        "outside Git workspaces",
    );
    assert!(!repository.join(".ssh").exists());
}
#[test]
fn wrong_password_damage_and_file_conflicts_do_not_restore_any_keys() {
    let f = Fixture::new(true);
    let file = f.backup(&[]);
    let target = f.root.join("target");
    let wrong = f.root.join("wrong password");
    private_write(&wrong, b"wrong\n");
    let result = f
        .command()
        .args(["restore", "--input"])
        .arg(&file)
        .arg("--output")
        .arg(&target)
        .arg("--passphrase-file")
        .arg(&wrong)
        .output()
        .unwrap();
    failure(result, "Unable to decrypt");
    assert!(!target.exists());
    assert!(!f.root.join("imports").exists());
    let damaged = f.root.join("damaged.age");
    let mut bytes = fs::read(&file).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&damaged, bytes).unwrap();
    let result = f.restore(&damaged, &target, &[]);
    assert!(!result.status.success());
    assert!(!target.exists());
    private_write(
        &target.join(".ssh/custom identity"),
        b"existing private key",
    );
    failure(f.restore(&file, &target, &[]), "conflicts with backup");
    assert_eq!(
        fs::read(target.join(".ssh/custom identity")).unwrap(),
        b"existing private key"
    );
    assert!(!f.root.join("imports").exists());
}
#[test]
fn gpg_backup_keeps_full_groups_and_revocations_and_checks_imported_private_material() {
    let f = Fixture::new(false);
    let revocation = format!("{A}.rev");
    private_write(
        &f.home.join(".gnupg/openpgp-revocs.d").join(&revocation),
        b"test-only revocation certificate",
    );
    let file = f.backup(&["--gpg"]);
    let target = f.root.join("target");
    success(f.restore(&file, &target, &[]));
    assert_eq!(
        fs::read(target.join(".gnupg/openpgp-revocs.d").join(revocation)).unwrap(),
        b"test-only revocation certificate"
    );
    let events = fs::read_to_string(f.root.join("events")).unwrap();
    assert!(events.contains("--list-secret-keys"));
}
#[test]
fn empty_requested_categories_fail_before_password_or_output_creation() {
    let f = Fixture::new(false);
    failure(
        f.command().args(["backup", "--ssh"]).output().unwrap(),
        "No exportable keys",
    );
    write(&f.root.join("no-secret"), "");
    failure(
        f.command().args(["backup", "--gpg"]).output().unwrap(),
        "No exportable keys",
    );
    assert!(!fs::read_dir(&f.root).unwrap().any(|e| e
        .unwrap()
        .path()
        .extension()
        .is_some_and(|v| v == "age")));
}
#[test]
fn ssh_companion_mismatch_and_missing_archive_category_are_rejected() {
    let f = Fixture::new(true);
    let file = f.backup(&["--ssh"]);
    failure(
        f.restore(&file, &f.root.join("gpg only"), &["--gpg"]),
        "no keys of the selected category",
    );
    fs::write(f.home.join(".ssh/custom identity.pub"), b"not a public key").unwrap();
    let result = f
        .command()
        .args(["backup", "--ssh", "--passphrase-file"])
        .arg(&f.password)
        .output()
        .unwrap();
    assert!(!result.status.success());
}
#[cfg(unix)]
#[test]
fn symlinks_and_restore_permissions_are_checked_without_following_links() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let f = Fixture::new(true);
    let file = f.backup(&["--ssh"]);
    let target = f.root.join("target");
    success(f.restore(&file, &target, &[]));
    assert_eq!(
        fs::metadata(target.join(".ssh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(target.join(".ssh/custom identity"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let redirected = f.root.join("redirected");
    fs::create_dir(&redirected).unwrap();
    let user = f.root.join("symlink user");
    fs::create_dir(&user).unwrap();
    symlink(&redirected, user.join(".ssh")).unwrap();
    failure(f.restore(&file, &user, &[]), "symlink");
    assert_eq!(fs::read_dir(&redirected).unwrap().count(), 0);
    let link = f.root.join("linked.age");
    symlink(&file, &link).unwrap();
    failure(f.restore(&link, &target, &[]), "symlink");
}

#[test]
fn incomplete_gpg_exports_or_changed_inventory_cannot_publish_a_backup() {
    for marker in [
        "partial-export",
        "partial-secret-primary",
        "partial-secret-subkey",
        "partial-public-subkey",
        "stub-secret-export",
        "inventory-changed",
        "missing-primary-fingerprint",
        "missing-subkey-fingerprint",
        "fail-secret-export",
        "fail-trust-export",
    ] {
        let f = Fixture::new(true);
        write(&f.root.join(marker), "");
        let output = f.root.join("rejected.tar.age");
        let result = f
            .command()
            .args(["backup", "--output"])
            .arg(&output)
            .arg("--passphrase-file")
            .arg(&f.password)
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "Unexpected backup success for {marker}"
        );
        assert!(!output.exists());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("STOOL-FAKE-SECRET"));
    }
}
#[test]
fn unavailable_gpg_components_are_reported_and_failed_imports_never_write_ssh() {
    let f = Fixture::new(true);
    write(&f.root.join("stub-only-work"), "");
    let file = f.backup(&[]);
    let target = f.root.join("only exportable");
    let result = f.restore(&file, &target, &[]);
    assert!(String::from_utf8_lossy(&result.stderr).contains("unavailable"));
    success(result);
    write(&f.root.join("fail-import"), "");
    let target = f.root.join("failed import");
    failure(f.restore(&file, &target, &[]), "GPG restore failed");
    assert!(!target.join(".ssh").exists());
    let f = Fixture::new(false);
    write(&f.root.join("all-stubs"), "");
    failure(
        f.command().args(["backup", "--gpg"]).output().unwrap(),
        "No exportable keys",
    );
}
#[cfg(unix)]
#[test]
fn existing_gpg_storage_links_and_loose_password_files_are_rejected() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let f = Fixture::new(true);
    let file = f.backup(&[]);
    let outside = f.root.join("outside");
    fs::create_dir(&outside).unwrap();
    for name in ["private-keys-v1.d", "pubring.kbx", "trustdb.gpg"] {
        let target = f.root.join(name);
        fs::create_dir_all(target.join(".gnupg")).unwrap();
        symlink(&outside, target.join(".gnupg").join(name)).unwrap();
        failure(f.restore(&file, &target, &[]), "symlink");
        assert!(!target.join(".ssh").exists());
        assert!(!f.root.join("imports").exists());
    }
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::set_permissions(&f.password, fs::Permissions::from_mode(0o644)).unwrap();
    failure(
        f.restore(&file, &f.root.join("bad password mode"), &[]),
        "private (0600)",
    );
}

#[test]
fn losing_private_material_after_import_is_not_reported_as_success() {
    let f = Fixture::new(true);
    let file = f.backup(&[]);
    write(&f.root.join("stub-subkey"), "");
    let target = f.root.join("missing restored private component");
    let result = f.restore(&file, &target, &[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("Restored GPG private component is missing"));
    assert!(!target.join(".ssh").exists());
}
#[test]
fn password_spaces_are_preserved_and_directory_inputs_are_rejected() {
    let f = Fixture::new(true);
    private_write(&f.password, "  test's 离线密码  \n".as_bytes());
    let file = f.backup(&["--ssh"]);
    success(f.restore(&file, &f.root.join("spaced password"), &[]));
    private_write(&f.password, "test's 离线密码\n".as_bytes());
    failure(
        f.restore(&file, &f.root.join("trimmed password"), &[]),
        "Unable to decrypt",
    );
    let directory_input = f.root.join("directory backup");
    fs::create_dir(&directory_input).unwrap();
    private_write(&directory_input.join("public.asc"), b"old public file");
    private_write(
        &directory_input.join("secret.asc.age"),
        b"old secret ciphertext",
    );
    let target = f.root.join("directory target");
    assert!(!f.restore(&directory_input, &target, &[]).status.success());
    assert!(!target.exists());
}
