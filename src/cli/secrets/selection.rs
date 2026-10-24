use clap::Args;

#[derive(Args, Debug, Default)]
pub struct Selection {
    /// Only GPG keys
    #[arg(long, conflicts_with_all = ["ssh", "all"])]
    pub gpg: bool,
    /// Only SSH keys
    #[arg(long, conflicts_with_all = ["gpg", "all"])]
    pub ssh: bool,
    /// All available key categories (the default)
    #[arg(long, conflicts_with_all = ["gpg", "ssh"])]
    pub all: bool,
}
impl Selection {
    pub fn gpg(&self) -> bool {
        !self.ssh
    }
    pub fn ssh(&self) -> bool {
        !self.gpg
    }
}
