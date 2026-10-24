use crate::common::*;

#[test]
fn git_url_init_preview_apply_update_and_idempotency() {
    let sandbox = Sandbox::new(&[
        ("dot_gitconfig", "[user]\nname = Test\n"),
        ("dot_config/fish/config.fish", "set -gx EDITOR vim\n"),
        (".stoolignore", "README.md\n"),
        ("README.md", "repository documentation"),
    ]);
    success(sandbox.init(&[]));
    assert!(!sandbox.destination.exists());
    assert!(!sandbox.control().join("state").exists());
    assert_eq!(
        git(&sandbox.source, &["branch", "--show-current"]).trim(),
        "dotfiles-trunk"
    );
    let control = snapshot(&sandbox.control());
    let diff = success(sandbox.cli(&["diff"]));
    assert!(diff.contains("+++ b/.gitconfig") && diff.contains("+name = Test"));
    success(sandbox.cli(&["status"]));
    success(sandbox.cli(&["managed"]));
    success(sandbox.cli(&["apply", "--dry-run"]));
    assert_eq!(snapshot(&sandbox.control()), control);
    assert!(!sandbox.destination.exists());
    success(sandbox.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".config/fish/config.fish")).unwrap(),
        "set -gx EDITOR vim\n"
    );
    assert!(!sandbox.destination.join("README.md").exists());
    let deployed = snapshot(&sandbox.destination);
    let state = snapshot(&sandbox.control().join("state"));
    assert!(success(sandbox.cli(&["apply"])).contains("Applied 0 change(s)"));
    assert_eq!(snapshot(&sandbox.destination), deployed);
    assert_eq!(snapshot(&sandbox.control().join("state")), state);
    success(sandbox.init(&[]));
    write(
        &sandbox.work.join("dot_gitconfig"),
        "[user]\nname = Updated\n",
    );
    sandbox.push();
    success(sandbox.cli(&["update"]));
    assert!(fs::read_to_string(sandbox.destination.join(".gitconfig"))
        .unwrap()
        .contains("Updated"));
    assert!(success(sandbox.cli(&["status"])).contains("0 change(s), 0 conflict(s)"));
}

#[test]
fn init_apply_and_existing_files_require_explicit_force_with_backup() {
    let sandbox = Sandbox::new(&[("dot_aaa", "new\n"), ("dot_gitconfig", "managed\n")]);
    write(
        &sandbox.destination.join(".gitconfig"),
        "my local configuration\n",
    );
    failure(sandbox.init(&["--apply"]), "Conflicting destination");
    assert!(
        !sandbox.destination.join(".aaa").exists(),
        "conflicts must be preflighted before any target writes"
    );
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".gitconfig")).unwrap(),
        "my local configuration\n"
    );
    success(sandbox.cli(&["apply", "--force"]));
    let backup = fs::read_dir(sandbox.control().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        fs::read_to_string(backup.join("files/.gitconfig")).unwrap(),
        "my local configuration\n"
    );
    let backups = snapshot(&sandbox.control().join("backups"));
    success(sandbox.cli(&["apply", "--force"]));
    assert_eq!(snapshot(&sandbox.control().join("backups")), backups);
}

#[test]
fn local_target_edits_conflict_but_already_converged_targets_are_adopted() {
    let sandbox = Sandbox::new(&[("dot_settings", "first\n")]);
    success(sandbox.init(&["--apply"]));
    write(&sandbox.destination.join(".settings"), "local edit\n");
    write(&sandbox.work.join("dot_settings"), "second\n");
    sandbox.push();
    failure(sandbox.cli(&["update"]), "Conflicting destination");
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".settings")).unwrap(),
        "local edit\n"
    );
    write(&sandbox.destination.join(".settings"), "second\n");
    success(sandbox.cli(&["apply"]));
    write(&sandbox.work.join("dot_settings"), "third\n");
    sandbox.push();
    success(sandbox.cli(&["update"]));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".settings")).unwrap(),
        "third\n"
    );
}

