use super::*;

#[test]
fn installation_guides_include_platform_commands_and_retry() {
    for (os, command) in [
        ("macos", "xcode-select --install"),
        ("windows", "winget install --id Git.Git"),
        ("linux", "sudo apt-get install git"),
        ("freebsd", "https://git-scm.com/install/"),
    ] {
        let guide = git_installation_guide(os);
        assert!(guide.contains(command));
        assert!(guide.contains("git --version"));
        assert!(guide.contains("rerun the same stool dotfile init command"));
    }
}
