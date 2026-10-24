use crate::common::*;

fn init_command(sandbox: &Sandbox, path: &Path) -> Command {
    let mut command = sandbox.command();
    command
        .env("PATH", path)
        .arg("--source")
        .arg(sandbox.root.join("not-created/source"))
        .arg("--destination")
        .arg(&sandbox.destination)
        .arg("init")
        .arg(sandbox.url());
    command
}

fn assert_not_initialized(sandbox: &Sandbox) {
    assert!(!sandbox.root.join("not-created").exists());
    assert!(!sandbox.destination.exists());
    assert!(!sandbox.control().join("config.toml").exists());
}

#[test]
fn missing_git_has_platform_installation_guidance_before_creating_source() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = sandbox.root.join("empty-bin");
    fs::create_dir(&bin).unwrap();
    let output = init_command(&sandbox, &bin)
        .arg("--apply")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    failure(output, "Git was not found on PATH");
    let expected = if cfg!(target_os = "macos") {
        "xcode-select --install"
    } else if cfg!(windows) {
        "winget install --id Git.Git"
    } else if cfg!(target_os = "linux") {
        "sudo apt-get install git"
    } else {
        "https://git-scm.com/install/"
    };
    assert!(stderr.contains(expected), "{stderr}");
    assert!(stderr.contains("git --version"));
    assert!(stderr.contains("rerun the same stool dotfile init command"));
    assert_not_initialized(&sandbox);
    // Once Git is available, the same initialization can be retried normally.
    let path = std::env::var_os("PATH").unwrap();
    success(
        init_command(&sandbox, Path::new(&path))
            .arg("--apply")
            .output()
            .unwrap(),
    );
    assert!(sandbox.destination.join(".seed").exists());
}

#[test]
fn recursive_builtin_init_checks_system_git_before_cloning() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = sandbox.root.join("empty-bin");
    fs::create_dir(&bin).unwrap();
    failure(
        init_command(&sandbox, &bin)
            .args(["--builtin", "--recurse-submodules"])
            .output()
            .unwrap(),
        "Git was not found on PATH",
    );
    assert_not_initialized(&sandbox);
}

#[test]
fn builtin_repository_can_be_reinitialized_without_system_git() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    success(sandbox.init(&["--builtin", "--apply"]));
    let before = snapshot(&sandbox.destination);
    let bin = sandbox.root.join("empty-bin");
    fs::create_dir(&bin).unwrap();
    success(
        sandbox
            .command()
            .env("PATH", &bin)
            .args(["init", &sandbox.url()])
            .output()
            .unwrap(),
    );
    assert_eq!(before, snapshot(&sandbox.destination));
}

#[cfg(unix)]
fn executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    write(path, contents);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn unusable_git_stops_before_clone_and_repair_can_be_retried() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = sandbox.root.join("bin");
    let log = sandbox.root.join("git-log");
    executable(
        &bin.join("git"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$STOOL_TEST_GIT_LOG\"\nprintf '%s\\n' 'broken Git fixture' >&2\nexit 19\n",
    );
    let output = init_command(&sandbox, &bin)
        .env("STOOL_TEST_GIT_LOG", &log)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    failure(output, "git --version` failed");
    assert!(stderr.contains("broken Git fixture"));
    assert_eq!(fs::read_to_string(&log).unwrap(), "--version\n");
    assert_not_initialized(&sandbox);

    executable(
        &bin.join("git"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$STOOL_TEST_GIT_LOG\"\nPATH=\"$STOOL_TEST_REAL_PATH\" exec git \"$@\"\n",
    );
    fs::write(&log, "").unwrap();
    success(
        init_command(&sandbox, &bin)
            .env("STOOL_TEST_GIT_LOG", &log)
            .env("STOOL_TEST_REAL_PATH", std::env::var_os("PATH").unwrap())
            .output()
            .unwrap(),
    );
    let entries = fs::read_to_string(&log).unwrap();
    let mut entries = entries.lines();
    assert_eq!(entries.next(), Some("--version"));
    assert!(entries.next().unwrap().starts_with("clone "));
}

#[cfg(unix)]
#[test]
fn authentication_failure_is_not_reported_as_missing_git() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = sandbox.root.join("bin");
    executable(
        &bin.join("git"),
        "#!/bin/sh\nif test \"$1\" = --version; then printf 'git version 2.43.0\\n'; exit 0; fi\nprintf '%s\\n' 'Authentication failed for test repository' >&2\nexit 128\n",
    );
    let output = init_command(&sandbox, &bin).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    failure(output, "Git failed: Authentication failed");
    assert!(!stderr.contains("Git was not found"));
    assert!(!stderr.contains("xcode-select --install"));
    assert!(!sandbox.root.join("not-created/source").exists());
    assert!(!sandbox.control().join("config.toml").exists());
    assert_eq!(
        fs::read_dir(sandbox.root.join("not-created"))
            .unwrap()
            .count(),
        0
    );
}

#[cfg(target_os = "macos")]
#[test]
fn apple_git_launcher_without_developer_tools_only_shows_installation_guide() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = sandbox.root.join("bin");
    fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink("/usr/bin/git", bin.join("git")).unwrap();
    let log = sandbox.root.join("xcode-select-log");
    executable(
        &bin.join("xcode-select"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$STOOL_TEST_TOOLS_LOG\"\nexit 2\n",
    );
    failure(
        init_command(&sandbox, &bin)
            .env("STOOL_TEST_TOOLS_LOG", &log)
            .output()
            .unwrap(),
        "xcode-select --install",
    );
    assert_eq!(fs::read_to_string(log).unwrap(), "--print-path\n");
    assert_not_initialized(&sandbox);
}

#[cfg(unix)]
#[test]
fn default_builtin_clone_does_not_probe_system_git_for_ordinary_repositories() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let bin = sandbox.root.join("transport-only-bin");
    fs::create_dir(&bin).unwrap();
    // Local gix transport needs the server-side upload-pack helper. Expose only
    // that helper, leaving the `git` client absent from PATH.
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("git"))
        .find(|candidate| candidate.is_file())
        .unwrap();
    executable(
        &bin.join("git-upload-pack"),
        &format!(
            "#!/bin/sh\nexec {} upload-pack \"$@\"\n",
            shell_words::quote(&real_git.to_string_lossy())
        ),
    );
    success(
        init_command(&sandbox, &bin)
            .args(["--builtin", "--apply"])
            .output()
            .unwrap(),
    );
    assert!(sandbox.destination.join(".seed").is_file());
}
