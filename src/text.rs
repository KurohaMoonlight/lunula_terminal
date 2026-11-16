use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

#[allow(dead_code)]
pub struct TextEngine {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub renderer: TextRenderer,
    pub buffer: Buffer,

    pub cell_width: f32,
    pub cell_height: f32,
    pub font_size: f32,
    pub line_height: f32,
}

impl TextEngine {
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        screen_size: [f32; 2],
    ) -> Self {
        let mut font_system = FontSystem::new();
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

        let font_size = 14.5;
        let line_height = 22.0;
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size, line_height));

        // Measure monospaced cell metrics using 'M'
        buffer.set_text(
            "M",
            &Attrs::new().family(Family::Monospace),
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

        println!("===========================================================");
        println!("   LUNULA TEXT ENGINE INITIALIZED                          ");
        println!("===========================================================");
        println!("  Font: Monospace (Cascadia / Consolas / System Mono)      ");
        println!("  Font Size: {:.1} pt                                      ", font_size);
        println!("  Cell Dimensions: {:.2} x {:.2} px                        ", cell_width, cell_height);
        println!("===========================================================");

        let initial_text = concat!(
            "Lunula Terminal [Version 0.1.0-alpha]\n",
            "(c) 2026 Lunula Project. GPU-Accelerated Native Terminal.\n",
            "\n",
            "PS E:\\My Projects\\lunula_terminal> Get-Process | Measure-Object\n",
            "\n",
            "Count    : 184\n",
            "Average  :\n",
            "Sum      :\n",
            "Maximum  :\n",
            "Minimum  :\n",
            "Property :\n",
            "\n",
            "PS E:\\My Projects\\lunula_terminal> _"
        );

        let text_attrs = Attrs::new()
            .family(Family::Monospace)
            .color(Color::rgb(205, 214, 244)); // Catppuccin Text (#cdd6f4)

        buffer.set_text(
            initial_text,
            &text_attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut font_system, false);

        Self {
            font_system,
            swash_cache,
            viewport,
            atlas,
            renderer,
            buffer,
            cell_width,
            cell_height,
            font_size,
            line_height,
        }
    }

    pub fn resize(&mut self, queue: &Queue, screen_w: f32, screen_h: f32) {
        self.viewport.update(
            queue,
            Resolution {
                width: screen_w.max(1.0) as u32,
                height: screen_h.max(1.0) as u32,
            },
        );
    }

    #[allow(dead_code)]
    pub fn set_text(&mut self, text: &str) {
        let text_attrs = Attrs::new()
            .family(Family::Monospace)
            .color(Color::rgb(205, 214, 244));
        self.buffer.set_text(
            text,
            &text_attrs,
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

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
        let _ = self
            .renderer
            .render(&self.atlas, &self.viewport, pass);
    }
}
