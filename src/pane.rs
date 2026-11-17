use crate::config::LunulaConfig;
use crate::pty::PtyManager;
use crate::terminal::{self, BgRect, CellStyle, ColorRgb, TerminalGrid};
use crate::text::TextEngine;
use glyphon::Buffer;
use std::collections::HashMap;
use std::time::Instant;
use winit::event_loop::EventLoopProxy;

pub const PANE_PAD: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // Split into Top and Bottom panes
    Vertical,   // Split into Left and Right panes
}

#[derive(Clone, Debug)]
pub enum PaneNode {
    Leaf(usize),
    Split {
        direction: SplitDirection,
        ratio: f32, // 0.0..1.0
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    pub fn compute_rects(
        &self,
        rect: [f32; 4],
        rects: &mut Vec<(usize, [f32; 4])>,
        separators: &mut Vec<(SplitDirection, [f32; 4])>,
    ) {
        match self {
            PaneNode::Leaf(id) => {
                rects.push((*id, rect));
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let sep_size = 2.0;
                match direction {
                    SplitDirection::Vertical => {
                        let total_w = (rect[2] - sep_size).max(10.0);
                        let w1 = (total_w * ratio).round().clamp(60.0, total_w - 60.0);
                        let w2 = total_w - w1;
                        let r1 = [rect[0], rect[1], w1, rect[3]];
                        let sep_rect = [rect[0] + w1, rect[1], sep_size, rect[3]];
                        let r2 = [rect[0] + w1 + sep_size, rect[1], w2, rect[3]];

                        separators.push((SplitDirection::Vertical, sep_rect));
                        first.compute_rects(r1, rects, separators);
                        second.compute_rects(r2, rects, separators);
                    }
                    SplitDirection::Horizontal => {
                        let total_h = (rect[3] - sep_size).max(10.0);
                        let h1 = (total_h * ratio).round().clamp(40.0, total_h - 40.0);
                        let h2 = total_h - h1;
                        let r1 = [rect[0], rect[1], rect[2], h1];
                        let sep_rect = [rect[0], rect[1] + h1, rect[2], sep_size];
                        let r2 = [rect[0], rect[1] + h1 + sep_size, rect[2], h2];

                        separators.push((SplitDirection::Horizontal, sep_rect));
                        first.compute_rects(r1, rects, separators);
                        second.compute_rects(r2, rects, separators);
                    }
                }
            }
        }
    }

    pub fn split_pane(
        &mut self,
        target_id: usize,
        new_id: usize,
        direction: SplitDirection,
    ) -> bool {
        match self {
            PaneNode::Leaf(id) if *id == target_id => {
                let old_leaf = PaneNode::Leaf(target_id);
                let new_leaf = PaneNode::Leaf(new_id);
                *self = PaneNode::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(old_leaf),
                    second: Box::new(new_leaf),
                };
                true
            }
            PaneNode::Split { first, second, .. } => {
                if first.split_pane(target_id, new_id, direction) {
                    true
                } else {
                    second.split_pane(target_id, new_id, direction)
                }
            }
            _ => false,
        }
    }

