//! What qtools has applied, and the backups that let it be undone.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::problem::Problem;
use crate::step::{Before, Step};
use crate::system::System;

/// A 64-bit FNV-1a digest, written as hexadecimal. It only has to notice a change,
/// so a short non-cryptographic digest is enough and needs no dependency.
pub fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Where qtools keeps its backups and its journal: `$XDG_STATE_HOME/quvyta-tools`,
/// or `~/.local/state/quvyta-tools` when that variable is not set.
pub fn folder() -> PathBuf {
    folder_from(std::env::var("XDG_STATE_HOME").ok(), std::env::var("HOME").ok())
}

/// The pure part of [`folder`], taking the two environment variables as arguments
/// so it can be tested without depending on the machine's real environment, and so the screen can
/// ask the same question of the state folder when it looks for a place to keep a session's
/// control channel.
pub(crate) fn folder_from(state_home: Option<String>, home: Option<String>) -> PathBuf {
    let base = state_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.filter(|value| !value.is_empty()).map(|home| Path::new(&home).join(".local/state")))
        .unwrap_or_default();
    base.join("quvyta-tools")
}

/// One step that qtools applied, and what it takes to undo it.
///
/// It is read from and written to TOML by hand, through `toml::Table`, so no
/// serialisation dependency is needed for six fields. The command that undoes a step is not
/// among them: it comes from the catalog's own step, so a journal edited on disk can never
/// make qtools run a command of its choosing as root.
#[derive(Debug, Clone)]
pub struct Applied {
    /// Which tweak this step belongs to.
    pub tweak: String,
    /// The step's place in that tweak.
    pub step: usize,
    /// Where the file's earlier content was saved, when a file was changed.
    pub backup: Option<PathBuf>,
    /// The file that was changed.
    pub target: Option<PathBuf>,
    /// When the step ran, in seconds since 1970, or `None` in a journal older than this field.
    pub when: Option<u64>,
    /// The digest of what qtools wrote, so a later change by someone else is noticed.
    pub hash: Option<String>,
}

/// Everything qtools has applied on this machine.
#[derive(Debug, Default)]
pub struct Journal {
    entries: Vec<Applied>,
}

impl Journal {
    /// Reads the journal, or an empty one when it is not there yet.
    pub fn load(system: &dyn System, folder: &Path) -> io::Result<Self> {
        let path = folder.join("applied.toml");
        let Some(text) = system.read(&path)? else {
            return Ok(Self::default());
        };
        let table: toml::Table =
            text.parse().map_err(|error| io::Error::other(format!("{}: {error}", path.display())))?;
        let mut entries = Vec::new();
        for (tweak, steps) in &table {
            let Some(list) = steps.as_array() else { continue };
            for item in list {
                let Some(item) = item.as_table() else { continue };
                entries.push(Applied {
                    tweak: tweak.clone(),
                    step: item.get("step").and_then(toml::Value::as_integer).unwrap_or(0).unsigned_abs() as usize,
                    backup: item.get("backup").and_then(toml::Value::as_str).map(PathBuf::from),
                    target: item.get("target").and_then(toml::Value::as_str).map(PathBuf::from),
                    // An older journal also kept the undo command here; it is not read, because
                    // the command comes from the catalog.
                    when: item.get("when").and_then(toml::Value::as_integer).map(i64::unsigned_abs),
                    hash: item.get("hash").and_then(toml::Value::as_str).map(str::to_owned),
                });
            }
        }
        Ok(Self { entries })
    }

    /// Writes the journal back.
    pub fn save(&self, system: &mut dyn System, folder: &Path) -> io::Result<()> {
        let mut grouped: BTreeMap<String, Vec<toml::Value>> = BTreeMap::new();
        for entry in &self.entries {
            let mut item = toml::Table::new();
            item.insert("step".into(), toml::Value::Integer(entry.step as i64));
            if let Some(backup) = &entry.backup {
                item.insert("backup".into(), toml::Value::String(backup.display().to_string()));
            }
            if let Some(target) = &entry.target {
                item.insert("target".into(), toml::Value::String(target.display().to_string()));
            }
            if let Some(when) = entry.when.and_then(|when| i64::try_from(when).ok()) {
                item.insert("when".into(), toml::Value::Integer(when));
            }
            if let Some(hash) = &entry.hash {
                item.insert("hash".into(), toml::Value::String(hash.clone()));
            }
            grouped.entry(entry.tweak.clone()).or_default().push(toml::Value::Table(item));
        }
        let table: toml::Table = grouped.into_iter().map(|(tweak, items)| (tweak, toml::Value::Array(items))).collect();
        system.write(&folder.join("applied.toml"), &toml::to_string_pretty(&table).map_err(io::Error::other)?, false)
    }

    /// Keeps the content a step is about to change, before it changes anything: the backup is
    /// on disk, or the step does not run. Answers where it was kept, or `None` when the step
    /// changes no file or the file is not there yet, which the undo answers by removing it.
    ///
    /// # Errors
    ///
    /// Fails when the backup cannot be written.
    pub fn keep_backup(
        tweak: &str,
        step: usize,
        before: &Before,
        system: &mut dyn System,
        folder: &Path,
    ) -> Result<Option<PathBuf>, Problem> {
        let Some((target, Some(content))) = &before.file else { return Ok(None) };
        let name = target.display().to_string().replace('/', "-");
        let backup = folder.join("backups").join(format!("{tweak}-{step}-{}{name}", now()));
        system.write(&backup, content, false)?;
        Ok(Some(backup))
    }

    /// Remembers one step that has just run, given with its place in its tweak: the file it
    /// wrote, with the digest of what is in it now, and the backup [`Journal::keep_backup`]
    /// kept of it.
    ///
    /// # Errors
    ///
    /// Fails when the file the step wrote cannot be read back.
    pub fn record(
        &mut self,
        tweak: &str,
        step: (usize, &Step),
        before: &Before,
        backup: Option<PathBuf>,
        system: &mut dyn System,
    ) -> Result<(), Problem> {
        let (index, step) = step;
        let mut entry =
            Applied { tweak: tweak.to_owned(), step: index, backup, target: None, when: Some(now()), hash: None };
        if let Some((target, _)) = &before.file {
            if step.keeps_tripwire() {
                entry.hash = system.read(target)?.as_deref().map(digest);
            }
            entry.target = Some(target.clone());
        }
        self.entries.push(entry);
        Ok(())
    }

    /// What was applied for one tweak, newest last.
    pub fn entries(&self, tweak: &str) -> Vec<&Applied> {
        self.entries.iter().filter(|entry| entry.tweak == tweak).collect()
    }

    /// Whether anything of this tweak is on record, which is what makes it something qtools
    /// can undo.
    pub fn has(&self, tweak: &str) -> bool {
        self.entries.iter().any(|entry| entry.tweak == tweak)
    }

    /// Drops one step of a tweak, once it has been undone, so an undo that stops halfway
    /// leaves on record exactly the steps that are still on the machine.
    pub fn remove(&mut self, tweak: &str, step: usize) {
        if let Some(at) = self.entries.iter().rposition(|entry| entry.tweak == tweak && entry.step == step) {
            self.entries.remove(at);
        }
    }
}

/// Seconds since 1970, or zero on a clock set before it.
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests;
