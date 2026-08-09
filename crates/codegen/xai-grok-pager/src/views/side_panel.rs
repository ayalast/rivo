//! Side-panel presentation state for durable `/side` conversations.
//!
//! A side chat is a durable conversation in `SideChatStore`; this type owns
//! only the *ephemeral presentation*: which tabs are open in this process,
//! which one is selected, whether keyboard input targets the panel, and the
//! user-selected main/panel split.  Keeping this separate prevents stale pane
//! state from reopening side chats after an application restart.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::{Deserialize, Serialize};

/// Default share of the conversation area reserved for the parent chat.
pub const DEFAULT_MAIN_RATIO: u16 = 65;
/// Height of the lightweight tab strip above the selected side conversation.
pub const SIDE_TAB_BAR_HEIGHT: u16 = 1;
/// Smallest usable width of either side of the split, in terminal cells.
pub const MIN_PANEL_WIDTH: u16 = 24;
/// Terminals narrower than this use the parent-only fallback.
pub const MIN_SPLIT_WIDTH: u16 = MIN_PANEL_WIDTH * 2;

/// Persisted user interface preference.  Open tabs intentionally do not live
/// here: an app restart must recover only the parent presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidePanelPreferences {
    /// Percentage of the content area occupied by the parent conversation.
    #[serde(default = "default_main_ratio")]
    pub main_ratio: u16,
}

const fn default_main_ratio() -> u16 {
    DEFAULT_MAIN_RATIO
}

impl Default for SidePanelPreferences {
    fn default() -> Self {
        Self {
            main_ratio: DEFAULT_MAIN_RATIO,
        }
    }
}

impl SidePanelPreferences {
    /// Clamp a percentage to the documented usable range.
    pub fn set_main_ratio(&mut self, ratio: u16) {
        self.main_ratio = ratio.clamp(20, 80);
    }

    /// Derive a legal percentage from an absolute mouse column.
    pub fn set_from_divider(&mut self, area: Rect, column: u16) {
        if area.width == 0 {
            return;
        }
        let relative = column.saturating_sub(area.x).min(area.width);
        let min_left = MIN_PANEL_WIDTH.min(area.width / 2);
        let min_right = MIN_PANEL_WIDTH.min(area.width / 2);
        let lower = min_left;
        let upper = area.width.saturating_sub(min_right).max(lower);
        let clamped = relative.clamp(lower, upper);
        let ratio = (u32::from(clamped) * 100 / u32::from(area.width)) as u16;
        self.set_main_ratio(ratio);
    }

    /// Returns parent and side areas separated by a one-cell divider.
    pub fn split(&self, area: Rect) -> Option<(Rect, Rect, Rect)> {
        if area.width < MIN_SPLIT_WIDTH || area.height < 4 {
            return None;
        }
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(u32::from(self.main_ratio), 100),
                Constraint::Length(1),
                Constraint::Ratio(u32::from(100 - self.main_ratio), 100),
            ])
            .split(area);
        Some((parts[0], parts[1], parts[2]))
    }
}

/// Process-local side panel state.  It deliberately has no serde derive.
#[derive(Debug, Clone, Default)]
pub struct SidePanelLayout {
    /// Ordered ids of durable chats rendered as tabs in this process.
    pub open_tabs: Vec<String>,
    /// Current tab in `open_tabs`.
    pub selected_tab: Option<String>,
    /// Whether typing/scrolling targets the selected side chat.
    pub focused: bool,
    /// A main/panel divider drag is in progress.
    pub dragging: bool,
    /// Whether the panel is drawn at all. Tabs may still be open while the
    /// panel is temporarily hidden (`/window hide`); `show()` restores it.
    pub visible: bool,
}

impl SidePanelLayout {
    /// Whether the panel should occupy a split column right now.
    pub fn is_open(&self) -> bool {
        self.visible && !self.open_tabs.is_empty()
    }

    /// Temporarily collapse the panel without archiving its tabs.
    pub fn hide(&mut self) {
        self.visible = false;
        self.focused = false;
        self.dragging = false;
    }

    /// Re-show the panel after a temporary hide.
    pub fn show(&mut self) {
        if !self.open_tabs.is_empty() {
            self.visible = true;
        }
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_tab.as_deref()
    }

    /// Reset only process-local presentation after startup. Durable metadata
    /// remains available through `SideChatStore` for intentional recovery.
    pub fn clear_for_startup(&mut self) {
        self.open_tabs.clear();
        self.selected_tab = None;
        self.focused = false;
        self.dragging = false;
        self.visible = false;
    }

