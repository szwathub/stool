use super::*;
#[test]
fn attributes_preserve_literal_names_and_validate_order() {
    assert_eq!(
        attributes("dot_gitconfig", false).unwrap().name,
        ".gitconfig"
    );
    assert_eq!(attributes("private_dot_ssh", true).unwrap().name, ".ssh");
    assert_eq!(
        attributes("literal_dot_name.tmpl.literal", false)
            .unwrap()
            .name,
        "dot_name.tmpl"
    );
    assert!(attributes("literal_name.tmpl", false).unwrap().template);
    assert!(attributes("dot_private_name", false).is_err());
    assert!(attributes("executable_dir", true).is_err());
    assert!(attributes("dot_", false).is_err());
    assert!(attributes("literal_..", false).is_err());
    let script = attributes("run_once_before_setup.sh.tmpl", false).unwrap();
    assert!(matches!(script.kind, Kind::Script(Condition::Once, Order::Before)) && script.template);
    let encrypted = attributes("encrypted_private_dot_config.tmpl.age", false).unwrap();
    assert!(encrypted.encrypted && encrypted.private && encrypted.template);
    assert_eq!(encrypted.name, ".config");
}
