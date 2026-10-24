use super::*;
use std::collections::VecDeque;
use std::os::unix::process::ExitStatusExt;

fn output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
    Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}
fn value(value: &str) -> Output {
    output(0, format!("{value}\n").as_bytes(), b"")
}
fn missing() -> Output {
    output(1, b"", b"2026-10-24 defaults[1:1]\nThe domain/default pair of (com.apple.dock, mineffect) does not exist\n")
}
fn run(dry_run: bool, outputs: Vec<Output>) -> (Result<()>, Vec<Vec<String>>, String) {
    let mut remaining = VecDeque::from(outputs);
    let mut calls = Vec::new();
    let mut report = Vec::new();
    let result = configure(
        dry_run,
        false,
        |args| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            Ok(remaining.pop_front().expect("Unexpected defaults call"))
        },
        &mut report,
    );
    assert!(remaining.is_empty(), "Not all expected calls were made");
    (result, calls, String::from_utf8(report).unwrap())
}

#[test]
fn unset_or_changed_preferences_are_written_and_verified() {
    for before in [missing(), value("genie"), value(" suck "), value("suck\n")] {
        let (result, calls, report) = run(false, vec![before, output(0, b"", b""), value(VALUE)]);
        result.unwrap();
        assert_eq!(
            calls,
            [
                vec!["read", DOMAIN, KEY],
                vec!["write", DOMAIN, KEY, "-string", VALUE],
                vec!["read", DOMAIN, KEY]
            ]
        );
        assert!(report.contains("Updated:"));
    }
}

#[test]
fn matching_preference_is_idempotent_and_dry_run_never_writes() {
    for dry_run in [false, true] {
        let (result, calls, report) = run(dry_run, vec![value(VALUE)]);
        result.unwrap();
        assert_eq!(calls, [vec!["read", DOMAIN, KEY]]);
        assert!(report.contains("Unchanged:"));
    }
    for before in [missing(), value("genie")] {
        let (result, calls, report) = run(true, vec![before]);
        result.unwrap();
        assert_eq!(calls, [vec!["read", DOMAIN, KEY]]);
        assert!(report.contains("Would set:"));
    }
}

#[test]
fn repeated_runs_recheck_preferences_and_reapply_user_changes() {
    let mut saved = "genie".to_owned();
    let mut writes = 0;
    for overridden in [false, false, true] {
        if overridden {
            saved = "scale".to_owned();
        }
        configure(
            false,
            false,
            |args| {
                if args[0] == "read" {
                    Ok(value(&saved))
                } else {
                    writes += 1;
                    saved = args[4].to_owned();
                    Ok(output(0, b"", b""))
                }
            },
            &mut Vec::new(),
        )
        .unwrap();
    }
    assert_eq!(writes, 2);
    assert_eq!(saved, VALUE);
}

#[test]
fn read_errors_are_not_treated_as_unset_preferences() {
    for before in [
        output(1, b"", b"Permission denied"),
        output(
            1,
            b"",
            b"The domain/default pair of (another.domain, mineffect) does not exist\n",
        ),
        output(
            2,
            b"",
            b"The domain/default pair of (com.apple.dock, mineffect) does not exist\n",
        ),
    ] {
        for dry_run in [false, true] {
            let (result, calls, _) = run(dry_run, vec![before.clone()]);
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains("Read macOS preference com.apple.dock mineffect failed"),
                "{error}"
            );
            assert_eq!(calls.len(), 1);
        }
    }
    let (result, calls, _) = run(false, vec![output(0, b"\xff", b"")]);
    assert!(result.unwrap_err().to_string().contains("UTF-8"));
    assert_eq!(calls.len(), 1);
}

#[test]
fn failed_write_is_reported_and_a_later_run_can_retry() {
    let (result, calls, report) = run(
        false,
        vec![missing(), output(13, b"", b"Permission denied")],
    );
    let error = result.unwrap_err().to_string();
    assert!(error.contains("Write macOS preference com.apple.dock mineffect failed"));
    assert!(error.contains("Permission denied"));
    assert_eq!(calls.len(), 2);
    assert!(!report.contains("Updated:"));
    run(false, vec![missing(), output(0, b"", b""), value(VALUE)])
        .0
        .unwrap();
}

#[test]
fn successful_exit_without_persisted_value_is_an_error() {
    for after in [missing(), value("genie")] {
        let (result, _, report) = run(false, vec![missing(), output(0, b"", b""), after]);
        assert!(result.unwrap_err().to_string().contains("did not retain"));
        assert!(!report.contains("Updated:"));
    }
    let (result, _, _) = run(
        false,
        vec![
            missing(),
            output(0, b"", b""),
            output(1, b"", b"Permission denied"),
        ],
    );
    assert!(format!("{:#}", result.unwrap_err()).contains("Verify saved macOS preference"));
}

#[test]
fn spawn_errors_preserve_the_setting_context() {
    let error = configure(
        false,
        false,
        |_| anyhow::bail!("executable unavailable"),
        &mut Vec::new(),
    )
    .unwrap_err();
    let error = format!("{error:#}");
    assert!(error.contains("Read macOS preference com.apple.dock mineffect"));
    assert!(error.contains("executable unavailable"));
}
