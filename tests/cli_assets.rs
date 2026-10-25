use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn root_assets_match_the_running_cli() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let version = Command::new(env!("CARGO_BIN_EXE_stool"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!("stool {}", env!("CARGO_PKG_VERSION"))
    );
    for (shell, filename) in [
        ("bash", "stool.bash"),
        ("zsh", "_stool"),
        ("fish", "stool.fish"),
        ("powershell", "_stool.ps1"),
        ("elvish", "stool.elv"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_stool"))
            .args(["completions", shell])
            .output()
            .unwrap();
        assert!(output.status.success(), "{shell}");
        assert_eq!(
            fs::read(root.join("completions").join(filename)).unwrap(),
            output.stdout,
            "{shell}"
        );
    }
    let temporary = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_stool"))
        .args(["man", "--output"])
        .arg(temporary.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut actual = files(&root.join("man-page"));
    actual.retain(|path, _| {
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("stool")
    });
    assert_eq!(actual, files(temporary.path()));
}

#[test]
fn generator_writes_target_commands_to_the_root_and_removes_obsolete_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    for name in ["completions", "man-page"] {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(root.join(name).join("README.txt"), "local notes").unwrap();
    }
    for os in ["macos", "linux", "windows"] {
        fs::write(root.join("man-page/stool-obsolete.1"), "obsolete").unwrap();
        fs::write(root.join("completions/stool-obsolete.bash"), "obsolete").unwrap();
        let output = Command::new(env!("STOOL_BUILD_SCRIPT"))
            .env("CARGO_MANIFEST_DIR", root)
            .env("CARGO_CFG_TARGET_OS", os)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(root.join("man-page/stool-apple.1").exists(), os == "macos");
        assert_eq!(
            root.join("man-page/stool-apple-brew-install.1").exists(),
            os == "macos"
        );
        let completion = fs::read_to_string(root.join("completions/stool.bash")).unwrap();
        assert_eq!(completion.contains("stool__apple"), os == "macos");
        assert!(!root.join("man-page/stool-obsolete.1").exists());
        assert!(!root.join("completions/stool-obsolete.bash").exists());
        assert_eq!(fs::read_dir(root).unwrap().count(), 2);
        let mut completions = files(&root.join("completions"));
        assert_eq!(
            completions.remove(Path::new("README.txt")).unwrap(),
            b"local notes"
        );
        assert_eq!(completions.len(), 5);
        assert!(root.join("man-page/stool.1").is_file());
        assert_eq!(
            fs::read(root.join("man-page/README.txt")).unwrap(),
            b"local notes"
        );
    }
}