#[test]
fn update_rejects_dirty_source_and_divergent_local_commits() {
    let sandbox = Sandbox::new(&[("dot_configfile", "first\n")]);
    success(sandbox.init(&["--apply"]));
    write(&sandbox.source.join("untracked"), "local\n");
    failure(sandbox.cli(&["update"]), "Source has local changes");
    fs::remove_file(sandbox.source.join("untracked")).unwrap();
    git(&sandbox.source, &["config", "user.name", "Local"]);
    git(
        &sandbox.source,
        &["config", "user.email", "local@example.invalid"],
    );
    git(&sandbox.source, &["config", "commit.gpgsign", "false"]);
    write(&sandbox.source.join("dot_local"), "local commit\n");
    git(&sandbox.source, &["add", "--all"]);
    git(&sandbox.source, &["commit", "-m", "local"]);
    write(&sandbox.work.join("dot_configfile"), "remote\n");
    sandbox.push();
    let head = git(&sandbox.source, &["rev-parse", "HEAD"]);
    failure(sandbox.cli(&["update"]), "only fast-forward");
    assert_eq!(git(&sandbox.source, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".configfile")).unwrap(),
        "first\n"
    );
}

#[test]
fn malformed_features_and_duplicate_targets_fail_before_writing() {
    for (name, contents, expected) in [
        ("dot_config.tmpl", "{{ missing_value }}", "Render template"),
        (
            "dot_private_invalid",
            "invalid order",
            "Invalid attribute order",
        ),
        (".stoolunknown", "invalid control", "Unsupported control"),
    ] {
        let sandbox = Sandbox::new(&[("dot_aaa", "first\n"), (name, contents)]);
        failure(sandbox.init(&["--apply"]), expected);
        assert!(!sandbox.destination.exists());
    }
    let sandbox = Sandbox::new(&[("dot_name", "one\n"), ("literal_.name", "two\n")]);
    failure(sandbox.init(&["--apply"]), "Multiple source entries");
    assert!(!sandbox.destination.exists());
}

#[test]
fn ignore_rules_use_target_paths_nested_rules_and_exception_priority() {
    let sandbox = Sandbox::new(&[
        (
            ".stoolignore",
            "README.md # comment\n.cache/**\n!.cache/keep.txt\n.cache/*\n",
        ),
        ("README.md", "ignored\n"),
        ("dot_cache/drop.txt", "ignored\n"),
        ("dot_cache/keep.txt", "keep\n"),
        ("dot_config/.stoolignore", "omit\n"),
        ("dot_config/omit", "ignored\n"),
        ("dot_config/keep", "keep\n"),
        ("literal_dot_literal", "literal name\n"),
        ("empty_dot_empty", ""),
    ]);
    success(sandbox.init(&["--apply"]));
    assert!(!sandbox.destination.join("README.md").exists());
    assert!(!sandbox.destination.join(".cache/drop.txt").exists());
    assert!(sandbox.destination.join(".cache/keep.txt").exists());
    assert!(!sandbox.destination.join(".config/omit").exists());
    assert!(sandbox.destination.join(".config/keep").exists());
    assert!(sandbox.destination.join("dot_literal").exists());
    assert_eq!(fs::read(sandbox.destination.join(".empty")).unwrap(), b"");
}

#[test]
fn hidden_source_metadata_is_ignored_while_encoded_dotfiles_are_managed() {
    let sandbox = Sandbox::new(&[
        (".repository-notes.tmpl", "{{ missing_metadata_value }}"),
        (".repository-cache/dot_accidental", "not a managed dotfile"),
        ("dot_config/.editor-state", "local metadata"),
        ("dot_config/dot_visible", "managed dotfile\n"),
    ]);
    success(sandbox.init(&[]));
    let managed = success(sandbox.cli(&["managed"]));
    let visible = sandbox.destination.join(".config").join(".visible");
    assert!(
        managed
            .lines()
            .any(|line| git_path(Path::new(line)) == visible),
        "Expected managed path {visible:?} in:\n{managed}"
    );
    assert!(!managed.contains("repository-notes"));
    assert!(!managed.contains("accidental"));
    assert!(!managed.contains("editor-state"));
    success(sandbox.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".config/.visible")).unwrap(),
        "managed dotfile\n"
    );
    assert!(!sandbox.destination.join(".repository-notes.tmpl").exists());
    assert!(!sandbox.destination.join(".repository-cache").exists());
    assert!(!sandbox.destination.join(".config/.editor-state").exists());
    success(sandbox.cli(&["verify"]));
}

