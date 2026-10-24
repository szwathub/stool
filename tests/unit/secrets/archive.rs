use super::*;
fn raw_tar(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes, kind) in entries {
        let mut header = tar::Header::new_ustar();
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_size(bytes.len() as u64);
        header.set_entry_type(*kind);
        header.set_mode(0o600);
        header.set_cksum();
        builder.append(&header, *bytes).unwrap();
    }
    builder.into_inner().unwrap()
}
#[test]
fn file_directory_collisions_are_rejected_case_insensitively() {
    for (first, second) in [("ssh/key", "ssh/key/child"), ("ssh/KEY/child", "ssh/key")] {
        let mut files = Files::new();
        add(&mut files, first.into(), SecretVec::new(vec![1])).unwrap();
        assert!(add(&mut files, second.into(), SecretVec::new(vec![2])).is_err());
    }
}
#[test]
fn untrusted_archive_paths_duplicates_and_special_files_are_rejected() {
    for name in [
        "../secret",
        "/absolute",
        "ssh/../../escape",
        "ssh/\\windows",
        "ssh/C:drive",
        "ssh/NUL",
        "ssh/.git/key",
        "ssh/a.",
    ] {
        let bytes = raw_tar(&[(name, b"test", tar::EntryType::Regular)]);
        assert!(unpack(&bytes).is_err(), "{name}");
    }
    let bytes = raw_tar(&[
        ("ssh/key", b"x", tar::EntryType::Regular),
        ("SSH/KEY", b"y", tar::EntryType::Regular),
    ]);
    assert!(unpack(&bytes)
        .err()
        .unwrap()
        .to_string()
        .contains("duplicate"));
    for kind in [
        tar::EntryType::Symlink,
        tar::EntryType::Link,
        tar::EntryType::Fifo,
        tar::EntryType::Char,
        tar::EntryType::Block,
    ] {
        assert!(unpack(&raw_tar(&[("ssh/key", b"", kind)])).is_err());
    }
}
#[test]
fn unsupported_versions_and_unlisted_payloads_are_rejected() {
    let bytes=raw_tar(&[("manifest.json",br#"{"format":"stool-key-backup","version":999,"created_unix":0,"gpg":null,"ssh":[],"files":{},"warnings":[]}"#,tar::EntryType::Regular)]);
    assert!(unpack(&bytes)
        .err()
        .unwrap()
        .to_string()
        .contains("version"));
    let bytes=raw_tar(&[("manifest.json",br#"{"format":"stool-key-backup","version":1,"created_unix":0,"gpg":null,"ssh":[],"files":{},"warnings":[]}"#,tar::EntryType::Regular),("ssh/key",b"unlisted private data",tar::EntryType::Regular)]);
    assert!(unpack(&bytes)
        .err()
        .unwrap()
        .to_string()
        .contains("checksum"));
}
