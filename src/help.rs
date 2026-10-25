use crate::cli::Cli;
use anyhow::Result;
use clap::CommandFactory;
use std::io::Write;
use std::path::Path;

pub fn completions(shell: clap_complete::Shell) {
    clap_complete::generate(shell, &mut Cli::command(), "stool", &mut std::io::stdout());
}
pub fn man(directory: Option<&Path>) -> Result<()> {
    let command = Cli::command();
    if let Some(directory) = directory {
        std::fs::create_dir_all(directory)?;
        write_man(&command, "stool", directory)?;
    } else {
        let mut bytes = Vec::new();
        clap_mangen::Man::new(command).render(&mut bytes)?;
        std::io::stdout().write_all(&bytes)?;
    }
    Ok(())
}
pub(crate) fn write_man(command: &clap::Command, name: &str, directory: &Path) -> Result<()> {
    let mut bytes = Vec::new();
    clap_mangen::Man::new(command.clone().name(name.to_owned())).render(&mut bytes)?;
    let text = String::from_utf8(bytes)?;
    let text = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        directory.join(format!("{name}.1")),
        format!("{}\n", text.trim_end()),
    )?;
    for child in command.get_subcommands() {
        write_man(child, &format!("{name}-{}", child.get_name()), directory)?;
    }
    Ok(())
}
