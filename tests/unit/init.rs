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
}
impl Archive for FakeArchive {
    fn has_gpg(&self) -> bool {
        self.gpg
    }
    fn has_ssh(&self) -> bool {
        self.ssh
    }
    fn restore(&self, gpg: bool, ssh: bool) -> Result<()> {
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
    for text in ["invalid = [", "version = 99", "dotfile = 'wrong type'"] {
        fs::write(root.join("config.toml"), text).unwrap();
        assert!(run(&temp, &mut platform, None, false).is_err());
        assert_eq!(fs::read_to_string(root.join("config.toml")).unwrap(), text);
        assert!(platform.events.borrow().is_empty());
    }
}
