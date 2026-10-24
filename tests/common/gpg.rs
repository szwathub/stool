use super::{fs, git_path, Command, Path, PathBuf};
use std::sync::OnceLock;

// A real executable rather than a shell script exercises Windows process
// creation, argument passing and stdin without requiring another runtime.
pub fn fake_gpg() -> &'static Path {
    static FIXTURE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &FIXTURE
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("fixture.rs");
            fs::write(&source, FAKE_GPG).unwrap();
            let binary = git_path(&directory.path().join(if cfg!(windows) {
                "gpg fixture.exe"
            } else {
                "gpg fixture"
            }));
            let output = Command::new("rustc")
                .args(["--edition=2021", "-O"])
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            (directory, binary)
        })
        .1
}

const FAKE_GPG: &str = r##"
use std::{env, fs, io::{self, Read, Write}, path::{Path, PathBuf}};
const A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const C: &str = "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";
const D: &str = "DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD";
const E: &str = "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE";
const F: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
fn append(path: PathBuf, value: &str) { let mut f=fs::OpenOptions::new().create(true).append(true).open(path).unwrap(); writeln!(f,"{value}").unwrap(); }
fn die() -> ! { eprintln!("fixture GPG failed; private output intentionally omitted"); std::process::exit(19); }
fn subkey(key: &str) -> &str { match key { A=>D, B=>E, C=>F, _=>die() } }
// Column 15 models GnuPG's local secret material, missing stub, or card serial.
fn record(kind: &str, key: &str, available: &str, fingerprint: bool) {
 let mut fields=vec!["";18]; fields[0]=kind; fields[1]="u"; fields[2]="256";
 fields[3]="22"; fields[4]=&key[24..]; fields[11]="scSC"; fields[14]=available;
 println!("{}",fields.join(":"));
 if fingerprint { println!("fpr:::::::::{key}:"); }
}
fn packet(root: &Path, kind: &str, keys: &[String]) -> String {
 let mut value=format!("STOOL-FAKE-{kind}\n");
 for key in keys {
  if ![A,B,C].contains(&key.as_str()) {die();}
  if !(kind=="SECRET" && key==B && root.join("partial-secret-primary").exists()) {value.push_str(key); value.push('\n');}
  let partial=(kind=="SECRET" && root.join("partial-secret-subkey").exists())
    || (kind=="PUBLIC" && root.join("partial-public-subkey").exists());
  if !(key==B && (partial || root.join("dropped-subkey").exists())) {
   let prefix=if kind=="SECRET" && key==B && root.join("stub-secret-export").exists() {"stub-sub:"} else {"sub:"};
   value.push_str(prefix); value.push_str(subkey(key)); value.push('\n');
  }
 }
 value
}
fn main() {
 let args:Vec<String>=env::args().skip(1).collect();
 let root=PathBuf::from(env::var_os("STOOL_TEST_GPG_ROOT").unwrap());
 if let Some(i)=args.iter().position(|a| a=="--stool-hook") {
   let phase=&args[i+1]; append(root.join("events"),&format!("hook:{phase}"));
   if phase=="before" { fs::write(root.join("before"),"").unwrap(); }
   if phase=="after" { if !root.join("imports").exists() { die(); } fs::write(root.join("after"),"").unwrap(); }
   return;
 }
 append(root.join("events"),&format!("gpg:{}",args.join(" ")));
 let has=|needle:&str| args.iter().any(|a| a==needle);
 let marked=|name:&str| root.join(name).exists();
 if has("--version") {if marked("fail-version") {die();} println!("gpg (GnuPG/MacGPG2) 2.4 fixture"); return;}
 if has("--list-secret-keys") {
   if marked("fail-discovery") {die();}
   if marked("no-secret") {return;}
   let changed=marked("inventory-changed") && marked("inventory-seen");
   if marked("inventory-changed") {fs::write(root.join("inventory-seen"), "").unwrap();}
   for key in [A,B] {
    if key==B && marked("only-personal") {continue;}
    let primary=if marked("all-stubs") || (key==B && (marked("stub-primary") || marked("stub-only-work"))) {"#"}
      else if key==B && (marked("token-primary") || marked("token-only-work")) {"D2760001240103040000000000010000"} else {"+"};
    let sub=if (changed && key==B) || marked("all-stubs") || (key==B && (marked("stub-subkey") || marked("stub-only-work"))) {"#"}
      else if key==B && (marked("token-subkey") || marked("token-only-work")) {"D2760001240103040000000000010000"} else {"+"};
    record("sec",key,primary,!(key==B && marked("missing-primary-fingerprint")));
    println!("uid:u::::::::Fixture:");
    if !(key==B && marked("dropped-subkey")) {record("ssb",subkey(key),sub,!(key==B && marked("missing-subkey-fingerprint")));}
   }
   return;
 }
 if has("--list-keys") {
   let selected=args.last().unwrap();
   let selected=match selected.as_str() { D=>A, E=>B, F=>C, key=>key };
   let keys=if selected=="--list-keys" {vec![A,B,C]} else {vec![selected]};
   for key in keys { if ![A,B,C].contains(&key) {die();} record("pub",key,"",true); record("sub",subkey(key),"",true); }
   return;
 }
 for (flag,kind,failure) in [("--export-secret-keys","SECRET","fail-secret-export"),("--export","PUBLIC","fail-public-export")] {
   if let Some(i)=args.iter().position(|a| a==flag) {
     if marked(failure) {die();}
     let mut keys=args[i+1..].to_vec();
     if marked("partial-export") && kind=="SECRET" {keys.retain(|k| k!=B);}
     print!("{}",packet(&root,kind,&keys)); return;
   }
 }
 if has("--export-ownertrust") {
   if marked("fail-trust-export") {die();}
   for key in [A,B,C] {println!("{key}:6:");} return;
 }
 let mut input=String::new(); io::stdin().read_to_string(&mut input).unwrap();
 if has("show-only") {
   let mut lines=input.lines();
   let kind=match lines.next() {Some("STOOL-FAKE-SECRET")=>"sec",Some("STOOL-FAKE-PUBLIC")=>"pub",_=>die()};
   for line in lines {
    if let Some(key)=line.strip_prefix("sub:") {if ![D,E,F].contains(&key) {die();} record(if kind=="sec" {"ssb"} else {"sub"},key,"",true);}
    else if let Some(key)=line.strip_prefix("stub-sub:") {if ![D,E,F].contains(&key) {die();} record("ssb",key,"#",true);}
    else {if ![A,B,C].contains(&line) {die();} record(kind,line,"",true); println!("uid:u::::::::Fixture:");}
   }
   return;
 }
 if has("--import") || has("--import-ownertrust") {
   if marked("fail-import") {die();}
   let deployed=PathBuf::from(env::var_os("HOME").unwrap()).join(".seed").exists();
   let kind=if has("--import-ownertrust") {"trust"} else if input.starts_with("STOOL-FAKE-SECRET") {"secret"} else {"public"};
   append(root.join("imports"),&format!("{kind}:deployed={deployed}")); append(root.join("events"),&format!("import:{kind}")); return;
 }
 die();
}
"##;

pub fn private_write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(windows)]
    {
        // Match a real owner-only password file, rather than relying on the
        // runner's inherited temporary-directory ACL.
        let script = r#"
$ErrorActionPreference = 'Stop'
$path = $env:STOOL_TEST_PASSWORD_PATH
$acl = Get-Acl -LiteralPath $path
$acl.SetAccessRuleProtection($true, $false)
foreach ($rule in @($acl.Access)) { $acl.RemoveAccessRuleSpecific($rule) }
$allow = [System.Security.AccessControl.AccessControlType]::Allow
$rights = [System.Security.AccessControl.FileSystemRights]::FullControl
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($me, $rights, $allow))
foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
    $identity = [System.Security.Principal.SecurityIdentifier]::new($sid)
    $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($identity, $rights, $allow))
}
Set-Acl -LiteralPath $path -AclObject $acl
"#;
        let powershell = std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
            .unwrap_or_else(|| "powershell.exe".into());
        let out = Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("STOOL_TEST_PASSWORD_PATH", path)
            .env_remove("PSModulePath")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
