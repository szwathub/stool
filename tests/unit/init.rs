use super::*;
use std::cell::RefCell;
use std::rc::Rc;

type Events = Rc<RefCell<Vec<&'static str>>>;
struct FakePlatform {
    events: Events,
    apple: bool,
    gpg: bool,
    installs_gpg: bool,
    fail_packages: bool,
}
impl Platform for FakePlatform {
    fn apple(&self) -> bool {
        self.apple
    }
    fn gpg_available(&mut self) -> Result<bool> {
        self.events.borrow_mut().push("check-gpg");
        Ok(self.gpg)
    }
    fn homebrew(&mut self, _: bool, _: bool) -> Result<()> {
        self.events.borrow_mut().push("homebrew");
        Ok(())
    }
    fn packages(&mut self, _: &Path, _: bool, _: bool) -> Result<()> {
        self.events.borrow_mut().push("packages");
        if self.fail_packages {
            bail!("synthetic package failure");
        }
        self.gpg |= self.installs_gpg;
        Ok(())
    }
    fn preferences(&mut self, _: bool) -> Result<()> {
        self.events.borrow_mut().push("preferences");
        Ok(())
    }
}
struct FakeArchive {
    events: Events,
    gpg: bool,
    ssh: bool,
    identity: Option<PathBuf>,
}
impl Archive for FakeArchive {
    fn has_gpg(&self) -> bool {
        self.gpg
    }
    fn has_ssh(&self) -> bool {
        self.ssh
    }
    fn restore(&self, gpg: bool, ssh: bool) -> Result<()> {
        if ssh {
            if let Some(key) = &self.identity {
                write_identity(key);
            }
        }
        self.events.borrow_mut().push(match (gpg, ssh) {
            (true, true) => "restore-all",
            (true, false) => "restore-gpg",
            (false, true) => "restore-ssh",
            _ => panic!("empty restore"),
        });
        Ok(())
    }
}
fn args() -> InitCommand {
    InitCommand {
        dotfile_url: None,
        ssh_key: None,
        key_file: None,
        brewfile: None,
        dry_run: false,
        non_interactive: true,
        passphrase_file: None,
    }
}
fn fixtures() -> (tempfile::TempDir, FakePlatform, FakeArchive) {
    let temporary = tempfile::tempdir().unwrap();
    let events = Events::default();
    let platform = FakePlatform {
        events: events.clone(),
        apple: true,
        gpg: false,
        installs_gpg: true,
        fail_packages: false,
    };
    let archive = FakeArchive {
        events,
        gpg: true,
        ssh: true,
        identity: None,
    };
    (temporary, platform, archive)
}
fn run(
    temporary: &tempfile::TempDir,
    platform: &mut FakePlatform,
    archive: Option<&dyn Archive>,
    brewfile: bool,
) -> Result<()> {
    execute(
        &args(),
        temporary.path(),
        "https://example.invalid/dotfiles.git",
        if brewfile {
            Some(Path::new("Brewfile"))
        } else {
            None
        },
        archive,
        platform,
        false,
    )
}
#[test]
fn fresh_mac_restores_ssh_then_leaves_gpg_pending_without_installing_packages() {
    let (temp, mut platform, archive) = fixtures();
    run(&temp, &mut platform, Some(&archive), false).unwrap();
    assert_eq!(
        *archive.events.borrow(),
        [
            "check-gpg",
            "restore-ssh",
            "homebrew",
            "check-gpg",
            "preferences"
        ]
    );
    assert_eq!(
        Config::load(temp.path()).unwrap().dotfile_url().unwrap(),
        Some("https://example.invalid/dotfiles.git")
    );
    assert!(!temp.path().join(".stool/dotfile").exists());
}
#[test]
fn gpg_recovery_follows_brewfile_installation_in_the_same_session() {
    let (temp, mut platform, archive) = fixtures();
    run(&temp, &mut platform, Some(&archive), true).unwrap();
    assert_eq!(
        *archive.events.borrow(),
        [
            "check-gpg",
            "restore-ssh",
            "homebrew",
            "packages",
            "check-gpg",
            "restore-gpg",
            "preferences"
        ]
    );
}
#[test]
fn existing_gpg_restores_everything_once_before_apple_initialization() {
    let (temp, mut platform, archive) = fixtures();
    platform.gpg = true;
    run(&temp, &mut platform, Some(&archive), true).unwrap();
    assert_eq!(
        *archive.events.borrow(),
        [
            "check-gpg",
            "restore-all",
            "homebrew",
            "packages",
            "preferences"
        ]
    );
}
#[test]
fn archive_categories_and_non_apple_platforms_do_not_create_unnecessary_dependencies() {
    let (temp, mut platform, mut archive) = fixtures();
    platform.apple = false;
    archive.gpg = false;
    run(&temp, &mut platform, Some(&archive), false).unwrap();
    assert_eq!(*archive.events.borrow(), ["restore-ssh"]);
    archive.events.borrow_mut().clear();
    archive.gpg = true;
    archive.ssh = false;
    run(&temp, &mut platform, Some(&archive), false).unwrap();
    assert_eq!(*archive.events.borrow(), ["check-gpg", "check-gpg"]);
}
#[test]
fn package_failures_stop_gpg_import_and_preferences_but_allow_retry() {
    let (temp, mut platform, archive) = fixtures();
    platform.fail_packages = true;
    assert!(run(&temp, &mut platform, Some(&archive), true)
        .unwrap_err()
        .to_string()
        .contains("package failure"));
    assert_eq!(
        *archive.events.borrow(),
        ["check-gpg", "restore-ssh", "homebrew", "packages"]
    );
    platform.fail_packages = false;
    archive.events.borrow_mut().clear();
    run(&temp, &mut platform, Some(&archive), true).unwrap();
    assert!(archive.events.borrow().contains(&"restore-gpg"));
}
#[test]
fn no_key_file_skips_all_gpg_detection_and_config_updates_preserve_other_defaults() {
    let (temp, mut platform, _) = fixtures();
    let root = temp.path().join(".stool");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("config.toml"),
        "version = 1\n[apple]\ncustom = 'preserved'\n[dotfile]\nbranch = 'retained'\n",
    )
    .unwrap();
    run(&temp, &mut platform, None, false).unwrap();
    assert_eq!(*platform.events.borrow(), ["homebrew", "preferences"]);
    let config: toml::Value = fs::read_to_string(root.join("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(config["apple"]["custom"].as_str(), Some("preserved"));
    assert_eq!(config["dotfile"]["branch"].as_str(), Some("retained"));
    assert!(!fs::read_to_string(root.join("config.toml"))
        .unwrap()
        .contains("key_file"));
}
#[test]
fn invalid_global_configuration_is_preserved() {
    let (temp, mut platform, _) = fixtures();
    let root = temp.path().join(".stool");
    fs::create_dir(&root).unwrap();
    for text in [
        "invalid = [",
        "version = 99",
        "dotfile = 'wrong type'",
        "[dotfile]\nssh_key = 42",
        "[dotfile]\nssh_key = 'relative-key'",
        "[dotfile]\nssh_key = ''",
    ] {
        fs::write(root.join("config.toml"), text).unwrap();
        assert!(run(&temp, &mut platform, None, false).is_err());
        assert_eq!(fs::read_to_string(root.join("config.toml")).unwrap(), text);
        assert!(platform.events.borrow().is_empty());
    }
}

fn write_identity(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        include_bytes!("../fixtures/offline-keys/test_ed25519"),
    )
    .unwrap();
    filesystem::set_mode(path, 0o600).unwrap();
}

