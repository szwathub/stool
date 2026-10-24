//! Hidden password input using native console handles.
use age::secrecy::SecretString;
use std::io;
#[cfg(windows)]
use std::{fs, io::Write};

#[cfg(not(windows))]
pub(crate) fn prompt_passphrase(prompt: &str) -> io::Result<SecretString> {
    rpassword::prompt_password(prompt).map(SecretString::new)
}

#[cfg(windows)]
pub(crate) fn prompt_passphrase(prompt: &str) -> io::Result<SecretString> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
        ENABLE_PROCESSED_INPUT,
    };

    struct ConsoleInput {
        file: fs::File,
        mode: u32,
    }
    impl Drop for ConsoleInput {
        fn drop(&mut self) {
            unsafe { SetConsoleMode(self.file.as_raw_handle() as _, self.mode) };
        }
    }

    // rpassword 7.3.1 writes a newline to stdout on Windows. Use its buffered
    // reader with explicit console handles so prompts cannot alter crypto output.
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")?;
    let mut mode = 0;
    let handle = file.as_raw_handle() as _;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let hidden_mode = (mode | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT) & !ENABLE_ECHO_INPUT;
    if unsafe { SetConsoleMode(handle, hidden_mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let input = ConsoleInput { file, mode };
    let mut terminal = fs::OpenOptions::new().write(true).open("CONOUT$")?;
    let mut reader = io::BufReader::new(&input.file);
    let password = SecretString::new(rpassword::prompt_password_from_bufread(
        &mut reader,
        &mut terminal,
        prompt,
    )?);
    terminal.write_all(b"\r\n")?;
    Ok(password)
}
