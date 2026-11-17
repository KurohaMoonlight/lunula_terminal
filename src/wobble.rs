use bytemuck::{Pod, Zeroable};
use std::time::Instant;
use wgpu::util::DeviceExt;

const GRID_COLS: usize = 10;
const GRID_ROWS: usize = 8;
const TOTAL_NODES: usize = GRID_COLS * GRID_ROWS;
pub const PADDING: f32 = 60.0;

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
    pub window_size: [f32; 2],
    pub time: f32,
    pub is_maximized: f32,
    pub hover_button: f32,
    pub _pad0: f32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControlAction {
    None,
    Close,
    Minimize,
    Maximize,
    Settings,
    Help,
    Drag,
}

pub struct WobbleWindow {
    pub target_pos: [f32; 2],
    pub size: [f32; 2],
    nodes: [SpringNode; TOTAL_NODES],
    springs: Vec<Spring>,

    pub is_dragging: bool,
    grab_uv: [f32; 2],
    prev_time: Instant,

    // Velocity injected from OS desktop window drag
    pub external_vel: [f32; 2],

    // Offscreen Content Texture (where text and terminal buffer are rendered)
    #[allow(dead_code)]
    content_texture: wgpu::Texture,
    pub content_view: wgpu::TextureView,
    pub content_size: [u32; 2],
    content_bind_group: wgpu::BindGroup,
    content_bind_group_layout: wgpu::BindGroupLayout,
    content_sampler: wgpu::Sampler,
    surface_format: wgpu::TextureFormat,

    pub pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    start_time: Instant,

    pub stiffness: f32,
    pub damping: f32,
    pub max_deform: f32,
    pub spring_shear: f32,
    pub opacity: f32,
    pub is_maximized: bool,
    pub hover_button: f32,
}

