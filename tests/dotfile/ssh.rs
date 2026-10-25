//! Exercise the actual Git transport against a local upload-pack server.
use crate::common::*;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

const URL: &str = "git@stool-ssh.invalid:owner/dotfiles.git";

struct SshFixture {
    sandbox: Sandbox,
    bin: PathBuf,
    log: PathBuf,
    key: PathBuf,
    remote_map: BTreeMap<String, String>,
}
impl SshFixture {
    fn new(key_name: &str) -> Self {
        let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
        let bin = sandbox.root.join("SSH transport tools");
        fs::create_dir(&bin).unwrap();
        let log = sandbox.root.join("ssh.jsonl");
        let key = sandbox.home.join(".ssh").join(key_name);
        fs::create_dir_all(key.parent().unwrap()).unwrap();
        fs::write(
            &key,
            include_bytes!("../fixtures/offline-keys/test_ed25519"),
        )
        .unwrap();
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
        let python = String::from_utf8(
            Command::new("python3")
                .args(["-c", "import sys; print(sys.executable)"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let script = format!("#!{}\n{}", python.trim(), FAKE_SSH);
        let ssh = bin.join("ssh");
        fs::write(&ssh, script).unwrap();
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
        let remote_map = BTreeMap::from([(
            "owner/dotfiles.git".into(),
            sandbox.remote.to_str().unwrap().into(),
        )]);
        Self {
            sandbox,
            bin,
            log,
            key,
            remote_map,
        }
    }
    fn command(&self) -> Command {
        let path = std::env::join_paths(
            std::iter::once(self.bin.clone())
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let mut command = self.sandbox.command();
        command
            .env("PATH", path)
            .env_remove("GIT_SSH")
            .env_remove("GIT_SSH_COMMAND")
            .env_remove("GIT_SSH_VARIANT")
            .env("GIT_ALLOW_PROTOCOL", "file:ssh")
            .env("STOOL_TEST_SSH_LOG", &self.log)
            .env("STOOL_TEST_SSH_KEY", &self.key)
            .env("STOOL_TEST_REAL_SSH", real_tool("ssh"))
            .env("STOOL_TEST_REAL_GIT", real_tool("git"))
            .env(
                "STOOL_TEST_SSH_REMOTES",
                serde_json::to_string(&self.remote_map).unwrap(),
            )
            .arg("--source")
            .arg(&self.sandbox.source)
            .arg("--destination")
            .arg(&self.sandbox.destination);
        command
    }
    fn initialize(&self) -> Command {
        let mut command = self.command();
        command.args(["init", URL]);
        command
    }
    fn calls(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn saved_key(&self) -> Option<String> {
        let cfg: toml::Value = fs::read_to_string(self.sandbox.control().join("config.toml"))
            .unwrap()
            .parse()
            .unwrap();
        cfg.get("git")
            .and_then(|git| git.get("ssh_key"))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    }
    fn assert_no_source(&self) {
        assert!(!self.sandbox.source.exists());
        assert!(!self.sandbox.control().join("config.toml").exists());
        if let Some(parent) = self.sandbox.source.parent() {
            assert!(fs::read_dir(parent).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".stool-clone-")
            }));
        }
    }
}

fn real_tool(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .unwrap()
}

const FAKE_SSH: &str = r#"import json, os, pathlib, subprocess, sys
args = sys.argv[1:]
if '-G' in args:
    sys.exit(0)
config = None
if '-F' in args:
    config = args[args.index('-F') + 1]
# A command line -i is accepted too, to keep the fixture a transport rather than
# an assertion about the implementation's choice of temporary config syntax.
identities = [args[i + 1] for i, arg in enumerate(args[:-1]) if arg == '-i']
if config:
    # Let OpenSSH itself check Host matching, includes and quoting. -G never
    # contacts a server and does not read private key contents.
    effective = subprocess.run([os.environ['STOOL_TEST_REAL_SSH'], '-G', *args[:-1]],
                               capture_output=True, text=True)
    if effective.returncode:
        sys.stderr.write(effective.stderr)
        sys.exit(effective.returncode)
    identities.extend(line[len('identityfile '):] for line in effective.stdout.splitlines()
                      if line.startswith('identityfile '))
entry = {'args': args, 'identities': identities, 'config': config}
if config:
    entry['configuration'] = pathlib.Path(config).read_text()
with open(os.environ['STOOL_TEST_SSH_LOG'], 'a') as stream:
    stream.write(json.dumps(entry) + '\n')
error = os.environ.get('STOOL_TEST_SSH_ERROR')
if error:
    sys.stderr.write(error + '\n')
    sys.exit(255)
expected = os.environ['STOOL_TEST_SSH_KEY']
if os.environ.get('STOOL_TEST_SSH_LOCKED') == '1' and 'BatchMode=yes' in args:
    # A locked key cannot be unlocked by a BatchMode probe. The non-batch path
    # stands in for the user entering its original passphrase in OpenSSH's TTY.
    sys.stderr.write('git@stool-ssh.invalid: Permission denied (publickey).\n')
    sys.exit(255)
if not (expected in identities or (not identities and os.environ.get('STOOL_TEST_SSH_DEFAULT') == '1')):
    sys.stderr.write('git@stool-ssh.invalid: Permission denied (publickey).\n')
    sys.exit(255)
import shlex
# System Git combines the remote command into one argument; gix passes
# the upload-pack command and shell-quoted path as two arguments.
upload = next((i for i, arg in enumerate(args) if arg == 'git-upload-pack' or arg.startswith('git-upload-pack ')), len(args) - 1)
request = shlex.split(' '.join(args[upload:]))
if len(request) != 2 or request[0] != 'git-upload-pack':
    sys.stderr.write('Unexpected SSH fixture command: ' + repr(args) + '\n')
    sys.exit(2)
remote = json.loads(os.environ['STOOL_TEST_SSH_REMOTES'])[request[1].lstrip('/')]
os.execv(os.environ['STOOL_TEST_REAL_GIT'],
         [os.environ['STOOL_TEST_REAL_GIT'], 'upload-pack', remote])
"#;

#[test]
fn restored_nonstandard_key_is_verified_and_reused_for_update() {
    let f = SshFixture::new("arbitrary/restored identity");
    // This private key has the same syntax but is not authorized by the server.
    let other = f.sandbox.home.join(".ssh/aaa-other-account");
    fs::write(
        &other,
        include_bytes!("../fixtures/offline-keys/test_ed25519"),
    )
    .unwrap();
    fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
    success(f.initialize().arg("--apply").output().unwrap());
    assert_eq!(f.saved_key().as_deref(), f.key.to_str());
    assert_eq!(
        fs::read_to_string(f.sandbox.destination.join(".seed")).unwrap(),
        "seed"
    );
    let calls = f.calls();
    assert!(calls
        .iter()
        .any(|call| call["identities"].as_array().unwrap().is_empty()));
    assert!(calls.iter().any(|call| call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == other.to_str())));
    assert!(calls.iter().any(|call| call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == f.key.to_str())));
    let before = calls.len();
    write(&f.sandbox.work.join("dot_seed"), "updated");
    f.sandbox.push();
    success(f.command().arg("update").output().unwrap());
    assert_eq!(
        fs::read_to_string(f.sandbox.destination.join(".seed")).unwrap(),
        "updated"
    );
    assert!(f.calls()[before..].iter().all(|call| call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == f.key.to_str())));
    // Stool does not install a user-wide SSH configuration or touch the agent.
    assert!(!f.sandbox.home.join(".ssh/config").exists());
}

