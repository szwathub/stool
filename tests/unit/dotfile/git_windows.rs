use super::*;

#[test]
fn git_arguments_use_compatible_paths_for_canonical_directories() {
    let canonical = Path::new(r"\\?\C:\dotfiles checkout");
    assert_eq!(system_path(canonical), Path::new(r"C:\dotfiles checkout"));
    let command = command(Some(canonical));
    let arguments: Vec<_> = command.get_args().collect();
    assert_eq!(
        arguments,
        [OsStr::new("-C"), OsStr::new(r"C:\dotfiles checkout")]
    );
}

#[test]
fn git_paths_preserve_unc_long_and_verbatim_only_names() {
    for path in [
        Path::new(r"\\?\UNC\server\share\dotfiles"),
        Path::new(r"\\?\C:\COM1"),
        Path::new(r"\\?\C:\dotfiles."),
    ] {
        assert_eq!(system_path(path), path);
    }
    let long = std::path::PathBuf::from(format!(r"\\?\C:\{}", "segment\\".repeat(40)));
    assert_eq!(system_path(&long), long);
}
