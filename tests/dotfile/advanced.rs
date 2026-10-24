use crate::common::*;
use std::io::Write;

#[test]
fn template_data_initial_config_include_and_overrides() {
    let s = Sandbox::new(&[
        (".stooldata.toml", "name='repo'\n[settings]\ncolor='blue'\n"),
        (".stooldata/nested.json", r#"{"nested":{"answer":42}}"#),
        (
            ".stool.toml.tmpl",
            "[data]\nname='{{ prompt_string(\"Name\", \"local\") }}'\n",
        ),
        (
            ".stooltemplates/shared",
            "{{ name }}:{{ settings.color }}:{{ nested.answer }}",
        ),
        (
            "dot_profile.tmpl",
            "{% include 'shared' %}:{{ stool.os }}\n",
        ),
        (
            ".stoolignore.tmpl",
            "{% if stool.os != 'unknown' %}skip{% endif %}\n",
        ),
        ("skip", "skip"),
    ]);
    success(s.init(&["--apply"]));
    let actual = fs::read_to_string(s.destination.join(".profile")).unwrap();
    assert!(actual.starts_with("local:blue:42:"), "{actual}");
    assert!(!s.destination.join("skip").exists());
    let before = snapshot(&s.root);
    assert!(success(s.cli(&["--data", "name=override", "diff"])).contains("override:blue:42"));
    assert_eq!(before, snapshot(&s.root));
    let data: serde_json::Value = serde_json::from_str(&success(s.cli(&["data"]))).unwrap();
    assert_eq!(data["name"], "local");
    assert_eq!(
        success(s.cli(&["execute-template", "{{ nested.answer + 1 }}"])),
        "43"
    );
    success(s.cli(&["verify"]));
}

#[test]
fn template_directives_and_source_root_version() {
    let s = Sandbox::new(&[
        (".stoolroot", "payload\n"),
        (".stoolversion", ">=0.2.0\n"),
        ("payload/dot_config.tmpl", "# stool:template: left-delimiter='[[' right-delimiter=']]' line-ending=crlf encoding=utf-16le\n[[ stool.version ]]\n"),
        ("outside", "not deployed"),
    ]);
    success(s.init(&["--apply"]));
    let bytes = fs::read(s.destination.join(".config")).unwrap();
    let text = String::from_utf16(
        &bytes
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(text, format!("{}\r\n", env!("CARGO_PKG_VERSION")));
    assert!(!s.destination.join("outside").exists());
    write(&s.source.join(".stoolversion"), ">9.0.0");
    failure(s.cli(&["diff"]), "version");
    write(&s.source.join(".stoolversion"), "0.2.0");
    write(&s.source.join(".stoolroot"), "../escape");
    failure(s.cli(&["diff"]), "inside");
}

#[test]
fn add_recursive_literals_and_unmanaged() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    success(s.init(&["--apply"]));
    write(
        &s.destination.join(".config/app/dot_literal.tmpl"),
        "unchanged\n",
    );
    write(&s.destination.join(".config/app/empty"), "");
    success(s.cli(&["add", ".config"]));
    assert!(success(s.cli(&["managed"])).contains("dot_literal.tmpl"));
    success(s.cli(&["verify"]));
    write(&s.destination.join("other"), "unmanaged");
    assert!(success(s.cli(&["unmanaged"])).contains("other"));
    // A literal suffix must survive when the imported file itself is a template.
    write(&s.destination.join("named.tmpl"), "{{ stool.version }}");
    success(s.cli(&["add", "--template", "named.tmpl"]));
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("named.tmpl")).unwrap(),
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(unix)]
fn executable(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    write(path, text);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn age_encrypted_add_edit_and_roundtrip() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let identity = s.home.join("identity.txt");
    let recipient = success(
        s.command()
            .args(["keygen", "--output"])
            .arg(&identity)
            .output()
            .unwrap(),
    )
    .trim()
    .to_owned();
    write(
        &s.work.join(".stool.toml.tmpl"),
        &format!(
            "[encryption]\nrecipients=['{recipient}']\nidentities=['{}']\n",
            identity.display()
        ),
    );
    s.push();
    success(s.init(&["--apply"]));
    write(&s.destination.join(".secret"), "plaintext secret\n");
    success(s.cli(&["add", "--encrypt", ".secret"]));
    let encrypted = fs::read(s.source.join("encrypted_private_dot_secret.age"))
        .or_else(|_| fs::read(s.source.join("encrypted_dot_secret.age")))
        .unwrap();
    assert!(!String::from_utf8_lossy(&encrypted).contains("plaintext secret"));
    fs::remove_file(s.destination.join(".secret")).unwrap();
    success(s.cli(&["apply", "--force"]));
    assert_eq!(
        fs::read_to_string(s.destination.join(".secret")).unwrap(),
        "plaintext secret\n"
    );
    let editor = s.root.join("editor");
    executable(&editor, "#!/bin/sh\nprintf 'edited secret\\n' > \"$1\"\n");
    success(
        s.command()
            .env("EDITOR", &editor)
            .args(["edit", "--apply", ".secret"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(s.destination.join(".secret")).unwrap(),
        "edited secret\n"
    );
    fs::remove_file(identity).unwrap();
    assert!(success(s.cli(&["managed"])).contains(".secret"));
    failure(s.cli(&["apply"]), "Decrypt");
}

#[cfg(unix)]
#[test]
fn scripts_phases_once_onchange_preview_and_failures() {
    let s=Sandbox::new(&[
        ("run_before_00.sh", "#!/bin/sh\ntest ! -e \"$STOOL_DEST_DIR/.seed\" || true\nprintf 'before\\n' >> \"$STOOL_DEST_DIR/log\"\n"),
        ("dot_seed","seed"),
        ("run_once_after_once.sh","#!/bin/sh\nprintf 'once\\n' >> \"$STOOL_DEST_DIR/log\"\n"),
        ("run_onchange_after_change.sh","#!/bin/sh\nprintf 'change-A\\n' >> \"$STOOL_DEST_DIR/log\"\n"),
        (".stoolscripts/after_extra.sh","#!/bin/sh\nprintf 'extra\\n' >> \"$STOOL_DEST_DIR/log\"\n"),
    ]);
    success(s.init(&[]));
    let before = snapshot(&s.root);
    success(s.cli(&["diff"]));
    assert_eq!(before, snapshot(&s.root));
    success(s.cli(&["apply"]));
    success(s.cli(&["apply"]));
    let log = fs::read_to_string(s.destination.join("log")).unwrap();
    assert_eq!(log.matches("once\n").count(), 1);
    assert_eq!(log.matches("change-A\n").count(), 1);
    assert_eq!(log.matches("before\n").count(), 2);
    assert_eq!(log.matches("extra\n").count(), 2);
    let script = s.source.join("run_onchange_after_change.sh");
    let original = fs::read_to_string(&script).unwrap();
    write(&script, &original.replace("change-A", "change-B"));
    success(s.cli(&["apply"]));
    write(&script, &original);
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("log"))
            .unwrap()
            .matches("change-A\n")
            .count(),
        2
    );
    write(&s.source.join("run_once_after_retry.sh"),"#!/bin/sh\nset -e\ntest -e \"$STOOL_DEST_DIR/allow\"\nprintf 'retry\\n' >> \"$STOOL_DEST_DIR/log\"\n");
    failure(s.cli(&["apply"]), "failed");
    write(&s.destination.join("allow"), "yes");
    success(s.cli(&["apply"]));
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("log"))
            .unwrap()
            .matches("retry\n")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn create_modify_remove_exact_readonly_and_backups() {
    use std::os::unix::fs::PermissionsExt;
    let s = Sandbox::new(&[
        ("create_keep", "new default"),
        ("modify_transform", "#!/bin/sh\nsed 's/old/new/g'\n"),
        ("remove_oldfile", ""),
        ("exact_dot_config/keep", "managed"),
        ("readonly_readonlydir/file", "first"),
        (".stoolremove.tmpl", "{{ 'obsolete' }}\n"),
    ]);
    write(&s.destination.join("keep"), "existing");
    write(&s.destination.join("transform"), "old\n");
    write(&s.destination.join("oldfile"), "remove");
    write(&s.destination.join("obsolete/nested"), "remove recursively");
    write(&s.destination.join(".config/extra"), "backup");
    success(s.init(&[]));
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("keep")).unwrap(),
        "existing"
    );
    assert_eq!(
        fs::read_to_string(s.destination.join("transform")).unwrap(),
        "new\n"
    );
    assert!(!s.destination.join("oldfile").exists());
    assert!(!s.destination.join("obsolete").exists());
    assert!(!s.destination.join(".config/extra").exists());
    assert_eq!(
        fs::metadata(s.destination.join("readonlydir"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o555
    );
    write(&s.source.join("readonly_readonlydir/file"), "second");
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("readonlydir/file")).unwrap(),
        "second"
    );
    success(s.cli(&["verify"]));
    assert!(s.control().join("backups").exists());
    fs::set_permissions(
        s.destination.join("readonlydir"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

#[test]
fn archive_all_formats_are_rendered_without_deploying() {
    let s = Sandbox::new(&[
        ("dot_config/app.tmpl", "{{ stool.version }}"),
        ("empty_empty", ""),
    ]);
    success(s.init(&[]));
    for format in ["tar", "tgz", "zip"] {
        let output = s.root.join(format!("result.{format}"));
        success(
            s.command()
                .args(["archive", "--format", format, "--output"])
                .arg(&output)
                .output()
                .unwrap(),
        );
        let data = fs::read(&output).unwrap();
        if format == "zip" {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).unwrap();
            use std::io::Read;
            let mut text = String::new();
            zip.by_name(".config/app")
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            assert_eq!(text, env!("CARGO_PKG_VERSION"));
        } else {
            let reader: Box<dyn std::io::Read> = if format == "tgz" {
                Box::new(flate2::read::GzDecoder::new(&data[..]))
            } else {
                Box::new(&data[..])
            };
            let mut archive = tar::Archive::new(reader);
            let mut found = false;
            for item in archive.entries().unwrap() {
                let mut entry = item.unwrap();
                if entry.path().unwrap() == Path::new(".config/app") {
                    use std::io::Read;
                    let mut text = String::new();
                    entry.read_to_string(&mut text).unwrap();
                    assert_eq!(text, env!("CARGO_PKG_VERSION"));
                    found = true;
                }
            }
            assert!(found);
        }
    }
    assert!(!s.destination.exists());
}

#[test]
fn external_cache_checksum_archives_and_path_guards() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let payload = s.root.join("payload");
    write(&payload, "download v1");
    let zip_path = s.root.join("payload.zip");
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        for (name, text) in [("package/keep", "keep"), ("package/skip", "skip")] {
            zip.start_file(name, zip::write::FileOptions::default())
                .unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    write(&s.work.join(".stoolexternal.toml"),&format!("[download]\nurl='{}'\nrefreshPeriod='24h'\n[tools]\ntype='archive'\nurl='{}'\nstripComponents=1\nexclude=['skip']\nexact=true\n",file_url(&payload),file_url(&zip_path)));
    s.push();
    success(s.init(&[]));
    let before = snapshot(&s.root);
    success(s.cli(&["diff"]));
    assert_eq!(before, snapshot(&s.root));
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("download")).unwrap(),
        "download v1"
    );
    assert!(s.destination.join("tools/keep").is_file());
    assert!(!s.destination.join("tools/skip").exists());
    write(&payload, "download v2");
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("download")).unwrap(),
        "download v1"
    );
    write(
        &s.source.join(".stoolexternal.toml"),
        &format!(
            "[download]\nurl='{}'\nrefreshPeriod='0s'\n[download.checksum]\nsha256='bad'\n",
            file_url(&payload)
        ),
    );
    failure(s.cli(&["apply"]), "checksum mismatch");
    assert_eq!(
        fs::read_to_string(s.destination.join("download")).unwrap(),
        "download v1"
    );
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        zip.start_file("../escape", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(b"evil").unwrap();
        zip.finish().unwrap();
    }
    write(
        &s.source.join(".stoolexternal.toml"),
        &format!(
            "[tools]\ntype='archive'\nurl='{}'\nrefreshPeriod='0s'\n",
            file_url(&zip_path)
        ),
    );
    failure(s.cli(&["apply"]), "Archive");
    assert!(!s.root.join("escape").exists());
}

