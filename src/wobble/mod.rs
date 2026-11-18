//! Wobbly jelly window mesh renderer with static GPU index buffer and dirty-gated vertex upload.

pub mod physics;

pub use physics::{
    Spring, SpringNode, GRID_COLS, GRID_ROWS, INDICES_PER_LAYER, PADDING,
    TOTAL_NODES,
};

use bytemuck::{Pod, Zeroable};
use std::time::Instant;
use wgpu::util::DeviceExt;

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
    /// 1.0 when the swapchain is an sRGB format (shader linearises its palette).
    pub srgb_target: f32,
    /// (energy 0..1, horizontal slosh px, vertical bob px, extra wave time s)
    pub wave: [f32; 4],
    /// (surface tilt -1..1, reserved, reserved, reserved)
    pub wave2: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControlAction {
    None,
    Close,
    Minimize,
    Maximize,
    #[allow(dead_code)]
    Settings,
    Help,
    Drag,
}

pub struct WobbleWindow {
    pub target_pos: [f32; 2],
    pub size: [f32; 2],
    pub(crate) nodes: [SpringNode; TOTAL_NODES],
    pub(crate) springs: Vec<Spring>,

    pub is_dragging: bool,
    pub(crate) grab_uv: [f32; 2],
    pub(crate) prev_time: Instant,
    pub external_vel: [f32; 2],

    /// Reactive Kanagawa wave state (see `physics::update_wave`).
    pub(crate) wave: physics::WaveState,
    pub wave_reactivity: f32,

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

