//! Autosave and crash recovery.
//!
//! Each design being edited keeps its own entry in the recovery folder:
//! `<id>.pcbcraft` (the design as of the last autosave), `<id>.json` (its file
//! path, name and time) and `<id>.lock`, which the running app holds locked. An
//! autosave is written only when the design has unsaved changes that are not in
//! the entry yet, and it goes through the same atomic write as a normal save,
//! so a half-written snapshot is never offered. An entry whose lock nobody
//! holds belongs to an app that died: the next launch offers to reopen it, and
//! a clean close that saved its work leaves nothing behind.
//!
//! The folder sits beside the recent-designs list, so both use the same per-user
//! configuration base, and both can be pointed elsewhere by a test through the
//! override below.
use crate::Session;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Overrides the recovery folder; tests set it so they never touch the real
/// one. Guard writes with [`RECOVERY_TEST_LOCK`] like the recents list does.
pub static RECOVERY_PATH_TEST: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Tests share one folder, so they take turns.
pub static RECOVERY_TEST_LOCK: Mutex<()> = Mutex::new(());

/// The folder a test pointed [`RECOVERY_PATH_TEST`] at, if it set one.
pub fn override_dir() -> Option<PathBuf> {
    RECOVERY_PATH_TEST
        .lock()
        .ok()
        .and_then(|locked| locked.as_ref().cloned())
}

/// The recovery folder beside the recent-designs list.
pub fn default_dir() -> Result<PathBuf, String> {
    if let Some(path) = override_dir() {
        return Ok(path);
    }
    let directory = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    directory("XDG_CONFIG_HOME")
        .or_else(|| directory("APPDATA"))
        .or_else(|| directory("HOME").map(|home| home.join(".config")))
        .map(|base| base.join("pcbcraft").join("recovery"))
        .ok_or_else(|| "no user configuration folder was found".into())
}

/// What an entry's `<id>.json` records about the snapshot beside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The snapshot's file stem, unique per design.
    pub id: String,
    /// The design's file, when it had been saved somewhere.
    #[serde(default)]
    pub path: Option<String>,
    /// The design name the prompt shows for this snapshot.
    pub name: String,
    /// When the snapshot was written, in seconds since 1970.
    pub saved_at: u64,
    /// The revision the snapshot holds.
    pub revision: u64,
    /// How much is in it, for the sentence the prompt shows.
    #[serde(default)]
    pub components: usize,
    /// Placed pads in the snapshot, for the sentence the prompt shows.
    #[serde(default)]
    pub pads: usize,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// An id names files in the folder, so it stays short and free of separators.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn file(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// Snapshots whose app is no longer running, newest first.
pub fn orphans(dir: &Path) -> Vec<Entry> {
    let mut found: Vec<Entry> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                return None;
            }
            let entry: Entry = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
            if !valid_id(&entry.id) || !file(dir, &entry.id, "pcbcraft").is_file() {
                return None;
            }
            // A lock file somebody holds means the app that wrote this is still
            // running: not an orphan.
            if let Ok(lock) = File::open(file(dir, &entry.id, "lock"))
                && lock.try_lock().is_err()
            {
                return None;
            }
            Some(entry)
        })
        .take(1000)
        .collect();
    found.sort_by_key(|e| std::cmp::Reverse(e.saved_at));
    found
}

/// Read an entry's design and what its record says about it.
pub fn load(dir: &Path, id: &str) -> Result<(pcbcraft_doc::Project, Entry), String> {
    if !valid_id(id) {
        return Err(format!("no recovered design `{id}`"));
    }
    let entry: Entry = fs::read(file(dir, id, "json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| format!("no recovered design `{id}`"))?;
    if entry.id != id {
        return Err(format!("`{id}.json` describes another design"));
    }
    let bytes = pcbcraft_io::read_bytes(&file(dir, id, "pcbcraft"))
        .map_err(|e| format!("recovered design `{id}`: {e}"))?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| format!("recovered design `{id}` is not text"))?;
    Ok((pcbcraft_io::read_project(text)?, entry))
}

/// Delete an entry. Only a real one: `<id>.json` must describe entry `id` and
/// its snapshot must be there, so a folder and id that merely name someone's
/// design file delete nothing.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let described = fs::read(file(dir, id, "json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Entry>(&bytes).ok())
        .is_some_and(|entry| entry.id == id);
    if !valid_id(id) || !described || !file(dir, id, "pcbcraft").is_file() {
        return Err(format!("no recovered design `{id}`"));
    }
    for ext in ["json", "pcbcraft", "lock"] {
        let _ = fs::remove_file(file(dir, id, ext));
    }
    Ok(())
}

/// One design's autosaver, held while that design is open. Dropping it releases
/// the lock, which is what makes its entry look crashed to the next launch.
pub struct Autosaver {
    dir: PathBuf,
    id: String,
    lock: Option<File>,
    /// The revision the entry holds, when it holds one.
    last: Option<u64>,
    has_entry: bool,
}

