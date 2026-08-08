//! WindowManager — tiled windows (Cursor-faithful scaffold).
//!
//! Scaffold behind feature flag; existing TUI still renders single-column.
//! Provides `WindowManager` state + tiled layout helper using
//! `ratatui::layout::Layout::horizontal/vertical` + `Constraint::Ratio`,
//! drag via `MouseEvent::Drag`, keyboard `Ctrl+←/→`.
//!
//! See `docs/cursor-research.md §8` and `docs/rivo-modes-implementation.md §7`.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Borders, Widget};
use serde::{Deserialize, Serialize};

/// Unique identifier for a window (tiled pane).
pub type WindowId = String;

/// Split ratio for a divisor between windows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Split {
    /// Ratio numerator (0..=100).
    pub ratio: u32,
    /// Whether this split is currently being dragged.
    #[serde(default)]
    pub dragging: bool,
}

impl Default for Split {
    fn default() -> Self {
        Self {
            ratio: 50,
            dragging: false,
        }
    }
}

impl Split {
    /// Create a split with given ratio (clamped 10..90).
    pub fn new(ratio: u32) -> Self {
        Self {
            ratio: ratio.clamp(10, 90),
            dragging: false,
        }
    }

    /// Constraint for this split.
    pub fn constraint(&self) -> Constraint {
        Constraint::Ratio(self.ratio, 100)
    }

    /// Complement constraint (100 - ratio).
    pub fn complement(&self) -> Constraint {
        Constraint::Ratio(100 - self.ratio, 100)
    }
}

/// A single tiled window (pane).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    /// Stable id.
    pub id: WindowId,
    /// Optional title (e.g., agent session id or side-chat id).
    #[serde(default)]
    pub title: String,
    /// Whether minimized (collapsed in sidebar, not rendered as tile).
    #[serde(default)]
    pub minimized: bool,
    /// Last known rect (filled during layout).
    #[serde(skip)]
    pub rect: Rect,
}

impl Window {
    /// Create a new window with given id and title.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            minimized: false,
            rect: Rect::default(),
        }
    }
}

/// Tiled window manager — owns window list, focused index, splits and minimized state.
///
/// Behind `tiling_enabled = false` by default so existing single-column layout is unchanged.
/// Not yet wired into draw path; state only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowManager {
    /// All windows in order.
    pub windows: Vec<Window>,
    /// Index of focused window in `windows` (valid when `windows` non-empty).
    #[serde(default)]
    pub focused: usize,
    /// Splits between windows (len = windows.len().saturating_sub(1)).
    #[serde(default)]
    pub splits: Vec<Split>,
    /// Whether tiling is enabled (feature flag). `false` = single-column.
    #[serde(default)]
    pub tiling_enabled: bool,
    /// Global minimized flag (all windows collapsed to sidebar).
    #[serde(default)]
    pub minimized: bool,
}

impl Default for WindowManager {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
            focused: 0,
            splits: Vec::new(),
            tiling_enabled: false,
            minimized: false,
        }
    }
}

impl WindowManager {
    /// Create an empty manager (tiling disabled).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with tiling enabled.
    pub fn with_tiling(mut self, enabled: bool) -> Self {
        self.tiling_enabled = enabled;
        self
    }

    /// Whether tiling is active and there is more than one visible window.
    pub fn is_tiled(&self) -> bool {
        self.tiling_enabled && self.visible_windows().len() > 1
    }

    /// Visible (non-minimized) windows.
    pub fn visible_windows(&self) -> Vec<&Window> {
        self.windows.iter().filter(|w| !w.minimized).collect()
    }

    /// Add a new window. Returns its id.
    pub fn add_window(&mut self, title: impl Into<String>) -> WindowId {
        let id = format!("win-{}", uuid::Uuid::new_v4());
        let win = Window::new(id.clone(), title);
        self.windows.push(win);
        self.focused = self.windows.len().saturating_sub(1);
        self.rebuild_splits();
        id
    }

