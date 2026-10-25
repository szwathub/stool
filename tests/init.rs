mod common;
use common::*;

fn command(sandbox: &Sandbox) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_stool"));
    command
        .current_dir(&sandbox.root)
        .env("HOME", &sandbox.home)
        .env("USERPROFILE", &sandbox.home)
        .arg("init");
    command
}
#[test]
fn preview_validates_inputs_but_never_decrypts_contacts_or_saves() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let backup = s.root.join("not-an-archive.age");
    fs::write(&backup, "not decrypted during preview").unwrap();
    let manifest = s.root.join("Brewfile");
    fs::write(&manifest, "system('touch should-not-exist')").unwrap();
    let output = command(&s)
        .args([
            "--dotfile-url",
            "https://example.invalid/private.git",
            "--dry-run",
            "--key-file",
        ])
        .arg(&backup)
        .arg("--brewfile")
        .arg(&manifest)
        .arg("--ssh-key")
        .arg(s.home.join(".ssh/not-yet-restored"))
        .output()
        .unwrap();
    assert!(success(output).contains("backup not decrypted"));
    assert!(!s.home.join(".stool").exists());
    assert!(!s.root.join("should-not-exist").exists());
    assert!(!s.source.exists());
}
#[test]
fn missing_invalid_files_and_bad_urls_fail_before_writes() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    failure(
        command(&s)
            .args(["--dotfile-url", "https://user:secret@example.invalid/repo"])
            .output()
            .unwrap(),
        "credential helper",
    );
    for flag in ["--key-file", "--brewfile"] {
        let output = command(&s)
            .args(["--dotfile-url", &s.url(), flag])
            .arg(s.root.join("missing"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!s.home.join(".stool").exists());
    }
    failure(command(&s).output().unwrap(), "--dotfile-url");
    assert!(!s.home.join(".stool").exists());
}
#[test]
fn saved_defaults_are_read_without_touching_dotfile_configuration() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    write(
        &s.home.join(".stool/config.toml"),
        &format!("version = 1\n[dotfile]\nurl = {:?}\n", s.url()),
    );
    write(
        &s.control().join("config.toml"),
        "malformed unrelated deployment config",
    );
    let before = snapshot(&s.home);
    assert!(success(command(&s).arg("--dry-run").output().unwrap()).contains(&s.url()));
    assert_eq!(snapshot(&s.home), before);
}
#[test]
fn dotfile_init_uses_global_url_preserves_explicit_override_and_persists_recursion() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    write(
        &s.home.join(".stool/config.toml"),
        &format!("version = 1\n[dotfile]\nurl = {:?}\n", s.url()),
    );
    success(
        s.command()
            .arg("--source")
            .arg(&s.source)
            .arg("--destination")
            .arg(&s.destination)
            .arg("init")
            .output()
            .unwrap(),
    );
    let config: toml::Value = fs::read_to_string(s.control().join("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config["repo"].as_str(), Some(s.url().as_str()));
    assert_eq!(config["recurse_submodules"].as_bool(), Some(true));
    write(
        &s.home.join(".stool/config.toml"),
        "[dotfile]\nurl = 'https://example.invalid/other.git'\n",
    );
    failure(
        s.command().args(["init"]).output().unwrap(),
        "origin differs",
    );
    success(s.init(&["--no-recurse-submodules"]));
    let config: toml::Value = fs::read_to_string(s.control().join("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config["recurse_submodules"].as_bool(), Some(false));
    success(s.init(&[]));
    let config: toml::Value = fs::read_to_string(s.control().join("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config["recurse_submodules"].as_bool(), Some(false));
}
#[test]
fn init_help_exposes_new_options_and_rejects_obsolete_destructive_flags() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let help = success(command(&s).arg("--help").output().unwrap());
    for option in [
        "--dotfile-url",
        "--key-file",
        "--ssh-key",
        "--brewfile",
        "--dry-run",
    ] {
        assert!(help.contains(option));
    }
    for option in ["--clean", "--force", "--key-path"] {
        assert!(!command(&s).arg(option).output().unwrap().status.success());
    }
    assert!(!s.home.join(".stool").exists());
}

#[test]
fn password_and_key_conflicts_fail_before_saving_global_defaults_or_running_apple() {
    use common::gpg::private_write;
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let private = include_bytes!("fixtures/offline-keys/test_ed25519");
    private_write(&s.home.join(".ssh/custom"), private);
    private_write(
        &s.home.join(".ssh/custom.pub"),
        include_bytes!("fixtures/offline-keys/test_ed25519.pub"),
    );
    let password = s.root.join("password");
    private_write(&password, b"synthetic archive password\n");
    let archive = s.root.join("keys.tar.age");
    let mut backup = Command::new(env!("CARGO_BIN_EXE_stool"));
    success(
        backup
            .env("HOME", &s.home)
            .env("USERPROFILE", &s.home)
            .current_dir(&s.root)
            .args(["secrets", "backup", "--ssh", "--output"])
            .arg(&archive)
            .arg("--passphrase-file")
            .arg(&password)
            .output()
            .unwrap(),
    );
    private_write(&password, b"incorrect password\n");
    let output = command(&s)
        .args(["--dotfile-url", &s.url(), "--key-file"])
        .arg(&archive)
        .arg("--passphrase-file")
        .arg(&password)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!s.home.join(".stool").exists());
    assert_eq!(fs::read(s.home.join(".ssh/custom")).unwrap(), private);
    private_write(&password, b"synthetic archive password\n");
    private_write(
        &s.home.join(".ssh/custom"),
        b"conflicting preexisting identity",
    );
    let output = command(&s)
        .args(["--dotfile-url", &s.url(), "--key-file"])
        .arg(&archive)
        .arg("--passphrase-file")
        .arg(&password)
        .output()
        .unwrap();
    failure(output, "Existing key conflicts");
    assert!(!s.home.join(".stool").exists());
    assert_eq!(
        fs::read(s.home.join(".ssh/custom")).unwrap(),
        b"conflicting preexisting identity"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn gpg_suite_version_is_reused_and_keys_restore_before_a_retryable_homebrew_failure() {
    use common::gpg::{fake_gpg, private_write};
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = s.root.join("isolated tools");
    fs::create_dir(&bin).unwrap();
    fs::copy(fake_gpg(), bin.join("gpg")).unwrap();
    private_write(
        &s.home.join(".ssh/custom"),
        include_bytes!("fixtures/offline-keys/test_ed25519"),
    );
    private_write(
        &s.home.join(".ssh/custom.pub"),
        include_bytes!("fixtures/offline-keys/test_ed25519.pub"),
    );
    fs::create_dir_all(s.home.join(".gnupg")).unwrap();
    let password = s.root.join("password");
    private_write(&password, b"synthetic archive password\n");
    let archive = s.root.join("keys.tar.age");
    success(
        Command::new(env!("CARGO_BIN_EXE_stool"))
            .env("HOME", &s.home)
            .env("USERPROFILE", &s.home)
            .env("PATH", &bin)
            .env("STOOL_TEST_GPG_ROOT", &s.root)
            .current_dir(&s.root)
            .args(["secrets", "backup", "--output"])
            .arg(&archive)
            .arg("--passphrase-file")
            .arg(&password)
            .output()
            .unwrap(),
    );
    fs::remove_dir_all(s.home.join(".ssh")).unwrap();
    let broken_brew = s.root.join("broken Homebrew");
    private_write(&broken_brew.join("bin/brew"), b"not executable");
    let output = command(&s)
        .args(["--dotfile-url", &s.url(), "--key-file"])
        .arg(&archive)
        .arg("--passphrase-file")
        .arg(&password)
        .arg("--non-interactive")
        .arg("--ssh-key")
        .arg(s.home.join(".ssh/custom"))
        .env("PATH", &bin)
        .env("STOOL_TEST_GPG_ROOT", &s.root)
        .env("HOMEBREW_PREFIX", &broken_brew)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    failure(output, "not an executable file");
    assert!(stdout.contains("2 primary keys"), "{stdout}");
    assert_eq!(
        fs::read(s.home.join(".ssh/custom")).unwrap(),
        include_bytes!("fixtures/offline-keys/test_ed25519")
    );
    assert_eq!(
        fs::read_to_string(s.root.join("imports"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    let config: toml::Value = fs::read_to_string(s.home.join(".stool/config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        config["dotfile"]["ssh_key"].as_str(),
        s.home.join(".ssh/custom").to_str()
    );
    assert!(!s.home.join(".stool/dotfile").exists());
}