#[test]
fn removing_source_files_unmanages_and_preserves_destination() {
    let sandbox = Sandbox::new(&[("dot_keep", "keep\n"), ("dot_removed", "preserve me\n")]);
    success(sandbox.init(&["--apply"]));
    fs::remove_file(sandbox.work.join("dot_removed")).unwrap();
    sandbox.push();
    success(sandbox.cli(&["update"]));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".removed")).unwrap(),
        "preserve me\n"
    );
    assert!(!success(sandbox.cli(&["managed"])).contains(".removed"));
}

#[test]
fn failed_clone_can_be_retried_and_existing_directories_are_preserved() {
    let sandbox = Sandbox::new(&[("dot_file", "content\n")]);
    let invalid = file_url(&sandbox.root.join("missing.git"));
    let output = sandbox
        .command()
        .arg("--source")
        .arg(&sandbox.source)
        .arg("init")
        .arg(invalid)
        .output()
        .unwrap();
    failure(output, "Git failed");
    assert!(!sandbox.source.exists());
    success(sandbox.init(&["--apply"]));
    assert!(sandbox.destination.join(".file").exists());
    let protected = sandbox.root.join("existing directory");
    write(&protected.join("personal"), "keep\n");
    let output = sandbox
        .command()
        .arg("--source")
        .arg(&protected)
        .arg("init")
        .arg(sandbox.url())
        .output()
        .unwrap();
    failure(output, "not an initialized Git repository");
    assert_eq!(
        fs::read_to_string(protected.join("personal")).unwrap(),
        "keep\n"
    );
}

#[test]
fn urls_with_embedded_credentials_are_rejected_without_disclosure() {
    let sandbox = Sandbox::new(&[("dot_file", "content\n")]);
    let output = sandbox
        .command()
        .arg("init")
        .arg("https://user:do-not-print-this@example.invalid/repo.git")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("do-not-print-this"));
    failure(output, "credential helper");
}

#[test]
fn concurrent_mutations_are_rejected() {
    use fs2::FileExt;
    success({
        let sandbox = Sandbox::new(&[("dot_file", "content\n")]);
        success(sandbox.init(&[]));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(sandbox.control().join("lock"))
            .unwrap();
        file.lock_exclusive().unwrap();
        failure(sandbox.cli(&["apply"]), "Another dotfile command");
        assert!(!sandbox.destination.exists());
        drop(file);
        sandbox.cli(&["apply"])
    });
}

#[cfg(unix)]
#[test]
fn unix_private_executable_and_explicit_symlinks() {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new(&[
        ("private_dot_ssh/private_config", "Host example\n"),
        ("dot_local/bin/executable_tool", "#!/bin/sh\n"),
        ("symlink_dot_alias", ".ssh/config\n"),
    ]);
    success(sandbox.init(&["--apply"]));
    assert_eq!(
        fs::metadata(sandbox.destination.join(".ssh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(sandbox.destination.join(".ssh/config"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(sandbox.destination.join(".local/bin/tool"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::read_link(sandbox.destination.join(".alias")).unwrap(),
        PathBuf::from(".ssh/config")
    );
}

#[cfg(unix)]
#[test]
fn symlinked_parents_cannot_redirect_deployment_even_with_force() {
    let sandbox = Sandbox::new(&[("dot_aaa", "first\n"), ("dot_config/fish", "managed\n")]);
    let outside = sandbox.root.join("outside");
    write(&outside.join("fish"), "outside\n");
    fs::create_dir(&sandbox.destination).unwrap();
    std::os::unix::fs::symlink(&outside, sandbox.destination.join(".config")).unwrap();
    success(sandbox.init(&[]));
    failure(sandbox.cli(&["apply", "--force"]), "parent is a symlink");
    assert_eq!(
        fs::read_to_string(outside.join("fish")).unwrap(),
        "outside\n"
    );
    assert!(!sandbox.destination.join(".aaa").exists());
}

#[test]
fn type_conflicts_can_be_backed_up_and_replaced() {
    let sandbox = Sandbox::new(&[("dot_config/child", "managed\n"), ("dot_old", "new file\n")]);
    write(&sandbox.destination.join(".config"), "old file\n");
    write(&sandbox.destination.join(".old/unmanaged"), "keep backup\n");
    success(sandbox.init(&[]));
    success(sandbox.cli(&["apply", "--force"]));
    assert!(sandbox.destination.join(".config/child").exists());
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".old")).unwrap(),
        "new file\n"
    );
    let backup = fs::read_dir(sandbox.control().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        fs::read_to_string(backup.join("files/.config")).unwrap(),
        "old file\n"
    );
    assert_eq!(
        fs::read_to_string(backup.join("files/.old/unmanaged")).unwrap(),
        "keep backup\n"
    );
}

