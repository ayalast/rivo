//! SideChat — persistent mini-fork side chats (Cursor-faithful scaffold).
//!
//! Scaffold behind feature flag; no tiling or draw changes yet.
//! Storage mirrors `~/.rivo/side-chats.json` + `~/.rivo/sessions/side-*.jsonl`
//! (compat: falls back to `xai_grok_config::grok_home()` when `RIVO_HOME` is unset
//! and `grok_home` is custom).
//!
//! See `docs/cursor-research.md §7` and `docs/rivo-modes-implementation.md §6`.

use agent_client_protocol as acp;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a side chat.
pub type SideChatId = String;

/// A single durable side chat attached to a parent agent.
///
/// `parent_snapshot` is **hidden** reference context (model sees, transcript hides).
/// Only `transcript` (prompt + follow-ups) renders in the side view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideChat {
    /// Stable id, e.g. `side-<uuid>`.
    pub id: SideChatId,
    /// Owning parent agent id (stringified `AgentId` or `SessionId`).
    pub parent_id: String,
    /// Hidden parent history copied at creation time (model-only, not rendered).
    #[serde(default)]
    pub parent_snapshot: Vec<acp::ContentBlock>,
    /// Optional session id for this side chat (pager-local).
    #[serde(default)]
    pub session_id: String,
    /// Visible transcript: only prompt + follow-ups (tool results included).
    #[serde(default)]
    pub transcript: Vec<acp::ContentBlock>,
    /// Number of turns taken in this side chat.
    #[serde(default)]
    pub turn_count: usize,
    /// `true` when archived via `X` (not deleted; scoped to parent+workspace).
    #[serde(default)]
    pub archived: bool,
    /// `true` when minimized (collapsed in list).
    #[serde(default)]
    pub minimized: bool,
    /// Friendly label for UI, e.g. "Side 1". Serde default for backwards compat.
    #[serde(default)]
    pub label: String,
    /// Draft input for this side chat (unsent prompt). Not rendered in transcript.
    #[serde(default)]
    pub draft: String,
    /// Transient agent linkage, not persisted.
    #[serde(skip)]
    pub agent_id: Option<crate::app::agent::AgentId>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_active: DateTime<Utc>,
}

/// Store for all side chats scoped to the pager process.
///
/// Persisted to `~/.rivo/side-chats.json` + per-chat `~/.rivo/sessions/side-*.jsonl`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SideChatStore {
    /// All chats, including archived. Ordered by creation time.
    pub chats: Vec<SideChat>,
    /// Currently active side chat id, if any.
    #[serde(default)]
    pub active_id: Option<SideChatId>,
}

impl SideChat {
    /// Create a new side chat anchored to `parent_id` with optional initial `prompt`.
    pub fn new(parent_id: impl Into<String>, prompt: Option<String>) -> Self {
        let now = Utc::now();
        let id = format!("side-{}", uuid::Uuid::new_v4());
        let session_id = format!("side-session-{}", uuid::Uuid::new_v4());
        let mut transcript = Vec::new();
        if let Some(p) = prompt {
            if !p.trim().is_empty() {
                transcript.push(acp::ContentBlock::Text(acp::TextContent::new(p)));
            }
        }
        Self {
            id,
            parent_id: parent_id.into(),
            parent_snapshot: Vec::new(),
            session_id,
            transcript,
            turn_count: 0,
            archived: false,
            minimized: false,
            label: String::new(),
            draft: String::new(),
            agent_id: None,
            created_at: now,
            last_active: now,
        }
    }

    /// Create with explicit hidden parent snapshot.
    pub fn with_snapshot(
        parent_id: impl Into<String>,
        parent_snapshot: Vec<acp::ContentBlock>,
        prompt: Option<String>,
    ) -> Self {
        let mut chat = Self::new(parent_id, prompt);
        chat.parent_snapshot = parent_snapshot;
        chat
    }

    /// Whether this side chat can create a nested side chat (always false — not nestable).
    pub fn can_create_nested(&self) -> bool {
        false
    }

    /// Touch `last_active` to now and bump turn count.
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
        self.turn_count = self.turn_count.saturating_add(1);
    }

    /// Append a follow-up message to transcript, bump turn, and toast-ready.
    /// Returns the new `turn_count`.
    pub fn append_message(&mut self, text: impl Into<String>) -> usize {
        let text = text.into();
        if !text.trim().is_empty() {
            self.transcript
                .push(acp::ContentBlock::Text(acp::TextContent::new(text)));
        }
        self.touch();
        self.turn_count
    }

    /// Friendly label for display: `label` if non-empty, else short id (last 6 chars).
    pub fn friendly_label(&self) -> String {
        if !self.label.trim().is_empty() {
            self.label.clone()
        } else if self.id.len() > 6 {
            self.id[self.id.len() - 6..].to_string()
        } else {
            self.id.clone()
        }
    }

    /// Set the friendly label.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Compute next label for a store: "Side N" where N = store.len() + 1.
    pub fn next_label(store: &SideChatStore) -> String {
        format!("Side {}", store.chats.len() + 1)
    }
}

