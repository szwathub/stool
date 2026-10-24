use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_stool"));
    command
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("PATH", home)
        .stdin(Stdio::null());
    command
}

#[test]
fn removed_agent_commands_fail_during_parsing_without_creating_files() {
    let temporary = tempfile::tempdir().unwrap();
    for args in [
        vec!["agent", "--help"],
        vec!["agent", "init", "fixture"],
        vec!["agent", "copilot"],
        vec!["help", "agent"],
    ] {
        let output = command(temporary.path()).args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}

#[test]
fn help_completions_and_manuals_do_not_advertise_agent() {
    let temporary = tempfile::tempdir().unwrap();
    let output = command(temporary.path()).arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    let names = help
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    assert!(!names.contains(&"agent"));
    for name in ["init", "dotfile", "secrets"] {
        assert!(names.contains(&name));
    }
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let output = command(temporary.path())
            .args(["completions", shell])
            .output()
            .unwrap();
        assert!(output.status.success(), "{shell}");
        let text = String::from_utf8(output.stdout).unwrap();
        for removed in ["copilot", "stool__agent", "stool;agent", "stool agent"] {
            assert!(!text.contains(removed), "{shell}: {removed}");
        }
    }
    let pages = temporary.path().join("man");
    let output = command(temporary.path())
        .args(["man", "--output"])
        .arg(&pages)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(pages.join("stool.1").is_file());
    for removed in [
        "stool-agent.1",
        "stool-agent-init.1",
        "stool-agent-copilot.1",
    ] {
        assert!(!pages.join(removed).exists());
    }
}
