use super::process::{self, Executor};
use crate::cli::apple::brew::Options;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{IsTerminal, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const INSTALLER: &str = "https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh";

pub(super) struct Environment {
    pub path: Vec<PathBuf>,
    pub prefix: Option<PathBuf>,
    pub default_prefix: PathBuf,
    pub home: Option<PathBuf>,
    pub terminal: bool,
}
impl Environment {
    pub fn current() -> Result<Self> {
        let current = std::env::current_dir().context("Read current directory")?;
        Ok(Self {
            path: std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        current.join(path)
                    }
                })
                .collect(),
            prefix: std::env::var_os("HOMEBREW_PREFIX")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            default_prefix: default_prefix(std::env::consts::ARCH),
            home: std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            terminal: std::io::stdin().is_terminal(),
        })
    }
}

fn default_prefix(architecture: &str) -> PathBuf {
    PathBuf::from(if architecture == "aarch64" {
        "/opt/homebrew"
    } else {
        "/usr/local"
    })
}

pub(super) struct Homebrew {
    pub prefix: PathBuf,
    pub executable: PathBuf,
    selected: bool,
}

fn prefix(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        bail!(
            "Homebrew prefix must be an absolute directory without .. and other than /: {}",
            path.display()
        );
    }
    Ok(path.to_owned())
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn locate(selected: Option<&Path>, environment: &Environment) -> Result<Homebrew> {
    if let Some(selected) = selected.or(environment.prefix.as_deref()) {
        let prefix = prefix(selected)?;
        return Ok(Homebrew {
            executable: prefix.join("bin/brew"),
            prefix,
            selected: true,
        });
    }
    for directory in &environment.path {
        let candidate = directory.join("brew");
        if executable(&candidate) {
            return Ok(Homebrew {
                prefix: directory.parent().unwrap_or(directory).to_owned(),
                executable: candidate,
                selected: false,
            });
        }
    }
    let prefix = prefix(&environment.default_prefix)?;
    Ok(Homebrew {
        executable: prefix.join("bin/brew"),
        prefix,
        selected: true,
    })
}

fn validate(brew: &mut Homebrew, executor: &mut impl Executor) -> Result<()> {
    if !executable(&brew.executable) {
        bail!(
            "Homebrew at {} is not an executable file; repair it before retrying",
            brew.executable.display()
        );
    }
    let mut command = process::command(&brew.executable);
    command.arg("--version");
    let output = process::capture(executor, command, "Check Homebrew version")?;
    if !output.stdout.starts_with(b"Homebrew ") {
        bail!(
            "{} did not report a Homebrew version; check the selected prefix or PATH",
            brew.executable.display()
        );
    }
    let mut command = process::command(&brew.executable);
    command.arg("--prefix");
    let output = process::capture(executor, command, "Read Homebrew prefix")?;
    let reported = prefix(Path::new(
        std::str::from_utf8(&output.stdout)
            .context("Decode Homebrew prefix")?
            .trim_end_matches('\n'),
    ))?;
    if brew.selected
        && fs::canonicalize(&reported).unwrap_or_else(|_| reported.clone())
            != fs::canonicalize(&brew.prefix).unwrap_or_else(|_| brew.prefix.clone())
    {
        bail!(
            "Selected Homebrew prefix {} does not match reported prefix {}",
            brew.prefix.display(),
            reported.display()
        );
    }
    brew.prefix = reported;
    Ok(())
}

fn prerequisites(executor: &mut impl Executor) -> Result<()> {
    let mut command = process::command("/usr/bin/xcode-select");
    command.arg("--print-path");
    process::capture(executor, command, "Homebrew needs Xcode or Command Line Tools; run xcode-select --install, complete installation, then retry stool apple brew init")?;
    let mut command = process::command("/usr/bin/git");
    command.arg("--version");
    let output = process::capture(
        executor,
        command,
        "Homebrew needs a working Git; repair Command Line Tools, verify git --version, then retry",
    )?;
    if !output.stdout.starts_with(b"git version ") {
        bail!(
            "/usr/bin/git did not report a Git version; repair Command Line Tools before retrying"
        );
    }
    let mut command = process::command("/usr/bin/curl");
    command.arg("--version");
    let output = process::capture(
        executor,
        command,
        "Homebrew needs curl; restore /usr/bin/curl before retrying",
    )?;
    if !output.stdout.starts_with(b"curl ") {
        bail!("/usr/bin/curl did not report a curl version; restore /usr/bin/curl before retrying");
    }
    Ok(())
}

pub(super) fn ensure(
    selected: Option<&Path>,
    options: &Options,
    environment: &Environment,
    executor: &mut impl Executor,
    report: &mut impl Write,
    verbose: bool,
) -> Result<Homebrew> {
    let mut brew = locate(selected, environment)?;
    if verbose {
        writeln!(
            report,
            "[VERBOSE] Homebrew executable: {}",
            brew.executable.display()
        )?;
    }
    match fs::symlink_metadata(&brew.executable) {
        Ok(_) => {
            validate(&mut brew, executor)?;
            writeln!(report, "Homebrew ready: {}", brew.prefix.display())?;
            shellenv(&brew, report)?;
            return Ok(brew);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Inspect Homebrew executable"),
    }
    prerequisites(executor)?;
    if options.dry_run {
        writeln!(
            report,
            "Would download the official Homebrew installer and install at {}",
            brew.prefix.display()
        )?;
        return Ok(brew);
    }
    if !environment.terminal && !options.non_interactive {
        bail!("Homebrew installation needs an interactive terminal; rerun in a terminal or explicitly use --non-interactive after preparing privileges");
    }
    let installer = tempfile::Builder::new()
        .prefix("stool-homebrew-")
        .suffix(".sh")
        .tempfile()
        .context("Create private Homebrew installer file")?;
    let mut command = process::command("/usr/bin/curl");
    command
        .args([
            "--fail",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--max-time",
            "120",
            INSTALLER,
            "--output",
        ])
        .arg(installer.path());
    writeln!(
        report,
        "Downloading the official Homebrew installer for {}",
        brew.prefix.display()
    )?;
    process::capture(executor, command, "Download official Homebrew installer")?;
    if installer.as_file().metadata()?.len() == 0 {
        bail!("The downloaded Homebrew installer is empty");
    }
    let mut command = process::command("/bin/bash");
    command
        .arg(installer.path())
        .arg("--path")
        .arg(&brew.prefix);
    process::run(
        executor,
        command,
        options.non_interactive,
        "Install Homebrew",
    )?;
    validate(&mut brew, executor).context("Verify Homebrew after installation")?;
    writeln!(report, "Homebrew ready: {}", brew.prefix.display())?;
    shellenv(&brew, report)?;
    Ok(brew)
}

fn shellenv(brew: &Homebrew, report: &mut impl Write) -> Result<()> {
    writeln!(
        report,
        "For zsh/bash, enable Homebrew in the current terminal:\n  eval \"$({} shellenv)\"",
        shell_words::quote(&brew.executable.to_string_lossy())
    )?;
    Ok(())
}
