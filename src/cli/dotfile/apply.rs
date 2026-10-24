use clap::Args;

#[derive(Args, Debug, Default)]
pub struct ApplyArgs {
    #[arg(long)]
    pub dry_run: bool,
    /// Back up and overwrite conflicting destination entries
    #[arg(long)]
    pub force: bool,
    /// Interactive conflict handling (auto only prompts on a terminal)
    #[arg(long, value_enum, default_value = "auto")]
    pub prompt: Prompt,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Default)]
pub enum Prompt {
    #[default]
    Auto,
    Always,
    Never,
}
