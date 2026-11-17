//! 2D mass-spring-damper mesh physics with distance-weighted grab anchoring,
//! EMA-smoothed drag inertia, symplectic Euler integration, and O(1) stationary early-exit.

use super::WobbleWindow;
use std::time::Instant;

pub const GRID_COLS: usize = 10;
pub const GRID_ROWS: usize = 8;
pub const TOTAL_NODES: usize = GRID_COLS * GRID_ROWS;
pub const PADDING: f32 = 60.0;
pub const CELLS_PER_LAYER: usize = (GRID_COLS - 1) * (GRID_ROWS - 1);
pub const INDICES_PER_LAYER: u32 = (CELLS_PER_LAYER * 6) as u32; // 378 indices

#[derive(Clone, Copy, Debug)]
pub struct SpringNode {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub a: usize,
    pub b: usize,
    pub rest_length: f32,
    pub stiffness: f32,
}

impl WobbleWindow {
    pub fn handle_mouse_up(&mut self) {
        if self.is_dragging {
            self.is_dragging = false;
            // Impart subtle forward momentum to distant corners upon release for natural overshoot
            let vx = self.external_vel[0];
            let vy = self.external_vel[1];
            for r in 0..GRID_ROWS {
                for c in 0..GRID_COLS {
                    let u = c as f32 / (GRID_COLS - 1) as f32;
                    let v = r as f32 / (GRID_ROWS - 1) as f32;
                    let dist = (u - self.grab_uv[0]).hypot(v - self.grab_uv[1]);
                    let idx = r * GRID_COLS + c;
                    self.nodes[idx].vel[0] += vx * dist * 0.18;
                    self.nodes[idx].vel[1] += vy * dist * 0.18;
                }
            }
            self.external_vel = [0.0, 0.0];
        }
    }

    pub fn inject_desktop_drag_velocity(&mut self, vel_x: f32, vel_y: f32) {
        let max_vel = 1600.0;
        let clamped_x = vel_x.clamp(-max_vel, max_vel);
        let clamped_y = vel_y.clamp(-max_vel, max_vel);
        // Smooth EMA filter eliminates jittery OS mouse-polling spikes
        let alpha = 0.35;
        self.external_vel[0] = self.external_vel[0] * (1.0 - alpha) + clamped_x * alpha;
        self.external_vel[1] = self.external_vel[1] * (1.0 - alpha) + clamped_y * alpha;
    }