#[test]
fn working_existing_ssh_identity_takes_precedence_over_discovered_files() {
    let f = SshFixture::new("should-not-be-selected");
    success(
        f.initialize()
            .env("STOOL_TEST_SSH_DEFAULT", "1")
            .output()
            .unwrap(),
    );
    assert_eq!(f.saved_key(), None);
    assert!(f
        .calls()
        .iter()
        .all(|call| call["identities"].as_array().unwrap().is_empty()));
}

#[test]
fn explicit_key_with_shell_characters_is_used_without_command_execution() {
    let f = SshFixture::new("identity with 'quote' $(touch INJECTED) `touch INJECTED2`");
    success(
        f.initialize()
            .arg("--ssh-key")
            .arg(&f.key)
            .output()
            .unwrap(),
    );
    assert_eq!(f.saved_key().as_deref(), f.key.to_str());
    assert!(!f.sandbox.root.join("INJECTED").exists());
    assert!(!f.sandbox.root.join("INJECTED2").exists());
    assert!(f.calls().iter().all(|call| call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == f.key.to_str())));
}

#[test]
fn saved_global_identity_is_used_only_for_its_repository_url() {
    let f = SshFixture::new("global identity");
    write(
        &f.sandbox.home.join(".stool/config.toml"),
        &format!(
            "version = 1\n[dotfile]\nurl = {URL:?}\nssh_key = {:?}\n",
            f.key.to_str().unwrap()
        ),
    );
    success(f.command().arg("init").output().unwrap());
    assert_eq!(f.saved_key().as_deref(), f.key.to_str());
    assert!(f.calls().iter().all(|call| call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == f.key.to_str())));
}

