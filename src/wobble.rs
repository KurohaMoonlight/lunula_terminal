use bytemuck::{Pod, Zeroable};
use std::time::Instant;
use wgpu::util::DeviceExt;

const GRID_COLS: usize = 10;
const GRID_ROWS: usize = 8;
const TOTAL_NODES: usize = GRID_COLS * GRID_ROWS;
pub const PADDING: f32 = 50.0;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct WobbleVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub screen_size: [f32; 2],
    pub time: f32,
    pub _padding: f32,
}

#[derive(Clone, Copy, Debug)]
struct SpringNode {
    pos: [f32; 2],
    vel: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
struct Spring {
    a: usize,
    b: usize,
    rest_length: f32,
    stiffness: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct WobbleParams {
    pub max_bend: f32,
    pub damping: f32,
    pub lag: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControlAction {
    None,
    Close,
    Minimize,
    Maximize,
    Drag,
}

pub struct WobbleWindow {
    pub target_pos: [f32; 2],
    pub size: [f32; 2],
    nodes: [SpringNode; TOTAL_NODES],
    springs: Vec<Spring>,

    pub params: WobbleParams,
    pub is_dragging: bool,
    grab_uv: [f32; 2],
    prev_time: Instant,

    // Velocity injected from OS desktop window drag
    pub external_vel: [f32; 2],

    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    start_time: Instant,
}

impl WobbleWindow {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        screen_size: [f32; 2],
    ) -> Self {
        let size = [
            (screen_size[0] - 2.0 * PADDING).max(200.0),
            (screen_size[1] - 2.0 * PADDING).max(150.0),
        ];
        let target_pos = [PADDING, PADDING];

        let mut nodes = [SpringNode {
            pos: [0.0, 0.0],
            vel: [0.0, 0.0],
        }; TOTAL_NODES];

        let cell_w = size[0] / (GRID_COLS - 1) as f32;
        let cell_h = size[1] / (GRID_ROWS - 1) as f32;

        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let idx = r * GRID_COLS + c;
                nodes[idx].pos = [target_pos[0] + u * size[0], target_pos[1] + v * size[1]];
                nodes[idx].vel = [0.0, 0.0];
            }
        }

