//! Shared path resolution, private permissions, locking and atomic writes.
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Resolve CLI roots once; descendant paths are subsequently checked without following symlinks.
pub fn resolve(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    let mut ancestor = normalized.as_path();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .context("Invalid absolute path")?
                        .to_owned(),
                );
                ancestor = ancestor.parent().context("Cannot resolve path")?;
            }
            Err(e) => return Err(e).with_context(|| format!("Inspect {}", ancestor.display())),
        }
    }
    let mut result =
        fs::canonicalize(ancestor).with_context(|| format!("Resolve {}", ancestor.display()))?;
    for part in missing.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}

pub fn expand_home(path: &Path, home: &Path) -> PathBuf {
    if path == Path::new("~") {
        home.to_owned()
    } else if let Ok(relative) = path.strip_prefix("~") {
        home.join(relative)
    } else {
        path.to_owned()
    }
}

/// Keep the file open for the command lifetime; OS locks are released even after a crash.
pub fn lock(root: &Path, write: bool) -> Result<Option<File>> {
    if write {
        private_dir(root)?;
    }
    let file = match OpenOptions::new()
        .read(true)
        .write(write)
        .create(write)
        .truncate(false)
        .open(root.join("lock"))
    {
        Ok(file) => file,
        Err(e) if !write && e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("Open dotfile lock"),
    };
    let result = if write {
        file.try_lock_exclusive()
    } else {
        FileExt::try_lock_shared(&file)
    };
    result.context("Another dotfile command is running; retry after it finishes")?;
    Ok(Some(file))
}

pub fn private_dir(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            bail!(
                "Control directory is not a regular directory: {}",
                path.display()
            );
        }
    }
    fs::create_dir_all(path).with_context(|| format!("Create {}", path.display()))?;
    set_mode(path, 0o700)
}

pub fn atomic_write(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().context("File has no parent directory")?)?;
    temp.write_all(data)?;
    // Windows persistence changes file attributes, so retain owner write access
    // until the replacement is complete; private access rules still apply.
    #[cfg(windows)]
    let staged_mode = mode | 0o200;
    #[cfg(not(windows))]
    let staged_mode = mode;
    set_mode(temp.path(), staged_mode)?;
    temp.as_file().sync_all()?;
    #[cfg(windows)]
    if path.is_file() {
        set_mode(path, mode | 0o200)?;
    }
    temp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("Replace {}", path.display()))?;
    // tempfile clears FILE_ATTRIBUTE_READONLY while persisting on Windows.
    // Set read-only permissions only after its atomic replacement completes.
    #[cfg(windows)]
    if mode & 0o200 == 0 {
        set_mode(path, mode)?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
#[cfg(windows)]
fn powershell(script: &str) -> std::process::Command {
    // A missing-Git check intentionally runs with an empty PATH. Use the OS
    // installation directly instead of depending on a shell search for it.
    let executable = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("WINDIR"))
        .map(|root| PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .unwrap_or_else(|| "powershell.exe".into());
    let mut command = std::process::Command::new(executable);
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        // PowerShell 7 can pass its incompatible module search path to the
        // Windows PowerShell 5 helper. Let that helper rebuild its own defaults.
        .env_remove("PSModulePath");
    command
}

#[cfg(windows)]
pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    // Keep all path values in the environment, never interpolate them as code.
    let script = r#"
$ErrorActionPreference = 'Stop'
$path = $env:STOOL_ACL_PATH
$item = Get-Item -LiteralPath $path -Force
$directory = $item.PSIsContainer
$mode = [Convert]::ToInt32($env:STOOL_ACL_MODE, 8)
$acl = Get-Acl -LiteralPath $path
$acl.SetAccessRuleProtection($true, $false)
foreach ($rule in @($acl.Access)) {
    $acl.RemoveAccessRuleSpecific($rule)
}
$inherit = if ($directory) {
    [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
} else {
    [System.Security.AccessControl.InheritanceFlags]::None
}
$prop = [System.Security.AccessControl.PropagationFlags]::None
$allow = [System.Security.AccessControl.AccessControlType]::Allow
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$rights = if (($mode -band 128) -ne 0) {
    [System.Security.AccessControl.FileSystemRights]::FullControl
} else {
    [System.Security.AccessControl.FileSystemRights]::ReadAndExecute
}
$acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
    $me, $rights, $inherit, $prop, $allow))
foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
        [System.Security.Principal.SecurityIdentifier]::new($sid),
        [System.Security.AccessControl.FileSystemRights]::FullControl,
        $inherit, $prop, $allow))
}
if (($mode -band 63) -ne 0) {
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
        [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'),
        [System.Security.AccessControl.FileSystemRights]::ReadAndExecute,
        $inherit, $prop, $allow))
}
if (!$directory -and (($mode -band 128) -eq 0)) {
    $item.IsReadOnly = $true
}
Set-Acl -LiteralPath $path -AclObject $acl
if (!$directory -and (($mode -band 128) -ne 0)) {
    $item.IsReadOnly = $false
}
"#;
    let output = powershell(script)
        .env("STOOL_ACL_PATH", path)
        .env("STOOL_ACL_MODE", format!("{mode:o}"))
        .output()
        .context("Set Windows ACL")?;
    if !output.status.success() {
        bail!(
            "Cannot set Windows permissions for {} ({}): {}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
#[cfg(not(any(unix, windows)))]
pub fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    bail!("Unsupported platform permissions")
}

#[cfg(unix)]
pub fn mode(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o777
}
#[cfg(not(unix))]
pub fn mode(meta: &fs::Metadata) -> u32 {
    if meta.is_dir() {
        0o755
    } else {
        0o644
    }
}

/// Windows maps private/read-only attributes to a protected DACL. Unix modes
/// remain native; execute bits have no separate meaning for Windows files.
#[cfg(not(windows))]
pub fn path_mode(_path: &Path, meta: &fs::Metadata) -> Result<u32> {
    Ok(mode(meta))
}

#[cfg(windows)]
fn acl_command(path: &Path, script: &str) -> Result<std::process::Output> {
    let output = powershell(script)
        .env("STOOL_ACL_PATH", path)
        .output()
        .context("Run Windows ACL helper")?;
    if !output.status.success() {
        bail!(
            "Cannot read or set Windows permissions for {} ({}): {}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

#[cfg(windows)]
pub fn path_mode(path: &Path, meta: &fs::Metadata) -> Result<u32> {
    let output = acl_command(
        path,
        r#"
$ErrorActionPreference = 'Stop'
$acl = Get-Acl -LiteralPath $env:STOOL_ACL_PATH
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$private = $true
$write = $false
foreach ($rule in $acl.Access) {
    if ($rule.AccessControlType -ne 'Allow') { continue }
    $sid = $rule.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
    if ($sid -notin @($me, 'S-1-5-18', 'S-1-5-32-544')) {
        $private = $false
    }
    if ($sid -eq $me -and (($rule.FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::WriteData) -ne 0)) {
        $write = $true
    }
}
@{private=$private; write=$write} | ConvertTo-Json -Compress
"#,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut mode = if meta.is_dir() { 0o755 } else { 0o644 };
    if value["private"] == true {
        mode &= 0o700;
    }
    if value["write"] == false || (!meta.is_dir() && meta.permissions().readonly()) {
        mode &= !0o222;
    }
    Ok(mode)
}

pub fn normalized_mode(mode: u32, directory: bool) -> u32 {
    if cfg!(windows) && !directory {
        mode & !0o111
    } else {
        mode
    }
}

#[cfg(all(test, windows))]
#[path = "../../tests/unit/common/fs_windows.rs"]
mod windows_tests;