    /// Advances spring mesh physics when active; immediately exits in O(1) when stationary.
    pub fn update_physics(&mut self) -> bool {
        let now = Instant::now();
        let dt = (now - self.prev_time).as_secs_f32().clamp(0.0005, 0.033);
        self.prev_time = now;

        if self.is_maximized {
            return false;
        }

        if !self.is_dragging && !self.is_wobbling() {
            if self.was_wobbling {
                // Snap all nodes to exact rest coordinates for razor-sharp text at rest
                for r in 0..GRID_ROWS {
                    for c in 0..GRID_COLS {
                        let u = c as f32 / (GRID_COLS - 1) as f32;
                        let v = r as f32 / (GRID_ROWS - 1) as f32;
                        let idx = r * GRID_COLS + c;
                        self.nodes[idx].pos = [
                            self.target_pos[0] + u * self.size[0],
                            self.target_pos[1] + v * self.size[1],
                        ];
                        self.nodes[idx].vel = [0.0, 0.0];
                    }
                }
                self.was_wobbling = false;
                self.mesh_dirty = true;
            }
            return false;
        }

        self.was_wobbling = true;

        // Smoothly decay external drag velocity when mouse pauses mid-drag
        if self.is_dragging {
            let decay = (-12.0 * dt).exp();
            self.external_vel[0] *= decay;
            self.external_vel[1] *= decay;
        }

        let sub_steps = 8;
        let sub_dt = dt / sub_steps as f32;

        let base_anchor = (self.stiffness * 0.55).max(80.0);
        let damping = self.damping;
        let max_deform = self.max_deform;
        let lag_scale = if self.is_dragging { 0.075 } else { 0.0 };

        let vel_x = self.external_vel[0];
        let vel_y = self.external_vel[1];

        for _ in 0..sub_steps {
            // 1. Internal structural & shear mesh springs
            for spring in &self.springs {
                let pa = self.nodes[spring.a].pos;
                let pb = self.nodes[spring.b].pos;
                let dx = pb[0] - pa[0];
                let dy = pb[1] - pa[1];
                let dist = (dx * dx + dy * dy).sqrt().max(0.001);
                let stretch = dist - spring.rest_length;
                let f = spring.stiffness * stretch;
                let fx = (dx / dist) * f;
                let fy = (dy / dist) * f;

                self.nodes[spring.a].vel[0] += fx * sub_dt;
                self.nodes[spring.a].vel[1] += fy * sub_dt;
                self.nodes[spring.b].vel[0] -= fx * sub_dt;
                self.nodes[spring.b].vel[1] -= fy * sub_dt;
            }

            // 2. Distance-weighted anchor + smooth inertia lag (Symplectic Euler)
            for r in 0..GRID_ROWS {
                for c in 0..GRID_COLS {
                    let u = c as f32 / (GRID_COLS - 1) as f32;
                    let v = r as f32 / (GRID_ROWS - 1) as f32;
                    let idx = r * GRID_COLS + c;

                    let rest_x = self.target_pos[0] + u * self.size[0];
                    let rest_y = self.target_pos[1] + v * self.size[1];

                    let du = u - self.grab_uv[0];
                    let dv = v - self.grab_uv[1];
                    let dist = (du * du + dv * dv).sqrt();

                    // Smooth quadratic falloff: grab area stays firm, opposite corners flex organically
                    let falloff = (dist / 1.414).clamp(0.0, 1.0);
                    let local_k_anchor = base_anchor * (1.85 - 1.25 * falloff);

                    let inertia_factor = if self.is_dragging {
                        (falloff * falloff * 0.6 + falloff * 0.4) * lag_scale
                    } else {
                        0.0
                    };

                    let lag_x = -vel_x * inertia_factor;
                    let lag_y = -vel_y * inertia_factor;

                    let mut disp_x = self.nodes[idx].pos[0] - (rest_x + lag_x);
                    let mut disp_y = self.nodes[idx].pos[1] - (rest_y + lag_y);

                    let disp_len = (disp_x * disp_x + disp_y * disp_y).sqrt();
                    if disp_len > max_deform {
                        let scale = max_deform / disp_len;
                        disp_x *= scale;
                        disp_y *= scale;
                        self.nodes[idx].pos[0] = rest_x + lag_x + disp_x;
                        self.nodes[idx].pos[1] = rest_y + lag_y + disp_y;
                    }

                    let anchor_fx = -local_k_anchor * disp_x;
                    let anchor_fy = -local_k_anchor * disp_y;

                    self.nodes[idx].vel[0] += anchor_fx * sub_dt;
                    self.nodes[idx].vel[1] += anchor_fy * sub_dt;

                    let damp_factor = (-damping * sub_dt).exp();
                    self.nodes[idx].vel[0] *= damp_factor;
                    self.nodes[idx].vel[1] *= damp_factor;

                    self.nodes[idx].pos[0] += self.nodes[idx].vel[0] * sub_dt;
                    self.nodes[idx].pos[1] += self.nodes[idx].vel[1] * sub_dt;
                }
            }
        }

        self.mesh_dirty = true;
        true
    }

    pub fn is_wobbling(&self) -> bool {
        if self.is_maximized {
            return false;
        }
        if self.external_vel[0].abs() > 1.5 || self.external_vel[1].abs() > 1.5 {
            return true;
        }
        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let idx = r * GRID_COLS + c;
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let rest_x = self.target_pos[0] + u * self.size[0];
                let rest_y = self.target_pos[1] + v * self.size[1];
                let node = &self.nodes[idx];
                if node.vel[0].abs() > 0.15
                    || node.vel[1].abs() > 0.15
                    || (node.pos[0] - rest_x).abs() > 0.25
                    || (node.pos[1] - rest_y).abs() > 0.25
                {
                    return true;
                }
            }
        }
        false
    }
}
