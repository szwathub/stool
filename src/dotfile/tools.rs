use super::storage::Paths;
use super::{git, scripts, template};
use crate::cli::dotfile::TemplateArgs;
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::process::Command;

pub fn execute_template(paths: &Paths, args: &TemplateArgs, overrides: &[String]) -> Result<()> {
    let engine = template::Engine::new(paths, overrides, args.init)?;
    let (name, bytes) = if let Some(file) = &args.file {
        (file.to_string_lossy().into_owned(), fs::read(file)?)
    } else if let Some(text) = &args.template {
        ("<argument>".into(), text.as_bytes().to_vec())
    } else {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        ("<stdin>".into(), bytes)
    };
    io::stdout().write_all(&engine.render(&name, &bytes)?)?;
    Ok(())
}

pub fn cd(paths: &Paths, print: bool) -> Result<()> {
    git::verify(&paths.source)?;
    if print {
        println!("{}", paths.source.display());
        return Ok(());
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "powershell.exe".into()
        } else {
            "sh".into()
        }
    });
    let mut command = Command::new(shell);
    command.current_dir(&paths.source);
    let status = command.status().context("Start source-directory shell")?;
    if !status.success() {
        bail!("Source shell exited with {status}");
    }
    Ok(())
}

pub fn available(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| {
            let path = directory.join(name);
            if path.is_file() {
                return true;
            }
            cfg!(windows)
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|extension| directory.join(format!("{name}.{extension}")).is_file())
        })
    })
}

pub fn doctor(paths: &Paths, overrides: &[String]) -> Result<()> {
    let mut failures = 0;
    println!(
        "stool {} / {} / {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if super::native_git::enabled(&paths.source) {
        println!("OK built-in Git (SSH transport, submodules and LFS may need external tools)");
    } else if available("git") {
        println!("OK system Git");
    } else {
        println!("ERROR system Git is missing");
        failures += 1;
    }
    for (name, result) in [
        ("repository", git::verify(&paths.source)),
        ("source/submodules", git::ready_for(paths)),
        ("applied state", paths.state().map(|_| ())),
        ("script database", scripts::Store::load(paths).map(|_| ())),
        (
            "template/data configuration",
            template::Engine::new(paths, overrides, false).map(|_| ()),
        ),
    ] {
        match result {
            Ok(()) => println!("OK {name}"),
            Err(error) => {
                println!("ERROR {name}: {error:#}");
                failures += 1;
            }
        }
    }
    if let Some(config) = &paths.config {
        if config.git.lfs && !available("git-lfs") {
            println!("ERROR git.lfs requires git-lfs");
            failures += 1;
        }
        if config.encryption.backend == "gpg" && !available("gpg") {
            println!("ERROR configured encryption backend requires gpg");
            failures += 1;
        }
        for identity in &config.encryption.identities {
            let path = filesystem::expand_home(identity, &paths.home);
            if !path.is_file() {
                println!("ERROR age identity is missing: {}", path.display());
                failures += 1;
            }
        }
        if let Ok(status) = git::run(
            &paths.source,
            ["status", "--porcelain=v1", "--untracked-files=all"],
        ) {
            if !status.is_empty() {
                println!("WARN source has local changes; update will require a clean worktree");
            }
        }
    } else {
        println!("WARN no saved dotfile configuration");
    }
    for executable in [
        "gpg",
        "git-lfs",
        "bw",
        "op",
        "pass",
        "gopass",
        "vault",
        "keepassxc-cli",
    ] {
        if available(executable) {
            println!("OK optional executable {executable}");
        }
    }
    println!("{failures} error(s)");
    if failures > 0 {
        bail!("Doctor found {failures} configuration/dependency error(s)");
    }
    Ok(())
}
