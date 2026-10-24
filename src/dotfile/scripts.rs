use super::process;
use super::source::{Condition, Order, Script};
use super::storage::Paths;
use crate::common::fs as filesystem;
use anyhow::{Context, Result};
use redb::{Database, ReadableTable, TableDefinition};
use std::collections::BTreeSet;
use std::fs;

const SUCCESS: TableDefinition<&str, &str> = TableDefinition::new("script_success");

pub struct Store {
    namespace: String,
    done: BTreeSet<String>,
}
impl Store {
    pub fn load(paths: &Paths) -> Result<Self> {
        let namespace = paths
            .state_file
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let file = paths.root.join("state/scripts.redb");
        let mut done = BTreeSet::new();
        if file.exists() {
            // redb 2.1 opens files read/write. A snapshot permits genuinely read-only
            // preview, including on read-only source/control volumes.
            let temp = tempfile::tempdir()?;
            let snapshot = temp.path().join("scripts.redb");
            fs::copy(&file, &snapshot)?;
            let db = Database::open(snapshot).context("Read script execution database")?;
            let transaction = db.begin_read()?;
            if let Ok(table) = transaction.open_table(SUCCESS) {
                for entry in table.iter()? {
                    let (key, _) = entry?;
                    done.insert(key.value().to_owned());
                }
            }
        }
        Ok(Self { namespace, done })
    }
    fn key(&self, script: &Script) -> String {
        match script.condition {
            Condition::Always => format!("{}:always:{}", self.namespace, script.name),
            Condition::Once => {
                format!(
                    "{}:once:{}",
                    self.namespace,
                    filesystem::digest(&script.bytes)
                )
            }
            Condition::OnChange => format!(
                "{}:onchange:{}:{}",
                self.namespace,
                script.name,
                filesystem::digest(&script.bytes)
            ),
        }
    }
    pub fn pending(&self, script: &Script) -> bool {
        script.condition == Condition::Always || !self.done.contains(&self.key(script))
    }
    pub fn run(&mut self, paths: &Paths, scripts: &[Script], order: Order) -> Result<()> {
        for script in scripts.iter().filter(|script| script.order == order) {
            if !self.pending(script) {
                continue;
            }
            println!("Running {}", script.name);
            process::script(paths, &script.name, &script.bytes, None, false)?;
            if script.condition != Condition::Always {
                let directory = paths.root.join("state");
                filesystem::private_dir(&directory)?;
                let file = directory.join("scripts.redb");
                let db = Database::create(&file)?;
                let transaction = db.begin_write()?;
                let key = self.key(script);
                {
                    let mut table = transaction.open_table(SUCCESS)?;
                    // On-change tracks the most recent successful content, so A→B→A runs again.
                    if script.condition == Condition::OnChange {
                        let prefix = format!("{}:onchange:{}:", self.namespace, script.name);
                        let old: Vec<String> = table
                            .iter()?
                            .filter_map(|entry| {
                                entry.ok().and_then(|(key, _)| {
                                    key.value()
                                        .starts_with(&prefix)
                                        .then(|| key.value().to_owned())
                                })
                            })
                            .collect();
                        for old in old {
                            table.remove(old.as_str())?;
                            self.done.remove(&old);
                        }
                    }
                    table.insert(key.as_str(), "success")?;
                }
                transaction.commit()?;
                filesystem::set_mode(&file, 0o600)?;
                self.done.insert(key);
            }
        }
        Ok(())
    }
}
