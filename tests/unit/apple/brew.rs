use super::homebrew::{self, Environment};
use super::packages;
use super::process::Executor;
use crate::cli::apple::brew::{InstallArgs, Options};
use anyhow::Result;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::{fs::PermissionsExt, process::ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

struct Call {
    program: PathBuf,
    args: Vec<OsString>,
    env: BTreeMap<OsString, Option<OsString>>,
    run: bool,
}
impl Call {
    fn new(command: &Command, run: bool) -> Self {
        Self {
            program: command.get_program().into(),
            args: command.get_args().map(Into::into).collect(),
            env: command
                .get_envs()
                .map(|(key, value)| (key.into(), value.map(Into::into)))
                .collect(),
            run,
        }
    }
}

struct Fake {
    prefix: PathBuf,
    calls: Vec<Call>,
    installers: Vec<PathBuf>,
    manifests: Vec<Vec<u8>>,
    fail: Option<&'static str>,
}
fn status(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code << 8)
}
fn output(code: i32, text: &str) -> Output {
    Output {
        status: status(code),
        stdout: text.as_bytes().to_vec(),
        stderr: if code == 0 {
            vec![]
        } else {
            b"fixture failure".to_vec()
        },
    }
}
fn executable(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"#!/bin/sh\nexit 99\n").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
impl Fake {
    fn new(prefix: &Path) -> Self {
        Self {
            prefix: prefix.to_owned(),
            calls: vec![],
            installers: vec![],
            manifests: vec![],
            fail: None,
        }
    }
    fn downloads(&self) -> usize {
        self.calls
            .iter()
            .filter(|call| {
                call.program == Path::new("/usr/bin/curl")
                    && call.args.first() != Some(&OsString::from("--version"))
            })
            .count()
    }
}
impl Executor for Fake {
    fn capture(&mut self, command: Command) -> Result<Output> {
        let call = Call::new(&command, false);
        let program = call.program.clone();
        let args = call.args.clone();
        self.calls.push(call);
        if program == Path::new("/usr/bin/xcode-select") {
            return Ok(output(
                if self.fail == Some("tools") { 1 } else { 0 },
                "/Library/Developer/CommandLineTools\n",
            ));
        }
        if program == Path::new("/usr/bin/git") {
            return Ok(output(
                if self.fail == Some("git") { 1 } else { 0 },
                "git version 2.47.0\n",
            ));
        }
        if program == Path::new("/usr/bin/curl") {
            if args == ["--version"] {
                return Ok(output(
                    if self.fail == Some("curl") { 1 } else { 0 },
                    "curl 8.0\n",
                ));
            }
            assert!(args.contains(&"=https".into()));
            assert!(args.contains(&"--proto-redir".into()));
            assert!(args.contains(
                &"https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh".into()
            ));
            let file = PathBuf::from(args.last().unwrap());
            assert_eq!(fs::metadata(&file)?.permissions().mode() & 0o777, 0o600);
            fs::write(&file, b"#!/bin/bash\n# disposable installer fixture\n")?;
            self.installers.push(file);
            return Ok(output(
                if self.fail == Some("download") { 22 } else { 0 },
                "",
            ));
        }
        assert_eq!(program, self.prefix.join("bin/brew"));
        if args == ["--version"] {
            return Ok(output(
                if self.fail == Some("version") { 9 } else { 0 },
                "Homebrew 5.0 fixture\n",
            ));
        }
        assert_eq!(args, ["--prefix"]);
        Ok(output(
            0,
            &format!(
                "{}\n",
                if self.fail == Some("prefix") {
                    "/another/prefix".to_owned()
                } else {
                    self.prefix.to_string_lossy().into_owned()
                }
            ),
        ))
    }
    fn run(&mut self, command: Command) -> Result<ExitStatus> {
        let call = Call::new(&command, true);
        if call.program == Path::new("/bin/bash") {
            assert!(Path::new(&call.args[0]).is_file());
            assert_eq!(call.args[1], "--path");
            assert_eq!(call.args[2], self.prefix);
            if self.fail != Some("installer") && self.fail != Some("missing-result") {
                executable(&self.prefix.join("bin/brew"));
            }
        } else {
            assert_eq!(call.program, self.prefix.join("bin/brew"));
            assert_eq!(
                &call.args[..4],
                ["bundle", "install", "--no-upgrade", "--file"]
            );
            self.manifests.push(fs::read(&call.args[4])?);
        }
        let failed = self.fail == Some("installer") && call.program == Path::new("/bin/bash")
            || self.fail == Some("packages") && call.program != Path::new("/bin/bash");
        self.calls.push(call);
        Ok(status(if failed { 17 } else { 0 }))
    }
}
fn environment(root: &Path) -> Environment {
    Environment {
        path: vec![],
        prefix: None,
        default_prefix: root.join("brew prefix"),
        home: Some(root.join("home")),
        terminal: true,
    }
}
fn options(dry_run: bool) -> Options {
    Options {
        dry_run,
        non_interactive: false,
    }
}
fn install_args(file: Option<PathBuf>, dry_run: bool) -> InstallArgs {
    InstallArgs {
        file,
        options: options(dry_run),
    }
}
fn ensure(env: &Environment, fake: &mut Fake, dry_run: bool) -> Result<homebrew::Homebrew> {
    homebrew::ensure(None, &options(dry_run), env, fake, &mut Vec::new(), false)
}

