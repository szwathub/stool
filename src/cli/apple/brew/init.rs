use super::Options;
use clap::Args;

#[derive(Args, Debug)]
pub struct InitArgs {
    #[command(flatten)]
    pub options: Options,
}
