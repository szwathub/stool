use super::*;

#[test]
fn atomic_private_writes_preserve_readonly_and_can_be_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("private [folder]");
    private_dir(&directory).unwrap();
    let file = directory.join("identity [test]");
    atomic_write(&file, b"first", 0o600).unwrap();
    assert_eq!(
        path_mode(&file, &fs::metadata(&file).unwrap()).unwrap(),
        0o600
    );
    atomic_write(&file, b"readonly", 0o400).unwrap();
    assert!(fs::metadata(&file).unwrap().permissions().readonly());
    assert_eq!(
        path_mode(&file, &fs::metadata(&file).unwrap()).unwrap(),
        0o400
    );
    atomic_write(&file, b"replacement", 0o600).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"replacement");
    assert!(!fs::metadata(&file).unwrap().permissions().readonly());
    assert_eq!(
        path_mode(&file, &fs::metadata(&file).unwrap()).unwrap(),
        0o600
    );
}
