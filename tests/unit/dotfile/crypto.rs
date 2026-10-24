use super::*;
#[test]
fn age_roundtrip_and_wrong_identity_are_verified() {
    let temp = tempfile::tempdir().unwrap();
    let key = temp.path().join("identity");
    let public = keygen(&key).unwrap();
    let mut settings = EncryptionSettings {
        recipients: vec![public],
        identities: vec![key],
        ..Default::default()
    };
    let plain = b"private\0configuration\n";
    let cipher = encrypt(&settings, plain).unwrap();
    assert!(!cipher.windows(plain.len()).any(|part| part == plain));
    assert_eq!(
        decrypt(&settings, temp.path(), &cipher, "age").unwrap(),
        plain
    );
    let other = temp.path().join("wrong");
    keygen(&other).unwrap();
    settings.identities = vec![other];
    assert!(decrypt(&settings, temp.path(), &cipher, "age").is_err());
}