    pub mesh_dirty: bool,
    pub(crate) was_wobbling: bool,
    scratch_vertices: Vec<WobbleVertex>,
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
        let mut springs = Vec::with_capacity(280);

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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wobble Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../render/shaders/wobble.wgsl").into()),
        });

        let uniforms = Uniforms {
            screen_size,
            window_size: size,
            time: 0.0,
            is_maximized: 0.0,
            hover_button: 0.0,
            srgb_target: if surface_format.is_srgb() { 1.0 } else { 0.0 },
            wave: [0.0; 4],
            wave2: [0.0; 4],
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

        // Pre-build static index buffer once (Layer 0: shadow 0..378, Layer 1: window body 378..756)
        let mut static_indices = Vec::with_capacity((INDICES_PER_LAYER * 2) as usize);
        for layer in 0..2u16 {
            let base = layer * (TOTAL_NODES as u16);
            for r in 0..(GRID_ROWS - 1) {
                for c in 0..(GRID_COLS - 1) {
                    let tl = base + (r * GRID_COLS + c) as u16;
                    static_indices.extend_from_slice(&[
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

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Static Wobble Index Buffer"),
            contents: bytemuck::cast_slice(&static_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Wobble Vertex Buffer"),
            size: (TOTAL_NODES * 2 * std::mem::size_of::<WobbleVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
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
            wave: physics::WaveState::default(),
            wave_reactivity: wobble_cfg.wave_reactivity,
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
            mesh_dirty: true,
            was_wobbling: false,
            scratch_vertices: Vec::with_capacity(TOTAL_NODES * 2),
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
            spring.stiffness = if is_diagonal {
                self.spring_shear
            } else {
                self.stiffness
            };
        }

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

        self.mesh_dirty = true;
    }

    pub fn screen_to_content_pos(&self, mx: f32, my: f32) -> Option<[f32; 2]> {
        let x0 = self.target_pos[0];
        let y0 = self.target_pos[1];
        if mx < x0 || mx > x0 + self.size[0] || my < y0 || my > y0 + self.size[1] {
            return None;
        }
        Some([mx - x0, my - y0])
    }

    pub fn handle_mouse_click(&mut self, mx: f32, my: f32) -> WindowControlAction {
        let x0 = self.target_pos[0];
        let y0 = self.target_pos[1];
        if mx >= x0 && mx <= x0 + self.size[0] && my >= y0 && my <= y0 + self.size[1] {
            let u = (mx - x0) / self.size[0];
            let v = (my - y0) / self.size[1];
            self.grab_uv = [u, v];

            let titlebar_h = 48.0;
            if my <= y0 + titlebar_h {
                let rx = mx - x0;
                let ry = my - y0;
                if (rx - 30.0).hypot(ry - 24.0) < 12.0 {
                    return WindowControlAction::Close;
                }
                if (rx - 52.0).hypot(ry - 24.0) < 12.0 {
                    return WindowControlAction::Minimize;
                }
                if (rx - 74.0).hypot(ry - 24.0) < 12.0 {
                    return WindowControlAction::Maximize;
                }

                let total_w = self.size[0];
                if rx >= total_w - 38.0 && rx <= total_w - 6.0 && ry >= 9.0 && ry <= 39.0 {
                    return WindowControlAction::Help;
                }
                if rx >= total_w - 74.0 && rx <= total_w - 42.0 && ry >= 9.0 && ry <= 39.0 {
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
        let titlebar_h = 48.0;
        if mx >= x0 && mx <= x0 + self.size[0] && my >= y0 && my <= y0 + titlebar_h {
            let rx = mx - x0;
            let ry = my - y0;
            if (rx - 30.0).hypot(ry - 24.0) < 12.0 {
                return 1.0;
            }
            if (rx - 52.0).hypot(ry - 24.0) < 12.0 {
                return 2.0;
            }
            if (rx - 74.0).hypot(ry - 24.0) < 12.0 {
                return 3.0;
            }
            let total_w = self.size[0];
            if rx >= total_w - 74.0 && rx <= total_w - 42.0 && ry >= 9.0 && ry <= 39.0 {
                return 4.0;
            }
            if rx >= total_w - 38.0 && rx <= total_w - 6.0 && ry >= 9.0 && ry <= 39.0 {
                return 5.0;
            }
        }
        0.0
    }

    /// Renders the wobble mesh using the static index buffer and dirty-gated vertex buffer upload.
    pub fn render(
        &mut self,
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
            srgb_target: if self.surface_format.is_srgb() { 1.0 } else { 0.0 },
            wave: self.wave.uniform_wave(),
            wave2: self.wave.uniform_wave2(),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        if self.mesh_dirty {
            self.scratch_vertices.clear();
            let shadow_off = 10.0;

            // Layer 0: Drop shadow vertices (80 nodes)
            for r in 0..GRID_ROWS {
                for c in 0..GRID_COLS {
                    let u = c as f32 / (GRID_COLS - 1) as f32;
                    let v = r as f32 / (GRID_ROWS - 1) as f32;
                    let idx = r * GRID_COLS + c;
                    self.scratch_vertices.push(WobbleVertex {
                        position: [
                            self.nodes[idx].pos[0] + 3.0,
                            self.nodes[idx].pos[1] + shadow_off,
                        ],
                        uv: [u, v],
                        color: [0.0, 0.0, 0.0, 0.30],
                    });
                }
            }

            // Layer 1: Main terminal window vertices (80 nodes)
            for r in 0..GRID_ROWS {
                for c in 0..GRID_COLS {
                    let u = c as f32 / (GRID_COLS - 1) as f32;
                    let v = r as f32 / (GRID_ROWS - 1) as f32;
                    let idx = r * GRID_COLS + c;
                    let color = if v < 0.08 {
                        [0.15, 0.16, 0.24, 0.98]
                    } else {
                        [0.10, 0.10, 0.15, self.opacity]
                    };
                    self.scratch_vertices.push(WobbleVertex {
                        position: self.nodes[idx].pos,
                        uv: [u, v],
                        color,
                    });
                }
            }

            queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.scratch_vertices),
            );
            self.mesh_dirty = false;
        }

        let index_range = if self.is_maximized {
            INDICES_PER_LAYER..(INDICES_PER_LAYER * 2)
        } else {
            0..(INDICES_PER_LAYER * 2)
        };

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.content_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(index_range, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_wobble_shader_syntax() {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            if let Ok(adapter) = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
            {
                if let Ok((device, _)) = adapter
                    .request_device(&wgpu::DeviceDescriptor::default())
                    .await
                {
                    let _shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("Test Wobble Shader"),
                        source: wgpu::ShaderSource::Wgsl(
                            include_str!("../render/shaders/wobble.wgsl").into(),
                        ),
                    });
                }
            }
        });
    }
}