#[cfg(unix)]
#[test]
fn secret_process_cache_and_errors_do_not_expose_values() {
    let s = Sandbox::new(&[
        ("a.tmpl", "{{ pass('item') }}"),
        ("b.tmpl", "{{ pass('item') }}"),
    ]);
    let bin = s.root.join("bin");
    let counter = s.root.join("calls");
    executable(
        &bin.join("pass"),
        "#!/bin/sh\necho call >> \"$COUNTER\"\nprintf secret-value\n",
    );
    success(s.init(&[]));
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    success(
        s.command()
            .env("PATH", &path)
            .env("COUNTER", &counter)
            .arg("apply")
            .output()
            .unwrap(),
    );
    assert_eq!(fs::read_to_string(counter).unwrap().lines().count(), 1);
    assert_eq!(
        fs::read_to_string(s.destination.join("a")).unwrap(),
        "secret-value"
    );
    executable(
        &s.root.join("bin/pass"),
        "#!/bin/sh\necho secret-value >&2\nexit 1\n",
    );
    let output = s.command().env("PATH", path).arg("apply").output().unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-value"));
}

#[test]
fn doctor_cd_prompt_and_runtime_documentation() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    success(s.init(&[]));
    assert!(success(s.cli(&["cd", "--print"])).contains(s.source.to_str().unwrap()));
    success(s.cli(&["doctor"]));
    write(&s.destination.join(".seed"), "conflict");
    failure(s.cli(&["apply", "--prompt", "always"]), "terminal");
    failure(s.cli(&["verify"]), "verification failed");
    let completion = success(
        Command::new(env!("CARGO_BIN_EXE_stool"))
            .args(["completions", "bash"])
            .output()
            .unwrap(),
    );
    assert!(completion.contains("execute-template"));
    for name in ["add", "edit", "keygen"] {
        assert!(completion.contains(&format!("stool__dotfile__{name}")));
    }
    for name in ["encrypt", "decrypt", "migrate", "gpg"] {
        assert!(!completion.contains(&format!("stool__dotfile__{name}")));
    }
    let help = success(s.cli(&["--help"]));
    let commands = help
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    for name in ["add", "edit", "keygen"] {
        assert!(commands.contains(&name));
    }
    for name in ["encrypt", "decrypt", "migrate", "gpg"] {
        assert!(!commands.contains(&name));
    }
    let pages = s.root.join("man");
    success(
        Command::new(env!("CARGO_BIN_EXE_stool"))
            .args(["man", "--output"])
            .arg(&pages)
            .output()
            .unwrap(),
    );
    for name in ["add", "edit", "keygen"] {
        assert!(pages.join(format!("stool-dotfile-{name}.1")).is_file());
    }
    for name in ["encrypt", "decrypt", "migrate", "gpg"] {
        assert!(!pages.join(format!("stool-dotfile-{name}.1")).exists());
    }
}

