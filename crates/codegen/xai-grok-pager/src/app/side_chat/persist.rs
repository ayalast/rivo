//! Persistence for `SideChatStore`.
//!
//! Index: `~/.rivo/side-chats.json` (or `rivo_home()/side-chats.json`).
//! Per-chat transcripts: `~/.rivo/sessions/side-<id>.jsonl`.
//!
//! Compat helper: `rivo_home()` prefers `RIVO_HOME` / `GROK_HOME` / `~/.rivo` / `~/.grok`
//! (in that order) and keeps store compat with `grok_home()` when customized.
//! Tests use a temp dir via `set_rivo_home_for_tests`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) static RIVO_HOME_OVERRIDE: OnceLock<Option<PathBuf>> = OnceLock::new();

pub(crate) fn rivo_home_override() -> Option<PathBuf> {
    RIVO_HOME_OVERRIDE.get().and_then(|v| v.clone())
}

/// Test-only override for `rivo_home()` (mirrors `subagent::set_replay_grok_home_for_tests`).
#[cfg(any(test, feature = "test-support"))]
pub fn set_rivo_home_for_tests(home: Option<PathBuf>) {
    let _ = RIVO_HOME_OVERRIDE.set(home);
}

/// Resolve the rivo home directory.
///
/// Precedence: `RIVO_HOME` env → `GROK_HOME` env → `~/.rivo` (fallback to `grok_home()`).
pub fn rivo_home() -> PathBuf {
    if let Some(overridden) = rivo_home_override() {
        return overridden;
    }
    if let Ok(v) = std::env::var("RIVO_HOME") {
        let p = PathBuf::from(v);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    if let Ok(v) = std::env::var("GROK_HOME") {
        let p = PathBuf::from(v);
        // When GROK_HOME is custom, keep side-chats co-located unless RIVO_HOME is set.
        // Use a `rivo` subdir to avoid stomping grok files, but tests pin via `GROK_HOME` anyway.
        // For prod compat the spec says "keep compat with grok home for now" — so use grok_home directly.
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    // Default: try xai_grok_config::grok_home() but prefer .rivo path when it exists.
    // Spec says to use `xai_grok_config::grok_home()` or new `rivo_home` helper, keep compat.
    let grok_home = xai_grok_config::grok_home();
    // If GROK_HOME was default `~/.grok`, prefer `~/.rivo` for new installs but read grok compat.
    if grok_home.ends_with(".grok") {
        if let Some(parent) = grok_home.parent() {
            let rivo = parent.join(".rivo");
            if rivo.exists() {
                return rivo;
            }
            // Also accept env-less default as .rivo for new installs; create lazily.
            // Keep reading .grok for migration but writing to .rivo.
            // For scaffold, just use .rivo when default.
            return rivo;
        }
    }
    grok_home
}

/// Path for the side-chats index file.
pub fn side_chats_index_path() -> PathBuf {
    rivo_home().join("side-chats.json")
}

/// Path for a per-chat transcript file.
pub fn side_chat_transcript_path(side_id: &str) -> PathBuf {
    rivo_home()
        .join("sessions")
        .join(format!("side-{side_id}.jsonl"))
}

/// Compatibility path under grok_home (for migration reads).
pub fn grok_side_chats_index_path() -> PathBuf {
    xai_grok_config::grok_home().join("side-chats.json")
}

/// Load the `SideChatStore` from disk. Returns empty store if file missing or invalid.
pub fn load_store() -> super::SideChatStore {
    load_store_at(&side_chats_index_path())
}

fn load_store_at(path: &Path) -> super::SideChatStore {
    let data = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Try grok compat path
            let grok_path = grok_side_chats_index_path();
            if grok_path != path {
                if let Ok(s) = std::fs::read_to_string(&grok_path) {
                    return parse_store(&s);
                }
            }
            return super::SideChatStore::default();
        }
        Err(_) => return super::SideChatStore::default(),
    };
    parse_store(&data)
}

fn parse_store(data: &str) -> super::SideChatStore {
    serde_json::from_str(data).unwrap_or_default()
}

/// Persist the `SideChatStore` to disk (atomic write via temp file + rename).
pub fn save_store(store: &super::SideChatStore) -> std::io::Result<()> {
    save_store_at(store, &side_chats_index_path())
}

fn save_store_at(store: &super::SideChatStore, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data =
        serde_json::to_string_pretty(store).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    // Atomic write
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Append a JSON line to a side chat's transcript file.
pub fn append_transcript_line(side_id: &str, line: &serde_json::Value) -> std::io::Result<()> {
    let path = side_chat_transcript_path(side_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", serde_json::to_string(line).unwrap_or_default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_store_at, save_store_at};
    use crate::app::side_chat::SideChatStore;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_via_temp_path() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("side-chats.json");
        let mut store = SideChatStore::new();
        let chat = store.create_side("parent-1", Some("hello".to_string()));
        save_store_at(&store, &path).unwrap();
        let loaded = load_store_at(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get(&chat.id).unwrap().parent_id, "parent-1");
    }

    #[test]
    fn missing_file_yields_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let store = load_store_at(&path);
        assert!(store.is_empty());
    }
}