    /// Open and select one tab, preserving existing tab order.
    pub fn open(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.open_tabs.iter().any(|tab| tab == &id) {
            self.open_tabs.push(id.clone());
        }
        self.selected_tab = Some(id);
        self.focused = true;
    }

    /// Select a visible tab.
    pub fn select(&mut self, id: &str) -> bool {
        if self.open_tabs.iter().any(|tab| tab == id) {
            self.selected_tab = Some(id.to_owned());
            self.focused = true;
            true
        } else {
            false
        }
    }

    /// Close a tab and select its closest surviving neighbour.
    pub fn close(&mut self, id: &str) -> bool {
        let Some(index) = self.open_tabs.iter().position(|tab| tab == id) else {
            return false;
        };
        self.open_tabs.remove(index);
        if self.selected_tab.as_deref() == Some(id) {
            self.selected_tab = self
                .open_tabs
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|i| self.open_tabs.get(i)))
                .cloned();
        }
        if self.open_tabs.is_empty() {
            self.focused = false;
            self.selected_tab = None;
        }
        true
    }

    /// Cycle within side tabs only.  The caller handles main ↔ side focus.
    pub fn cycle_tab(&mut self, backwards: bool) -> Option<String> {
        let selected = self.selected_tab.as_ref()?;
        let index = self.open_tabs.iter().position(|tab| tab == selected)?;
        let len = self.open_tabs.len();
        if len == 0 {
            return None;
        }
        let next = if backwards {
            (index + len - 1) % len
        } else {
            (index + 1) % len
        };
        let id = self.open_tabs[next].clone();
        self.selected_tab = Some(id.clone());
        self.focused = true;
        Some(id)
    }

    /// Remove missing or archived entries after loading durable metadata.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.open_tabs.retain(|id| keep(id));
        if self
            .selected_tab
            .as_deref()
            .is_some_and(|id| !self.open_tabs.iter().any(|tab| tab == id))
        {
            self.selected_tab = self.open_tabs.last().cloned();
        }
        if self.open_tabs.is_empty() {
            self.focused = false;
        }
    }

    /// Drop tabs whose durable record is archived or gone. Call after loading
    /// the store so stale presentation never outlives its conversation.
    pub fn normalize(&mut self, store: &crate::app::side_chat::SideChatStore) {
        self.retain(|id| store.get(id).is_some_and(|chat| !chat.archived));
        if self.open_tabs.is_empty() {
            self.selected_tab = None;
            self.focused = false;
        }
    }

    /// The `AgentId` behind the selected tab, when its `AgentView` is alive.
    pub fn selected_agent_id(
        &self,
        store: &crate::app::side_chat::SideChatStore,
    ) -> Option<crate::app::agent::AgentId> {
        let id = self.selected_id()?;
        store.get(id).and_then(|chat| chat.agent_id)
    }

    /// Move keyboard focus forward: into the panel from Main, then across
    /// tabs. Returns the newly selected tab id (`None` when focus left the
    /// panel, i.e. it wrapped from the last tab back to Main).
    pub fn advance_focus(&mut self) -> Option<String> {
        if !self.is_open() {
            self.focused = false;
            return None;
        }
        if !self.focused {
            self.focused = true;
            self.selected_tab.clone()
        } else {
            // Wrapping past the last tab returns focus to Main.
            let first = self.open_tabs.first().cloned();
            let next = self.cycle_tab(false);
            if next.as_ref() == first.as_ref() {
                self.focused = false;
                None
            } else {
                next
            }
        }
    }

    /// Move keyboard focus backward: from the first tab back to Main, then
    /// across tabs in reverse. Returns the newly selected tab id.
    pub fn retreat_focus(&mut self) -> Option<String> {
        if !self.is_open() {
            self.focused = false;
            return None;
        }
        if !self.focused {
            return None;
        }
        let selected = self.selected_id()?.to_owned();
        if self.open_tabs.first() == Some(&selected) {
            self.focused = false;
            None
        } else {
            self.cycle_tab(true)
        }
    }
}

/// Rectangles the previous frame rendered for the split. Input hit-testing
/// reads these; they are never persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SidePanelFrame {
    pub main: Rect,
    pub divider: Rect,
    pub panel: Rect,
}

impl SidePanelFrame {
    pub fn is_open(&self) -> bool {
        self.panel.width > 0 && self.panel.height > 0
    }
}

/// An input target produced by the same geometry that renders the tab strip.
/// Mouse handling consumes these before forwarding input into an `AgentView`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideTabHit {
    Select(String),
    Close(String),
    Create,
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideTabHitbox {
    pub start: u16,
    pub end: u16,
    pub hit: SideTabHit,
}

