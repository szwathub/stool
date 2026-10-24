use super::*;

fn assert_origin_url_round_trip(url: &str) {
    let temp = tempfile::tempdir().unwrap();
    let repo = gix::init(temp.path()).unwrap();
    save_origin_url(&repo, url).unwrap();
    let args = ["remote", "get-url", "origin"].map(str::to_owned);
    assert_eq!(run(temp.path(), &args).unwrap().as_deref(), Some(url));
    let config = gix::config::File::from_path_no_includes(
        repo.git_dir().join("config"),
        gix::config::Source::Local,
    )
    .unwrap();
    assert_eq!(
        config
            .string("core", None, "repositoryformatversion")
            .unwrap()
            .to_str()
            .unwrap(),
        "0"
    );
}

#[test]
fn origin_url_preserves_windows_file_url_spelling() {
    for url in [
        "file:///C:/Users/runner/work/dotfiles.git",
        "file:///C:/Users/runner/work/dotfiles%20repo.git",
        "file:///C:/Users/runner/work/dotfiles repo.git",
    ] {
        assert_origin_url_round_trip(url);
    }
}

#[test]
fn origin_url_round_trip_escapes_git_config_syntax() {
    assert_origin_url_round_trip("https://example.invalid/a#b;\"c\\d repository.git");
}
