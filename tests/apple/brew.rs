#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    prefix: PathBuf,
    home: PathBuf,
    log: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let prefix = root.join("brew's literal $(touch unexpected)");
        let home = root.join("home");
        let log = root.join("brew-log");
        fs::create_dir_all(prefix.join("bin")).unwrap();
        fs::create_dir(&home).unwrap();
        fs::write(
            prefix.join("bin/brew"),
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$STOOL_TEST_BREW_LOG"
case "$1" in
  --version) printf 'Homebrew 5.0 fixture\n' ;;
  --prefix) printf '%s\n' "$STOOL_TEST_BREW_PREFIX" ;;
  bundle)
    test "$#" = 5 || exit 91
    test "$2" = install || exit 92
    test "$3" = --no-upgrade || exit 93
    test "$4" = --file || exit 94
    test -f "$5" || exit 95
    test "$HOMEBREW_NO_AUTO_UPDATE" = 1 || exit 96
    test "$HOMEBREW_BUNDLE_NO_UPGRADE" = 1 || exit 97
    test "$NONINTERACTIVE" = 1 || exit 98
    test -z "${HOMEBREW_BUNDLE_INSTALL_CLEANUP:-}" || exit 99
    if test "${STOOL_TEST_FAIL_BUNDLE:-}" = 1; then exit 17; fi ;;
  *) exit 90 ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(prefix.join("bin/brew"), fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _temporary: temporary,
            root,
            prefix,
            home,
            log,
        }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_stool"));
        command
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("HOMEBREW_PREFIX", &self.prefix)
            .env("STOOL_TEST_BREW_PREFIX", &self.prefix)
            .env("STOOL_TEST_BREW_LOG", &self.log)
            .env("PATH", "")
            .env("HOMEBREW_BUNDLE_INSTALL_CLEANUP", "1")
            .args(["apple", "brew"])
            .args(args);
        command
    }
    fn success(&self, args: &[&str]) -> String {
        let Output {
            status,
            stdout,
            stderr,
        } = self.command(args).output().unwrap();
        assert!(status.success(), "{}", String::from_utf8_lossy(&stderr));
        String::from_utf8(stdout).unwrap()
    }
}

#[test]
fn help_and_parser_expose_brew_subcommands_without_installing() {
    for (args, expected) in [
        (vec!["apple", "brew", "--help"], "install"),
        (vec!["apple", "brew", "init", "--help"], "--non-interactive"),
        (vec!["apple", "brew", "install", "--help"], "--file"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_stool"))
            .args(args)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8(result.stdout).unwrap().contains(expected));
    }
    for args in [
        vec!["apple", "brew"],
        vec!["apple", "brew", "remove"],
        vec!["apple", "brew", "init", "--file", "x"],
        vec!["apple", "brew", "install", "extra"],
    ] {
        assert!(!Command::new(env!("CARGO_BIN_EXE_stool"))
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
}

#[test]
fn real_cli_reuses_brew_previews_literal_manifest_and_retries_failed_installation() {
    let fixture = Fixture::new();
    let manifest = fixture.home.join(".Brewfile");
    let contents = "system('touch unexpected-ruby-execution')\ncask 'gpg-suite'\n";
    fs::write(&manifest, contents).unwrap();
    fixture.success(&["init"]);
    let preview = fixture.success(&["install", "--dry-run"]);
    assert!(preview.contains("not evaluated"));
    assert!(preview.contains(contents));
    assert!(!fs::read_to_string(&fixture.log).unwrap().contains("bundle"));
    let failed = fixture
        .command(&["install", "--non-interactive"])
        .env("STOOL_TEST_FAIL_BUNDLE", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(error.contains("retry"));
    fixture.success(&["install", "--non-interactive"]);
    let custom = fixture.root.join("custom Brewfile");
    fs::write(&custom, "brew 'git'\n").unwrap();
    fixture.success(&[
        "install",
        "--file",
        custom.to_str().unwrap(),
        "--non-interactive",
        "--prefix",
        fixture.prefix.to_str().unwrap(),
    ]);
    let log = fs::read_to_string(&fixture.log).unwrap();
    assert_eq!(
        log.lines()
            .filter(|line| line.starts_with("bundle"))
            .count(),
        3
    );
    assert!(!fixture.home.join(".stool").exists());
    assert!(!fixture.root.join("unexpected-ruby-execution").exists());
    assert!(!fixture.root.join("unexpected").exists());
    assert_eq!(fs::read_to_string(manifest).unwrap(), contents);
}

#[test]
fn missing_default_manifest_fails_before_any_brew_or_installer_command() {
    let fixture = Fixture::new();
    let result = fixture
        .command(&["install", "--non-interactive"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Read Brewfile"));
    assert!(!fixture.log.exists());
}