impl SideTabHitbox {
    pub fn contains(&self, column: u16) -> bool {
        (self.start..self.end).contains(&column)
    }
}

/// Allocate visible tab targets left-to-right without overlap. Entries that
/// cannot fit leave space for an overflow target instead of drawing on top of
/// adjacent tabs.
pub fn tab_hitboxes(
    panel_x: u16,
    panel_width: u16,
    tabs: impl IntoIterator<Item = (String, String)>,
) -> (Vec<SideTabHitbox>, bool) {
    const CREATE_WIDTH: u16 = 3;
    const OVERFLOW_WIDTH: u16 = 3;
    const MIN_TAB_WIDTH: u16 = 8;
    const MAX_TAB_WIDTH: u16 = 22;

    let mut hits = Vec::new();
    if panel_width < CREATE_WIDTH {
        return (hits, true);
    }
    let right = panel_x.saturating_add(panel_width);
    let create_start = right.saturating_sub(CREATE_WIDTH);
    let mut cursor = panel_x;
    let mut overflow = false;

    for (id, label) in tabs {
        let title_width = label
            .chars()
            .count()
            .min(usize::from(MAX_TAB_WIDTH.saturating_sub(4))) as u16;
        let width = title_width.saturating_add(4).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH);
        if cursor.saturating_add(width).saturating_add(CREATE_WIDTH) > right {
            overflow = true;
            break;
        }
        let close_start = cursor.saturating_add(width.saturating_sub(2));
        hits.push(SideTabHitbox {
            start: cursor,
            end: close_start,
            hit: SideTabHit::Select(id.clone()),
        });
        hits.push(SideTabHitbox {
            start: close_start,
            end: cursor.saturating_add(width),
            hit: SideTabHit::Close(id),
        });
        cursor = cursor.saturating_add(width);
    }

    if overflow && cursor.saturating_add(OVERFLOW_WIDTH).saturating_add(CREATE_WIDTH) <= right {
        hits.push(SideTabHitbox {
            start: cursor,
            end: cursor.saturating_add(OVERFLOW_WIDTH),
            hit: SideTabHit::Overflow,
        });
    }
    hits.push(SideTabHitbox {
        start: create_start,
        end: right,
        hit: SideTabHit::Create,
    });
    (hits, overflow)
}

/// Paint the side-panel tab strip with the same geometry used by `tab_hitboxes`:
/// icon, truncated title, selected state, `×` close target and `+` create target.
pub fn draw_tab_bar(
    buffer: &mut ratatui::buffer::Buffer,
    side_area: Rect,
    hits: &[SideTabHitbox],
    tabs: &[(String, String)],
    selected: Option<&str>,
    focused: bool,
) {
    use ratatui::style::Modifier;
    let theme = crate::theme::Theme::current();
    let tab_bar = Rect {
        x: side_area.x,
        y: side_area.y,
        width: side_area.width,
        height: SIDE_TAB_BAR_HEIGHT,
    };
    let right = tab_bar.x + tab_bar.width;
    let label_of = |sid: &str| {
        tabs.iter()
            .find(|(id, _)| id == sid)
            .map(|(_, l)| l.as_str())
            .unwrap_or("Side")
    };
    for hit in hits {
        if hit.start >= right {
            break;
        }
        match &hit.hit {
            SideTabHit::Select(sid) => {
                let label = label_of(sid);
                let is_selected = selected == Some(sid.as_str());
                let glyph = if is_selected {
                    crate::glyphs::diamond_filled()
                } else {
                    crate::glyphs::diamond_hollow()
                };
                let mut style = if is_selected {
                    theme.fg(theme.accent_user)
                } else {
                    theme.fg(theme.text_secondary)
                };
                if is_selected && focused {
                    style = style.add_modifier(Modifier::BOLD);
                }
                let mut x = hit.start;
                for ch in glyph.chars().chain(label.chars()) {
                    if x >= hit.end {
                        break;
                    }
                    if let Some(cell) = buffer.cell_mut((x, tab_bar.y)) {
                        cell.set_char(ch);
                        cell.set_style(style);
                    }
                    x += 1;
                }
            }
            SideTabHit::Close(_sid) => {
                let style = theme.fg(theme.text_secondary);
                if hit.start < right
                    && let Some(cell) = buffer.cell_mut((hit.start, tab_bar.y))
                {
                    cell.set_char('×');
                    cell.set_style(style);
                }
            }
            SideTabHit::Create => {
                let style = theme.fg(theme.accent_user);
                if hit.start < right
                    && let Some(cell) = buffer.cell_mut((hit.start, tab_bar.y))
                {
                    cell.set_char('+');
                    cell.set_style(style);
                }
            }
            SideTabHit::Overflow => {
                let style = theme.fg(theme.text_secondary);
                if hit.start < right
                    && let Some(cell) = buffer.cell_mut((hit.start, tab_bar.y))
                {
                    cell.set_char('»');
                    cell.set_style(style);
                }
            }
        }
    }
    // Dim empty cells so the bar reads as a distinct band without erasing glyphs.
    for x in tab_bar.x..right {
        if let Some(cell) = buffer.cell_mut((x, tab_bar.y))
            && cell.symbol().trim().is_empty()
        {
            cell.set_bg(theme.bg_dark);
        }
    }
}

