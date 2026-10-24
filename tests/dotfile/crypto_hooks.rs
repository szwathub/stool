use crate::common::*;
use fs2::FileExt;

#[test]
fn keygen_remains_available_under_deployment_lock_but_writers_are_locked() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    success(sandbox.init(&[]));
    let before = snapshot(&sandbox.control());
    let source_before = snapshot(&sandbox.source);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(sandbox.control().join("lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let identity = sandbox.home.join("identity.txt");
    let recipient = success(
        sandbox
            .command()
            .args(["keygen", "--output"])
            .arg(&identity)
            .output()
            .unwrap(),
    );
    assert!(recipient.trim().starts_with("age1"));
    assert!(identity.is_file());
    for args in [vec!["apply"], vec!["add", ".seed"], vec!["edit", ".seed"]] {
        failure(sandbox.cli(&args), "Another dotfile command is running");
    }
    // A Windows byte-range lock also prevents another handle reading the file.
    drop(lock);
    assert_eq!(snapshot(&sandbox.control()), before);
    assert_eq!(snapshot(&sandbox.source), source_before);
    assert!(!sandbox.destination.exists());
}

#[cfg(unix)]
fn executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    write(path, contents);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn after_hook_uses_the_running_binary_without_requiring_it_in_path() {
    let sandbox = Sandbox::new(&[
        ("dot_seed", "seed"),
        ("run_once_after_check.sh", "#!/bin/sh\nset -eu\nif command -v stool >/dev/null 2>&1; then exit 83; fi\ntest -f \"$STOOL_DEST_DIR/.seed\"\n\"$STOOL_EXECUTABLE\" --version > \"$STOOL_DEST_DIR/running-version\"\n"),
    ]);
    success(sandbox.init(&[]));
    let before = snapshot(&sandbox.root);
    success(sandbox.cli(&["diff"]));
    assert_eq!(snapshot(&sandbox.root), before);
    let bin = sandbox.root.join("hook tools");
    executable(
        &bin.join("git"),
        "#!/bin/sh\nPATH=\"$STOOL_TEST_REAL_PATH\" exec git \"$@\"\n",
    );
    success(
        sandbox
            .command()
            .arg("apply")
            .env("PATH", &bin)
            .env("STOOL_TEST_REAL_PATH", std::env::var_os("PATH").unwrap())
            .env("STOOL_EXECUTABLE", "/missing/older-stool")
            .output()
            .unwrap(),
    );
    let version = fs::read_to_string(sandbox.destination.join("running-version")).unwrap();
    assert_eq!(version, format!("stool {}\n", env!("CARGO_PKG_VERSION")));
    success(sandbox.cli(&["verify"]));
    assert!(success(sandbox.cli(&["apply"])).contains("Applied 0 change(s)"));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join("running-version")).unwrap(),
        version
    );
}
