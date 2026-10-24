use crate::common::*;

fn fixture() -> (Sandbox, Vec<&'static str>) {
    let values = vec![
        "",
        "plain",
        " leading and trailing ",
        "Sun \"Work\" O'Reilly \\ path # ;",
        "line\nbreak\r\tbackspace\u{8}",
        "$(touch quote-injected) \u{60}touch quote-injected\u{60} $HOME; #",
        "中文 café 🦀 \\'\"\\",
    ];
    let data = serde_json::to_string(&serde_json::json!({"values": values})).unwrap();
    let s = Sandbox::new(&[
        (".stooldata.json", &data),
        (
            ".stooldata.json.tmpl",
            r#"{"from_data": {{ sh_quote("bootstrap") | tojson }}}"#,
        ),
        (
            ".stool.toml.tmpl",
            "[data]\nfrom_init = {{ fish_quote(\"bootstrap\") | tojson }}\n",
        ),
        (
            "dot_gitconfig.tmpl",
            "{% for value in values %}[sample \"{{ loop.index0 }}\"]\nvalue = {{ git_quote(value) }}\n{% endfor %}",
        ),
        (
            "dot_sh-script.tmpl",
            "{% for value in values %}printf '%s\\0' {{ sh_quote(value) }}\n{% endfor %}",
        ),
        (
            "dot_fish-script.tmpl",
            "{% for value in values %}printf '%s\\0' {{ fish_quote(value) }}\n{% endfor %}",
        ),
    ]);
    (s, values)
}

#[test]
fn builtin_quotes_work_without_includes_in_data_init_and_git_configuration() {
    let (s, values) = fixture();
    success(s.init(&["--apply"]));
    assert!(!s.source.join(".stooltemplates").exists());
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            git(
                &s.root,
                &[
                    "config",
                    "--file",
                    s.destination.join(".gitconfig").to_str().unwrap(),
                    "--null",
                    &format!("sample.{i}.value"),
                ],
            ),
            format!("{value}\0")
        );
    }
    let data: serde_json::Value = serde_json::from_str(&success(s.cli(&["data"]))).unwrap();
    assert_eq!(data["from_data"], "'bootstrap'");
    assert_eq!(data["from_init"], "'bootstrap'");
    success(s.cli(&["verify"]));
}

#[cfg(unix)]
#[test]
fn shell_quotes_preserve_bytes_and_do_not_execute_embedded_commands() {
    let (s, values) = fixture();
    success(s.init(&["--apply"]));
    let expected: Vec<u8> = values
        .iter()
        .flat_map(|value| value.bytes().chain(std::iter::once(0)))
        .collect();
    let output = Command::new("/bin/sh")
        .arg(s.destination.join(".sh-script"))
        .env("HOME", &s.home)
        .current_dir(&s.root)
        .output()
        .unwrap();
    assert_eq!(success(output).as_bytes(), expected);
    if Command::new("fish").arg("--version").output().is_ok() {
        let output = Command::new("fish")
            .arg("--no-config")
            .arg(s.destination.join(".fish-script"))
            .env("HOME", &s.home)
            .current_dir(&s.root)
            .output()
            .unwrap();
        assert_eq!(success(output).as_bytes(), expected);
    }
    assert!(!s.root.join("quote-injected").exists());
}

#[test]
fn quoting_rejects_nul_non_strings_and_missing_values() {
    let (s, _) = fixture();
    success(s.init(&[]));
    let bad = format!("bad={}", serde_json::to_string("\0").unwrap());
    for function in ["sh_quote", "git_quote", "fish_quote"] {
        failure(
            s.cli(&[
                "--data",
                &bad,
                "execute-template",
                &format!("{{{{ {function}(bad) }}}}"),
            ]),
            "NUL",
        );
        for value in ["none", "42", "missing"] {
            failure(
                s.cli(&[
                    "execute-template",
                    &format!("{{{{ {function}({value}) }}}}"),
                ]),
                "string",
            );
        }
    }
}