#[test]
fn explicit_wrong_identity_preserves_existing_defaults_and_leaves_no_checkout() {
    let f = SshFixture::new("correct identity");
    let wrong = f.sandbox.home.join(".ssh/wrong");
    fs::write(
        &wrong,
        include_bytes!("../fixtures/offline-keys/test_ed25519"),
    )
    .unwrap();
    fs::set_permissions(&wrong, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!f
        .initialize()
        .arg("--ssh-key")
        .arg(&wrong)
        .output()
        .unwrap()
        .status
        .success());
    f.assert_no_source();
    assert!(!f.calls().is_empty());
    assert!(f.calls().iter().all(|call| !call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == f.key.to_str())));
}

#[test]
fn host_key_and_network_failures_do_not_try_other_private_keys() {
    for error in [
        "Host key verification failed.",
        "Could not resolve hostname stool-ssh.invalid",
    ] {
        let f = SshFixture::new("restored identity");
        failure(
            f.initialize()
                .env("STOOL_TEST_SSH_ERROR", error)
                .output()
                .unwrap(),
            error,
        );
        f.assert_no_source();
        assert!(f
            .calls()
            .iter()
            .all(|call| call["identities"].as_array().unwrap().is_empty()));
    }
}

#[test]
fn explicit_identity_also_works_with_builtin_git() {
    let f = SshFixture::new("builtin identity");
    success(
        f.initialize()
            .args(["--builtin", "--apply"])
            .arg("--ssh-key")
            .arg(&f.key)
            .output()
            .unwrap(),
    );
    assert_eq!(f.saved_key().as_deref(), f.key.to_str());
    assert_eq!(
        fs::read_to_string(f.sandbox.destination.join(".seed")).unwrap(),
        "seed"
    );
    write(&f.sandbox.work.join("dot_seed"), "builtin updated");
    f.sandbox.push();
    success(f.command().arg("update").output().unwrap());
    assert_eq!(
        fs::read_to_string(f.sandbox.destination.join(".seed")).unwrap(),
        "builtin updated"
    );
}

