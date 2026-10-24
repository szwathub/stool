#![cfg(unix)]

use crate::common::gpg::{fake_gpg, private_write};
use crate::common::{decrypt_age_fixture, Sandbox};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

const PERSONAL: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn command(sandbox: &Sandbox, operation: &str) -> Command {
    fs::create_dir_all(sandbox.home.join(".gnupg")).unwrap();
    fs::write(sandbox.root.join("only-personal"), "").unwrap();
    let bin = sandbox.root.join("key tools");
    fs::create_dir_all(&bin).unwrap();
    if !bin.join("gpg").exists() {
        fs::copy(fake_gpg(), bin.join("gpg")).unwrap();
    }
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_stool"));
    command
        .current_dir(&sandbox.root)
        .env("HOME", &sandbox.home)
        .env("USERPROFILE", &sandbox.home)
        .env("GNUPGHOME", sandbox.home.join(".gnupg"))
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("STOOL_TEST_GPG_ROOT", &sandbox.root)
        .arg("secrets")
        .arg(operation)
        .arg("--gpg");
    let file = sandbox.root.join("not-created/keys.tar.age");
    if operation == "backup" {
        command.arg("--output").arg(file);
    } else {
        command
            .arg("--input")
            .arg(file)
            .arg("--output")
            .arg(&sandbox.home);
    }
    command
}

fn termios(fd: RawFd, stage: &str) -> libc::termios {
    let mut settings = std::mem::MaybeUninit::uninit();
    unsafe {
        assert_eq!(
            libc::tcgetattr(fd, settings.as_mut_ptr()),
            0,
            "tcgetattr({fd}) {stage}: {}",
            std::io::Error::last_os_error()
        );
        settings.assume_init()
    }
}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct TerminalOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    transcript: String,
}

fn read_terminal(master: &mut File, output: &mut Vec<u8>) {
    loop {
        let mut bytes = [0; 8192];
        match master.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => output.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => panic!("Read terminal: {error}"),
        }
    }
}

fn terminal(mut command: Command, answers: &[&str]) -> TerminalOutput {
    let (mut master, mut slave) = (0, 0);
    unsafe {
        assert_eq!(
            libc::openpty(
                std::ptr::addr_of_mut!(master),
                std::ptr::addr_of_mut!(slave),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ),
            0
        );
    }
    let mut master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    let initial_flags = termios(master.as_raw_fd(), "before spawning").c_lflag;
    assert_ne!(initial_flags & libc::ECHO, 0);
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
        assert!(flags >= 0);
        assert!(libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0);
    }
    command
        // Password input must use the controlling terminal, independently of
        // stdin, which may be disconnected during GPG recovery.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(slave.try_clone().unwrap()));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(2, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = KillOnDrop(command.spawn().unwrap());
    let mut transcript = Vec::new();
    let (mut answered, mut cursor) = (0, 0);
    let start = Instant::now();
    let status = loop {
        read_terminal(&mut master, &mut transcript);
        if answered < answers.len()
            && String::from_utf8_lossy(&transcript[cursor..])
                .to_ascii_lowercase()
                .contains("password")
            && termios(master.as_raw_fd(), "while waiting for a prompt").c_lflag & libc::ECHO == 0
        {
            // Only send an answer once the terminal itself has disabled echo.
            // This avoids racing prompt output against hidden-input setup.
            master.write_all(answers[answered].as_bytes()).unwrap();
            master.write_all(b"\n").unwrap();
            answered += 1;
            cursor = transcript.len();
        }
        if let Some(status) = child.0.try_wait().unwrap() {
            read_terminal(&mut master, &mut transcript);
            break status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "Terminal prompt timed out: {}",
            String::from_utf8_lossy(&transcript)
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        answered,
        answers.len(),
        "Missing GPG backup password prompt: {}",
        String::from_utf8_lossy(&transcript)
    );
    assert_eq!(
        // macOS detaches the slave when its controlling session exits. The
        // master retains the terminal settings and remains queryable.
        termios(master.as_raw_fd(), "after child exited").c_lflag & libc::ECHO,
        initial_flags & libc::ECHO,
        "Terminal echo was not restored"
    );
    let mut stdout = Vec::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    TerminalOutput {
        status,
        stdout,
        transcript: String::from_utf8(transcript).unwrap(),
    }
}

