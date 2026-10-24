use anyhow::{anyhow, bail, Result};
use std::io;
use std::process::Command;

fn git_installation_guide(os: &str) -> &'static str {
    match os {
        "macos" => "Install Apple's Command Line Tools (including Git):\n  xcode-select --install\nComplete the installer, then verify with `git --version` and rerun the same stool dotfile init command.\nhttps://developer.apple.com/documentation/xcode/installing-the-command-line-tools",
        "windows" => "Install Git for Windows:\n  winget install --id Git.Git -e --source winget\nOr use https://git-scm.com/install/windows. Reopen your terminal, verify with `git --version`, then rerun the same stool dotfile init command.",
        "linux" => "Install Git with your distribution's package manager:\n  Debian/Ubuntu: sudo apt-get install git\n  Fedora: sudo dnf install git\n  Arch: sudo pacman -S git\n  Alpine: sudo apk add git\nVerify with `git --version`, then rerun the same stool dotfile init command.\nhttps://git-scm.com/install/linux",
        _ => "Install Git from https://git-scm.com/install/ and make it available on PATH.\nVerify with `git --version`, then rerun the same stool dotfile init command.",
    }
}

pub fn git_spawn_error(error: io::Error) -> anyhow::Error {
    if error.kind() == io::ErrorKind::NotFound {
        anyhow!(
            "Git was not found on PATH.\n{}",
            git_installation_guide(std::env::consts::OS)
        )
    } else {
        anyhow!(error).context("Cannot execute Git; check the executable and its permissions")
    }
}

/// Check before creating a clone staging directory or contacting the remote.
pub fn git() -> Result<()> {
    #[cfg(target_os = "macos")]
    if resolves_to_apple_git() {
        // /usr/bin/git is a launcher on fresh macOS installations. Executing it
        // without developer tools would open a system installer unexpectedly.
        let tools = Command::new("xcode-select").arg("--print-path").output();
        if !tools.is_ok_and(|output| output.status.success()) {
            bail!(
                "Git requires Xcode or Command Line Tools on this Mac.\n{}",
                git_installation_guide("macos")
            );
        }
    }
    let output = Command::new("git")
        .arg("--version")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(git_spawn_error)?;
    if !output.status.success() {
        bail!(
            "Git is installed but `git --version` failed. Repair the installation before cloning.\n{}\n{}",
            String::from_utf8_lossy(&output.stderr).trim(),
            git_installation_guide(std::env::consts::OS)
        );
    }
    if !String::from_utf8_lossy(&output.stdout).starts_with("git version ") {
        bail!("The git executable on PATH did not report a Git version. Check your PATH before retrying.");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn resolves_to_apple_git() -> bool {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
    std::env::split_paths(&path)
        .map(|directory| directory.join("git"))
        .find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .and_then(|git| std::fs::canonicalize(git).ok())
        .is_some_and(|git| git == Path::new("/usr/bin/git"))
}

#[cfg(test)]
#[path = "../../tests/unit/dotfile/prerequisites.rs"]
mod tests;
