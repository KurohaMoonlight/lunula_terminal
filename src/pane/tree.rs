//! Binary split tree (`PaneNode`) for recursive horizontal and vertical pane layouts.

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
        ratio: f32,
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
