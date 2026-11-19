//! Multi-tab and split-pane workspace manager (`TabManager`).
//!
//! Optimization highlights:
//! - `drain_all_ptys` drains incoming PTY bytes across ALL panes (including background tabs)
//!   so inactive tabs never leak `Vec<u8>` allocations in `mpsc::channel`, while only running
//!   HarfBuzz text shaping on the active tab's `visible_leaves`.

pub mod item;
pub mod tree;
pub mod vi_mode;

pub use item::{Pane, PANE_PAD};
pub use tree::{NavDirection, PaneNode, SplitDirection};
pub use vi_mode::{ViMode, ViModeKind};

use crate::config::LunulaConfig;
use crate::render::TextEngine;
use crate::terminal::{CellStyle, ColorRgb};
use glyphon::Buffer;
use std::collections::HashMap;
use winit::event_loop::EventLoopProxy;

pub struct Tab {
    #[allow(dead_code)]
    pub id: usize,
    pub name: String,
    pub root: PaneNode,
    pub active_pane_id: usize,
    pub buffer: Buffer,
    pub width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarClickResult {
    None,
    Handled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneClickResult {
    Focused(usize),
    ClosePane(usize),
}

/// Cleans and formats raw process / directory window titles into sleek, readable tab titles.
/// e.g. "C:\WINDOWS\system32\WindowsPowerShell\v1.0\powershell.exe" -> "pwsh",
///      "C:\Users\kuroha\Pictures" -> "Pictures",
///      "longname12345678" -> "longname1…".
pub fn clean_tab_title(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "pwsh".to_string();
    }
    let lower = trimmed.to_lowercase();
    if lower.contains("powershell") || lower.contains("pwsh") {
        return "pwsh".to_string();
    }
    if lower == "cmd" || lower.ends_with("\\cmd.exe") || lower.ends_with("/cmd.exe") {
        return "cmd".to_string();
    }

    let norm = trimmed.replace('\\', "/");
    let base = norm.trim_end_matches('/').split('/').last().unwrap_or(trimmed);
    let stripped = base.strip_suffix(".exe").unwrap_or(base);
    let s_clean = stripped.trim();
    if s_clean.is_empty() {
        return "pwsh".to_string();
    }

    let count = s_clean.chars().count();
    if count > 10 {
        let mut s: String = s_clean.chars().take(9).collect();
        s.push('…');
        s
    } else {
        s_clean.to_string()
    }
}

pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub panes: HashMap<usize, Pane>,
    pub active_tab_index: usize,
    pub next_pane_id: usize,
    pub next_tab_id: usize,
    pub plus_buffer: Buffer,
    pub separators: Vec<(SplitDirection, [f32; 4])>,
    pub proxy: EventLoopProxy<()>,
}

impl TabManager {
    pub fn new(
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        proxy: EventLoopProxy<()>,
        config: &LunulaConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pane_id = 0;
        let tab_id = 0;

        let cell_w = text_engine.cell_width;
        let cell_h = text_engine.cell_height;
        let titlebar_h = 48.0;
        let tab_bar_h = if config.tabs.show_single_tab {
            config.tabs.height
        } else {
            0.0
        };
        let avail_h = (content_h - titlebar_h - tab_bar_h).max(20.0);
        let cols = ((content_w - 2.0 * PANE_PAD) / cell_w).max(10.0) as u16;
        let rows = ((avail_h - 2.0 * PANE_PAD) / cell_h).max(4.0) as u16;

        let initial_pane = Pane::new(pane_id, cols, rows, text_engine, proxy.clone(), config)?;
        let mut panes = HashMap::new();
        panes.insert(pane_id, initial_pane);

        let initial_tab = Tab {
            id: tab_id,
            name: "pwsh".to_string(),
            root: PaneNode::Leaf(pane_id),
            active_pane_id: pane_id,
            buffer: text_engine.create_buffer(),
            width: 80.0,
        };

        let plus_buffer = text_engine.create_buffer();

        let mut mgr = Self {
            tabs: vec![initial_tab],
            panes,
            active_tab_index: 0,
            next_pane_id: 1,
            next_tab_id: 1,
            plus_buffer,
            separators: Vec::new(),
            proxy,
        };

        mgr.layout(content_w, content_h, text_engine, config);
        Ok(mgr)
    }