impl WobbleWindow {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        screen_size: [f32; 2],
        wobble_cfg: &crate::config::WobbleConfig,
        opacity: f32,
    ) -> Self {
        let padding = PADDING;
        let size = [
            (screen_size[0] - 2.0 * padding).max(200.0),
            (screen_size[1] - 2.0 * padding).max(150.0),
        ];
        let target_pos = [padding, padding];

        let mut nodes = [SpringNode {
            pos: [0.0, 0.0],
            vel: [0.0, 0.0],
        }; TOTAL_NODES];

        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let idx = r * GRID_COLS + c;
                nodes[idx].pos = [target_pos[0] + u * size[0], target_pos[1] + v * size[1]];
            }
        }

        let cell_w = size[0] / (GRID_COLS - 1) as f32;
        let cell_h = size[1] / (GRID_ROWS - 1) as f32;
        let mut springs = Vec::new();

        let k_struct = wobble_cfg.stiffness;
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

        let k_shear = wobble_cfg.spring_shear;
        let diag_len = (cell_w * cell_w + cell_h * cell_h).sqrt();
        for r in 0..(GRID_ROWS - 1) {
            for c in 0..(GRID_COLS - 1) {
                let tl = r * GRID_COLS + c;
                let tr = tl + 1;
                let bl = (r + 1) * GRID_COLS + c;
                let br = bl + 1;
                springs.push(Spring {
                    a: tl,
                    b: br,
                    rest_length: diag_len,
                    stiffness: k_shear,
                });
                springs.push(Spring {
                    a: tr,
                    b: bl,
                    rest_length: diag_len,
                    stiffness: k_shear,
                });
            }
        }

        // --- Offscreen Content Texture & Sampler ---
        let content_w = (size[0] as u32).max(64);
        let content_h = (size[1] as u32).max(64);
        let content_size = [content_w, content_h];

        let content_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Terminal Content Texture"),
            size: wgpu::Extent3d {
                width: content_w,
                height: content_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let content_view = content_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let content_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Terminal Content Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let content_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Content BG Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let content_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Content Bind Group"),
            layout: &content_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&content_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&content_sampler),
                },
            ],
        });

        // --- GPU Pipeline ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wobble Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wobble.wgsl").into()),
        });

        let uniforms = Uniforms {
            screen_size,
            window_size: size,
            time: 0.0,
            is_maximized: 0.0,
            hover_button: 0.0,
            _pad0: 0.0,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Wobble UB"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let ubgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Wobble UBGL"),
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

        let ubg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Wobble UBG"),
            layout: &ubgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Wobble PL"),
            bind_group_layouts: &[Some(&ubgl), Some(&content_bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wobble RP"),
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
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WVB"),
            size: (1024 * std::mem::size_of::<WobbleVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WIB"),
            size: (2048 * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            target_pos,
            size,
            nodes,
            springs,
            is_dragging: false,
            grab_uv: [0.5, 0.05],
            prev_time: Instant::now(),
            external_vel: [0.0, 0.0],
            content_texture,
            content_view,
            content_size,
            content_bind_group,
            content_bind_group_layout,
            content_sampler,
            surface_format,
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group: ubg,
            start_time: Instant::now(),
            stiffness: wobble_cfg.stiffness,
            damping: wobble_cfg.damping,
            max_deform: wobble_cfg.max_deform,
            spring_shear: wobble_cfg.spring_shear,
            opacity,
            is_maximized: false,
            hover_button: 0.0,
        }
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        screen_w: f32,
        screen_h: f32,
        is_maximized: bool,
    ) {
        self.is_maximized = is_maximized;
        let padding = if is_maximized { 0.0 } else { PADDING };
        self.size = [
            (screen_w - 2.0 * padding).max(200.0),
            (screen_h - 2.0 * padding).max(150.0),
        ];
        self.target_pos = [padding, padding];

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

        let cell_w = self.size[0] / (GRID_COLS - 1) as f32;
        let cell_h = self.size[1] / (GRID_ROWS - 1) as f32;
        for spring in &mut self.springs {
            let ca = spring.a % GRID_COLS;
            let ra = spring.a / GRID_COLS;
            let cb = spring.b % GRID_COLS;
            let rb = spring.b / GRID_COLS;
            let dc = (cb as f32 - ca as f32) * cell_w;
            let dr = (rb as f32 - ra as f32) * cell_h;
            spring.rest_length = (dc * dc + dr * dr).sqrt();
            let is_diagonal = ca != cb && ra != rb;
            spring.stiffness = if is_diagonal { self.spring_shear } else { self.stiffness };
        }

        // Resize offscreen texture (full window height)
        let content_w = (self.size[0] as u32).max(64);
        let content_h = (self.size[1] as u32).max(64);
        self.content_size = [content_w, content_h];

        self.content_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Terminal Content Texture"),
            size: wgpu::Extent3d {
                width: content_w,
                height: content_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.content_view = self
            .content_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.content_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Content Bind Group"),
            layout: &self.content_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.content_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.content_sampler),
                },
            ],
        });
    }

    pub fn screen_to_content_pos(&self, mx: f32, my: f32) -> Option<[f32; 2]> {
        let x0 = self.target_pos[0];
        let y0 = self.target_pos[1];
        if mx < x0 || mx > x0 + self.size[0] || my < y0 || my > y0 + self.size[1] {
            return None;
        }
        let px = mx - x0;
        let py = my - y0;
        Some([px, py])
    }

    pub fn handle_mouse_click(&mut self, mx: f32, my: f32) -> WindowControlAction {
        let x0 = self.target_pos[0];
        let y0 = self.target_pos[1];
        if mx >= x0 && mx <= x0 + self.size[0] && my >= y0 && my <= y0 + self.size[1] {
            let u = (mx - x0) / self.size[0];
            let v = (my - y0) / self.size[1];
            self.grab_uv = [u, v];

            let titlebar_h = 42.0;
            if my <= y0 + titlebar_h {
                let rx = mx - x0;
                let ry = my - y0;
                if (rx - 30.0).hypot(ry - 21.0) < 12.0 {
                    return WindowControlAction::Close;
                }
                if (rx - 52.0).hypot(ry - 21.0) < 12.0 {
                    return WindowControlAction::Minimize;
                }
                if (rx - 74.0).hypot(ry - 21.0) < 12.0 {
                    return WindowControlAction::Maximize;
                }

                let total_w = self.size[0];
                // [? Help] at total_w - 38.0..total_w - 6.0
                if rx >= total_w - 38.0 && rx <= total_w - 6.0 && ry >= 6.0 && ry <= 36.0 {
                    return WindowControlAction::Help;
                }
                // [⚙ Settings] at total_w - 74.0..total_w - 42.0
                if rx >= total_w - 74.0 && rx <= total_w - 42.0 && ry >= 6.0 && ry <= 36.0 {
                    return WindowControlAction::Settings;
                }

                if !self.is_maximized {
                    self.is_dragging = true;
                    return WindowControlAction::Drag;
                }
            }
        }
        WindowControlAction::None
    }

    pub fn get_hover_button(&self, mx: f32, my: f32) -> f32 {
        let x0 = self.target_pos[0];
        let y0 = self.target_pos[1];
        let titlebar_h = 42.0;
        if mx >= x0 && mx <= x0 + self.size[0] && my >= y0 && my <= y0 + titlebar_h {
            let rx = mx - x0;
            let ry = my - y0;
            if (rx - 30.0).hypot(ry - 21.0) < 12.0 {
                return 1.0;
            }
            if (rx - 52.0).hypot(ry - 21.0) < 12.0 {
                return 2.0;
            }
            if (rx - 74.0).hypot(ry - 21.0) < 12.0 {
                return 3.0;
            }
            let total_w = self.size[0];
            if rx >= total_w - 74.0 && rx <= total_w - 42.0 && ry >= 6.0 && ry <= 36.0 {
                return 4.0; // Settings hover
            }
            if rx >= total_w - 38.0 && rx <= total_w - 6.0 && ry >= 6.0 && ry <= 36.0 {
                return 5.0; // Help hover
            }
        }
        0.0
    }

    pub fn handle_mouse_up(&mut self) {
        self.is_dragging = false;
        self.external_vel = [0.0, 0.0];
    }

    pub fn inject_desktop_drag_velocity(&mut self, vel_x: f32, vel_y: f32) {
        let max_vel = 800.0;
        self.external_vel = [
            vel_x.clamp(-max_vel, max_vel),
            vel_y.clamp(-max_vel, max_vel),
        ];
    }

    pub fn update_physics(&mut self) {
        if self.is_maximized {
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
            return;
        }

        let now = Instant::now();
        let dt = (now - self.prev_time).as_secs_f32().min(0.033);
        self.prev_time = now;

        let sub_steps = 4;
        let sub_dt = dt / sub_steps as f32;

        let k_anchor = 180.0;
        let damping = self.damping;
        let max_deform = self.max_deform;
        let lag_scale = if self.is_dragging { 0.10 } else { 0.0 };

        let vel_x = self.external_vel[0];
        let vel_y = self.external_vel[1];

        for _ in 0..sub_steps {
            // 1. Internal mesh springs
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

            // 2. Anchor + Inertia Lag from desktop drag
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

                    let inertia_factor = if self.is_dragging {
                        dist * lag_scale
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

    pub fn is_wobbling(&self) -> bool {
        if self.is_maximized {
            return false;
        }
        if self.external_vel[0].abs() > 1.0 || self.external_vel[1].abs() > 1.0 {
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
                if node.vel[0].abs() > 0.1
                    || node.vel[1].abs() > 0.1
                    || (node.pos[0] - rest_x).abs() > 0.2
                    || (node.pos[1] - rest_y).abs() > 0.2
                {
                    return true;
                }
            }
        }
        false
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
            window_size: self.size,
            time: elapsed,
            is_maximized: if self.is_maximized { 1.0 } else { 0.0 },
            hover_button: self.hover_button,
            _pad0: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut vertices = Vec::new();
        let mut indices: Vec<u16> = Vec::new();

        // 1. Drop shadow (only in floating windowed mode)
        if !self.is_maximized {
            let shadow_off = 10.0;
            for r in 0..GRID_ROWS {
                for c in 0..GRID_COLS {
                    let u = c as f32 / (GRID_COLS - 1) as f32;
                    let v = r as f32 / (GRID_ROWS - 1) as f32;
                    let idx = r * GRID_COLS + c;
                    vertices.push(WobbleVertex {
                        position: [
                            self.nodes[idx].pos[0] + 3.0,
                            self.nodes[idx].pos[1] + shadow_off,
                        ],
                        uv: [u, v],
                        color: [0.0, 0.0, 0.0, 0.30],
                    });
                }
            }
            for r in 0..(GRID_ROWS - 1) {
                for c in 0..(GRID_COLS - 1) {
                    let tl = (r * GRID_COLS + c) as u16;
                    indices.extend_from_slice(&[
                        tl,
                        tl + GRID_COLS as u16,
                        tl + 1,
                        tl + 1,
                        tl + GRID_COLS as u16,
                        tl + GRID_COLS as u16 + 1,
                    ]);
                }
            }
        }

        // 2. Main terminal window
        let base = vertices.len() as u16;
        for r in 0..GRID_ROWS {
            for c in 0..GRID_COLS {
                let u = c as f32 / (GRID_COLS - 1) as f32;
                let v = r as f32 / (GRID_ROWS - 1) as f32;
                let idx = r * GRID_COLS + c;
                let color = if v < 0.08 {
                    [0.15, 0.16, 0.24, 0.98] // Titlebar
                } else {
                    [0.10, 0.10, 0.15, self.opacity] // Terminal body base color
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
                let tl = base + (r * GRID_COLS + c) as u16;
                indices.extend_from_slice(&[
                    tl,
                    tl + GRID_COLS as u16,
                    tl + 1,
                    tl + 1,
                    tl + GRID_COLS as u16,
                    tl + GRID_COLS as u16 + 1,
                ]);
            }
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.content_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}
