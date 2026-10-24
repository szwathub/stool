use minijinja::value::Value;
use minijinja::{Error, ErrorKind};

fn text(value: &Value) -> Result<&str, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "Quoting requires a string"))?;
    if text.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "Quoting cannot represent NUL",
        ));
    }
    Ok(text)
}

/// One literal POSIX shell word, also accepted by Bash.
pub(super) fn sh_quote(value: Value) -> Result<String, Error> {
    Ok(format!("'{}'", text(&value)?.replace('\'', "'\\''")))
}

/// One double-quoted Git configuration value.
pub(super) fn git_quote(value: Value) -> Result<String, Error> {
    let value = text(&value)?
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\u{8}', "\\b");
    Ok(format!("\"{value}\""))
}

/// One literal Fish shell word; Fish has different single-quote escapes.
pub(super) fn fish_quote(value: Value) -> Result<String, Error> {
    let value = text(&value)?.replace('\\', "\\\\").replace('\'', "\\'");
    Ok(format!("'{value}'"))
}