#[test]
fn existing_homebrew_is_validated_and_reused_without_installer_prerequisites() {
    let temporary = tempfile::tempdir().unwrap();
    let env = environment(temporary.path());
    let mut fake = Fake::new(&env.default_prefix);
    executable(&env.default_prefix.join("bin/brew"));
    fake.fail = Some("tools");
    for dry_run in [false, true] {
        assert_eq!(
            ensure(&env, &mut fake, dry_run).unwrap().prefix,
            env.default_prefix
        );
    }
    assert_eq!(fake.calls.len(), 4);
    assert_eq!(fake.downloads(), 0);
    assert!(fake.calls.iter().all(|call| !call.run));
}

#[test]
fn explicit_prefix_environment_and_path_discovery_use_the_selected_installation() {
    let temporary = tempfile::tempdir().unwrap();
    let mut env = environment(temporary.path());
    for name in ["explicit", "environment", "on path"] {
        executable(&temporary.path().join(name).join("bin/brew"));
    }
    let explicit = temporary.path().join("explicit");
    let inherited = temporary.path().join("environment");
    let on_path = temporary.path().join("on path");
    env.prefix = Some(inherited.clone());
    env.path = vec![on_path.join("bin")];
    let mut fake = Fake::new(&explicit);
    assert_eq!(
        homebrew::ensure(
            Some(&explicit),
            &options(false),
            &env,
            &mut fake,
            &mut Vec::new(),
            false
        )
        .unwrap()
        .prefix,
        explicit
    );
    let mut fake = Fake::new(&inherited);
    assert_eq!(ensure(&env, &mut fake, false).unwrap().prefix, inherited);
    env.prefix = None;
    let mut fake = Fake::new(&on_path);
    assert_eq!(ensure(&env, &mut fake, false).unwrap().prefix, on_path);
    assert_eq!(fake.downloads(), 0);
}