fn submodule_sandbox() -> (Sandbox, PathBuf) {
    let sandbox = Sandbox::new(&[("dot_file", "root\n")]);
    let module = sandbox.root.join("module");
    fs::create_dir(&module).unwrap();
    git(&module, &["init"]);
    git(&module, &["config", "user.name", "Test"]);
    git(&module, &["config", "user.email", "test@example.invalid"]);
    git(&module, &["config", "commit.gpgsign", "false"]);
    write(&module.join("function.fish"), "first function\n");
    git(&module, &["add", "--all"]);
    git(&module, &["commit", "-m", "module"]);
    git(
        &sandbox.work,
        &[
            "submodule",
            "add",
            module.to_str().unwrap(),
            "dot_config/fish/functions",
        ],
    );
    sandbox.push();
    (sandbox, module)
}

#[test]
fn submodules_are_recursive_by_default_and_follow_updates() {
    let (sandbox, module) = submodule_sandbox();
    success(sandbox.init(&["--apply"]));
    assert_eq!(
        fs::read_to_string(
            sandbox
                .destination
                .join(".config/fish/functions/function.fish")
        )
        .unwrap(),
        "first function\n"
    );
    write(&module.join("function.fish"), "updated function\n");
    git(&module, &["add", "--all"]);
    git(&module, &["commit", "-m", "module update"]);
    git(
        &sandbox.work.join("dot_config/fish/functions"),
        &["fetch", "origin"],
    );
    git(
        &sandbox.work.join("dot_config/fish/functions"),
        &["checkout", "FETCH_HEAD"],
    );
    sandbox.push();
    success(sandbox.cli(&["update"]));
    assert_eq!(
        fs::read_to_string(
            sandbox
                .destination
                .join(".config/fish/functions/function.fish")
        )
        .unwrap(),
        "updated function\n"
    );
}

#[test]
fn default_directories_and_saved_configuration_work_without_overrides() {
    let sandbox = Sandbox::new(&[("dot_settings", "default paths\n")]);
    success(
        sandbox
            .command()
            .arg("init")
            .arg(sandbox.url())
            .arg("--apply")
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(sandbox.home.join(".settings")).unwrap(),
        "default paths\n"
    );
    assert!(sandbox.control().join("source/.git").exists());
    assert!(success(sandbox.cli(&["status"])).contains("0 change(s), 0 conflict(s)"));
}

#[test]
fn source_cannot_overwrite_stool_control_storage() {
    let mut sandbox = Sandbox::new(&[
        ("dot_aaa", "new\n"),
        ("dot_stool/dotfile/config.toml", "malicious\n"),
    ]);
    sandbox.destination = sandbox.home.clone();
    failure(
        sandbox.init(&["--apply"]),
        "overlaps source/control storage",
    );
    assert!(!sandbox.home.join(".aaa").exists());
    let config = fs::read_to_string(sandbox.control().join("config.toml")).unwrap();
    assert!(config.contains("version = 1") && !config.contains("malicious"));
}

