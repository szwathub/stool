use crate::common::*;

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_stool"));
    command
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home);
    command
}

#[test]
fn secrets_replaces_key_in_help_and_rejects_the_old_entry_before_io() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let before = snapshot(home);
    let help = success(command(home).arg("--help").output().unwrap());
    let names = help
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    assert!(names.contains(&"secrets"));
    assert!(!names.contains(&"key"));
    for operation in ["backup", "restore"] {
        let help = success(
            command(home)
                .args(["secrets", operation, "--help"])
                .output()
                .unwrap(),
        );
        assert!(help.contains(&format!("stool secrets {operation}")));
        for option in [
            "--gpg",
            "--ssh",
            "--all",
            "--input",
            "--output",
            "--passphrase-file",
        ] {
            assert!(help.contains(option), "Missing option {option}: {help}");
        }
    }
    let init_help = success(command(home).args(["init", "--help"]).output().unwrap());
    assert!(init_help.contains("stool secrets backup"));
    let old = command(home).args(["key", "--help"]).output().unwrap();
    assert_eq!(old.status.code(), Some(2));
    failure(old, "unrecognized subcommand");
    assert_eq!(snapshot(home), before);
}

#[test]
fn completions_and_manuals_expose_secrets_backup_and_restore() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let completions = success(
        command(home)
            .args(["completions", "bash"])
            .output()
            .unwrap(),
    );
    for suffix in ["", "__backup", "__restore"] {
        assert!(completions.contains(&format!("stool__secrets{suffix}")));
    }
    assert!(!completions.contains("stool__key"));
    let pages = home.join("man");
    success(
        command(home)
            .args(["man", "--output"])
            .arg(&pages)
            .output()
            .unwrap(),
    );
    for suffix in ["", "-backup", "-restore"] {
        assert!(pages.join(format!("stool-secrets{suffix}.1")).is_file());
        assert!(!pages.join(format!("stool-key{suffix}.1")).exists());
    }
}