    pub fn remove_pane(&self, target_id: usize) -> Option<PaneNode> {
        match self {
            PaneNode::Leaf(id) => {
                if *id == target_id {
                    None
                } else {
                    Some(self.clone())
                }
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let new_first = first.remove_pane(target_id);
                let new_second = second.remove_pane(target_id);
                match (new_first, new_second) {
                    (Some(f), Some(s)) => Some(PaneNode::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: Box::new(f),
                        second: Box::new(s),
                    }),
                    (Some(f), None) => Some(f),
                    (None, Some(s)) => Some(s),
                    (None, None) => None,
                }
            }
        }
    }

    pub fn first_leaf(&self) -> usize {
        match self {
            PaneNode::Leaf(id) => *id,
            PaneNode::Split { first, .. } => first.first_leaf(),
        }
    }

    pub fn all_leaves(&self) -> Vec<usize> {
        let mut leaves = Vec::new();
        self.collect_leaves(&mut leaves);
        leaves
    }

    fn collect_leaves(&self, out: &mut Vec<usize>) {
        match self {
            PaneNode::Leaf(id) => out.push(*id),
            PaneNode::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    pub fn find_neighbor(
        &self,
        current_id: usize,
        rects: &[(usize, [f32; 4])],
        dir: NavDirection,
    ) -> Option<usize> {
        let cur_rect = rects.iter().find(|(id, _)| *id == current_id)?.1;
        let cur_cx = cur_rect[0] + cur_rect[2] * 0.5;
        let cur_cy = cur_rect[1] + cur_rect[3] * 0.5;

        let mut best: Option<(usize, f32)> = None;
        for (id, r) in rects {
            if *id == current_id {
                continue;
            }
            let cx = r[0] + r[2] * 0.5;
            let cy = r[1] + r[3] * 0.5;

            let is_in_direction = match dir {
                NavDirection::Left => cx < cur_cx && (cy - cur_cy).abs() < cur_rect[3] * 0.65,
                NavDirection::Right => cx > cur_cx && (cy - cur_cy).abs() < cur_rect[3] * 0.65,
                NavDirection::Up => cy < cur_cy && (cx - cur_cx).abs() < cur_rect[2] * 0.65,
                NavDirection::Down => cy > cur_cy && (cx - cur_cx).abs() < cur_rect[2] * 0.65,
            };

            if is_in_direction {
                let dist = (cx - cur_cx).hypot(cy - cur_cy);
                if best.map_or(true, |(_, d)| dist < d) {
                    best = Some((*id, dist));
                }
            }
        }
        best.map(|(id, _)| id)
    }
}

pub struct Pane {
    #[allow(dead_code)]
    pub id: usize,
    pub pty: PtyManager,
    pub terminal: TerminalGrid,
    pub buffer: Buffer,
    pub cached_bg_rects: Vec<BgRect>,
    pub rect: [f32; 4], // x, y, w, h
    pub title: String,
    pub cursor_head: [f32; 2],
    pub cursor_tail: [f32; 2],
    pub cursor_head_vel: [f32; 2],
    pub cursor_tail_vel: [f32; 2],
    pub cursor_blink_time: f32,
    pub cursor_last_moved: Instant,
    pub prev_cursor_grid: (usize, usize),
}

impl Pane {
    pub fn new(
        id: usize,
        cols: u16,
        rows: u16,
        text_engine: &mut TextEngine,
        proxy: EventLoopProxy<()>,
        config: &LunulaConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut terminal = TerminalGrid::new(cols as usize, rows as usize);
        terminal.max_scrollback = config.terminal.max_scrollback;
        terminal.selection_color = terminal::ColorRgb {
            r: config.colors.selection[0],
            g: config.colors.selection[1],
            b: config.colors.selection[2],
        };
        terminal.cursor_config.shape = match config.cursor.shape.as_str() {
            "beam" => terminal::CursorShape::Beam,
            "underline" => terminal::CursorShape::Underline,
            _ => terminal::CursorShape::Block,
        };
        terminal.cursor_config.blinking = config.cursor.blinking;
        terminal.cursor_config.color = terminal::ColorRgb {
            r: config.cursor.color[0],
            g: config.cursor.color[1],
            b: config.cursor.color[2],
        };

        let pty = PtyManager::new(cols, rows, proxy)?;
        let buffer = text_engine.create_buffer();

        Ok(Self {
            id,
            pty,
            terminal,
            buffer,
            cached_bg_rects: Vec::new(),
            rect: [0.0, 0.0, 100.0, 100.0],
            title: "pwsh".to_string(),
            cursor_head: [PANE_PAD, PANE_PAD],
            cursor_tail: [PANE_PAD, PANE_PAD],
            cursor_head_vel: [0.0, 0.0],
            cursor_tail_vel: [0.0, 0.0],
            cursor_blink_time: 0.0,
            cursor_last_moved: Instant::now(),
            prev_cursor_grid: (0, 0),
        })
    }

    pub fn resize(&mut self, new_cols: u16, new_rows: u16) {
        if new_cols > 0 && new_rows > 0 {
            self.terminal.resize(new_cols as usize, new_rows as usize);
            self.pty.resize(new_cols, new_rows);
        }
    }

    pub fn update_physics(&mut self, dt: f32, config: &LunulaConfig, cell_w: f32, cell_h: f32) {
        let cur_grid = (self.terminal.cursor_x, self.terminal.cursor_y);
        let now = Instant::now();
        if cur_grid != self.prev_cursor_grid {
            self.cursor_last_moved = now;
            self.cursor_blink_time = 0.0;
            self.prev_cursor_grid = cur_grid;
            self.terminal.dirty = true;
        }
        self.cursor_blink_time += dt;

        let target_x = self.rect[0] + PANE_PAD + self.terminal.cursor_x as f32 * cell_w;
        let target_y = self.rect[1] + PANE_PAD + self.terminal.cursor_y as f32 * cell_h;

        let k_head = config.cursor.trail_stiffness;
        let d_head = config.cursor.trail_damping;
        let dx = target_x - self.cursor_head[0];
        let dy = target_y - self.cursor_head[1];
        self.cursor_head_vel[0] =
            (self.cursor_head_vel[0] + dx * k_head * dt) * (1.0 - d_head * dt).max(0.0);
        self.cursor_head_vel[1] =
            (self.cursor_head_vel[1] + dy * k_head * dt) * (1.0 - d_head * dt).max(0.0);
        self.cursor_head[0] += self.cursor_head_vel[0] * dt;
        self.cursor_head[1] += self.cursor_head_vel[1] * dt;

        let k_tail = k_head * 0.45;
        let d_tail = d_head * 0.70;
        let tdx = self.cursor_head[0] - self.cursor_tail[0];
        let tdy = self.cursor_head[1] - self.cursor_tail[1];
        self.cursor_tail_vel[0] =
            (self.cursor_tail_vel[0] + tdx * k_tail * dt) * (1.0 - d_tail * dt).max(0.0);
        self.cursor_tail_vel[1] =
            (self.cursor_tail_vel[1] + tdy * k_tail * dt) * (1.0 - d_tail * dt).max(0.0);
        self.cursor_tail[0] += self.cursor_tail_vel[0] * dt;
        self.cursor_tail[1] += self.cursor_tail_vel[1] * dt;
    }

    pub fn drain_pty(&mut self, text_engine: &mut TextEngine) -> bool {
        let mut received = false;
        while let Ok(bytes) = self.pty.rx.try_recv() {
            self.terminal.advance_bytes(&bytes);
            received = true;
        }

        if !self.terminal.responses.is_empty() {
            for resp in self.terminal.responses.drain(..) {
                self.pty.write_bytes(&resp);
            }
        }

        if received || self.terminal.dirty {
            let (spans, bg_rects) = self.terminal.render_spans_and_backgrounds(
                text_engine.cell_width,
                text_engine.cell_height,
                PANE_PAD,
                PANE_PAD,
            );
            text_engine.shape_buffer(&mut self.buffer, &spans);
            self.cached_bg_rects = bg_rects;
            self.terminal.dirty = false;
            true
        } else {
            false
        }
    }
}

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
        let titlebar_h = 42.0;
        let tab_bar_h = if config.tabs.show_single_tab { config.tabs.height } else { 0.0 };
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
        let titlebar_h = 42.0;
        let tab_bar_h = if config.tabs.show_single_tab { config.tabs.height } else { 0.0 };
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
            // Close just this pane in the split
            if let Some(new_root) = tab.root.remove_pane(target_id) {
                tab.root = new_root;
                tab.active_pane_id = tab.root.first_leaf();
                self.panes.remove(&target_id);
                self.layout(content_w, content_h, text_engine, config);
                return true;
            }
        }

        // Only 1 pane in this tab: close the entire tab!
        self.panes.remove(&target_id);
        self.tabs.remove(self.active_tab_index);

        if self.tabs.is_empty() {
            return false; // Exit app!
        }

        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }

        self.layout(content_w, content_h, text_engine, config);
        true
    }

    pub fn next_tab(&mut self, content_w: f32, content_h: f32, text_engine: &mut TextEngine, config: &LunulaConfig) {
        if self.tabs.len() > 1 {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
            self.layout(content_w, content_h, text_engine, config);
        }
    }

    pub fn prev_tab(&mut self, content_w: f32, content_h: f32, text_engine: &mut TextEngine, config: &LunulaConfig) {
        if self.tabs.len() > 1 {
            if self.active_tab_index == 0 {
                self.active_tab_index = self.tabs.len() - 1;
            } else {
                self.active_tab_index -= 1;
            }
            self.layout(content_w, content_h, text_engine, config);
        }
    }

    pub fn switch_tab(&mut self, index: usize, content_w: f32, content_h: f32, text_engine: &mut TextEngine, config: &LunulaConfig) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            self.layout(content_w, content_h, text_engine, config);
        }
    }

    pub fn navigate_pane(&mut self, dir: NavDirection) {
        let tab = &mut self.tabs[self.active_tab_index];
        let mut rects = Vec::new();
        let mut seps = Vec::new();
        tab.root.compute_rects([0.0, 0.0, 1000.0, 1000.0], &mut rects, &mut seps);

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
        let titlebar_h = 42.0;
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
                pane.resize(cols, rows);
                pane.terminal.dirty = true;
            }
        }

        // Shape tab bar text
        if tab_bar_h > 0.0 {
            let mut spans = Vec::new();
            for (idx, t) in self.tabs.iter().enumerate() {
                let is_active = idx == self.active_tab_index;
                let style = CellStyle {
                    fg: if is_active {
                        Some(ColorRgb { r: 245, g: 245, b: 255 })
                    } else {
                        Some(ColorRgb { r: 166, g: 173, b: 200 })
                    },
                    bg: None,
                    bold: is_active,
                    dim: !is_active,
                    italic: false,
                    underline: false,
                    undercurl: false,
                    strikethrough: false,
                    underline_color: None,
                    reverse: false,
                };
                let display_title = if !t.name.is_empty() { &t.name } else { "pwsh" };
                spans.push((format!("  {}: {}  ×  ", idx + 1, display_title), style));
            }
            spans.push((
                "  +  ".to_string(),
                CellStyle {
                    fg: Some(ColorRgb { r: 137, g: 220, b: 235 }),
                    bg: None,
                    bold: true,
                    dim: false,
                    italic: false,
                    underline: false,
                    undercurl: false,
                    strikethrough: false,
                    underline_color: None,
                    reverse: false,
                },
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
        let titlebar_h = 42.0;
        let tab_bar_h = config.tabs.height;
        if !self.tab_bar_visible(config) || cy < titlebar_h || cy >= titlebar_h + tab_bar_h {
            return TabBarClickResult::None;
        }

        let mut pill_x = 10.0;
        let pill_w = 140.0;

        for idx in 0..self.tabs.len() {
            if cx >= pill_x && cx <= pill_x + pill_w {
                // Check if close '×' button at right of pill was clicked
                if cx >= pill_x + pill_w - 24.0 {
                    let _tab_id = self.tabs[idx].id;
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

        // Check '+' button
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
                    // If multiple panes in split, check if close badge in top-right was clicked
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


    pub fn drain_all_ptys(&mut self, text_engine: &mut TextEngine) -> bool {
        let mut changed = false;
        let tab = &self.tabs[self.active_tab_index];
        let leaves = tab.root.all_leaves();

        for id in leaves {
            if let Some(pane) = self.panes.get_mut(&id) {
                if pane.drain_pty(text_engine) {
                    changed = true;
                }
                if !pane.terminal.title.is_empty() && pane.terminal.title != pane.title {
                    pane.title = pane.terminal.title.clone();
                    changed = true;
                }
            }
        }

        // Update tab title from active pane
        let active_id = self.tabs[self.active_tab_index].active_pane_id;
        if let Some(p) = self.panes.get(&active_id) {
            if !p.title.is_empty() {
                self.tabs[self.active_tab_index].name = p.title.clone();
            }
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
                    return false; // Exit app!
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

        // Split vertically: 0 and 1
        assert!(root.split_pane(0, 1, SplitDirection::Vertical));
        assert_eq!(root.all_leaves(), vec![0, 1]);

        // Split pane 1 horizontally: 1 and 2
        assert!(root.split_pane(1, 2, SplitDirection::Horizontal));
        assert_eq!(root.all_leaves(), vec![0, 1, 2]);

        // Compute rects
        let mut rects = Vec::new();
        let mut seps = Vec::new();
        root.compute_rects([0.0, 0.0, 1000.0, 600.0], &mut rects, &mut seps);
        assert_eq!(rects.len(), 3);
        assert_eq!(seps.len(), 2);

        // Remove pane 2 -> sibling pane 1 fills space
        let new_root = root.remove_pane(2).unwrap();
        assert_eq!(new_root.all_leaves(), vec![0, 1]);

        // Remove pane 0 -> pane 1 becomes only root leaf
        let final_root = new_root.remove_pane(0).unwrap();
        assert_eq!(final_root.all_leaves(), vec![1]);

        // Removing the last leaf returns None (meaning tab or window closes)
        assert!(final_root.remove_pane(1).is_none());
    }
}
