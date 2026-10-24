use super::*;

#[test]
fn drive_prefixes_are_checked_only_after_the_root_is_present() {
    let temporary = tempfile::tempdir().unwrap();
    let canonical = temporary.path().canonicalize().unwrap();
    for base in [canonical.as_path(), dunce::simplified(&canonical)] {
        assert_eq!(checked(base).unwrap(), canonical);
        let file = base.join("not-created").join("keys.tar.age");
        assert_eq!(
            checked(&file).unwrap(),
            canonical.join("not-created/keys.tar.age")
        );
        // PathBuf::join normalizes parent components on verbatim Windows paths.
        // Preserve the caller's spelling so the traversal guard is actually exercised.
        let mut traversal = base.as_os_str().to_os_string();
        traversal.push(r"\..\escape");
        assert!(checked(Path::new(&traversal)).is_err());
    }
}