#[test]
fn removed_commands_are_rejected_before_creating_dotfile_directories() {
    let sandbox = Sandbox::new(&[]);
    let before = snapshot(&sandbox.root);
    for name in ["encrypt", "decrypt", "migrate", "gpg"] {
        failure(
            sandbox.cli(&[name, "missing-input"]),
            "unrecognized subcommand",
        );
        assert!(!sandbox.source.exists());
        assert!(!sandbox.destination.exists());
        assert!(!sandbox.control().exists());
        assert_eq!(snapshot(&sandbox.root), before);
    }
}

#[test]
fn builtin_git_clone_branch_fast_forward_and_preserved_metadata() {
    let s = Sandbox::new(&[("dot_seed", "first")]);
    git(&s.work, &["branch", "alternate"]);
    git(&s.work, &["push", "origin", "alternate"]);
    success(s.init(&["--builtin", "--branch", "alternate", "--apply"]));
    assert!(s.source.join(".git/stool-builtin").is_file());
    git(&s.source, &["config", "stool.custom", "preserved"]);
    write(&s.source.join(".git/hooks/local-hook"), "keep this hook");
    git(&s.source, &["branch", "local-reference"]);
    git(&s.work, &["checkout", "alternate"]);
    write(&s.work.join("dot_seed"), "second");
    s.push();
    success(s.cli(&["update"]));
    assert_eq!(
        fs::read_to_string(s.destination.join(".seed")).unwrap(),
        "second"
    );
    assert_eq!(git(&s.source, &["config", "stool.custom"]), "preserved\n");
    assert_eq!(
        fs::read_to_string(s.source.join(".git/hooks/local-hook")).unwrap(),
        "keep this hook"
    );
    assert!(!git(&s.source, &["rev-parse", "local-reference"]).is_empty());
    success(s.cli(&["verify"]));
    write(&s.source.join("untracked"), "preserve");
    let before = snapshot(&s.root);
    failure(s.cli(&["update"]), "untracked");
    assert_eq!(before, snapshot(&s.root));
    fs::remove_file(s.source.join("untracked")).unwrap();
    write(&s.source.join("dot_seed"), "dirty");
    failure(s.cli(&["update"]), "local changes");
    git(&s.source, &["checkout", "--", "dot_seed"]);
    git(&s.source, &["config", "user.name", "Local"]);
    git(
        &s.source,
        &["config", "user.email", "local@example.invalid"],
    );
    write(&s.source.join("local"), "local commit");
    git(&s.source, &["add", "local"]);
    git(
        &s.source,
        &["-c", "commit.gpgsign=false", "commit", "-m", "local"],
    );
    failure(s.cli(&["update"]), "fast-forward");
}

