use std::process::Command;

fn command(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_stool"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn apple_commands_are_available_only_on_macos() {
    let output = command(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert_eq!(help.contains("  apple "), cfg!(target_os = "macos"));
    let output = command(&["apple", "--help"]);
    assert_eq!(output.status.success(), cfg!(target_os = "macos"));
    if cfg!(target_os = "macos") {
        assert!(String::from_utf8(output.stdout).unwrap().contains("init"));
    }
}

#[cfg(target_os = "macos")]
#[test]
fn apple_init_help_and_invalid_arguments_do_not_execute_preferences() {
    let output = command(&["apple", "init", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--dry-run"));
    for args in [
        vec!["apple"],
        vec!["apple", "unknown"],
        vec!["apple", "init", "--unknown"],
        vec!["apple", "init", "extra"],
    ] {
        assert!(!command(&args).status.success(), "{args:?}");
    }
}
