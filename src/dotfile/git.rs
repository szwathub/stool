use super::storage::{Config, Paths};
use crate::cli::dotfile::InitArgs;
use crate::common::git::validate_url;
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Keep canonical paths internally, adapting only arguments passed to Git.
fn system_path(path: &Path) -> &Path {
    #[cfg(windows)]
    {
        dunce::simplified(path)
    }
    #[cfg(not(windows))]
    {
        path
    }
}

fn command(directory: Option<&Path>) -> Command {
    let mut cmd = Command::new("git");
    if let Some(directory) = directory {
        cmd.arg("-C").arg(system_path(directory));
    }
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
    ] {
        cmd.env_remove(key);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

fn checked(mut command: Command) -> Result<Output> {
    let output = command
        .output()
        .map_err(super::prerequisites::git_spawn_error)?;
    if !output.status.success() {
        bail!(
            "Git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

pub fn run<I, S>(directory: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect();
    if super::native_git::enabled(directory) {
        if let Some(output) = super::native_git::run(directory, &args)? {
            return Ok(output);
        }
    }
    let mut cmd = command(Some(directory));
    cmd.args(args);
    let output = checked(cmd)?;
    Ok(String::from_utf8(output.stdout)
        .context("Git returned non-UTF-8 output")?
        .trim_end()
        .to_owned())
}

pub fn clone_repository(
    url: &str,
    destination: &Path,
    branch: Option<&str>,
    recursive: bool,
) -> Result<()> {
    validate_url(url)?;
    let mut cmd = command(None);
    cmd.arg("clone");
    if let Some(branch) = branch {
        cmd.arg("--branch").arg(branch);
    }
    if recursive {
        cmd.arg("--recurse-submodules");
    }
    cmd.arg("--").arg(url).arg(system_path(destination));
    checked(cmd)?;
    ready_recursive(&fs::canonicalize(destination)?, recursive)
}

pub fn lfs(paths: &Paths) -> Result<()> {
    if paths.config.as_ref().is_some_and(|cfg| cfg.git.lfs) {
        run(&paths.source, ["lfs", "version"]).context("Install git-lfs before using git.lfs")?;
        run(&paths.source, ["lfs", "pull"]).context("Download Git LFS objects")?;
    }
    Ok(())
}

pub fn verify(source: &Path) -> Result<()> {
    if !source.join(".git").exists() {
        bail!(
            "Source is not an initialized Git repository: {}. Run stool dotfile init <git-url>",
            source.display()
        );
    }
    let top = run(source, ["rev-parse", "--show-toplevel"])?;
    if fs::canonicalize(top)? != source {
        bail!("Source must be the repository root");
    }
    run(source, ["rev-parse", "--verify", "HEAD"])
        .context("The source repository must contain a commit")?;
    Ok(())
}

pub fn ready_for(paths: &Paths) -> Result<()> {
    ready_recursive(
        &paths.source,
        paths
            .config
            .as_ref()
            .map_or(true, |config| config.recurse_submodules),
    )
}

fn ready_recursive(source: &Path, recursive: bool) -> Result<()> {
    verify(source)?;
    if !recursive || !source.join(".gitmodules").exists() {
        return Ok(());
    }
    let status = run(source, ["submodule", "status", "--recursive"])?;
    if status
        .lines()
        .any(|line| matches!(line.as_bytes().first(), Some(b'-' | b'+' | b'U')))
    {
        bail!("Submodules are missing or do not match the recorded commits. Initialize with --recurse-submodules or run git -C '{}' submodule update --init --recursive", source.display());
    }
    Ok(())
}

pub fn initialize(paths: &Paths, args: &InitArgs) -> Result<()> {
    let repo = match &args.repo {
        Some(url) => url.clone(),
        None => crate::common::config::Config::load(&paths.home)?.dotfile_url()?.context("No default dotfile URL; run stool init --dotfile-url <git-url> or pass a URL to stool dotfile init")?.to_owned(),
    };
    validate_url(&repo)?;
    let saved = paths
        .config
        .as_ref()
        .filter(|config| config.source == paths.source);
    let recursive = if args.no_recurse_submodules {
        false
    } else if args.recurse_submodules {
        true
    } else {
        saved.map_or(true, |config| config.recurse_submodules)
    };
    if let Some(branch) = &args.branch {
        if branch.starts_with('-') {
            bail!("Invalid branch name");
        }
        gix::refs::FullName::try_from(format!("refs/heads/{branch}"))
            .context("Invalid branch name")?;
    }
    let system_backend = if paths.source.exists() {
        !super::native_git::enabled(&paths.source)
    } else {
        !args.builtin
    };
    if system_backend
        || args.recurse_submodules
        || (recursive && paths.source.join(".gitmodules").exists())
    {
        super::prerequisites::git().context("Check Git before initializing dotfiles")?;
    }
    if fs::symlink_metadata(&paths.source).is_ok() {
        verify(&paths.source)?;
        if run(&paths.source, ["remote", "get-url", "origin"])? != repo {
            bail!(
                "Source origin differs from the requested URL; choose another --source directory"
            );
        }
        let actual_branch = run(&paths.source, ["symbolic-ref", "--short", "HEAD"])?;
        let expected = args.branch.as_ref().or_else(|| {
            paths
                .config
                .as_ref()
                .filter(|cfg| cfg.source == paths.source)
                .map(|cfg| &cfg.branch)
        });
        if expected.is_some_and(|expected| expected != &actual_branch) {
            bail!("Source branch differs from the requested/saved branch; existing files were preserved");
        }
        if recursive && paths.source.join(".gitmodules").exists() {
            ensure_clean(&paths.source)?;
            run(
                &paths.source,
                ["submodule", "update", "--init", "--recursive"],
            )?;
        }
        ready_recursive(&paths.source, recursive)?;
        println!("Reusing {}", paths.source.display());
    } else {
        let parent = paths.source.parent().context("Source has no parent")?;
        fs::create_dir_all(parent)?;
        let temporary = tempfile::Builder::new()
            .prefix(".stool-clone-")
            .tempdir_in(parent)?;
        let checkout = temporary.path().join("repository");
        if args.builtin {
            super::native_git::clone(&repo, &checkout, args.branch.as_deref())?;
            if recursive && checkout.join(".gitmodules").exists() {
                super::prerequisites::git().context("Recursive submodules need system Git")?;
                run(&checkout, ["submodule", "update", "--init", "--recursive"])?;
            }
        } else {
            clone_repository(&repo, &checkout, args.branch.as_deref(), recursive)?;
        }
        ready_recursive(&fs::canonicalize(&checkout)?, recursive)?;
        fs::rename(&checkout, &paths.source)?;
        println!("Cloned {}", paths.source.display());
    }
    let config = Config {
        version: 1,
        repo,
        branch: run(&paths.source, ["symbolic-ref", "--short", "HEAD"])?,
        source: paths.source.clone(),
        destination: paths.destination.clone(),
        recurse_submodules: recursive,
        data: paths
            .config
            .as_ref()
            .map(|cfg| cfg.data.clone())
            .unwrap_or_else(super::storage::empty_object),
        encryption: paths.encryption(),
        git: paths
            .config
            .as_ref()
            .map(|cfg| cfg.git.clone())
            .unwrap_or_default(),
        editor: paths.config.as_ref().and_then(|cfg| cfg.editor.clone()),
    };
    let mut config = config;
    config.git.builtin |= args.builtin;
    paths.save_config(&config)?;
    println!(
        "Source: {}\nDestination: {}",
        paths.source.display(),
        paths.destination.display()
    );
    if !args.apply {
        println!("Next: stool dotfile diff; stool dotfile apply");
    }
    Ok(())
}

fn ensure_clean(source: &Path) -> Result<()> {
    if !run(
        source,
        [
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?
    .is_empty()
    {
        bail!("Source has local changes (including untracked files/submodules); commit or move them before update");
    }
    Ok(())
}

pub fn update(paths: &Paths) -> Result<()> {
    let config = paths
        .config
        .as_ref()
        .context("Run stool dotfile init <git-url> before update")?;
    validate_url(&config.repo)?;
    if config.source != paths.source {
        bail!("Selected source differs from saved configuration; initialize it first");
    }
    verify(&paths.source)?;
    if run(&paths.source, ["remote", "get-url", "origin"])? != config.repo
        || run(&paths.source, ["symbolic-ref", "--short", "HEAD"])? != config.branch
    {
        bail!("Source origin or branch differs from saved configuration");
    }
    if super::native_git::enabled(&paths.source) && !paths.source.join(".gitmodules").exists() {
        return super::native_git::update(paths);
    }
    ensure_clean(&paths.source)?;
    run(&paths.source, ["fetch", "--prune", "origin"])?;
    let upstream = format!("refs/remotes/origin/{}", config.branch);
    run(&paths.source, ["rev-parse", "--verify", &upstream])?;
    let mut cmd = command(Some(&paths.source));
    cmd.args(["merge-base", "--is-ancestor", "HEAD", &upstream]);
    let output = cmd.output().context("Check fast-forward update")?;
    if output.status.code() == Some(1) {
        bail!("Source has local commits or has diverged from origin; only fast-forward updates are allowed");
    }
    if !output.status.success() {
        bail!(
            "Cannot check fast-forward update: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    run(
        &paths.source,
        ["merge", "--ff-only", "--no-edit", &upstream],
    )?;
    if config.recurse_submodules {
        run(&paths.source, ["submodule", "update", "--init", "--recursive"])
            .context("Source was fast-forwarded, but submodule checkout failed; repair submodules before apply")?;
    }
    ready_for(paths)?;
    println!(
        "Updated source to {}",
        run(&paths.source, ["rev-parse", "--short", "HEAD"])?
    );
    Ok(())
}

#[cfg(all(test, windows))]
#[path = "../../tests/unit/dotfile/git_windows.rs"]
mod windows_tests;
