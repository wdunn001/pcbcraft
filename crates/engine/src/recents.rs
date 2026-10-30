//! The designs this computer opened or saved most recently. This is host
//! knowledge rather than design content, so it lives beside the library
//! source list and is offered to the interface and to agents for context.
use std::path::PathBuf;
use std::sync::Mutex;

/// A short list reads faster than a long one, and eight covers the handful
/// of boards a person cycles through while working on one project.
const MAX_RECENT: usize = 8;
pub static RECENTS_PATH_TEST: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Tests share the one override file, so they take turns.
pub static RECENTS_TEST_LOCK: Mutex<()> = Mutex::new(());

fn recents_path() -> Result<PathBuf, String> {
    if let Some(path) = RECENTS_PATH_TEST
        .lock()
        .ok()
        .and_then(|locked| locked.as_ref().cloned())
    {
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
        .map(|base| base.join("pcbcraft").join("recent-designs.json"))
        .ok_or_else(|| "no user configuration folder was found".into())
}
/// The designs to offer at a click, newest first.
pub fn load_recent_paths() -> Vec<String> {
    let Ok(path) = recents_path() else {
        return vec![];
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| !path.trim().is_empty() && path.chars().count() <= 1024)
        .take(MAX_RECENT)
        .collect()
}
/// Remember that a design was opened or saved: it moves to the front, and a
/// path that was already listed is not repeated further down.
pub fn record_recent_path(path: &str) {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 1024 {
        return;
    }
    let mut list = load_recent_paths();
    list.retain(|seen| seen != trimmed);
    list.insert(0, trimmed.to_owned());
    list.truncate(MAX_RECENT);
    write(&list);
}
/// Take one design off the list, as the Files page does for Remove from list.
/// The file itself is never touched.
pub fn forget_recent_path(path: &str) {
    let trimmed = path.trim();
    let mut list = load_recent_paths();
    let before = list.len();
    list.retain(|seen| seen != trimmed);
    if list.len() != before {
        write(&list);
    }
}
/// Forget every remembered design.
pub fn clear_recent_paths() {
    write(&[]);
}
fn write(list: &[String]) {
    let Ok(path) = recents_path() else {
        return;
    };
    if let Some(directory) = path.parent() {
        let _ = std::fs::create_dir_all(directory);
    }
    if let Ok(text) = serde_json::to_string_pretty(list) {
        let _ = std::fs::write(path, text);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_designs_stay_newest_first_short_and_free_of_duplicates() -> Result<(), String> {
        let _turn = RECENTS_TEST_LOCK.lock().map_err(|_| "lock poisoned")?;
        let directory =
            std::env::temp_dir().join(format!("pcbcraft-recents-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "lock poisoned")?
            .replace(directory.join("recent-designs.json"));
        clear_recent_paths();
        for index in 0..12 {
            record_recent_path(&format!("/designs/board{index}.board.json"));
        }
        let list = load_recent_paths();
        assert_eq!(list.len(), MAX_RECENT, "the list stays short");
        assert_eq!(
            list[0], "/designs/board11.board.json",
            "the most recent design leads"
        );
        record_recent_path("/designs/board5.board.json");
        let list = load_recent_paths();
        assert_eq!(
            list[0], "/designs/board5.board.json",
            "reopening moves it up"
        );
        assert_eq!(
            list.iter()
                .filter(|seen| *seen == "/designs/board5.board.json")
                .count(),
            1,
            "a design is never listed twice"
        );
        record_recent_path("   ");
        record_recent_path("/designs/board5.board.json");
        assert_eq!(
            load_recent_paths(),
            list,
            "blank names and no-op repeats change nothing"
        );
        forget_recent_path("/designs/board5.board.json");
        let after_forget = load_recent_paths();
        assert!(
            !after_forget.iter().any(|seen| seen == "/designs/board5.board.json"),
            "forget removes one path"
        );
        assert_eq!(
            after_forget.len(),
            list.len() - 1,
            "forget only drops the matching entry"
        );
        forget_recent_path("/designs/board5.board.json");
        assert_eq!(
            load_recent_paths(),
            after_forget,
            "forgetting again is a no-op"
        );
        clear_recent_paths();
        assert!(load_recent_paths().is_empty(), "clearing empties the list");
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }
}
