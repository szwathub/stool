use crate::cli::{
    init::InitCommand,
    secrets::{RestoreArgs, Selection},
};
use crate::common::config::{self, Config};
use crate::common::{fs as filesystem, git::validate_url};
use crate::secrets::{self, RestoreSession};
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

trait Archive {
    fn has_gpg(&self) -> bool;
    fn has_ssh(&self) -> bool;
    fn restore(&self, gpg: bool, ssh: bool) -> Result<()>;
}
impl Archive for RestoreSession {
    fn has_gpg(&self) -> bool {
        self.has_gpg()
    }
    fn has_ssh(&self) -> bool {
        self.has_ssh()
    }
    fn restore(&self, gpg: bool, ssh: bool) -> Result<()> {
        self.restore(gpg, ssh)
    }
}

trait Platform {
    fn apple(&self) -> bool;
    fn gpg_available(&mut self) -> Result<bool>;
    fn homebrew(&mut self, non_interactive: bool, verbose: bool) -> Result<()>;
    fn packages(&mut self, file: &Path, non_interactive: bool, verbose: bool) -> Result<()>;
    fn preferences(&mut self, verbose: bool) -> Result<()>;
}
struct System;
impl Platform for System {
    fn apple(&self) -> bool {
        cfg!(target_os = "macos")
    }
    fn gpg_available(&mut self) -> Result<bool> {
        secrets::gpg_available()
    }
    fn homebrew(&mut self, non_interactive: bool, verbose: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let prefix = crate::apple::bootstrap(None, false, non_interactive, verbose)?;
            extend_path(&[prefix.join("bin"), prefix.join("sbin")])?;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (non_interactive, verbose);
        Ok(())
    }
    fn packages(&mut self, file: &Path, non_interactive: bool, verbose: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            crate::apple::bootstrap(Some(file), true, non_interactive, verbose)?;
            if !non_interactive {
                if !io::stdin().is_terminal() {
                    bail!("Brewfile installation needs a terminal; explicitly use --non-interactive after reviewing the trusted Brewfile and preparing privileges");
                }
                eprint!("Install this trusted Brewfile? [y/N] ");
                io::stderr().flush()?;
                let mut reply = String::new();
                io::stdin().read_line(&mut reply)?;
                if !matches!(reply.trim(), "y" | "Y" | "yes" | "YES") {
                    bail!("Brewfile installation cancelled; completed initialization steps are retained");
                }
            }
            let prefix = crate::apple::bootstrap(Some(file), false, non_interactive, verbose)?;
            // GPG Suite exposes its command in /usr/local/bin, including on Apple Silicon.
            // The executable is still resolved and invoked as `gpg` through PATH.
            extend_path(&[
                prefix.join("bin"),
                prefix.join("sbin"),
                PathBuf::from("/usr/local/bin"),
            ])?;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (file, non_interactive, verbose);
        Ok(())
    }
    fn preferences(&mut self, verbose: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        crate::apple::initialize_preferences(verbose)?;
        #[cfg(not(target_os = "macos"))]
        let _ = verbose;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn extend_path(directories: &[PathBuf]) -> Result<()> {
    let mut paths = directories.to_vec();
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::set_var(
        "PATH",
        std::env::join_paths(paths)
            .context("Prepare command lookup after Homebrew installation")?,
    );
    Ok(())
}

pub fn run(args: &InitCommand, verbose: bool) -> Result<()> {
    let home = config::home()?;
    let global = Config::load(&home)?;
    let url = match args.dotfile_url.as_deref().or(global.dotfile_url()?) {
        Some(url) => url.to_owned(),
        None => {
            if !io::stdin().is_terminal() || args.non_interactive {
                bail!("No default dotfile URL; pass --dotfile-url <git-url>");
            }
            eprint!("Dotfiles Git URL: ");
            io::stderr().flush()?;
            let mut value = String::new();
            io::stdin().read_line(&mut value)?;
            value.trim().to_owned()
        }
    };
    validate_url(&url)?;
    let brewfile = local_brewfile(args, &home)?;
    if let Some(file) = &args.key_file {
        RestoreSession::check_file(file)?;
    }
    if args.dry_run {
        println!("Would save dotfile.url to {}: {url}", global.path.display());
        if args.key_file.is_some() {
            println!("Would decrypt and restore offline keys; SSH first when GPG is unavailable (backup not decrypted in preview).");
        }
        if cfg!(target_os = "macos") {
            println!("Would check/initialize Homebrew and configure macOS preferences.");
            if let Some(file) = &brewfile {
                println!(
                    "Would review and install Brewfile: {} (not evaluated)",
                    file.display()
                );
            } else {
                println!("Skipped packages: ~/.Brewfile is absent; deploy dotfiles first.");
            }
        } else {
            println!("Skipped Apple initialization: this platform is not macOS.");
        }
        println!("Preview only: no configuration, keys or system settings changed; no repository contacted.");
        return Ok(());
    }
    let session = args
        .key_file
        .as_ref()
        .map(|file| {
            RestoreSession::open(&RestoreArgs {
                selection: Selection::default(),
                input: file.clone(),
                output: Some(home.clone()),
                passphrase_file: args.passphrase_file.clone(),
            })
        })
        .transpose()?;
    if let Some(session) = &session {
        session.preflight()?;
    }
    execute(
        args,
        &home,
        &url,
        brewfile.as_deref(),
        session.as_ref().map(|s| s as &dyn Archive),
        &mut System,
        verbose,
    )
}

fn local_brewfile(args: &InitCommand, home: &Path) -> Result<Option<PathBuf>> {
    let file = filesystem::expand_home(
        args.brewfile.as_deref().unwrap_or(&home.join(".Brewfile")),
        home,
    );
    match fs::metadata(&file) {
        Ok(metadata) => {
            if !metadata.is_file() {
                bail!("Brewfile must be a regular file: {}", file.display());
            }
            fs::read(&file).context("Read trusted local Brewfile")?;
            Ok(Some(file))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound && args.brewfile.is_none() => {
            Ok(None)
        }
        Err(error) => Err(error).with_context(|| format!("Read Brewfile {}", file.display())),
    }
}

fn execute(
    args: &InitCommand,
    home: &Path,
    url: &str,
    brewfile: Option<&Path>,
    archive: Option<&dyn Archive>,
    platform: &mut impl Platform,
    verbose: bool,
) -> Result<()> {
    Config::save_url(home, url)?;
    println!("Saved dotfile.url. Next configuration step: stool dotfile init");
    let mut pending_gpg = false;
    if let Some(archive) = archive {
        if archive.has_gpg() && platform.gpg_available()? {
            archive.restore(true, archive.has_ssh())?;
        } else {
            if archive.has_ssh() {
                archive.restore(false, true)?;
            }
            pending_gpg = archive.has_gpg();
        }
    } else {
        println!("Skipped keys: no --key-file supplied.");
    }
    if platform.apple() {
        platform.homebrew(args.non_interactive, verbose)?;
        if let Some(file) = brewfile {
            platform.packages(file, args.non_interactive, verbose)?;
        } else {
            println!("Skipped packages: ~/.Brewfile is absent; deploy dotfiles, then run stool apple brew install.");
        }
    } else {
        println!("Skipped Apple initialization: this platform is not macOS.");
    }
    if pending_gpg && platform.gpg_available()? {
        archive.unwrap().restore(true, false)?;
        pending_gpg = false;
    }
    if platform.apple() {
        platform.preferences(verbose)?;
    }
    if pending_gpg {
        println!("Pending: GPG keys have NOT been restored. Install GPG from your trusted Brewfile (stool apple brew install), or your platform's package manager; then run stool secrets restore --gpg --input <backup-file>.");
        println!("Basic initialization completed with pending GPG recovery; key migration is incomplete.");
    } else {
        println!("Initialization completed. Dotfiles remain a separate step: stool dotfile init; stool dotfile diff; stool dotfile apply; stool dotfile verify.");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/init.rs"]
mod tests;