impl Autosaver {
    /// Start autosaving `dir` (created if needed), holding this design's lock.
    pub fn new(dir: &Path) -> Result<Autosaver, String> {
        let io = |e: std::io::Error| format!("{}: {e}", dir.display());
        fs::create_dir_all(dir).map_err(io)?;
        // Locks of apps that ended with nothing to recover.
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) == Some("lock")
                && !path.with_extension("json").exists()
                && File::open(&path).is_ok_and(|lock| lock.try_lock().is_ok())
            {
                let _ = fs::remove_file(&path);
            }
        }
        // Unique within the process as well as across processes: an app started
        // again in the same second must not take over the crashed app's entry.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!("{}-{}-{sequence}", unix_now(), std::process::id());
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(file(dir, &id, "lock"))
            .map_err(io)?;
        lock.lock().map_err(io)?;
        Ok(Autosaver {
            dir: dir.to_path_buf(),
            id,
            lock: Some(lock),
            last: None,
            has_entry: false,
        })
    }

    /// This design's entry id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The recovery folder this autosaver writes into.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Autosave if there is anything new to write. `saved` is the revision the
    /// design's file holds (`None` when it has no file yet); a design whose
    /// revision is saved has nothing to recover and loses its entry.
    ///
    /// Returns whether a snapshot was written. Each snapshot is the whole
    /// design at one revision, so a killed process loses at most the edits
    /// after the last one.
    pub fn tick(&mut self, s: &Session, saved: Option<u64>) -> Result<bool, String> {
        let dirty = s.revision != 0 && saved != Some(s.revision);
        if !dirty {
            self.drop_entry();
            self.last = Some(s.revision);
            return Ok(false);
        }
        if self.last == Some(s.revision) {
            return Ok(false);
        }
        self.write(s)?;
        Ok(true)
    }

    /// Autosave now, whether or not the entry is already at this revision.
    pub fn save(&mut self, s: &Session) -> Result<(), String> {
        self.write(s)
    }

    fn write(&mut self, s: &Session) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&s.project).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > pcbcraft_io::MAX_FILE_BYTES {
            return Err("this design is too large to autosave".into());
        }
        pcbcraft_io::atomic_write(&file(&self.dir, &self.id, "pcbcraft"), &bytes)?;
        let entry = Entry {
            id: self.id.clone(),
            path: None,
            name: s.project.name.clone(),
            saved_at: unix_now(),
            revision: s.revision,
            components: s.project.components.len(),
            pads: s
                .project
                .board
                .footprints
                .iter()
                .map(|f| f.pads.len())
                .sum(),
        };
        let meta = serde_json::to_vec_pretty(&entry).map_err(|e| e.to_string())?;
        pcbcraft_io::atomic_write(&file(&self.dir, &self.id, "json"), &meta)?;
        self.last = Some(s.revision);
        self.has_entry = true;
        Ok(())
    }

    fn drop_entry(&mut self) {
        if self.has_entry {
            for ext in ["json", "pcbcraft"] {
                let _ = fs::remove_file(file(&self.dir, &self.id, ext));
            }
            self.has_entry = false;
        }
    }

    /// The design was closed after saving, or the user chose to discard the
    /// snapshot: nothing is left to recover.
    pub fn discard(mut self) {
        self.drop_entry();
        self.lock = None;
        let _ = fs::remove_file(file(&self.dir, &self.id, "lock"));
    }

    /// Closing the app: with something unsaved the entry stays for the next
    /// launch to offer, and the lock is released either way.
    pub fn close(mut self, s: &Session, saved: Option<u64>) {
        let _ = self.tick(s, saved);
        if !self.has_entry {
            let _ = fs::remove_file(file(&self.dir, &self.id, "lock"));
        }
        self.lock = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dir(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("pcbcraft-recovery-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn add_track(s: &mut Session) -> Result<(), String> {
        let net = s
            .execute(
                "net.create",
                json!({"name":format!("SIG{}", s.revision)}),
                None,
            )
            .map_err(|e| e.to_string())?
            .pointer("/result/id")
            .and_then(serde_json::Value::as_u64)
            .ok_or("a new net has an id")?;
        let x = 10.0 + s.revision as f64;
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":x,"y":10},"end":{"x":x + 5.0,"y":10},"width":0.3,"layer":"F.Cu"}),
            None,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    #[test]
    fn recovery_offers_the_newest_snapshot_after_a_crash() -> Result<(), String> {
        let d = dir("crash");
        let mut s = Session::default();
        add_track(&mut s)?;
        let mut a = Autosaver::new(&d)?;
        a.tick(&s, None)?;
        let older = a.id().to_owned();
        // Nothing is offered while this app holds its lock.
        assert!(orphans(&d).is_empty());
        drop(a);

        let mut t = Session::default();
        add_track(&mut t)?;
        let mut b = Autosaver::new(&d)?;
        b.tick(&t, None)?;
        let newer = b.id().to_owned();
        drop(b);

        let offered = orphans(&d);
        assert_eq!(offered.len(), 2, "{offered:?}");
        assert_eq!(offered[0].id, newer, "newest first");
        let (project, entry) = load(&d, &newer)?;
        assert_eq!(project, t.project);
        assert_eq!(entry.revision, t.revision);
        // Accepting one leaves the other to be dealt with.
        remove(&d, &newer)?;
        assert_eq!(orphans(&d).len(), 1);
        assert!(load(&d, &newer).is_err());
        assert!(load(&d, &older).is_ok());
        remove(&d, &older)?;
        assert!(orphans(&d).is_empty());
        let _ = fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn recovery_ignores_a_snapshot_whose_app_is_still_running() -> Result<(), String> {
        let d = dir("running");
        let mut s = Session::default();
        add_track(&mut s)?;
        let mut a = Autosaver::new(&d)?;
        a.tick(&s, None)?;
        assert!(file(&d, a.id(), "pcbcraft").is_file());
        assert!(orphans(&d).is_empty(), "a live app's entry is not offered");
        a.discard();
        let _ = fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn autosave_skips_when_nothing_changed() -> Result<(), String> {
        let d = dir("unchanged");
        let mut s = Session::default();
        let mut a = Autosaver::new(&d)?;
        assert!(!a.tick(&s, None)?, "a fresh design has nothing to recover");
        assert!(!a.tick(&s, Some(0))?);
        add_track(&mut s)?;
        assert!(a.tick(&s, None)?, "the first edit is written");
        let before = fs::read(file(&d, a.id(), "json")).map_err(|e| e.to_string())?;
        assert!(!a.tick(&s, None)?, "the same revision is not written twice");
        assert_eq!(
            fs::read(file(&d, a.id(), "json")).map_err(|e| e.to_string())?,
            before
        );
        let _ = fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn autosave_loses_at_most_one_revision() -> Result<(), String> {
        let d = dir("revision");
        let mut s = Session::default();
        let mut a = Autosaver::new(&d)?;
        for _ in 0..3 {
            add_track(&mut s)?;
            let written = s.revision;
            assert!(a.tick(&s, None)?);
            let (_, entry) = load(&d, a.id())?;
            assert_eq!(entry.revision, written, "every revision is on disk");
        }
        // One more edit that no tick saw: the entry is exactly one behind.
        let net = s.project.nets[0].id;
        s.execute("net.rename", json!({"id":net,"name":"RENAMED"}), None)
            .map_err(|e| e.to_string())?;
        let (project, entry) = load(&d, a.id())?;
        assert_eq!(
            s.revision - entry.revision,
            1,
            "one edit is all a kill costs"
        );
        assert_ne!(project.nets[0].name, s.project.nets[0].name);
        let _ = fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn a_clean_close_leaves_no_recovery_entry() -> Result<(), String> {
        let d = dir("clean");
        let mut s = Session::default();
        add_track(&mut s)?;
        let mut a = Autosaver::new(&d)?;
        a.tick(&s, None)?;
        // Saving the design makes the entry pointless: the next tick drops it.
        let saved = s.revision;
        assert!(!a.tick(&s, Some(saved))?);
        assert!(!file(&d, a.id(), "json").exists());
        assert!(!file(&d, a.id(), "pcbcraft").exists());
        a.close(&s, Some(saved));
        assert_eq!(
            fs::read_dir(&d).map_err(|e| e.to_string())?.count(),
            0,
            "nothing left to recover"
        );
        // Closing with unsaved changes keeps them for the next launch.
        let b = Autosaver::new(&d)?;
        add_track(&mut s)?;
        b.close(&s, Some(saved));
        assert_eq!(orphans(&d).len(), 1);
        let _ = fs::remove_dir_all(&d);
        Ok(())
    }

    #[test]
    fn removing_refuses_what_is_not_a_recovery_entry() -> Result<(), String> {
        let d = dir("refuse");
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        let part = d.join("part.pcbcraft");
        fs::write(&part, b"{}").map_err(|e| e.to_string())?;
        assert!(remove(&d, "part").is_err(), "no record describes it");
        assert!(remove(&d, "../part").is_err());
        assert!(part.exists());
        let entry = Entry {
            id: "other".into(),
            path: None,
            name: "x".into(),
            saved_at: 1,
            revision: 1,
            components: 0,
            pads: 0,
        };
        fs::write(
            d.join("part.json"),
            serde_json::to_vec(&entry).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        assert!(remove(&d, "part").is_err(), "the record names another id");
        assert!(part.exists() && d.join("part.json").exists());
        let _ = fs::remove_dir_all(&d);
        Ok(())
    }
}
