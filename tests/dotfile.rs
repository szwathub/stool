mod common;

#[path = "dotfile/advanced.rs"]
mod advanced;
#[path = "dotfile/cli.rs"]
mod cli;
#[path = "dotfile/crypto_hooks.rs"]
mod crypto_hooks;
#[path = "dotfile/passphrase.rs"]
mod passphrase;
#[path = "dotfile/prerequisites.rs"]
mod prerequisites;
#[path = "dotfile/quoting.rs"]
mod quoting;

#[cfg(unix)]
#[path = "dotfile/ssh.rs"]
mod ssh;
