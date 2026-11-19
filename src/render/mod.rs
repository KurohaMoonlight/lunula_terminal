//! GPU text rendering (`glyphon`) and batched background quad pipeline (`wgpu`).
//!
//! Optimization highlights:
//! - Reuses an internal `scratch_vertices: Vec<BgVertex>` buffer across frames so uploading
//!   background and decoration quads requires zero heap allocations in the render hot-path.

use crate::terminal::{BgRect, CellStyle, ColorRgb};
use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, Style,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BlendState, BufferBindingType, BufferDescriptor,
    BufferUsages, ColorTargetState, ColorWrites, Device, FragmentState, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, Queue,
    RenderPass, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
    VertexStepMode,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BgVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl BgVertex {
    pub const fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<BgVertex>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

impl ImageVertex {
    pub const fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<ImageVertex>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub struct ImageDrawCall {
    pub image_id: u32,
    pub vertex_start: u32,
}

#[allow(dead_code)]

/// sRGB-encoded channel (0..1) -> linear light. Needed because the swapchain is an sRGB
/// format: hex colours written straight into vertex colours would otherwise look washed-out.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn srgb_u8_lut() -> &'static [f32; 256] {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            *v = srgb_to_linear(i as f32 / 255.0);
        }
        t
    })
}

pub struct TextEngine {
    pub font_system: FontSystem,
    pub font_family: String,
    pub requested_font_family: String,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub renderer: TextRenderer,
    pub buffer: Buffer,

    pub cell_width: f32,
    pub cell_height: f32,
    pub font_size: f32,
    pub line_height: f32,

    pub screen_size: [f32; 2],
    pub bg_pipeline: RenderPipeline,
    pub bg_bind_group: BindGroup,
    pub bg_uniform_buffer: wgpu::Buffer,
    pub bg_vertex_buffer: wgpu::Buffer,
    pub bg_vertex_capacity: usize,
    pub bg_vertex_count: u32,
    scratch_vertices: Vec<BgVertex>,
    /// True when the render target is an sRGB format: palette bytes must be linearised.
    pub srgb_target: bool,

    pub image_pipeline: RenderPipeline,
    pub image_bind_group_layout: wgpu::BindGroupLayout,
    pub image_sampler: wgpu::Sampler,
    pub image_vertex_buffer: wgpu::Buffer,
    pub image_vertex_capacity: usize,
    pub image_textures: std::collections::HashMap<u32, (wgpu::Texture, BindGroup)>,
    pub active_image_draws: Vec<ImageDrawCall>,
    scratch_image_vertices: Vec<ImageVertex>,
}

impl TextEngine {
    pub fn resolve_family_name(font_system: &FontSystem, preferred: &str) -> (String, bool) {
        let db = font_system.db();
        if !preferred.trim().is_empty() {
            let pref = preferred.trim();
            for face in db.faces() {
                for (fam, _) in &face.families {
                    if fam.eq_ignore_ascii_case(pref) {
                        return (fam.clone(), true);
                    }
                }
            }
        }

        let candidates = [
            "0xProto Nerd Font Mono",
            "0xProto Nerd Font",
            "JetBrainsMono Nerd Font",
            "JetBrains Mono",
            "FiraCode Nerd Font",
            "Fira Code",
            "Hack Nerd Font",
            "Hack",
            "Cascadia Code",
            "Cascadia Mono",
            "Consolas",
        ];

        for cand in candidates {
            for face in db.faces() {
                for (fam, _) in &face.families {
                    if fam.eq_ignore_ascii_case(cand) {
                        return (fam.clone(), false);
                    }
                }
            }
        }

        (db.family_name(&Family::Monospace).to_string(), false)
    }

    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        screen_size: [f32; 2],
        font_size: f32,
        font_family: &str,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let (resolved_font, is_exact) = Self::resolve_family_name(&font_system, font_family);
        if !is_exact && !font_family.is_empty() {
            eprintln!(
                "[Lunula] Font '{}' not found on system. Auto-selected monospaced font: '{}'",
                font_family, resolved_font
            );
        }
        font_system.db_mut().set_monospace_family(&resolved_font);

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut viewport = Viewport::new(device, &cache);