#[test]
fn recursive_submodules_use_the_saved_identity() {
    let mut f = SshFixture::new("recursive identity");
    let child = Sandbox::new(&[("content", "child content")]);
    git(
        &f.sandbox.work,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            child.remote.to_str().unwrap(),
            "module",
        ],
    );
    git(
        &f.sandbox.work,
        &[
            "config",
            "-f",
            ".gitmodules",
            "submodule.module.url",
            "git@stool-ssh.invalid:owner/child.git",
        ],
    );
    f.sandbox.push();
    f.remote_map.insert(
        "owner/child.git".into(),
        child.remote.to_str().unwrap().into(),
    );
    success(
        f.initialize()
            .arg("--ssh-key")
            .arg(&f.key)
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(f.sandbox.source.join("module/content")).unwrap(),
        "child content"
    );
    let before = f.calls().len();
    // Force init to exercise its existing-checkout recursive path as well.
    success(f.initialize().output().unwrap());
    assert!(f.calls()[before..].iter().all(|call| call["identities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key.as_str() == f.key.to_str())));
    write(&child.work.join("content"), "updated child");
    child.push();
    let module = f.sandbox.work.join("module");
    git(&module, &["fetch", "origin"]);
    git(&module, &["checkout", "--detach", "origin/dotfiles-trunk"]);
    f.sandbox.push();
    success(f.command().arg("update").output().unwrap());
    assert_eq!(
        fs::read_to_string(f.sandbox.source.join("module/content")).unwrap(),
        "updated child"
    );
    assert!(f.calls()[before..]
        .iter()
        .any(|call| call["args"].as_array().unwrap().iter().any(|arg| arg
            .as_str()
            .is_some_and(|arg| arg.contains("owner/child.git")))));
}

#[test]
fn unrelated_default_url_does_not_apply_its_key_to_an_explicit_repository() {
    let f = SshFixture::new("target repository identity");
    let missing = f.sandbox.home.join("missing-unrelated-key");
    write(&f.sandbox.home.join(".stool/config.toml"), &format!("version = 1\n[dotfile]\nurl = 'git@other.invalid:unrelated/repo.git'\nssh_key = {:?}\n", missing.to_str().unwrap()));
    success(f.initialize().output().unwrap());
    assert_eq!(f.saved_key().as_deref(), f.key.to_str());
    assert!(f
        .calls()
        .iter()
        .any(|call| call["identities"].as_array().unwrap().is_empty()));
}

#[test]
fn selected_identity_is_host_scoped_and_existing_ssh_configuration_is_preserved() {
    let f = SshFixture::new("private repository identity");
    let other = f.sandbox.home.join(".ssh/other-host-identity");
    fs::write(
        &other,
        include_bytes!("../fixtures/offline-keys/test_ed25519"),
    )
    .unwrap();
    fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
    let user_configuration = format!(
        "Host other.invalid\n    IdentityFile \"{}\"\n",
        other.display()
    );
    let user_config = f.sandbox.home.join(".ssh/config");
    write(&user_config, &user_configuration);
    success(
        f.initialize()
            .arg("--ssh-key")
            .arg(&f.key)
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(&user_config).unwrap(),
        user_configuration
    );
    let temporary_config = f
        .calls()
        .into_iter()
        .find_map(|call| call["configuration"].as_str().map(str::to_owned))
        .unwrap();
    assert!(!temporary_config
        .to_lowercase()
        .contains("stricthostkeychecking no"));
    let replay = f.sandbox.root.join("captured-ssh-config");
    write(&replay, &temporary_config);
    let effective = success(
        Command::new(real_tool("ssh"))
            .args(["-G", "-F"])
            .arg(&replay)
            .arg("git@other.invalid")
            .output()
            .unwrap(),
    );
    let identities: Vec<&str> = effective
        .lines()
        .filter_map(|line| line.strip_prefix("identityfile "))
        .collect();
    assert!(identities.contains(&other.to_str().unwrap()), "{effective}");
    assert!(
        !identities.contains(&f.key.to_str().unwrap()),
        "{effective}"
    );
}

#[test]
fn locked_identities_require_explicit_selection_without_a_terminal() {
    let f = SshFixture::new("locked identity");
    let output = f
        .initialize()
        .env("STOOL_TEST_SSH_LOCKED", "1")
        .output()
        .unwrap();
    failure(output, "--ssh-key");
    f.assert_no_source();
    assert!(f.calls().iter().any(|call| call["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "BatchMode=yes")));
    let before = f.calls().len();
    success(
        f.initialize()
            .env("STOOL_TEST_SSH_LOCKED", "1")
            .arg("--ssh-key")
            .arg(&f.key)
            .output()
            .unwrap(),
    );
    assert_eq!(f.saved_key().as_deref(), f.key.to_str());
    assert!(f.calls()[before..].iter().all(|call| !call["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "BatchMode=yes")));
}