        let mut springs = Vec::new();
        let k_struct = 700.0;
        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let idx = r * GRID_COLS + c;
                if c + 1 < GRID_COLS {
                    springs.push(Spring {
                        a: idx,
                        b: r * GRID_COLS + (c + 1),
                        rest_length: cell_w,
                        stiffness: k_struct,
                    });
                }
                if r + 1 < GRID_ROWS {
                    springs.push(Spring {
                        a: idx,
                        b: (r + 1) * GRID_COLS + c,
                        rest_length: cell_h,
                        stiffness: k_struct,
                    });
                }
            }
        }

        let k_shear = 450.0;
        let diag_len = (cell_w * cell_w + cell_h * cell_h).sqrt();
        for r in 0..(GRID_ROWS - 1) {
            for c in 0..(GRID_COLS - 1) {
                let top_left = r * GRID_COLS + c;
                let top_right = top_left + 1;
                let bottom_left = (r + 1) * GRID_COLS + c;
                let bottom_right = bottom_left + 1;

                springs.push(Spring {
                    a: top_left,
                    b: bottom_right,
                    rest_length: diag_len,
                    stiffness: k_shear,
                });
                springs.push(Spring {
                    a: top_right,
                    b: bottom_left,
                    rest_length: diag_len,
                    stiffness: k_shear,
                });
            }
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wobble Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wobble.wgsl").into()),
        });

        let uniforms = Uniforms {
            screen_size,
            time: 0.0,
            _padding: 0.0,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Wobble Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Wobble Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Wobble Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Wobble Pipeline Layout"),
            bind_group_layouts: &[Some(&uniform_bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wobble Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WobbleVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let max_vertices = 1024;
        let max_indices = 2048;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Wobble Vertex Buffer"),
            size: (max_vertices * std::mem::size_of::<WobbleVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Wobble Index Buffer"),
            size: (max_indices * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            target_pos,
            size,
            nodes,
            springs,
            // User-calibrated parameters!
            params: WobbleParams {
                max_bend: 35.0,
                damping: 14.5,
                lag: 0.077,
            },
            is_dragging: false,
            grab_uv: [0.5, 0.05],
            prev_time: Instant::now(),
            external_vel: [0.0, 0.0],
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group,
            start_time: Instant::now(),
        }
    }

    pub fn resize(&mut self, screen_w: f32, screen_h: f32) {
        self.size = [
            (screen_w - 2.0 * PADDING).max(200.0),
            (screen_h - 2.0 * PADDING).max(150.0),
        ];
        self.target_pos = [PADDING, PADDING];

        let cell_w = self.size[0] / (GRID_COLS - 1) as f32;
        let cell_h = self.size[1] / (GRID_ROWS - 1) as f32;

        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let idx = r * GRID_COLS + c;
                self.nodes[idx].pos = [self.target_pos[0] + u * self.size[0], self.target_pos[1] + v * self.size[1]];
            }
        }
        for spring in &mut self.springs {
            // Update rest lengths proportionally
            let idx_a = spring.a;
            let idx_b = spring.b;
            let ca = idx_a % GRID_COLS;
            let ra = idx_a / GRID_COLS;
            let cb = idx_b % GRID_COLS;
            let rb = idx_b / GRID_COLS;
            let dc = (cb as f32 - ca as f32) * cell_w;
            let dr = (rb as f32 - ra as f32) * cell_h;
            spring.rest_length = (dc * dc + dr * dr).sqrt();
        }
    }

    pub fn handle_mouse_click(&mut self, mouse_x: f32, mouse_y: f32) -> WindowControlAction {
        let min_x = self.target_pos[0];
        let max_x = self.target_pos[0] + self.size[0];
        let min_y = self.target_pos[1];
        let titlebar_h = 42.0;

        if mouse_x >= min_x && mouse_x <= max_x && mouse_y >= min_y && mouse_y <= min_y + self.size[1] {
            let u = (mouse_x - min_x) / self.size[0];
            let v = (mouse_y - min_y) / self.size[1];
            self.grab_uv = [u, v];

            // Inside titlebar
            if mouse_y <= min_y + titlebar_h {
                // Button circles
                let rel_x = mouse_x - min_x;
                let rel_y = mouse_y - min_y;

                // Close (red): rel_x in ~22..38, rel_y in ~13..29
                if (rel_x - 30.0).hypot(rel_y - 21.0) < 12.0 {
                    return WindowControlAction::Close;
                }
                // Minimize (yellow): rel_x in ~46..62
                if (rel_x - 52.0).hypot(rel_y - 21.0) < 12.0 {
                    return WindowControlAction::Minimize;
                }
                // Maximize (green): rel_x in ~68..84
                if (rel_x - 74.0).hypot(rel_y - 21.0) < 12.0 {
                    return WindowControlAction::Maximize;
                }

                self.is_dragging = true;
                return WindowControlAction::Drag;
            }

            // Inside window body
            self.is_dragging = true;
            return WindowControlAction::Drag;
        }

        WindowControlAction::None
    }

    pub fn handle_mouse_up(&mut self) {
        self.is_dragging = false;
        self.external_vel = [0.0, 0.0];
    }

    pub fn inject_desktop_drag_velocity(&mut self, vel_x: f32, vel_y: f32) {
        // Clamp to sane range — raw desktop velocity can be thousands of px/s
        let max_vel = 800.0;
        self.external_vel = [
            vel_x.clamp(-max_vel, max_vel),
            vel_y.clamp(-max_vel, max_vel),
        ];
    }

    pub fn update_physics(&mut self) {
        let now = Instant::now();
        let dt = (now - self.prev_time).as_secs_f32().min(0.033);
        self.prev_time = now;

        let sub_steps = 4;
        let sub_dt = dt / sub_steps as f32;

        let k_anchor = 180.0;
        let damping = self.params.damping;
        let max_bend = self.params.max_bend;
        // Scale down lag for desktop drag — screen velocities are much larger
        let lag_scale = if self.is_dragging { self.params.lag * 0.15 } else { 0.0 };

        let vel_x = self.external_vel[0];
        let vel_y = self.external_vel[1];

        for _ in 0..sub_steps {
            // 1. Internal mesh springs
            for spring in &self.springs {
                let pos_a = self.nodes[spring.a].pos;
                let pos_b = self.nodes[spring.b].pos;

                let dx = pos_b[0] - pos_a[0];
                let dy = pos_b[1] - pos_a[1];
                let dist = (dx * dx + dy * dy).sqrt().max(0.0001);

                let stretch = dist - spring.rest_length;
                let force = spring.stiffness * stretch;

                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;

                self.nodes[spring.a].vel[0] += fx * sub_dt;
                self.nodes[spring.a].vel[1] += fy * sub_dt;
                self.nodes[spring.b].vel[0] -= fx * sub_dt;
                self.nodes[spring.b].vel[1] -= fy * sub_dt;
            }

            // 2. Anchor + Inertia Lag from desktop drag
            for r in 0..GRID_ROWS {
                for c in 0..GRID_COLS {
                    let u = c as f32 / (GRID_COLS - 1) as f32;
                    let v = r as f32 / (GRID_ROWS - 1) as f32;
                    let idx = r * GRID_COLS + c;

                    let rest_x = self.target_pos[0] + u * self.size[0];
                    let rest_y = self.target_pos[1] + v * self.size[1];

                    // Distance from grab point
                    let du = u - self.grab_uv[0];
                    let dv = v - self.grab_uv[1];
                    let dist_from_grab = (du * du + dv * dv).sqrt();

                    let inertia_factor = if self.is_dragging {
                        dist_from_grab * lag_scale
                    } else {
                        0.0
                    };

                    let lag_x = -vel_x * inertia_factor;
                    let lag_y = -vel_y * inertia_factor;

                    let mut disp_x = self.nodes[idx].pos[0] - (rest_x + lag_x);
                    let mut disp_y = self.nodes[idx].pos[1] - (rest_y + lag_y);

                    let disp_len = (disp_x * disp_x + disp_y * disp_y).sqrt();
                    if disp_len > max_bend {
                        let scale = max_bend / disp_len;
                        disp_x *= scale;
                        disp_y *= scale;
                        self.nodes[idx].pos[0] = rest_x + lag_x + disp_x;
                        self.nodes[idx].pos[1] = rest_y + lag_y + disp_y;
                    }

                    let anchor_fx = -k_anchor * disp_x;
                    let anchor_fy = -k_anchor * disp_y;

                    self.nodes[idx].vel[0] += anchor_fx * sub_dt;
                    self.nodes[idx].vel[1] += anchor_fy * sub_dt;

                    self.nodes[idx].vel[0] -= self.nodes[idx].vel[0] * damping * sub_dt;
                    self.nodes[idx].vel[1] -= self.nodes[idx].vel[1] * damping * sub_dt;

                    self.nodes[idx].pos[0] += self.nodes[idx].vel[0] * sub_dt;
                    self.nodes[idx].pos[1] += self.nodes[idx].vel[1] * sub_dt;
                }
            }
        }
    }

    pub fn render(
        &self,
        queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass,
        screen_size: [f32; 2],
    ) {
        let elapsed = self.start_time.elapsed().as_secs_f32();

        let uniforms = Uniforms {
            screen_size,
            time: elapsed,
            _padding: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        // 1. Drop shadow behind the wobbling window
        let shadow_offset = 12.0;
        let shadow_nodes: Vec<[f32; 2]> = self.nodes.iter().map(|n| [n.pos[0] + 4.0, n.pos[1] + shadow_offset]).collect();
        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let idx = r * GRID_COLS + c;

                vertices.push(WobbleVertex {
                    position: shadow_nodes[idx],
                    uv: [u, v],
                    color: [0.0, 0.0, 0.0, 0.35],
                });
            }
        }

        for r in 0..(GRID_ROWS - 1) {
            for c in 0..(GRID_COLS - 1) {
                let top_left = (r * GRID_COLS + c) as u16;
                let top_right = top_left + 1;
                let bottom_left = ((r + 1) * GRID_COLS + c) as u16;
                let bottom_right = bottom_left + 1;

                indices.push(top_left);
                indices.push(bottom_left);
                indices.push(top_right);

                indices.push(top_right);
                indices.push(bottom_left);
                indices.push(bottom_right);
            }
        }

        // 2. Main wobbling terminal window
        let base_idx = vertices.len() as u16;
        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let idx = r * GRID_COLS + c;

                let color = if v < 0.10 {
                    // Header titlebar color (Catppuccin Mantle)
                    [0.15, 0.16, 0.24, 0.98]
                } else {
                    // Main terminal body color (Deep modern acrylic dark glass)
                    [0.10, 0.10, 0.15, 0.95]
                };

                vertices.push(WobbleVertex {
                    position: self.nodes[idx].pos,
                    uv: [u, v],
                    color,
                });
            }
        }

        for r in 0..(GRID_ROWS - 1) {
            for c in 0..(GRID_COLS - 1) {
                let top_left = base_idx + (r * GRID_COLS + c) as u16;
                let top_right = top_left + 1;
                let bottom_left = base_idx + ((r + 1) * GRID_COLS + c) as u16;
                let bottom_right = bottom_left + 1;

                indices.push(top_left);
                indices.push(bottom_left);
                indices.push(top_right);

                indices.push(top_right);
                indices.push(bottom_left);
                indices.push(bottom_right);
            }
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}