        viewport.update(
            queue,
            Resolution {
                width: screen_size[0] as u32,
                height: screen_size[1] as u32,
            },
        );

        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);

        let line_height = (font_size * 1.5).round().max(10.0);
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size, line_height));

        buffer.set_text(
            "M",
            &Attrs::new().family(Family::Name(&resolved_font)),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut font_system, false);

        let cell_width = buffer
            .layout_runs()
            .next()
            .and_then(|run| run.glyphs.first())
            .map(|g| g.w)
            .unwrap_or(8.7);
        let cell_height = line_height;

        let bg_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Bg Shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/bg.wgsl").into()),
        });

        let bg_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Bg Uniform Buffer"),
            contents: bytemuck::cast_slice(&screen_size),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bg_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Bg Bind Group Layout"),
                entries: &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bg_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Bg Bind Group"),
            layout: &bg_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: bg_uniform_buffer.as_entire_binding(),
            }],
        });

        let bg_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Bg Pipeline Layout"),
            bind_group_layouts: &[Some(&bg_bind_group_layout)],
            immediate_size: 0,
        });

        let bg_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Bg Render Pipeline"),
            layout: Some(&bg_pipeline_layout),
            vertex: VertexState {
                module: &bg_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(BgVertex::desc())],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &bg_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let initial_capacity = 2048;
        let bg_vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Bg Vertex Buffer"),
            size: (initial_capacity * std::mem::size_of::<BgVertex>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let image_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Kitty Image Shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
        });

        let image_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Kitty Image Texture Bind Group Layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let image_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Kitty Image Pipeline Layout"),
            bind_group_layouts: &[
                Some(&bg_bind_group_layout),
                Some(&image_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let image_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Kitty Image Render Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(ImageVertex::desc())],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Kitty Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let image_vertex_capacity = 256;
        let image_vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Kitty Image Vertex Buffer"),
            size: (image_vertex_capacity * std::mem::size_of::<ImageVertex>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            font_system,
            font_family: resolved_font,
            requested_font_family: font_family.to_string(),
            swash_cache,
            viewport,
            atlas,
            renderer,
            buffer,
            cell_width,
            cell_height,
            font_size,
            line_height,
            screen_size,
            bg_pipeline,
            bg_bind_group,
            bg_uniform_buffer,
            bg_vertex_buffer,
            bg_vertex_capacity: initial_capacity,
            bg_vertex_count: 0,
            scratch_vertices: Vec::with_capacity(initial_capacity),
            srgb_target: format.is_srgb(),
            image_pipeline,
            image_bind_group_layout,
            image_sampler,
            image_vertex_buffer,
            image_vertex_capacity,
            image_textures: std::collections::HashMap::new(),
            active_image_draws: Vec::new(),
            scratch_image_vertices: Vec::with_capacity(image_vertex_capacity),
        }
    }

    pub fn resize(&mut self, queue: &Queue, screen_w: f32, screen_h: f32) {
        self.screen_size = [screen_w, screen_h];
        self.viewport.update(
            queue,
            Resolution {
                width: screen_w.max(1.0) as u32,
                height: screen_h.max(1.0) as u32,
            },
        );
        queue.write_buffer(
            &self.bg_uniform_buffer,
            0,
            bytemuck::cast_slice(&self.screen_size),
        );
    }

    pub fn update_font_size(&mut self, font_size: f32) {
        self.font_size = font_size;
        self.line_height = (font_size * 1.5).round().max(10.0);
        self.buffer
            .set_metrics(Metrics::new(self.font_size, self.line_height));
        self.buffer.set_text(
            "M",
            &Attrs::new().family(Family::Name(&self.font_family)),
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);

        self.cell_width = self
            .buffer
            .layout_runs()
            .next()
            .and_then(|run| run.glyphs.first())
            .map(|g| g.w)
            .unwrap_or(8.7);
        self.cell_height = self.line_height;
    }

    pub fn update_font_family(&mut self, font_family: &str) {
        self.requested_font_family = font_family.to_string();
        let (resolved_font, is_exact) = Self::resolve_family_name(&self.font_system, font_family);
        if !is_exact && !font_family.is_empty() {
            eprintln!(
                "[Lunula] Font '{}' not found on system. Auto-selected monospaced font: '{}'",
                font_family, resolved_font
            );
        }
        self.font_family = resolved_font;
        self.font_system
            .db_mut()
            .set_monospace_family(&self.font_family);
        self.update_font_size(self.font_size);
    }

    #[allow(dead_code)]
    pub fn set_terminal_content(
        &mut self,
        device: &Device,
        queue: &Queue,
        spans: &[(String, CellStyle)],
        bg_rects: &[BgRect],
    ) {
        let mut tmp = Buffer::new(&mut self.font_system, Metrics::new(self.font_size, self.line_height));
        std::mem::swap(&mut tmp, &mut self.buffer);
        self.shape_buffer(&mut tmp, spans);
        std::mem::swap(&mut tmp, &mut self.buffer);
        self.update_bg_rects(device, queue, bg_rects);
    }

    /// Uploads background quads using the pre-allocated `scratch_vertices` buffer (zero heap alloc).
    pub fn update_bg_rects(&mut self, device: &Device, queue: &Queue, bg_rects: &[BgRect]) {
        let num_vertices = bg_rects.len() * 6;
        if num_vertices > 0 {
            self.scratch_vertices.clear();
            if self.scratch_vertices.capacity() < num_vertices {
                self.scratch_vertices
                    .reserve(num_vertices - self.scratch_vertices.capacity());
            }

            let lut = srgb_u8_lut();
            for rect in bg_rects {
                let c = if self.srgb_target {
                    [
                        lut[rect.color.r as usize],
                        lut[rect.color.g as usize],
                        lut[rect.color.b as usize],
                        rect.alpha,
                    ]
                } else {
                    [
                        rect.color.r as f32 / 255.0,
                        rect.color.g as f32 / 255.0,
                        rect.color.b as f32 / 255.0,
                        rect.alpha,
                    ]
                };
                let x0 = rect.x;
                let y0 = rect.y;
                let x1 = rect.x + rect.w;
                let y1 = rect.y + rect.h;

                self.scratch_vertices
                    .push(BgVertex { position: [x0, y0], color: c });
                self.scratch_vertices
                    .push(BgVertex { position: [x1, y0], color: c });
                self.scratch_vertices
                    .push(BgVertex { position: [x1, y1], color: c });
                self.scratch_vertices
                    .push(BgVertex { position: [x0, y0], color: c });
                self.scratch_vertices
                    .push(BgVertex { position: [x1, y1], color: c });
                self.scratch_vertices
                    .push(BgVertex { position: [x0, y1], color: c });
            }

            if num_vertices > self.bg_vertex_capacity {
                self.bg_vertex_capacity = num_vertices.next_power_of_two().max(2048);
                self.bg_vertex_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Terminal Bg Vertex Buffer"),
                    size: (self.bg_vertex_capacity * std::mem::size_of::<BgVertex>()) as u64,
                    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            queue.write_buffer(
                &self.bg_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.scratch_vertices),
            );
            self.bg_vertex_count = num_vertices as u32;
        } else {
            self.bg_vertex_count = 0;
        }
    }

    /// Returns the shaped horizontal pixel width of a `glyphon::Buffer`.
    pub fn measure_buffer_width(buffer: &Buffer) -> f32 {
        buffer
            .layout_runs()
            .map(|r| {
                let glyph_w: f32 = r.glyphs.iter().map(|g| g.w).sum();
                r.line_w.max(glyph_w)
            })
            .fold(0.0f32, f32::max)
    }

    pub fn create_buffer(&mut self) -> Buffer {
        Buffer::new(
            &mut self.font_system,
            Metrics::new(self.font_size, self.line_height),
        )
    }

    pub fn shape_buffer(&mut self, buffer: &mut Buffer, spans: &[(String, CellStyle)]) {
        let family = Family::Name(&self.font_family);
        let default_attrs = Attrs::new()
            .family(family)
            .color(Color::rgb(220, 215, 186));

        let rich_spans = spans.iter().map(|(text, style)| {
            let mut attrs = Attrs::new().family(family);
            let mut color = style.fg.unwrap_or(ColorRgb {
                r: 220,
                g: 215,
                b: 186,
            });
            if style.dim() {
                color = ColorRgb {
                    r: (color.r as f32 * 0.62) as u8,
                    g: (color.g as f32 * 0.62) as u8,
                    b: (color.b as f32 * 0.62) as u8,
                };
            }
            attrs = attrs.color(Color::rgb(color.r, color.g, color.b));
            if style.bold() {
                attrs = attrs.weight(Weight::BOLD);
            }
            if style.italic() {
                attrs = attrs.style(Style::Italic);
            }
            (text.as_str(), attrs)
        });

        buffer.set_rich_text(rich_spans, &default_attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn update_buffer_line(
        &mut self,
        buffer: &mut Buffer,
        line_idx: usize,
        spans: &[(String, CellStyle)],
        is_last_line: bool,
    ) {
        let family = Family::Name(&self.font_family);
        let default_attrs = Attrs::new()
            .family(family)
            .color(Color::rgb(220, 215, 186));
        let ending = if is_last_line {
            glyphon::cosmic_text::LineEnding::None
        } else {
            glyphon::cosmic_text::LineEnding::Lf
        };

        let mut line_text = String::new();
        let mut attrs_list = glyphon::cosmic_text::AttrsList::new(&default_attrs);

        for (text, style) in spans {
            let start = line_text.len();
            line_text.push_str(text);
            let end = line_text.len();

            if start < end {
                let mut attrs = Attrs::new().family(family);
                let mut color = style.fg.unwrap_or(ColorRgb {
                    r: 220,
                    g: 215,
                    b: 186,
                });
                if style.dim() {
                    color = ColorRgb {
                        r: (color.r as f32 * 0.62) as u8,
                        g: (color.g as f32 * 0.62) as u8,
                        b: (color.b as f32 * 0.62) as u8,
                    };
                }
                attrs = attrs.color(Color::rgb(color.r, color.g, color.b));
                if style.bold() {
                    attrs = attrs.weight(Weight::BOLD);
                }
                if style.italic() {
                    attrs = attrs.style(Style::Italic);
                }
                attrs_list.add_span(start..end, &attrs);
            }
        }

        if line_idx < buffer.lines.len() {
            buffer.lines[line_idx].set_text(&line_text, ending, attrs_list);
        }
    }

    pub fn prepare_areas<'a, I>(&mut self, device: &Device, queue: &Queue, areas: I)
    where
        I: IntoIterator<Item = TextArea<'a>>,
    {
        let _ = self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
    }



    #[allow(dead_code)]
    pub fn prepare(&mut self, device: &Device, queue: &Queue, pos_x: f32, pos_y: f32) {
        let text_area = TextArea {
            buffer: &self.buffer,
            left: pos_x,
            top: pos_y,
            scale: 1.0,
            bounds: TextBounds::default(),
            default_color: Color::rgb(220, 215, 186),
            custom_glyphs: &[],
        };

        let _ = self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            std::iter::once(text_area),
            &mut self.swash_cache,
        );
    }

    pub fn update_images<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        images: impl IntoIterator<Item = (&'a crate::terminal::graphics::KittyImage, [f32; 4], [f32; 4])>,
        live_ids: &std::collections::HashSet<u32>,
    ) {
        self.scratch_image_vertices.clear();
        self.active_image_draws.clear();

        // Release GPU textures for cleared or evicted images so VRAM never leaks
        self.image_textures.retain(|id, _| live_ids.contains(id));

        let tex_format = if self.srgb_target {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        for (img, rect, uv) in images {
            if img.dirty.get() || !self.image_textures.contains_key(&img.id) {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Kitty Image Texture"),
                    size: wgpu::Extent3d {
                        width: img.pixel_width.max(1),
                        height: img.pixel_height.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: tex_format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &img.rgba_data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * img.pixel_width),
                        rows_per_image: Some(img.pixel_height),
                    },
                    wgpu::Extent3d {
                        width: img.pixel_width.max(1),
                        height: img.pixel_height.max(1),
                        depth_or_array_layers: 1,
                    },
                );

                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Kitty Image Bind Group"),
                    layout: &self.image_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                        },
                    ],
                });

                self.image_textures.insert(img.id, (texture, bind_group));
                img.dirty.set(false);
            }

            let vertex_start = self.scratch_image_vertices.len() as u32;
            let [x0, y0, x1, y1] = rect;
            let [u0, v0, u1, v1] = uv;

            self.scratch_image_vertices.push(ImageVertex { position: [x0, y0], uv: [u0, v0] });
            self.scratch_image_vertices.push(ImageVertex { position: [x1, y0], uv: [u1, v0] });
            self.scratch_image_vertices.push(ImageVertex { position: [x1, y1], uv: [u1, v1] });
            self.scratch_image_vertices.push(ImageVertex { position: [x0, y0], uv: [u0, v0] });
            self.scratch_image_vertices.push(ImageVertex { position: [x1, y1], uv: [u1, v1] });
            self.scratch_image_vertices.push(ImageVertex { position: [x0, y1], uv: [u0, v1] });

            self.active_image_draws.push(ImageDrawCall {
                image_id: img.id,
                vertex_start,
            });
        }

        let num_vertices = self.scratch_image_vertices.len();
        if num_vertices > 0 {
            if num_vertices > self.image_vertex_capacity {
                self.image_vertex_capacity = num_vertices.next_power_of_two().max(256);
                self.image_vertex_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Kitty Image Vertex Buffer"),
                    size: (self.image_vertex_capacity * std::mem::size_of::<ImageVertex>()) as u64,
                    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            queue.write_buffer(
                &self.image_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.scratch_image_vertices),
            );
        }
    }

    pub fn render<'pass>(&'pass self, pass: &mut RenderPass<'pass>) {
        if self.bg_vertex_count > 0 {
            pass.set_pipeline(&self.bg_pipeline);
            pass.set_bind_group(0, &self.bg_bind_group, &[]);
            pass.set_vertex_buffer(0, self.bg_vertex_buffer.slice(..));
            pass.draw(0..self.bg_vertex_count, 0..1);
        }

        // Render Kitty Graphics Protocol images between BgRect and glyphon text
        if !self.active_image_draws.is_empty() {
            pass.set_pipeline(&self.image_pipeline);
            pass.set_bind_group(0, &self.bg_bind_group, &[]);
            pass.set_vertex_buffer(0, self.image_vertex_buffer.slice(..));
            for draw in &self.active_image_draws {
                if let Some((_, bind_group)) = self.image_textures.get(&draw.image_id) {
                    pass.set_bind_group(1, bind_group, &[]);
                    pass.draw(draw.vertex_start..draw.vertex_start + 6, 0..1);
                }
            }
        }

        let _ = self.renderer.render(&self.atlas, &self.viewport, pass);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_font() {
        let mut font_system = FontSystem::new();
        let (resolved, is_direct) =
            TextEngine::resolve_family_name(&font_system, "JetBrainsMono Nerd Font");
        println!(
            "Requested: 'JetBrainsMono Nerd Font', Resolved: '{}', is_direct: {}",
            resolved, is_direct
        );
        assert!(!resolved.is_empty());

        font_system.db_mut().set_monospace_family(&resolved);
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(14.0, 20.0));
        let family = Family::Name(&resolved);
        buffer.set_text(
            "echo hello world",
            &Attrs::new().family(family),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut font_system, false);
        for run in buffer.layout_runs() {
            println!(
                "Run line_top={}, line_height={}",
                run.line_top, run.line_height
            );
            for glyph in run.glyphs {
                if let Some(face) = font_system.db().face(glyph.font_id) {
                    println!(
                        "  c='{}' w={} font={:?}",
                        glyph.glyph_id, glyph.w, face.post_script_name
                    );
                }
            }
        }
    }
}