    /// Remove window by id. Returns true if found.
    pub fn remove_window(&mut self, id: &str) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            self.windows.remove(idx);
            if self.focused >= self.windows.len() && !self.windows.is_empty() {
                self.focused = self.windows.len() - 1;
            } else if self.windows.is_empty() {
                self.focused = 0;
            }
            self.rebuild_splits();
            true
        } else {
            false
        }
    }

    /// Focus window by id. Returns true if found.
    pub fn focus(&mut self, id: &str) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            self.focused = idx;
            true
        } else {
            false
        }
    }

    /// Focused window, if any.
    pub fn focused_window(&self) -> Option<&Window> {
        self.windows.get(self.focused)
    }

    /// Cycle focus to next window (Ctrl+Tab).
    pub fn cycle_focus(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        self.focused = (self.focused + 1) % self.windows.len();
    }

    /// Resize focused split by `delta` columns (keyboard Ctrl+←/→ = 2, Ctrl+Shift+←/→ = 10).
    ///
    /// `delta` is in columns (positive = grow focused, negative = shrink).
    /// For scaffold, just adjusts the split ratio by `delta` clamped 10..90.
    pub fn resize_focused(&mut self, delta: i32) {
        if self.splits.is_empty() {
            return;
        }
        let idx = self.focused.min(self.splits.len().saturating_sub(1));
        let split = &mut self.splits[idx];
        let new_ratio = (split.ratio as i32 + delta).clamp(10, 90) as u32;
        split.ratio = new_ratio;
    }

    /// Handle mouse drag on a divisor. `x` is the absolute column where drag occurred.
    ///
    /// Scaffold: updates the nearest split ratio based on `x` relative to `area`.
    /// Call with `MouseEvent::Drag` containing `column`.
    pub fn handle_drag(&mut self, area: Rect, x: u16) {
        if self.splits.is_empty() || area.width == 0 {
            return;
        }
        // Single horizontal split scaffold: ratio = (x - left) / total_width * 100
        let left = area.x as i32;
        let total = area.width as i32;
        let pos = x as i32 - left;
        let ratio = ((pos as f32 / total as f32) * 100.0).round() as i32;
        let ratio = ratio.clamp(10, 90) as u32;
        // For now, adjust first split (multi-split will map x to correct divisor).
        if let Some(split) = self.splits.first_mut() {
            split.ratio = ratio;
            split.dragging = true;
        }
    }

    /// End any drag (on MouseUp).
    pub fn end_drag(&mut self) {
        for split in &mut self.splits {
            split.dragging = false;
        }
    }

    /// Compute tiled layout rects for visible windows inside `area`.
    ///
    /// Uses `Layout::horizontal` with `Constraint::Ratio` per the spec.
    /// Returns one `Rect` per visible window in order.
    /// If tiling is disabled or only one visible window, returns single `area`.
    pub fn compute_tiled_layout(&self, area: Rect) -> Vec<Rect> {
        let visible = self.visible_windows();
        if !self.tiling_enabled || visible.len() <= 1 {
            return vec![area];
        }
        let n = visible.len();
        // Build constraints from splits or equal ratios
        let constraints: Vec<Constraint> = if self.splits.len() + 1 == n {
            // Use stored splits: for n windows, n-1 splits define ratios.
            // Scaffold: horizontal split with ratios derived from splits.
            // For 2 windows: [ratio, 100-ratio]. For more, equal for now.
            if n == 2 {
                let r = self.splits[0].ratio;
                vec![Constraint::Ratio(r, 100), Constraint::Ratio(100 - r, 100)]
            } else {
                // Equal split fallback for >2 (future: nested Layout::vertical)
                vec![Constraint::Ratio(1, n as u32); n]
            }
        } else {
            vec![Constraint::Ratio(1, n as u32); n]
        };
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area)
            .to_vec()
    }

    /// Compute nested layout: horizontal outer + vertical inner for 4-pane case (future).
    ///
    /// Scaffold helper showing vertical nesting; not yet used in draw.
    pub fn compute_nested_layout(&self, area: Rect, rows: usize, cols: usize) -> Vec<Rect> {
        if rows == 0 || cols == 0 {
            return vec![area];
        }
        let row_constraints = vec![Constraint::Ratio(1, rows as u32); rows];
        let rows_rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(area);
        let mut out = Vec::new();
        for row_rect in rows_rects.iter() {
            let col_constraints = vec![Constraint::Ratio(1, cols as u32); cols];
            let cols_rects = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(col_constraints)
                .split(*row_rect);
            out.extend(cols_rects.iter().cloned());
        }
        out
    }

    fn rebuild_splits(&mut self) {
        let n = self.windows.len();
        if n <= 1 {
            self.splits.clear();
        } else {
            let needed = n - 1;
            if self.splits.len() < needed {
                self.splits.resize_with(needed, Split::default);
            } else if self.splits.len() > needed {
                self.splits.truncate(needed);
            }
            // Ensure ratios sane
            for split in &mut self.splits {
                split.ratio = split.ratio.clamp(10, 90);
            }
        }
    }

    /// Draw tiled boxes for visible windows inside `area`.
    ///
    /// Behind flag: no-op when not tiled (single visible window or tiling off).
    /// For now just visual `Block` boxes with titles (ratatui `Block` +
    /// `Borders::ALL`). Full per-tile `AgentView` comes later.
    /// Minimized windows render as pill `Tab` chips at the bottom; focused
    /// window gets highlight border.
    pub fn draw_tiled_boxes(&self, area: Rect, buf: &mut Buffer, focused_id: Option<&str>) {
        if !self.is_tiled() {
            return;
        }
        let rects = self.compute_tiled_layout(area);
        let visible: Vec<&Window> = self.visible_windows();
        for (i, win) in visible.iter().enumerate() {
            if i >= rects.len() {
                break;
            }
            let rect = rects[i];
            if rect.width < 3 || rect.height < 3 {
                continue;
            }
            let is_focused = Some(win.id.as_str()) == focused_id;
            let border_style = if is_focused {
                Style::default().fg(ratatui::style::Color::Cyan)
            } else {
                Style::default().fg(ratatui::style::Color::DarkGray)
            };
            let title = if win.title.is_empty() {
                win.id.clone()
            } else {
                win.title.clone()
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(title);
            let inner = block.inner(rect);
            block.render(rect, buf);
            // Reserve inner for future AgentView draw; clear with base bg.
            for y in inner.y..inner.y + inner.height {
                for x in inner.x..inner.x + inner.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(' ');
                    }
                }
            }
        }
    }

    /// Draw minimized windows as pill tabs at the bottom of `area`.
    ///
    /// Returns the rect occupied (height 1 when any minimized, else empty).
    pub fn draw_minimized_pills(&self, area: Rect, buf: &mut Buffer) -> Rect {
        let minimized: Vec<&Window> = self.windows.iter().filter(|w| w.minimized).collect();
        if minimized.is_empty() || area.height == 0 {
            return Rect::default();
        }
        let pill_y = area.y + area.height.saturating_sub(1);
        let pill_area = Rect::new(area.x, pill_y, area.width, 1);
        let mut x = pill_area.x;
        for win in minimized {
            let label = format!("[ {} ]", win.title);
            let w = label.len() as u16;
            if x + w > pill_area.x + pill_area.width {
                break;
            }
            let style = Style::default()
                .fg(ratatui::style::Color::Yellow)
                .bg(ratatui::style::Color::DarkGray);
            for (i, ch) in label.chars().enumerate() {
                if let Some(cell) = buf.cell_mut((x + i as u16, pill_y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }
            x += w + 1;
        }
        pill_area
    }

    /// Whether any window is minimized.
    pub fn has_minimized(&self) -> bool {
        self.windows.iter().any(|w| w.minimized)
    }

    /// Hit-test which split divisor contains `x` (for drag). Returns split index.
    pub fn hit_test_split(&self, area: Rect, x: u16) -> Option<usize> {
        if !self.is_tiled() {
            return None;
        }
        let rects = self.compute_tiled_layout(area);
        for (i, rect) in rects.iter().enumerate() {
            if i + 1 < rects.len() {
                let div_x = rect.x + rect.width;
                // Divisor is 1-char wide gutter between rects
                if x == div_x || x + 1 == div_x {
                    return Some(i);
                }
            }
        }
        None
    }
}

pub mod persist {
    //! Persistence for `WindowManager` — `~/.rivo/windows.json` (atomic write).
    //!
    //! Uses the same `rivo_home()` resolution pattern as `side_chat::persist`.

    use std::path::{Path, PathBuf};

    use super::WindowManager;

    /// Path for the windows layout file.
    pub fn windows_path() -> PathBuf {
        crate::app::side_chat::persist::rivo_home().join("windows.json")
    }

    /// Compatibility path (grok home fallback for migration reads).
    pub fn grok_windows_path() -> PathBuf {
        xai_grok_config::grok_home().join("windows.json")
    }

    /// Load `WindowManager` from disk. Returns default (tiling off) if missing/invalid.
    pub fn load() -> WindowManager {
        load_at(&windows_path())
    }

    fn load_at(path: &Path) -> WindowManager {
        let data = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let grok_path = grok_windows_path();
                if grok_path != path {
                    if let Ok(s) = std::fs::read_to_string(&grok_path) {
                        return parse(&s);
                    }
                }
                return WindowManager::default();
            }
            Err(_) => return WindowManager::default(),
        };
        parse(&data)
    }

    fn parse(data: &str) -> WindowManager {
        serde_json::from_str(data).unwrap_or_default()
    }

    /// Persist `WindowManager` atomically (temp file + rename).
    pub fn save(wm: &WindowManager) -> std::io::Result<()> {
        save_at(wm, &windows_path())
    }

    fn save_at(wm: &WindowManager, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(wm)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::{load_at, save_at};
        use crate::views::window_manager::WindowManager;
        use tempfile::TempDir;

        #[test]
        fn roundtrip_via_temp_path() {
            let tmp = TempDir::new().unwrap();
            let path = tmp.path().join("windows.json");
            let mut wm = WindowManager::new().with_tiling(true);
            wm.add_window("test");
            save_at(&wm, &path).unwrap();
            let loaded = load_at(&path);
            assert_eq!(loaded.windows.len(), 1);
            assert!(loaded.tiling_enabled);
        }

        #[test]
        fn missing_file_yields_default() {
            let tmp = TempDir::new().unwrap();
            let path = tmp.path().join("nonexistent.json");
            let wm = load_at(&path);
            assert!(wm.windows.is_empty());
            assert!(!wm.tiling_enabled);
        }

        #[test]
        fn tiling_off_by_default_after_load_failure() {
            let wm = super::parse("not-json");
            assert!(!wm.tiling_enabled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Split, WindowManager};
    use ratatui::layout::Rect;

    #[test]
    fn new_is_empty_and_tiling_off() {
        let wm = WindowManager::new();
        assert!(wm.windows.is_empty());
        assert!(!wm.tiling_enabled);
        assert!(!wm.is_tiled());
    }

    #[test]
    fn add_window_sets_focus() {
        let mut wm = WindowManager::new().with_tiling(true);
        let id1 = wm.add_window("win1");
        assert_eq!(wm.focused_window().unwrap().id, id1);
        let id2 = wm.add_window("win2");
        assert_eq!(wm.focused_window().unwrap().id, id2);
        assert_eq!(wm.windows.len(), 2);
        assert_eq!(wm.splits.len(), 1);
    }

    #[test]
    fn remove_window_updates_focus() {
        let mut wm = WindowManager::new();
        let id1 = wm.add_window("a");
        let id2 = wm.add_window("b");
        assert!(wm.remove_window(&id1));
        assert_eq!(wm.windows.len(), 1);
        assert_eq!(wm.windows[0].id, id2);
    }

    #[test]
    fn cycle_focus_wraps() {
        let mut wm = WindowManager::new();
        wm.add_window("a");
        wm.add_window("b");
        let first = wm.focused;
        wm.cycle_focus();
        assert_ne!(wm.focused, first);
        wm.cycle_focus();
        assert_eq!(wm.focused, first);
    }

    #[test]
    fn compute_tiled_layout_single_returns_area() {
        let wm = WindowManager::new().with_tiling(true);
        let area = Rect::new(0, 0, 100, 30);
        let rects = wm.compute_tiled_layout(area);
        assert_eq!(rects, vec![area]);
    }

    #[test]
    fn compute_tiled_layout_two_splits_horizontally() {
        let mut wm = WindowManager::new().with_tiling(true);
        wm.add_window("a");
        wm.add_window("b");
        // Default 50/50
        let area = Rect::new(0, 0, 100, 20);
        let rects = wm.compute_tiled_layout(area);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].width + rects[1].width, 100);
        // Resize
        wm.resize_focused(10);
        assert_eq!(wm.splits[0].ratio, 60);
        let rects2 = wm.compute_tiled_layout(area);
        assert_eq!(rects2.len(), 2);
    }

    #[test]
    fn handle_drag_updates_ratio() {
        let mut wm = WindowManager::new().with_tiling(true);
        wm.add_window("a");
        wm.add_window("b");
        let area = Rect::new(0, 0, 100, 20);
        wm.handle_drag(area, 30);
        assert_eq!(wm.splits[0].ratio, 30);
        wm.end_drag();
        assert!(!wm.splits[0].dragging);
    }

    #[test]
    fn split_clamps_ratio() {
        assert_eq!(Split::new(5).ratio, 10);
        assert_eq!(Split::new(95).ratio, 90);
        assert_eq!(Split::new(50).ratio, 50);
    }

    #[test]
    fn visible_windows_filters_minimized() {
        let mut wm = WindowManager::new();
        wm.add_window("a");
        let id2 = wm.add_window("b");
        wm.windows.iter_mut().find(|w| w.id == id2).unwrap().minimized = true;
        assert_eq!(wm.visible_windows().len(), 1);
    }
}