pub mod persist {
    //! Atomic persistence for the sole durable side-panel UI preference.

    use super::SidePanelPreferences;
    use std::path::{Path, PathBuf};

    pub fn path() -> PathBuf {
        crate::app::side_chat::persist::rivo_home().join("side-panel.json")
    }

    pub fn load() -> SidePanelPreferences {
        load_at(&path())
    }

    fn load_at(path: &Path) -> SidePanelPreferences {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(preferences: &SidePanelPreferences) -> std::io::Result<()> {
        save_at(preferences, &path())
    }

    fn save_at(preferences: &SidePanelPreferences, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(preferences)
            .map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(tmp, path)
    }

    #[cfg(test)]
    mod tests {
        use super::{load_at, save_at};
        use crate::views::side_panel::SidePanelPreferences;
        use tempfile::TempDir;

        #[test]
        fn roundtrip_keeps_user_ratio() {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("side-panel.json");
            let mut preferences = SidePanelPreferences::default();
            preferences.set_main_ratio(71);
            save_at(&preferences, &path).unwrap();
            assert_eq!(load_at(&path).main_ratio, 71);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        tab_hitboxes, SidePanelLayout, SidePanelPreferences, SideTabHit, DEFAULT_MAIN_RATIO,
    };
    use ratatui::layout::Rect;

    #[test]
    fn default_ratio_is_65_35() {
        assert_eq!(SidePanelPreferences::default().main_ratio, DEFAULT_MAIN_RATIO);
    }

    #[test]
    fn closing_last_tab_collapses_panel() {
        let mut panel = SidePanelLayout::default();
        panel.open("one");
        assert!(panel.close("one"));
        assert!(!panel.is_open());
        assert!(!panel.focused);
        assert!(panel.visible == false);
    }

    #[test]
    fn open_sets_visible_and_focused() {
        let mut panel = SidePanelLayout::default();
        panel.open("one");
        assert!(panel.is_open());
        assert!(panel.focused);
        assert!(panel.visible);
    }

    #[test]
    fn hide_and_show_keep_tabs() {
        let mut panel = SidePanelLayout::default();
        panel.open("one");
        panel.hide();
        assert!(!panel.is_open());
        assert!(!panel.focused);
        panel.show();
        assert!(panel.is_open());
        assert_eq!(panel.selected_id(), Some("one"));
    }

    #[test]
    fn second_tab_does_not_replace_first() {
        let mut panel = SidePanelLayout::default();
        panel.open("one");
        panel.open("two");
        assert_eq!(panel.open_tabs, vec!["one", "two"]);
        assert_eq!(panel.selected_id(), Some("two"));
    }

    #[test]
    fn divider_respects_minimum_widths() {
        let mut preferences = SidePanelPreferences::default();
        let area = Rect::new(0, 0, 100, 30);
        preferences.set_from_divider(area, 1);
        let (main, _, side) = preferences.split(area).unwrap();
        assert!(main.width >= super::MIN_PANEL_WIDTH);
        assert!(side.width >= super::MIN_PANEL_WIDTH);
    }

    #[test]
    fn startup_does_not_restore_visible_tabs() {
        let mut panel = SidePanelLayout::default();
        panel.open("one");
        panel.clear_for_startup();
        assert!(!panel.is_open());
        assert_eq!(panel.selected_id(), None);
    }

    #[test]
    fn tab_hits_never_overlap_and_include_create() {
        let (hits, overflow) = tab_hitboxes(
            20,
            35,
            [
                ("one".to_string(), "A sufficiently long first title".to_string()),
                ("two".to_string(), "Another title".to_string()),
                ("three".to_string(), "Third title".to_string()),
            ],
        );
        assert!(overflow);
        for pair in hits.windows(2) {
            assert!(pair[0].end <= pair[1].start);
        }
        assert!(hits.iter().any(|hit| matches!(hit.hit, SideTabHit::Create)));
    }
}