#[test]
fn missing_prerequisites_stop_before_download_or_prefix_creation() {
    for (failure, message) in [
        ("tools", "xcode-select --install"),
        ("git", "git --version"),
        ("curl", "restore /usr/bin/curl"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let env = environment(temporary.path());
        let mut fake = Fake::new(&env.default_prefix);
        fake.fail = Some(failure);
        for dry_run in [false, true] {
            let error = format!("{:#}", ensure(&env, &mut fake, dry_run).err().unwrap());
            assert!(error.contains(message), "{error}");
        }
        assert_eq!(fake.downloads(), 0);
        assert!(!env.default_prefix.exists());
        assert!(fake.calls.iter().all(|call| !call.run));
    }
}

#[test]
fn preview_of_missing_homebrew_and_nonterminal_failures_never_download() {
    let temporary = tempfile::tempdir().unwrap();
    let mut env = environment(temporary.path());
    let mut fake = Fake::new(&env.default_prefix);
    ensure(&env, &mut fake, true).unwrap();
    assert_eq!(fake.calls.len(), 3);
    assert_eq!(fake.downloads(), 0);
    env.terminal = false;
    assert!(ensure(&env, &mut fake, false)
        .err()
        .unwrap()
        .to_string()
        .contains("--non-interactive"));
    assert_eq!(fake.downloads(), 0);
    assert!(!env.default_prefix.exists());
}

#[test]
fn installation_preserves_literal_paths_verifies_result_and_cleans_private_installer() {
    let temporary = tempfile::tempdir().unwrap();
    let mut env = environment(temporary.path());
    env.default_prefix = temporary.path().join("brew's $(touch injected)");
    env.terminal = false;
    let mut fake = Fake::new(&env.default_prefix);
    let mut report = Vec::new();
    homebrew::ensure(
        None,
        &Options {
            dry_run: false,
            non_interactive: true,
        },
        &env,
        &mut fake,
        &mut report,
        true,
    )
    .unwrap();
    assert_eq!(fake.downloads(), 1);
    assert!(env.default_prefix.join("bin/brew").is_file());
    assert!(fake.installers.iter().all(|path| !path.exists()));
    let installer = fake.calls.iter().find(|call| call.run).unwrap();
    assert_eq!(
        installer.env.get(&OsString::from("NONINTERACTIVE")),
        Some(&Some("1".into()))
    );
    assert_eq!(
        installer.env.get(&OsString::from("INTERACTIVE")),
        Some(&None)
    );
    assert!(String::from_utf8(report).unwrap().contains("shellenv"));
    assert!(!temporary.path().join("injected").exists());
}

#[test]
fn download_and_installer_failures_are_retryable_and_clean_temporary_files() {
    for failure in ["download", "installer"] {
        let temporary = tempfile::tempdir().unwrap();
        let env = environment(temporary.path());
        let mut fake = Fake::new(&env.default_prefix);
        fake.fail = Some(failure);
        assert!(ensure(&env, &mut fake, false).is_err());
        assert!(fake.installers.iter().all(|path| !path.exists()));
        assert!(!env.default_prefix.join("bin/brew").exists());
        fake.fail = None;
        ensure(&env, &mut fake, false).unwrap();
        assert_eq!(fake.downloads(), 2);
        assert!(fake.installers.iter().all(|path| !path.exists()));
    }
}

#[test]
fn broken_existing_installations_and_unsuccessful_verification_are_errors() {
    for failure in ["version", "prefix", "missing-result"] {
        let temporary = tempfile::tempdir().unwrap();
        let env = environment(temporary.path());
        let mut fake = Fake::new(&env.default_prefix);
        fake.fail = Some(failure);
        let error = format!("{:#}", ensure(&env, &mut fake, false).err().unwrap());
        assert!(
            error.contains("Verify Homebrew after installation"),
            "{error}"
        );
        assert!(fake.installers.iter().all(|path| !path.exists()));
    }
    let temporary = tempfile::tempdir().unwrap();
    let env = environment(temporary.path());
    let mut fake = Fake::new(&env.default_prefix);
    executable(&env.default_prefix.join("bin/brew"));
    fake.fail = Some("version");
    assert!(ensure(&env, &mut fake, false).is_err());
    assert_eq!(fake.downloads(), 0);
    fs::set_permissions(
        env.default_prefix.join("bin/brew"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(ensure(&env, &mut fake, false)
        .err()
        .unwrap()
        .to_string()
        .contains("not an executable"));
    for invalid in [
        Path::new("relative/prefix"),
        Path::new("/"),
        Path::new("/tmp/.."),
    ] {
        assert!(homebrew::ensure(
            Some(invalid),
            &options(false),
            &env,
            &mut fake,
            &mut Vec::new(),
            false
        )
        .is_err());
    }
}

#[test]
fn missing_or_directory_brewfiles_fail_before_checking_or_installing_homebrew() {
    let temporary = tempfile::tempdir().unwrap();
    let env = environment(temporary.path());
    let mut fake = Fake::new(&env.default_prefix);
    for file in [
        temporary.path().join("missing"),
        temporary.path().to_owned(),
    ] {
        assert!(packages::install(
            None,
            &install_args(Some(file), false),
            &env,
            &mut fake,
            &mut Vec::new(),
            false
        )
        .is_err());
    }
    assert!(fake.calls.is_empty());
    assert!(!env.default_prefix.exists());
}

#[test]
fn package_preview_displays_but_never_evaluates_a_ruby_brewfile() {
    let temporary = tempfile::tempdir().unwrap();
    let env = environment(temporary.path());
    let mut fake = Fake::new(&env.default_prefix);
    let file = temporary.path().join("Brewfile with spaces");
    fs::write(&file, b"system('touch should-not-exist')\nbrew 'git'\n").unwrap();
    let mut report = Vec::new();
    packages::install(
        None,
        &install_args(Some(file), true),
        &env,
        &mut fake,
        &mut report,
        false,
    )
    .unwrap();
    assert_eq!(fake.downloads(), 0);
    assert_eq!(fake.calls.len(), 3);
    assert!(fake.manifests.is_empty());
    let report = String::from_utf8(report).unwrap();
    assert!(report.contains("not evaluated"));
    assert!(report.contains("system('touch"));
    assert!(!env.default_prefix.exists());
}

#[test]
fn packages_default_to_home_brewfile_and_recheck_changed_manifests_without_success_hashes() {
    let temporary = tempfile::tempdir().unwrap();
    let env = environment(temporary.path());
    let mut fake = Fake::new(&env.default_prefix);
    executable(&env.default_prefix.join("bin/brew"));
    let file = env.home.as_ref().unwrap().join(".Brewfile");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    for contents in [
        "brew 'git'\n",
        "brew 'git'\n",
        "brew 'git'\ncask 'gpg-suite'\n",
    ] {
        fs::write(&file, contents).unwrap();
        packages::install(
            None,
            &install_args(None, false),
            &env,
            &mut fake,
            &mut Vec::new(),
            false,
        )
        .unwrap();
    }
    assert_eq!(fake.manifests.len(), 3);
    assert!(String::from_utf8_lossy(&fake.manifests[2]).contains("gpg-suite"));
    for call in fake.calls.iter().filter(|call| call.run) {
        assert_eq!(
            call.env.get(&OsString::from("HOMEBREW_NO_AUTO_UPDATE")),
            Some(&Some("1".into()))
        );
        assert_eq!(
            call.env.get(&OsString::from("HOMEBREW_BUNDLE_NO_UPGRADE")),
            Some(&Some("1".into()))
        );
        assert_eq!(
            call.env
                .get(&OsString::from("HOMEBREW_BUNDLE_INSTALL_CLEANUP")),
            Some(&None)
        );
    }
    assert!(!env.home.as_ref().unwrap().join(".stool").exists());
    assert_eq!(fake.downloads(), 0);
}

#[test]
fn failed_packages_can_retry_and_explicit_file_works_without_home() {
    let temporary = tempfile::tempdir().unwrap();
    let mut env = environment(temporary.path());
    env.home = None;
    let mut fake = Fake::new(&env.default_prefix);
    executable(&env.default_prefix.join("bin/brew"));
    assert!(packages::install(
        None,
        &install_args(None, false),
        &env,
        &mut fake,
        &mut Vec::new(),
        false
    )
    .is_err());
    assert!(fake.calls.is_empty());
    let file = temporary.path().join("my Brewfile");
    fs::write(&file, "cask 'gpg-suite'\n").unwrap();
    let args = install_args(Some(file), false);
    fake.fail = Some("packages");
    assert!(
        packages::install(None, &args, &env, &mut fake, &mut Vec::new(), false)
            .unwrap_err()
            .to_string()
            .contains("retry")
    );
    fake.fail = None;
    packages::install(None, &args, &env, &mut fake, &mut Vec::new(), false).unwrap();
    assert_eq!(fake.manifests.len(), 2);
    env.terminal = false;
    let previous = fake.calls.len();
    assert!(
        packages::install(None, &args, &env, &mut fake, &mut Vec::new(), false)
            .unwrap_err()
            .to_string()
            .contains("--non-interactive")
    );
    assert_eq!(fake.calls.len(), previous);
}
