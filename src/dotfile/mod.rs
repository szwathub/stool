use crate::common::fs as filesystem;
mod archive;
mod crypto;
mod external;
mod git;
mod maintenance;
mod native_git;
mod plan;
mod prerequisites;
mod process;
mod quoting;
mod scripts;
mod secrets;
mod source;
mod storage;
mod template;
mod tools;
mod ui;

use crate::cli::dotfile::{Commands, DotfileCommand};
use anyhow::{bail, Result};

pub fn run(command: &DotfileCommand, verbose: bool) -> Result<()> {
    if let Commands::Keygen { output } = &command.command {
        println!("{}", crypto::keygen(output)?);
        return Ok(());
    }
    let initial = storage::Paths::load(command.source.as_deref(), command.destination.as_deref())?;
    if let Commands::Cd { print } = &command.command {
        return tools::cd(&initial, *print);
    }
    let write = matches!(
        &command.command,
        Commands::Init(_) | Commands::Update | Commands::Add(_) | Commands::Edit(_)
    ) || matches!(&command.command,Commands::Apply(args) if !args.dry_run);
    let _lock = filesystem::lock(&initial.root, write)?;
    let mut paths =
        storage::Paths::load(command.source.as_deref(), command.destination.as_deref())?;
    if verbose {
        eprintln!(
            "Source: {}\nDestination: {}",
            paths.source.display(),
            paths.destination.display()
        );
    }
    match &command.command {
        Commands::ExecuteTemplate(args) => {
            return tools::execute_template(&paths, args, &command.data)
        }
        Commands::Doctor => return tools::doctor(&paths, &command.data),
        Commands::Init(args) => {
            git::initialize(&paths, args)?;
            paths =
                storage::Paths::load(command.source.as_deref(), command.destination.as_deref())?;
            template::initialize_config(&paths, &command.data)?;
            paths =
                storage::Paths::load(command.source.as_deref(), command.destination.as_deref())?;
            if args.builtin {
                let mut config = paths.config.clone().unwrap();
                config.git.builtin = true;
                paths.save_config(&config)?;
                paths = storage::Paths::load(
                    command.source.as_deref(),
                    command.destination.as_deref(),
                )?;
            }
            if paths.config.as_ref().is_some_and(|cfg| cfg.git.builtin) {
                native_git::mark(&paths.source)?;
            } else if native_git::enabled(&paths.source) {
                std::fs::remove_file(paths.source.join(".git/stool-builtin"))?;
            }
            git::lfs(&paths)?;
            if !args.apply {
                return Ok(());
            }
        }
        Commands::Update => {
            git::update(&paths)?;
            git::lfs(&paths)?;
        }
        _ => {}
    }
    git::ready_for(&paths)?;
    let engine = template::Engine::new(&paths, &command.data, false)?;
    match &command.command {
        Commands::Data => {
            println!("{}", serde_json::to_string_pretty(&engine.data)?);
            return Ok(());
        }
        Commands::Add(args) => return maintenance::add(&paths, &engine, args),
        Commands::Edit(args) => {
            maintenance::edit(&paths, &engine, args)?;
            if !args.apply {
                return Ok(());
            }
        }
        Commands::Unmanaged => return maintenance::unmanaged(&paths, &engine),
        _ => {}
    }
    let mut tree = source::scan(
        &paths,
        &engine,
        !matches!(command.command, Commands::Managed),
    )?;
    external::load(&paths, &engine, &mut tree, write)?;
    if matches!(command.command, Commands::Managed) {
        for entry in tree.entries.values() {
            paths.check_target(&paths.destination.join(&entry.relative))?;
            if !matches!(
                entry.content,
                source::Content::Absent | source::Content::Remove { .. }
            ) {
                println!("{}", paths.destination.join(&entry.relative).display());
            }
        }
        return Ok(());
    }
    let mut state = paths.state()?;
    let plan = plan::Plan::build(&paths, tree, &state)?;
    match &command.command {
        Commands::Diff => {
            plan.show(true);
            plan.show_scripts(&paths)?;
        }
        Commands::Status => {
            plan.show(false);
            plan.show_scripts(&paths)?;
        }
        Commands::Verify => {
            plan.show(false);
            if plan.changed() {
                bail!("Destination verification failed");
            }
        }
        Commands::Archive(args) => archive::export(&plan, args.format, &args.output)?,
        Commands::Apply(args) if args.dry_run => {
            plan.show(true);
            plan.show_scripts(&paths)?;
        }
        Commands::Apply(args) => {
            let force = args.force || (plan.conflicts() && ui::confirm(&plan, args.prompt)?);
            plan.apply(&paths, &mut state, force)?;
        }
        Commands::Init(_) | Commands::Update | Commands::Edit(_) => {
            plan.apply(&paths, &mut state, false)?
        }
        _ => unreachable!("command handled above"),
    }
    Ok(())
}