    pub fn tab_bar_visible(&self, config: &LunulaConfig) -> bool {
        self.tabs.len() > 1 || config.tabs.show_single_tab
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab_index]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab_index]
    }

    pub fn active_pane(&self) -> Option<&Pane> {
        if self.tabs.is_empty() || self.active_tab_index >= self.tabs.len() {
            return None;
        }
        let pane_id = self.tabs[self.active_tab_index].active_pane_id;
        self.panes.get(&pane_id)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        if self.tabs.is_empty() || self.active_tab_index >= self.tabs.len() {
            return None;
        }
        let pane_id = self.tabs[self.active_tab_index].active_pane_id;
        self.panes.get_mut(&pane_id)
    }

    pub fn new_tab(
        &mut self,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        let cell_w = text_engine.cell_width;
        let cell_h = text_engine.cell_height;
        let titlebar_h = 48.0;
        let tab_bar_h = if config.tabs.show_single_tab {
            config.tabs.height
        } else {
            0.0
        };
        let avail_h = (content_h - titlebar_h - tab_bar_h).max(20.0);
        let cols = ((content_w - 2.0 * PANE_PAD) / cell_w).max(10.0) as u16;
        let rows = ((avail_h - 2.0 * PANE_PAD) / cell_h).max(4.0) as u16;

        let new_pane = Pane::new(pane_id, cols, rows, text_engine, self.proxy.clone(), config)?;
        self.panes.insert(pane_id, new_pane);

        let new_tab = Tab {
            id: tab_id,
            name: "pwsh".to_string(),
            root: PaneNode::Leaf(pane_id),
            active_pane_id: pane_id,
            buffer: text_engine.create_buffer(),
            width: 80.0,
        };

        self.tabs.push(new_tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.layout(content_w, content_h, text_engine, config);
        Ok(())
    }

    pub fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let target_id = self.active_tab().active_pane_id;
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        let new_pane = Pane::new(new_id, 20, 10, text_engine, self.proxy.clone(), config)?;
        self.panes.insert(new_id, new_pane);

        let tab = self.active_tab_mut();
        tab.root.split_pane(target_id, new_id, direction);
        tab.active_pane_id = new_id;

        self.layout(content_w, content_h, text_engine, config);
        Ok(())
    }

    pub fn close_pane_by_id(
        &mut self,
        target_id: usize,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) -> bool {
        let tab = &mut self.tabs[self.active_tab_index];
        let all_leaves = tab.root.all_leaves();

        if all_leaves.len() > 1 && all_leaves.contains(&target_id) {
            if let Some(new_root) = tab.root.remove_pane(target_id) {
                tab.root = new_root;
                if tab.active_pane_id == target_id {
                    tab.active_pane_id = tab.root.first_leaf();
                }
                self.panes.remove(&target_id);
                self.layout(content_w, content_h, text_engine, config);
                return true;
            }
        }
        self.close_active_pane(content_w, content_h, text_engine, config)
    }

    pub fn close_active_pane(
        &mut self,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) -> bool {
        let tab = &mut self.tabs[self.active_tab_index];
        let target_id = tab.active_pane_id;
        let all_leaves = tab.root.all_leaves();

        if all_leaves.len() > 1 {
            if let Some(new_root) = tab.root.remove_pane(target_id) {
                tab.root = new_root;
                tab.active_pane_id = tab.root.first_leaf();
                self.panes.remove(&target_id);
                self.layout(content_w, content_h, text_engine, config);
                return true;
            }
        }

        self.panes.remove(&target_id);
        self.tabs.remove(self.active_tab_index);

        if self.tabs.is_empty() {
            return false;
        }

        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }

        self.layout(content_w, content_h, text_engine, config);
        true
    }

    pub fn next_tab(
        &mut self,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) {
        if self.tabs.len() > 1 {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
            self.layout(content_w, content_h, text_engine, config);
        }
    }

    pub fn prev_tab(
        &mut self,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) {
        if self.tabs.len() > 1 {
            if self.active_tab_index == 0 {
                self.active_tab_index = self.tabs.len() - 1;
            } else {
                self.active_tab_index -= 1;
            }
            self.layout(content_w, content_h, text_engine, config);
        }
    }

    pub fn switch_tab(
        &mut self,
        index: usize,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            self.layout(content_w, content_h, text_engine, config);
        }
    }

    pub fn navigate_pane(&mut self, dir: NavDirection) {
        let tab = &mut self.tabs[self.active_tab_index];
        let mut rects = Vec::new();
        let mut seps = Vec::new();
        tab.root
            .compute_rects([0.0, 0.0, 1000.0, 1000.0], &mut rects, &mut seps);

        if let Some(next_id) = tab.root.find_neighbor(tab.active_pane_id, &rects, dir) {
            tab.active_pane_id = next_id;
        }
    }

    pub fn layout(
        &mut self,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) {
        let titlebar_h = 48.0;
        let tab_bar_h = if self.tab_bar_visible(config) {
            config.tabs.height
        } else {
            0.0
        };

        let pane_area = [
            0.0,
            titlebar_h + tab_bar_h,
            content_w,
            (content_h - titlebar_h - tab_bar_h).max(20.0),
        ];

        let tab = &mut self.tabs[self.active_tab_index];
        let mut rects = Vec::new();
        let mut separators = Vec::new();
        tab.root.compute_rects(pane_area, &mut rects, &mut separators);
        self.separators = separators;

        let cell_w = text_engine.cell_width;
        let cell_h = text_engine.cell_height;

        for (id, r) in rects {
            if let Some(pane) = self.panes.get_mut(&id) {
                pane.rect = r;
                let cols = ((r[2] - 2.0 * PANE_PAD) / cell_w).max(10.0) as u16;
                let rows = ((r[3] - 2.0 * PANE_PAD) / cell_h).max(4.0) as u16;
                pane.resize(cols, rows, cell_h);
                pane.terminal.dirty = true;
            }
        }

        self.shape_tab_bar(text_engine, config);
    }

    pub fn shape_tab_bar(&mut self, text_engine: &mut TextEngine, config: &LunulaConfig) {
        if self.tab_bar_visible(config) {
            let cell_w = text_engine.cell_width;
            for (idx, t) in self.tabs.iter_mut().enumerate() {
                let is_active = idx == self.active_tab_index;
                let fg = if is_active {
                    ColorRgb { r: 220, g: 215, b: 186 } // fujiWhite
                } else {
                    ColorRgb { r: 140, g: 140, b: 135 } // fujiGray
                };
                let style = CellStyle::new()
                    .with_fg(fg)
                    .with_bold(is_active)
                    .with_dim(!is_active);

                let clean = clean_tab_title(&t.name);
                let label = format!("{}  {}", idx + 1, clean);
                let char_len = label.chars().count();
                t.width = (char_len as f32 * cell_w + 20.0).clamp(56.0, 160.0);

                let spans = [(label, style)];
                text_engine.shape_buffer(&mut t.buffer, &spans);
            }

            let plus_style = CellStyle::new()
                .with_fg(ColorRgb { r: 127, g: 180, b: 202 }) // springBlue
                .with_bold(true);
            let plus_spans = [("+".to_string(), plus_style)];
            text_engine.shape_buffer(&mut self.plus_buffer, &plus_spans);
        }
    }

    pub fn handle_tab_bar_click(
        &mut self,
        cx: f32,
        cy: f32,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) -> TabBarClickResult {
        let titlebar_h = 48.0;
        let tab_bar_h = config.tabs.height;
        if !self.tab_bar_visible(config) || cy < titlebar_h || cy >= titlebar_h + tab_bar_h {
            return TabBarClickResult::None;
        }

        let mut pill_x = 10.0;
        for idx in 0..self.tabs.len() {
            let pill_w = self.tabs[idx].width;
            if cx >= pill_x && cx <= pill_x + pill_w {
                // Focus/switch tab on click. Close button removed completely per user request!
                self.active_tab_index = idx;
                self.layout(content_w, content_h, text_engine, config);
                return TabBarClickResult::Handled;
            }
            pill_x += pill_w + 4.0;
        }

        // Plus button to open a new tab
        if cx >= pill_x && cx <= pill_x + 28.0 {
            let _ = self.new_tab(content_w, content_h, text_engine, config);
            return TabBarClickResult::Handled;
        }

        TabBarClickResult::None
    }

    pub fn handle_pane_click_action(&mut self, cx: f32, cy: f32) -> Option<PaneClickResult> {
        let tab = &mut self.tabs[self.active_tab_index];
        let leaves = tab.root.all_leaves();
        for id in leaves.iter().copied() {
            if let Some(pane) = self.panes.get(&id) {
                let r = pane.rect;
                if cx >= r[0] && cx <= r[0] + r[2] && cy >= r[1] && cy <= r[1] + r[3] {
                    if leaves.len() > 1 {
                        let btn_x = r[0] + r[2] - 26.0;
                        let btn_y = r[1] + 4.0;
                        if cx >= btn_x && cx <= btn_x + 22.0 && cy >= btn_y && cy <= btn_y + 22.0 {
                            return Some(PaneClickResult::ClosePane(id));
                        }
                    }
                    tab.active_pane_id = id;
                    return Some(PaneClickResult::Focused(id));
                }
            }
        }
        None
    }

    /// Drains incoming PTY bytes across ALL panes (including inactive tabs) and shapes visible leaves.
    pub fn drain_all_ptys(&mut self, text_engine: &mut TextEngine, config: &LunulaConfig) -> bool {
        let mut changed = false;
        let mut title_changed = false;

        // 1. Drain bytes for EVERY pane across all tabs so background tabs never leak memory
        for pane in self.panes.values_mut() {
            if pane.drain_pty_bytes() {
                changed = true;
            }
            if !pane.terminal.title.is_empty() && pane.terminal.title != pane.title {
                pane.title = pane.terminal.title.clone();
                title_changed = true;
                changed = true;
            }
        }

        // 2. Shape buffers and update BgRects ONLY for the active tab's visible leaves
        let leaves = self.tabs[self.active_tab_index].root.all_leaves();
        for id in leaves {
            if let Some(pane) = self.panes.get_mut(&id) {
                if pane.sync_render_cache(text_engine) {
                    changed = true;
                }
            }
        }

        // 3. Synchronize active tab title if the active pane title changed
        let active_id = self.tabs[self.active_tab_index].active_pane_id;
        if let Some(p) = self.panes.get(&active_id) {
            if !p.title.is_empty() && self.tabs[self.active_tab_index].name != p.title {
                self.tabs[self.active_tab_index].name = p.title.clone();
                title_changed = true;
            }
        }

        if title_changed {
            self.shape_tab_bar(text_engine, config);
        }

        changed
    }

    pub fn reap_exited_panes(
        &mut self,
        content_w: f32,
        content_h: f32,
        text_engine: &mut TextEngine,
        config: &LunulaConfig,
    ) -> bool {
        let mut exited_ids = Vec::new();
        for (id, pane) in &mut self.panes {
            if !pane.pty.is_child_alive() {
                exited_ids.push(*id);
            }
        }

        for id in exited_ids {
            let mut found_tab_idx = None;
            for (t_idx, tab) in self.tabs.iter().enumerate() {
                if tab.root.all_leaves().contains(&id) {
                    found_tab_idx = Some(t_idx);
                    break;
                }
            }

            if let Some(t_idx) = found_tab_idx {
                self.active_tab_index = t_idx;
                self.tabs[t_idx].active_pane_id = id;
                if !self.close_active_pane(content_w, content_h, text_engine, config) {
                    return false;
                }
            }
        }
        !self.tabs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_node_split_and_remove() {
        let mut root = PaneNode::Leaf(0);
        assert_eq!(root.all_leaves(), vec![0]);

        assert!(root.split_pane(0, 1, SplitDirection::Vertical));
        assert_eq!(root.all_leaves(), vec![0, 1]);

        assert!(root.split_pane(1, 2, SplitDirection::Horizontal));
        assert_eq!(root.all_leaves(), vec![0, 1, 2]);

        let mut rects = Vec::new();
        let mut seps = Vec::new();
        root.compute_rects([0.0, 0.0, 1000.0, 600.0], &mut rects, &mut seps);
        assert_eq!(rects.len(), 3);
        assert_eq!(seps.len(), 2);

        let new_root = root.remove_pane(2).unwrap();
        assert_eq!(new_root.all_leaves(), vec![0, 1]);

        let final_root = new_root.remove_pane(0).unwrap();
        assert_eq!(final_root.all_leaves(), vec![1]);

        assert!(final_root.remove_pane(1).is_none());
    }

    #[test]
    fn test_clean_tab_title() {
        assert_eq!(clean_tab_title(""), "pwsh");
        assert_eq!(clean_tab_title("   "), "pwsh");
        assert_eq!(
            clean_tab_title(r"C:\WINDOWS\system32\WindowsPowerShell\v1.0\powershell.exe"),
            "pwsh"
        );
        assert_eq!(clean_tab_title("Administrator: Windows PowerShell"), "pwsh");
        assert_eq!(clean_tab_title(r"C:\Users\kuroha\Pictures"), "Pictures");
        assert_eq!(clean_tab_title("pwsh"), "pwsh");
        assert_eq!(clean_tab_title("nvim"), "nvim");
        assert_eq!(clean_tab_title("verylongnamethatexceeds"), "verylongn…");
    }
}
