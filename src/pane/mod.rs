//! Multi-tab and split-pane workspace manager (`TabManager`).
//!
//! Optimization highlights:
//! - `drain_all_ptys` drains incoming PTY bytes across ALL panes (including background tabs)
//!   so inactive tabs never leak `Vec<u8>` allocations in `mpsc::channel`, while only running
//!   HarfBuzz text shaping on the active tab's `visible_leaves`.

pub mod item;
pub mod tree;

pub use item::{Pane, PANE_PAD};
pub use tree::{NavDirection, PaneNode, SplitDirection};

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

pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub panes: HashMap<usize, Pane>,
    pub active_tab_index: usize,
    pub next_pane_id: usize,
    pub next_tab_id: usize,
    pub tab_bar_buffer: Buffer,
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
        };

        let tab_bar_buffer = text_engine.create_buffer();

        let mut mgr = Self {
            tabs: vec![initial_tab],
            panes,
            active_tab_index: 0,
            next_pane_id: 1,
            next_tab_id: 1,
            tab_bar_buffer,
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
        let pane_id = self.active_tab().active_pane_id;
        self.panes.get(&pane_id)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        let pane_id = self.active_tab().active_pane_id;
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
            let mut spans = Vec::with_capacity(self.tabs.len() + 1);
            for (idx, t) in self.tabs.iter().enumerate() {
                let is_active = idx == self.active_tab_index;
                let fg = if is_active {
                    ColorRgb { r: 220, g: 215, b: 186 }
                } else {
                    ColorRgb { r: 114, g: 113, b: 105 }
                };
                let style = CellStyle::new()
                    .with_fg(fg)
                    .with_bold(is_active)
                    .with_dim(!is_active);
                let display_title = if !t.name.is_empty() { &t.name } else { "pwsh" };
                let mut title = display_title.to_string();
                if title.chars().count() > 7 {
                    let mut s: String = title.chars().take(6).collect();
                    s.push('…');
                    title = s;
                }
                let title_fixed = format!("{:<7}", title);
                spans.push((format!("  {}: {}  × ", idx + 1, title_fixed), style));
            }
            spans.push((
                "  +  ".to_string(),
                CellStyle::new()
                    .with_fg(ColorRgb { r: 127, g: 180, b: 202 })
                    .with_bold(true),
            ));
            text_engine.shape_buffer(&mut self.tab_bar_buffer, &spans);
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
        let pill_w = 140.0;

        for idx in 0..self.tabs.len() {
            if cx >= pill_x && cx <= pill_x + pill_w {
                if cx >= pill_x + pill_w - 24.0 {
                    let leaves = self.tabs[idx].root.all_leaves();
                    for leaf_id in leaves {
                        self.panes.remove(&leaf_id);
                    }
                    self.tabs.remove(idx);
                    if self.tabs.is_empty() {
                        let _ = self.new_tab(content_w, content_h, text_engine, config);
                    } else if self.active_tab_index >= self.tabs.len() {
                        self.active_tab_index = self.tabs.len() - 1;
                    }
                } else {
                    self.active_tab_index = idx;
                }
                self.layout(content_w, content_h, text_engine, config);
                return TabBarClickResult::Handled;
            }
            pill_x += pill_w + 6.0;
        }

        if cx >= pill_x && cx <= pill_x + 34.0 {
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
        true
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
}