impl SideChatStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Next friendly label for a new chat: "Side N" where N = len+1.
    pub fn next_label(&self) -> String {
        SideChat::next_label(self)
    }

    /// Create a new side chat attached to `parent_id` with optional initial `prompt`.
    ///
    /// Mirrors `SideChat::new` but inserts into the store and returns a clone
    /// of the created chat (so callers can read `id` without borrowing).
    /// Assigns `label` as "Side N" where N = chats.len()+1.
    pub fn create_side(&mut self, parent_id: impl Into<String>, prompt: Option<String>) -> SideChat {
        let mut chat = SideChat::new(parent_id, prompt);
        chat.label = self.next_label();
        let cloned = chat.clone();
        self.chats.push(chat);
        self.active_id = Some(cloned.id.clone());
        cloned
    }

    /// Create with hidden parent snapshot (Cursor-faithful hidden context).
    /// Assigns `label` as "Side N" where N = chats.len()+1.
    pub fn create_side_with_snapshot(
        &mut self,
        parent_id: impl Into<String>,
        parent_snapshot: Vec<acp::ContentBlock>,
        prompt: Option<String>,
    ) -> SideChat {
        let mut chat = SideChat::with_snapshot(parent_id, parent_snapshot, prompt);
        chat.label = self.next_label();
        let cloned = chat.clone();
        self.chats.push(chat);
        self.active_id = Some(cloned.id.clone());
        cloned
    }

    /// List all chats (including archived). Caller may filter.
    pub fn list(&self) -> &[SideChat] {
        &self.chats
    }

    /// List only active (non-archived) chats.
    pub fn list_active(&self) -> Vec<&SideChat> {
        self.chats.iter().filter(|c| !c.archived).collect()
    }

    /// List only archived chats.
    pub fn list_archived(&self) -> Vec<&SideChat> {
        self.chats.iter().filter(|c| c.archived).collect()
    }

    /// Switch active side chat to `id`. Returns `true` if found.
    pub fn switch(&mut self, id: &str) -> bool {
        if self.chats.iter().any(|c| c.id == id) {
            self.active_id = Some(id.to_string());
            if let Some(chat) = self.chats.iter_mut().find(|c| c.id == id) {
                chat.last_active = Utc::now();
            }
            true
        } else {
            false
        }
    }

    /// Get active side chat, if any.
    pub fn active(&self) -> Option<&SideChat> {
        let id = self.active_id.as_ref()?;
        self.chats.iter().find(|c| &c.id == id)
    }

    /// Mutable active side chat.
    pub fn active_mut(&mut self) -> Option<&mut SideChat> {
        let id = self.active_id.clone()?;
        self.chats.iter_mut().find(|c| c.id == id)
    }

    /// Get side chat by id.
    pub fn get(&self, id: &str) -> Option<&SideChat> {
        self.chats.iter().find(|c| c.id == id)
    }

    /// Get mutable side chat by id.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut SideChat> {
        self.chats.iter_mut().find(|c| c.id == id)
    }

    /// Close (archive) a side chat. `X` archives, not deletes.
    pub fn close(&mut self, id: &str) -> bool {
        if let Some(chat) = self.chats.iter_mut().find(|c| c.id == id) {
            chat.archived = true;
            chat.last_active = Utc::now();
            if self.active_id.as_deref() == Some(id) {
                self.active_id = None;
            }
            true
        } else {
            false
        }
    }

    /// Re-open an archived side chat.
    pub fn open(&mut self, id: &str) -> bool {
        if let Some(chat) = self.chats.iter_mut().find(|c| c.id == id) {
            if chat.archived {
                chat.archived = false;
                chat.last_active = Utc::now();
                self.active_id = Some(id.to_string());
                return true;
            }
        }
        false
    }

    /// Number of side chats (including archived).
    pub fn len(&self) -> usize {
        self.chats.len()
    }

    /// Whether store is empty.
    pub fn is_empty(&self) -> bool {
        self.chats.is_empty()
    }
}

pub mod persist;

#[cfg(test)]
mod tests {
    use super::{SideChat, SideChatStore};

    #[test]
    fn create_side_generates_ids() {
        let mut store = SideChatStore::new();
        let chat = store.create_side("parent-1", Some("hello".to_string()));
        assert!(!chat.id.is_empty());
        assert!(!chat.session_id.is_empty());
        assert_eq!(chat.parent_id, "parent-1");
        assert_eq!(store.len(), 1);
        assert_eq!(store.active().unwrap().id, chat.id);
    }

    #[test]
    fn switch_updates_active() {
        let mut store = SideChatStore::new();
        let a = store.create_side("p1", None);
        let b = store.create_side("p1", None);
        assert_eq!(store.active().unwrap().id, b.id);
        assert!(store.switch(&a.id));
        assert_eq!(store.active().unwrap().id, a.id);
        assert!(!store.switch("nonexistent"));
    }

    #[test]
    fn close_archives_and_clears_active() {
        let mut store = SideChatStore::new();
        let chat = store.create_side("p1", None);
        assert!(store.close(&chat.id));
        assert!(store.get(&chat.id).unwrap().archived);
        assert!(store.active().is_none());
        assert_eq!(store.list_active().len(), 0);
        assert_eq!(store.list_archived().len(), 1);
    }

    #[test]
    fn open_unarchives() {
        let mut store = SideChatStore::new();
        let chat = store.create_side("p1", None);
        store.close(&chat.id);
        assert!(store.open(&chat.id));
        assert!(!store.get(&chat.id).unwrap().archived);
        assert_eq!(store.active().unwrap().id, chat.id);
    }

    #[test]
    fn side_chat_not_nestable() {
        let chat = SideChat::new("p1", None);
        assert!(!chat.can_create_nested());
    }

    #[test]
    fn list_filters_archived() {
        let mut store = SideChatStore::new();
        let a = store.create_side("p1", None);
        let _b = store.create_side("p1", None);
        store.close(&a.id);
        assert_eq!(store.list().len(), 2);
        assert_eq!(store.list_active().len(), 1);
        assert_eq!(store.list_archived().len(), 1);
    }
}
