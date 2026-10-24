use crate::common::*;

#[test]
fn environment_passphrase_deploys_encrypted_source_without_persisting_secrets() {
    let sandbox = Sandbox::new(&[("dot_seed", "seed")]);
    let password = "  fixture '密码' password  ";
    let plain = b"fixture private configuration\0\n";
    let source = sandbox.work.join("encrypted_private_dot_secret.age");
    fs::write(&source, encrypt_age_fixture(password, plain)).unwrap();
    sandbox.push();
    success(sandbox.init(&[]));
    let missing = sandbox.command().arg("apply").output().unwrap();
    failure(missing, "STOOL_AGE_PASSPHRASE");
    assert!(!sandbox.destination.exists());
    success(
        sandbox
            .command()
            .arg("apply")
            .env("STOOL_AGE_PASSPHRASE", password)
            .output()
            .unwrap(),
    );
    assert_eq!(
        fs::read(sandbox.destination.join(".secret")).unwrap(),
        plain
    );
    let source_before = snapshot(&sandbox.source);
    let state_before = snapshot(&sandbox.control());
    let target_before = snapshot(&sandbox.destination);
    let failed = sandbox
        .command()
        .arg("apply")
        .env("STOOL_AGE_PASSPHRASE", password.trim())
        .output()
        .unwrap();
    for output in [&failed.stdout, &failed.stderr] {
        let output = String::from_utf8_lossy(output);
        assert!(!output.contains(password.trim()));
        assert!(!output.contains("fixture private configuration"));
    }
    failure(failed, "Decrypt");
    assert_eq!(snapshot(&sandbox.source), source_before);
    assert_eq!(snapshot(&sandbox.control()), state_before);
    assert_eq!(snapshot(&sandbox.destination), target_before);
    for file in snapshot(&sandbox.control()).values() {
        assert!(!file.contains(password));
        assert!(!file.contains("fixture private configuration"));
    }
}