#[cfg(unix)]
#[test]
fn lfs_and_gpg_external_tools_are_exercised_without_real_credentials() {
    let s = Sandbox::new(&[
        (
            "dot_lfs",
            "version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 7\n",
        ),
        (
            ".stool.toml.tmpl",
            "[git]\nlfs=true\n[encryption]\nbackend='gpg'\ngpg_recipient='test'\n",
        ),
    ]);
    let bin = s.root.join("bin");
    executable(&bin.join("git-lfs"),"#!/bin/sh\ncase \"$1\" in version) echo git-lfs-test;; pull) printf checked > dot_lfs;; *) exit 1;; esac\n");
    executable(&bin.join("gpg"),"#!/bin/sh\ncase \" $* \" in *' --encrypt '*) printf 'GPG:';cat;; *' --decrypt '*) sed 's/^GPG://' ;; *) exit 1;; esac\n");
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    success(
        s.command()
            .env("PATH", &path)
            .arg("--source")
            .arg(&s.source)
            .arg("--destination")
            .arg(&s.destination)
            .args(["init", &s.url(), "--apply"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(s.destination.join(".lfs")).unwrap(),
        "checked"
    );
    write(&s.destination.join(".secret"), "secret");
    success(
        s.command()
            .env("PATH", &path)
            .args(["add", "--encrypt", ".secret"])
            .output()
            .unwrap(),
    );
    let cipher = s.source.join("encrypted_dot_secret.asc");
    assert_eq!(fs::read_to_string(&cipher).unwrap(), "GPG:secret");
    fs::remove_file(s.destination.join(".secret")).unwrap();
    success(
        s.command()
            .env("PATH", &path)
            .args(["apply", "--force"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(s.destination.join(".secret")).unwrap(),
        "secret"
    );
    let editor = s.root.join("gpg-editor");
    executable(
        &editor,
        "#!/bin/sh\nprintf 'edited GPG secret\\n' > \"$1\"\n",
    );
    success(
        s.command()
            .env("PATH", &path)
            .env("EDITOR", &editor)
            .args(["edit", "--apply", ".secret"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read_to_string(&cipher).unwrap(),
        "GPG:edited GPG secret\n"
    );
    assert_eq!(
        fs::read_to_string(s.destination.join(".secret")).unwrap(),
        "edited GPG secret\n"
    );
}

#[cfg(unix)]
#[test]
fn secret_adapters_and_bitwarden_unlock_are_cached_per_process() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    success(s.init(&[]));
    let bin = s.root.join("bin");
    let log = s.root.join("provider-log");
    for name in [
        "op",
        "gopass",
        "vault",
        "lpass",
        "doppler",
        "aws",
        "az",
        "security",
        "secret-tool",
        "custom",
    ] {
        executable(&bin.join(name),"#!/bin/sh\nprintf '%s %s\\n' \"${0##*/}\" \"$*\" >> \"$PROVIDER_LOG\"\nprintf '{\"value\":\"ok\"}'\n");
    }
    executable(&bin.join("keepassxc-cli"),"#!/bin/sh\nread password\ntest \"$password\" = test-password || exit 1\nprintf '{\"value\":\"ok\"}'\n");
    executable(&bin.join("bw"),"#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$PROVIDER_LOG\"\ncase \"$1\" in status) printf '{\"status\":\"locked\"}';; unlock) test \"$BW_PASSWORD\" = test-password || exit 1;printf test-session;; get) test \"$BW_SESSION\" = test-session || exit 1;printf '{\"value\":\"ok\"}';; *) exit 1;; esac\n");
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let template="{{ onepassword('op://item').value }}{{ gopass('item').value }}{{ vault('item').value }}{{ lastpass('item').value }}{{ doppler('item').value }}{{ aws('item').value }}{{ azure('item').value }}{{ keyring('service','account').value }}{{ keepassxc('db','entry').value }}{{ secret('command','custom').value }}{{ bitwarden('one').value }}{{ bitwarden('two').value }}{{ bitwarden('one').value }}";
    let out = success(
        s.command()
            .env("PATH", path)
            .env("PROVIDER_LOG", &log)
            .env("BW_PASSWORD", "test-password")
            .env_remove("BW_SESSION")
            .env("STOOL_KEEPASS_PASSWORD", "test-password")
            .args(["execute-template", template])
            .output()
            .unwrap(),
    );
    assert_eq!(out, "ok".repeat(13));
    let calls = fs::read_to_string(log).unwrap();
    assert_eq!(calls.lines().filter(|s| *s == "status").count(), 1);
    assert_eq!(calls.lines().filter(|s| *s == "unlock").count(), 1);
    assert_eq!(calls.lines().filter(|s| *s == "get").count(), 2);
}

#[cfg(unix)]
#[test]
fn scripts_are_ordered_and_before_failure_preserves_targets_and_retries() {
    let s=Sandbox::new(&[
        ("run_once_before_bootstrap.sh","#!/bin/sh\nset -e\ntest -e \"$STOOL_HOME/allow\"\nprintf bootstrap >> \"$STOOL_HOME/log\"\n"),
        ("a","first"),
        ("run_b.sh","#!/bin/sh\nset -e\ntest -e \"$STOOL_DEST_DIR/a\"\ntest ! -e \"$STOOL_DEST_DIR/z\"\n"),
        ("z","last"),
        ("run_once_after_done.sh","#!/bin/sh\nset -e\ntest -e \"$STOOL_DEST_DIR/z\"\n"),
    ]);
    success(s.init(&[]));
    failure(s.cli(&["apply"]), "failed");
    assert!(!s.destination.join("a").exists());
    write(&s.home.join("allow"), "yes");
    success(s.cli(&["apply"]));
    assert_eq!(fs::read_to_string(s.home.join("log")).unwrap(), "bootstrap");
    fs::remove_file(s.source.join("run_b.sh")).unwrap();
    success(s.cli(&["apply"]));
    assert_eq!(fs::read_to_string(s.home.join("log")).unwrap(), "bootstrap");
}

#[test]
fn removal_empty_guard_and_protected_removal_patterns() {
    let s = Sandbox::new(&[("remove_old", ""), ("new", "must not be written")]);
    write(&s.destination.join("old/keep"), "preserve");
    success(s.init(&[]));
    failure(s.cli(&["apply", "--force"]), "not empty");
    assert!(!s.destination.join("new").exists());
    fs::remove_file(s.destination.join("old/keep")).unwrap();
    success(s.cli(&["apply"]));
    assert!(!s.destination.join("old").exists());
    write(&s.source.join(".stoolremove"), "../escape");
    failure(s.cli(&["apply", "--force"]), "inside destination");
    write(&s.source.join(".stoolremove"), ".stool");
    failure(
        s.command()
            .arg("--destination")
            .arg(&s.home)
            .arg("apply")
            .arg("--force")
            .output()
            .unwrap(),
        "control storage",
    );
}

#[test]
fn external_git_and_archive_file_and_symlink_ancestry() {
    let resource = Sandbox::new(&[("literal_file", "raw filename"), (".hidden", "hidden")]);
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    let archive = s.root.join("resource.tar");
    {
        let mut tar = tar::Builder::new(fs::File::create(&archive).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "tool", &b"tool"[..]).unwrap();
        tar.finish().unwrap();
    }
    write(&s.work.join(".stoolexternal.toml"),&format!("[vendor]\ntype='git-repo'\nurl='{}'\n[tool]\ntype='archive-file'\nurl='{}'\npath='tool'\n",resource.url(),file_url(&archive)));
    s.push();
    success(s.init(&["--apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("vendor/literal_file")).unwrap(),
        "raw filename"
    );
    assert!(s.destination.join("vendor/.hidden").is_file());
    assert!(!s.destination.join("vendor/.git").exists());
    assert_eq!(
        fs::read_to_string(s.destination.join("tool")).unwrap(),
        "tool"
    );
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.add_symlink("redirect", "../outside", zip::write::FileOptions::default())
            .unwrap();
        zip.start_file("redirect/file", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
    }
    write(
        &s.source.join(".stoolexternal.toml"),
        &format!(
            "[attack]\ntype='archive'\nurl='{}'\nrefreshPeriod='0s'\n",
            file_url(&archive)
        ),
    );
    failure(s.cli(&["apply"]), "traverses a file or symlink");
    assert!(!s.destination.join("attack").exists());
}

#[test]
fn external_http_fetch_refresh_and_failed_download_preserve_deployment() {
    use std::io::Read;
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/payload", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for body in [Some("one"), Some("two"), None] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            assert!(stream.read(&mut buffer).unwrap() > 0);
            match body {
                Some(body) => write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap(),
                None => write!(
                    stream,
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap(),
            };
        }
    });
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    write(
        &s.work.join(".stoolexternal.toml"),
        &format!("[download]\nurl='{url}'\nrefreshPeriod='0s'\n"),
    );
    s.push();
    success(s.init(&[]));
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("download")).unwrap(),
        "one"
    );
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("download")).unwrap(),
        "two"
    );
    failure(s.cli(&["apply"]), "HTTP download failed");
    assert_eq!(
        fs::read_to_string(s.destination.join("download")).unwrap(),
        "two"
    );
    server.join().unwrap();
}

#[test]
fn add_dot_literal_prefix_and_template_initial_prompt_boundaries() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    success(s.init(&["--apply"]));
    for name in [
        ".private_config",
        ".dot_file",
        "literal_name",
        "file.literal",
        ".tmpl",
    ] {
        write(&s.destination.join(name), "literal");
        success(s.cli(&["add", name]));
    }
    success(s.cli(&["verify"]));
    let result=success(s.cli(&["execute-template","--init","{{ prompt_bool('ok', true) }}:{{ prompt_int('number', 3) }}:{{ prompt_choice('choice', ['a','b'], 'b') }}"]));
    assert_eq!(result, "true:3:b");
    failure(
        s.cli(&[
            "execute-template",
            "--init",
            "{{ prompt_string('required') }}",
        ]),
        "default",
    );
    failure(
        s.cli(&["execute-template", "{{ prompt_string('forbidden', 'x') }}"]),
        "Render template",
    );
    failure(s.cli(&["--data", "stool.os=other", "data"]), "reserved");
}