#[test]
fn each_destination_has_an_independent_applied_baseline() {
    let sandbox = Sandbox::new(&[("dot_settings", "v1\n")]);
    success(sandbox.init(&["--apply"]));
    write(&sandbox.source.join("dot_settings"), "v2\n");
    let other = sandbox.root.join("other destination");
    write(&other.join(".settings"), "v1\n");
    failure(
        sandbox
            .command()
            .arg("--destination")
            .arg(&other)
            .arg("apply")
            .output()
            .unwrap(),
        "Conflicting destination",
    );
    assert_eq!(fs::read_to_string(other.join(".settings")).unwrap(), "v1\n");
    success(sandbox.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".settings")).unwrap(),
        "v2\n"
    );
}

#[test]
fn empty_regular_source_removes_managed_target_with_a_backup() {
    let sandbox = Sandbox::new(&[("dot_settings", "previous contents\n")]);
    success(sandbox.init(&["--apply"]));
    write(&sandbox.work.join("dot_settings"), "");
    sandbox.push();
    assert!(success(sandbox.cli(&["update"])).contains("Applied 1 change(s)"));
    assert!(!sandbox.destination.join(".settings").exists());
    let backup = fs::read_dir(sandbox.control().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        fs::read_to_string(backup.join("files/.settings")).unwrap(),
        "previous contents\n"
    );
    assert!(success(sandbox.cli(&["apply"])).contains("Applied 0 change(s)"));
}

#[cfg(unix)]
#[test]
fn partial_io_failure_preserves_successful_state_and_can_be_retried() {
    let sandbox = Sandbox::new(&[("dot_aaa", "first\n"), ("dot_zzz", "replacement\n")]);
    fs::create_dir_all(sandbox.destination.join(".zzz")).unwrap();
    let fifo = sandbox.destination.join(".zzz/special");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    success(sandbox.init(&[]));
    failure(
        sandbox.cli(&["apply", "--force"]),
        "Deployment stopped after 1 change(s)",
    );
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".aaa")).unwrap(),
        "first\n"
    );
    let state = fs::read_dir(sandbox.control().join("state"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let state: serde_json::Value = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert!(state["entries"][".aaa"].is_object());
    assert!(state["entries"][".zzz"].is_null());
    assert!(fs::symlink_metadata(&fifo).is_ok());
    fs::remove_file(fifo).unwrap();
    assert!(success(sandbox.cli(&["apply", "--force"])).contains("Applied 1 change(s)"));
    assert_eq!(
        fs::read_to_string(sandbox.destination.join(".zzz")).unwrap(),
        "replacement\n"
    );
}

#[test]
fn recursive_submodules_can_be_disabled_and_explicitly_enabled_later() {
    let (sandbox, _) = submodule_sandbox();
    success(sandbox.init(&["--no-recurse-submodules", "--apply"]));
    assert!(sandbox.destination.join(".file").is_file());
    let module_file = sandbox
        .source
        .join("dot_config/fish/functions/function.fish");
    assert!(!module_file.exists());
    success(sandbox.cli(&["update"]));
    assert!(!module_file.exists());
    success(sandbox.init(&["--recurse-submodules", "--apply"]));
    assert!(module_file.is_file());
    assert!(sandbox
        .destination
        .join(".config/fish/functions/function.fish")
        .is_file());
    assert!(!sandbox
        .init(&["--no-recurse-submodules", "--recurse-submodules"])
        .status
        .success());
}

#[test]
fn deployment_cannot_remove_or_overwrite_the_new_global_initialization_config() {
    for (source, contents) in [
        (".stoolremove", ".stool/config.toml\n"),
        ("dot_STOOL/config.toml", "malicious\n"),
    ] {
        let mut sandbox = Sandbox::new(&[("dot_aaa", "new\n"), (source, contents)]);
        sandbox.destination = sandbox.home.clone();
        let global = sandbox.home.join(".stool/config.toml");
        let original = "version = 1\n[dotfile]\nurl = 'https://example.invalid/preserved.git'\n";
        write(&global, original);
        failure(
            sandbox.init(&["--apply"]),
            "overlaps source/control storage",
        );
        assert_eq!(fs::read_to_string(global).unwrap(), original);
        assert!(!sandbox.home.join(".aaa").exists());
    }
}
