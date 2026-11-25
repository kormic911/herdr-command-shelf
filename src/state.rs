use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub title: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub pane: String,
    pub terminal: String,
}

#[derive(Clone)]
pub struct Store(pub PathBuf);

pub struct ShelfLease(File);

impl Drop for ShelfLease {
    fn drop(&mut self) {
        // A subprocess can briefly inherit the descriptor before exec closes it.
        let _ = FileExt::unlock(&self.0);
    }
}

impl Store {
    pub fn new(root: &Path, terminal: &str) -> Result<Self> {
        anyhow::ensure!(
            !terminal.is_empty()
                && terminal
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "Invalid terminal identity"
        );
        Ok(Self(root.join(terminal)))
    }
    pub fn tab(root: &Path, tab: &str) -> Result<Self> {
        use sha2::{Digest, Sha256};
        anyhow::ensure!(
            !tab.is_empty()
                && tab.len() <= 100
                && tab.bytes().all(|c| c.is_ascii_alphanumeric() || c == b':'),
            "Invalid tab identity"
        );
        let socket = std::env::var_os("HERDR_SOCKET_PATH").unwrap_or_default();
        let session = format!("{:x}", Sha256::digest(socket.as_encoded_bytes()));
        let key: String = tab.bytes().map(|b| format!("{b:02x}")).collect();
        Ok(Self(root.join("tabs").join(session).join(key)))
    }
    pub fn launch_lock(&self) -> Result<File> {
        let file = self.lock("launch.lock")?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }
    pub fn migrate(&self, sources: &[Store]) -> Result<()> {
        // commands.json commits the import. Even an empty list is authoritative.
        if self.0.join("commands.json").try_exists()? {
            return Ok(());
        }
        let mut entries = Vec::new();
        let mut target = None;
        for source in sources {
            entries.extend(source.entries()?);
            if target.is_none() {
                target = source.target()?;
            }
        }
        if let Some(target) = target {
            self.bind(&target)?;
        }
        self.write("commands.json", &entries)
    }
    fn lock(&self, name: &str) -> Result<File> {
        fs::create_dir_all(&self.0)?;
        Ok(OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.0.join(name))?)
    }
    pub fn claim(&self) -> Result<ShelfLease> {
        let f = self.lock("live")?;
        FileExt::try_lock_exclusive(&f).context("Shelf already open")?;
        Ok(ShelfLease(f))
    }
    pub fn live(&self) -> Result<bool> {
        let f = match File::open(self.0.join("live")) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        match FileExt::try_lock_shared(&f) {
            Ok(()) => {
                FileExt::unlock(&f)?;
                Ok(false)
            }
            Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => Ok(true),
            Err(e) => Err(e.into()),
        }
    }
    fn read<T: serde::de::DeserializeOwned + Default>(&self, name: &str) -> Result<T> {
        match File::open(self.0.join(name)) {
            Ok(f) => Ok(serde_json::from_reader(f)?),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn write(&self, name: &str, value: &impl Serialize) -> Result<()> {
        fs::create_dir_all(&self.0)?;
        let mut f = NamedTempFile::new_in(&self.0)?;
        serde_json::to_writer_pretty(&mut f, value)?;
        f.flush()?;
        f.as_file().sync_all()?;
        f.persist(self.0.join(name))?;
        Ok(())
    }
    pub fn entries(&self) -> Result<Vec<Entry>> {
        self.read("commands.json")
    }
    pub fn target(&self) -> Result<Option<Target>> {
        self.read("target.json")
    }
    pub fn bind(&self, t: &Target) -> Result<()> {
        self.write("target.json", t)
    }
    pub fn delete(&self, i: usize, original: &Entry) -> Result<()> {
        let lock = self.lock("edit.lock")?;
        FileExt::lock_exclusive(&lock)?;
        let mut entries = self.entries()?;
        anyhow::ensure!(
            entries.get(i) == Some(original),
            "The command changed. Select it again."
        );
        entries.remove(i);
        self.write("commands.json", &entries)
    }
    pub fn save(&self, edit: Option<(usize, &Entry)>, entry: Entry) -> Result<()> {
        anyhow::ensure!(!entry.command.trim().is_empty(), "Enter a command");
        let f = self.lock("edit.lock")?;
        FileExt::lock_exclusive(&f)?;
        let mut entries = self.entries()?;
        if let Some((i, original)) = edit {
            if entries.get(i) != Some(original) {
                bail!("This command changed. Reopen the editor.");
            }
            entries[i] = entry;
        } else {
            entries.push(entry);
        }
        self.write("commands.json", &entries)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn ending_lease_releases_lock_with_a_duplicate_descriptor() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = Store::new(root.path(), "term_1")?;
        let lease = store.claim()?;
        let duplicate = lease.0.try_clone()?;
        drop(lease);
        assert!(!store.live()?);
        let _next = store.claim()?;
        drop(duplicate);
        assert!(store.live()?);
        Ok(())
    }
}
