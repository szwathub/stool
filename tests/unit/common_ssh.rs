use super::*;

const PRIVATE: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nsynthetic test header\n";

#[test]
fn ssh_hosts_are_parsed_without_treating_https_as_ssh() {
    assert_eq!(
        host("git@github.com:owner/repo.git").unwrap().as_deref(),
        Some("github.com")
    );
    assert_eq!(
        host("ssh://git@company.example:2222/repo.git")
            .unwrap()
            .as_deref(),
        Some("company.example")
    );
    assert_eq!(
        host("ssh://git@[::1]:2222/repo.git").unwrap().as_deref(),
        Some("::1")
    );
    assert_eq!(host("https://github.com/owner/repo.git").unwrap(), None);
    assert!(host("ssh://git@host*/repo.git").is_err());
}

#[test]
fn private_identity_paths_are_expanded_and_public_files_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(root.join(".ssh")).unwrap();
    let key = root.join(".ssh/private key");
    fs::write(&key, PRIVATE).unwrap();
    assert_eq!(
        identity_path(&root, Path::new("~/.ssh/private key")).unwrap(),
        key
    );
    let public = root.join(".ssh/private key.pub");
    fs::write(&public, "ssh-ed25519 test").unwrap();
    assert!(identity_path(&root, &public).is_err());
    assert!(identity_path(&root, Path::new("missing")).is_err());
}

#[test]
fn ssh_config_escapes_percent_tokens_and_rejects_injected_lines() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let key = root.join("key 100% $(literal) ' name");
    fs::write(&key, PRIVATE).unwrap();
    let identity = Identity::new(&root, "git@github.com:owner/repo", &key)
        .unwrap()
        .unwrap();
    let session = identity.session(true).unwrap();
    let words = shell_words::split(&session.command).unwrap();
    let config = fs::read_to_string(&words[2]).unwrap();
    assert!(config.contains("100%% $(literal) ' name"));
    assert!(config.starts_with("Host github.com\n  IdentitiesOnly yes\n"));
    assert!(config_path(Path::new("bad\nIdentityFile injected")).is_err());
    assert!(config_path(Path::new("${INJECTED}/key")).is_err());
    let path = PathBuf::from(&words[2]);
    drop(session);
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn discovery_finds_nested_private_keys_without_following_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir_all(root.join(".ssh/work")).unwrap();
    fs::write(root.join(".ssh/work/private"), PRIVATE).unwrap();
    fs::write(root.join(".ssh/config"), "Host example\n").unwrap();
    fs::write(root.join("outside"), PRIVATE).unwrap();
    std::os::unix::fs::symlink(root.join("outside"), root.join(".ssh/link")).unwrap();
    assert_eq!(
        candidates(&root).unwrap(),
        vec![root.join(".ssh/work/private")]
    );
}

#[cfg(windows)]
#[test]
fn git_for_windows_ssh_arguments_simplify_verbatim_drive_paths() {
    assert_eq!(
        command_path(Path::new(r"\\?\C:\Users\Sun\.ssh\private key")).unwrap(),
        "C:/Users/Sun/.ssh/private key"
    );
}