#[test]
fn gpg_password_terminal_input_is_hidden_confirmed_and_restore_prompts_once() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let password = "fixture-terminal-password-42";
    let backed_up = terminal(command(&sandbox, "backup"), &[password, password]);
    assert!(backed_up.status.success(), "{}", backed_up.transcript);
    assert!(!backed_up.transcript.contains(password));
    assert!(!String::from_utf8_lossy(&backed_up.stdout).contains(password));
    let secret = fs::read(sandbox.root.join("not-created/keys.tar.age")).unwrap();
    assert!(!String::from_utf8_lossy(&secret).contains("STOOL-FAKE-SECRET"));
    let restored = terminal(command(&sandbox, "restore"), &[password]);
    assert!(restored.status.success(), "{}", restored.transcript);
    assert!(!restored.transcript.contains(password));
    assert!(!String::from_utf8_lossy(&restored.stdout).contains(password));
    assert!(!restored.transcript.to_ascii_lowercase().contains("confirm"));
    assert_eq!(
        fs::read_to_string(sandbox.root.join("imports"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn gpg_password_confirmation_mismatch_creates_no_backup_or_echoed_answers() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let directory = sandbox.root.join("not-created/keys.tar.age");
    let backup = command(&sandbox, "backup");
    let answers = ["fixture-first-password-42", "fixture-different-password-43"];
    let output = terminal(backup, &answers);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.transcript.to_ascii_lowercase().contains("match"));
    assert!(!output.transcript.contains("BEGIN AGE ENCRYPTED FILE"));
    for answer in answers {
        assert!(!output.transcript.contains(answer));
    }
    assert!(!directory.parent().unwrap().exists());
    assert!(!sandbox.root.join("imports").exists());
}

fn piped_password(mut command: Command, password: &str) -> Output {
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .args(["--passphrase-file", "/dev/fd/0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(password.as_bytes()).unwrap();
    input.write_all(b"\n").unwrap();
    drop(input);
    child.wait_with_output().unwrap()
}

#[test]
fn gpg_backup_accepts_a_password_pipe_without_a_terminal_or_prompt() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let password = "fixture-only-pipe-password-42";
    let backed_up = piped_password(command(&sandbox, "backup"), password);
    assert!(
        backed_up.status.success(),
        "{}",
        String::from_utf8_lossy(&backed_up.stderr)
    );
    assert!(backed_up.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&backed_up.stdout).contains(password));
    let cipher = fs::read(sandbox.root.join("not-created/keys.tar.age")).unwrap();
    assert_eq!(
        archive_secret(password, &cipher),
        format!("STOOL-FAKE-SECRET\n{PERSONAL}\nsub:DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD\n")
            .as_bytes()
    );
    // Explicit restore requires a private regular file rather than a pipe.
    let password_file = sandbox.root.join("private password");
    private_write(&password_file, password.as_bytes());
    let restored = command(&sandbox, "restore")
        .arg("--passphrase-file")
        .arg(password_file)
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert!(restored.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&restored.stdout).contains(password));
    assert_eq!(
        fs::read_to_string(sandbox.root.join("imports"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

fn archive_secret(password: &str, cipher: &[u8]) -> Vec<u8> {
    let payload = decrypt_age_fixture(password, cipher);
    let mut archive = tar::Archive::new(payload.as_slice());
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().as_ref() == std::path::Path::new("gpg/secret.asc") {
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            return data;
        }
    }
    panic!("Missing GPG secret export in encrypted archive");
}