#[test]
fn saved_identity_is_absolute_retained_for_the_same_url_and_cleared_for_another_url() {
    let temp = tempfile::tempdir().unwrap();
    let key = temp.path().join(".ssh/GitHub's key");
    write_identity(&key);
    let first = "git@github.com:example/dotfiles.git";
    let second = "git@example.invalid:example/other.git";
    Config::save_dotfile(temp.path(), first, Some(Path::new("~/.ssh/GitHub's key"))).unwrap();
    let expected = fs::canonicalize(&key).unwrap();
    assert_eq!(
        Config::load(temp.path())
            .unwrap()
            .dotfile_ssh_key()
            .unwrap(),
        Some(expected.as_path())
    );
    Config::save_dotfile(temp.path(), first, None).unwrap();
    assert_eq!(
        Config::load(temp.path())
            .unwrap()
            .dotfile_ssh_key()
            .unwrap(),
        Some(expected.as_path())
    );
    Config::save_dotfile(temp.path(), second, None).unwrap();
    let config = Config::load(temp.path()).unwrap();
    assert_eq!(config.dotfile_url().unwrap(), Some(second));
    assert_eq!(config.dotfile_ssh_key().unwrap(), None);
    Config::save_dotfile(temp.path(), second, Some(&key)).unwrap();
    assert_eq!(
        Config::load(temp.path())
            .unwrap()
            .dotfile_ssh_key()
            .unwrap(),
        Some(expected.as_path())
    );
}

#[test]
fn invalid_explicit_identity_does_not_replace_saved_defaults_or_run_apple_steps() {
    let (temp, mut platform, _) = fixtures();
    let old_url = "https://example.invalid/old.git";
    Config::save_dotfile(temp.path(), old_url, None).unwrap();
    let before = fs::read(temp.path().join(".stool/config.toml")).unwrap();
    let key = temp.path().join(".ssh/not-a-private-key");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    fs::write(&key, "ssh-ed25519 public key only").unwrap();
    let mut options = args();
    options.ssh_key = Some(key);
    assert!(execute(
        &options,
        temp.path(),
        "git@github.com:example/dotfiles.git",
        None,
        None,
        &mut platform,
        false,
    )
    .is_err());
    assert!(platform.events.borrow().is_empty());
    assert_eq!(
        fs::read(temp.path().join(".stool/config.toml")).unwrap(),
        before
    );
}

#[test]
fn offline_identity_is_validated_and_saved_after_restore_before_package_failure() {
    let (temp, mut platform, mut archive) = fixtures();
    let key = temp.path().join(".ssh/restored-github-key");
    assert!(!key.exists());
    archive.identity = Some(key.clone());
    platform.fail_packages = true;
    let mut options = args();
    options.ssh_key = Some(PathBuf::from("~/.ssh/restored-github-key"));
    let url = "git@github.com:example/dotfiles.git";
    assert!(execute(
        &options,
        temp.path(),
        url,
        Some(Path::new("Brewfile")),
        Some(&archive),
        &mut platform,
        false,
    )
    .unwrap_err()
    .to_string()
    .contains("package failure"));
    assert_eq!(
        *archive.events.borrow(),
        ["check-gpg", "restore-ssh", "homebrew", "packages"]
    );
    let config = Config::load(temp.path()).unwrap();
    assert_eq!(config.dotfile_url().unwrap(), Some(url));
    assert_eq!(
        config.dotfile_ssh_key().unwrap(),
        Some(fs::canonicalize(key).unwrap().as_path())
    );
}