#[cfg(unix)]
#[test]
fn real_terminal_conflict_confirmation_cancels_or_backs_up_and_applies() {
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    fn terminal(s: &Sandbox, key: u8) -> Output {
        let (mut master, mut slave) = (0, 0);
        let mut size = libc::winsize {
            ws_row: 24,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // Each child has its own controlling terminal so this also exercises raw
        // mode setup, crossterm input, rendering and restoration on exit.
        unsafe {
            assert_eq!(
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(size)
                ),
                0
            );
        }
        let mut master = unsafe { fs::File::from_raw_fd(master) };
        let slave = unsafe { fs::File::from_raw_fd(slave) };
        unsafe {
            assert!(libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) >= 0);
        }
        let mut command = s.command();
        command
            .env("TERM", "xterm-256color")
            .args(["apply", "--prompt", "always"])
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        drop(slave);
        let mut output = Vec::new();
        let mut sent = false;
        let start = std::time::Instant::now();
        let status = loop {
            let mut bytes = [0; 8192];
            match master.read(&mut bytes) {
                Ok(n) => output.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.raw_os_error() == Some(libc::EIO) => {}
                Err(error) => panic!("Read terminal: {error}"),
            };
            if !sent && String::from_utf8_lossy(&output).contains("overwrite") {
                master.write_all(&[key]).unwrap();
                sent = true;
            }
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > std::time::Duration::from_secs(15) {
                child.kill().unwrap();
                panic!(
                    "Terminal prompt timed out: {}",
                    String::from_utf8_lossy(&output)
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(
            sent,
            "prompt was not rendered: {}",
            String::from_utf8_lossy(&output)
        );
        Output {
            status,
            stdout: Vec::new(),
            stderr: output,
        }
    }
    let s = Sandbox::new(&[("dot_seed", "new")]);
    success(s.init(&[]));
    write(&s.destination.join(".seed"), "original");
    let cancel = terminal(&s, b'n');
    assert!(!cancel.status.success());
    assert_eq!(
        fs::read_to_string(s.destination.join(".seed")).unwrap(),
        "original"
    );
    success(terminal(&s, b'y'));
    assert_eq!(
        fs::read_to_string(s.destination.join(".seed")).unwrap(),
        "new"
    );
    assert!(s.control().join("backups").is_dir());
}

#[test]
fn data_scalar_paths_and_init_config_errors_are_reported_without_panics() {
    let s = Sandbox::new(&[("dot_seed", "seed")]);
    success(s.init(&[]));
    failure(
        s.cli(&["--data", "a=1", "--data", "a.b=2", "data"]),
        "scalar",
    );
    write(&s.source.join(".stool.toml.tmpl"), "data = 1\n");
    failure(s.init(&["--apply"]), "data must be an object");
    assert!(!s.destination.exists());
    write(
        &s.source.join(".stool.toml.tmpl"),
        "repo = 'file:///override'\n",
    );
    failure(s.init(&["--apply"]), "unknown field");
}

#[cfg(unix)]
#[test]
fn external_literal_tree_and_template_include_directory_confinement() {
    let s = Sandbox::new(&[
        (
            "external_dot_vendor/private_literal.tmpl",
            "literal {{ unrendered }}",
        ),
        ("dot_seed", "seed"),
    ]);
    std::os::unix::fs::symlink(
        "private_literal.tmpl",
        s.work.join("external_dot_vendor/link"),
    )
    .unwrap();
    s.push();
    success(s.init(&["--apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join(".vendor/private_literal.tmpl")).unwrap(),
        "literal {{ unrendered }}"
    );
    assert_eq!(
        fs::read_link(s.destination.join(".vendor/link")).unwrap(),
        Path::new("private_literal.tmpl")
    );
    let outside = s.root.join("outside-templates");
    write(&outside.join("secret"), "outside");
    std::os::unix::fs::symlink(outside, s.source.join(".stooltemplates")).unwrap();
    write(&s.source.join("leak.tmpl"), "{% include 'secret' %}");
    failure(s.cli(&["apply"]), "Render template");
    assert!(!s.destination.join("leak").exists());
}

#[cfg(windows)]
#[test]
fn windows_private_readonly_acl_and_replacement_are_idempotent() {
    let s = Sandbox::new(&[
        ("private_dot_secret", "private"),
        ("readonly_settings", "one"),
    ]);
    success(s.init(&["--apply"]));
    success(s.cli(&["verify"]));
    assert!(fs::metadata(s.destination.join("settings"))
        .unwrap()
        .permissions()
        .readonly());
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$a=Get-Acl -LiteralPath $env:TARGET; if(!$a.AreAccessRulesProtected){exit 1}; [Console]::Write('protected')",
        ])
        .env_remove("PSModulePath")
        .env("TARGET", s.destination.join(".secret"))
        .output()
        .unwrap();
    assert_eq!(success(output), "protected");
    write(&s.source.join("readonly_settings"), "two");
    success(s.cli(&["apply"]));
    assert_eq!(
        fs::read_to_string(s.destination.join("settings")).unwrap(),
        "two"
    );
    success(s.cli(&["verify"]));
    // Exercise clearing the readonly DACL and leave the test directory removable.
    fs::rename(
        s.source.join("readonly_settings"),
        s.source.join("settings"),
    )
    .unwrap();
    success(s.cli(&["apply"]));
    assert!(!fs::metadata(s.destination.join("settings"))
        .unwrap()
        .permissions()
        .readonly());
}
