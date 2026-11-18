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

#[allow(dead_code)]
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

            for rect in bg_rects {
                let c = [
                    rect.color.r as f32 / 255.0,
                    rect.color.g as f32 / 255.0,
                    rect.color.b as f32 / 255.0,
                    rect.alpha,
                ];
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
            .color(Color::rgb(205, 214, 244));

        let rich_spans = spans.iter().map(|(text, style)| {
            let mut attrs = Attrs::new().family(family);
            let mut color = style.fg.unwrap_or(ColorRgb {
                r: 205,
                g: 214,
                b: 244,
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
            default_color: Color::rgb(205, 214, 244),
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

    pub fn render<'pass>(&'pass self, pass: &mut RenderPass<'pass>) {
        if self.bg_vertex_count > 0 {
            pass.set_pipeline(&self.bg_pipeline);
            pass.set_bind_group(0, &self.bg_bind_group, &[]);
            pass.set_vertex_buffer(0, self.bg_vertex_buffer.slice(..));
            pass.draw(0..self.bg_vertex_count, 0..1);
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
