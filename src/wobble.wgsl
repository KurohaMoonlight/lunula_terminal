struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Uniforms {
    screen_size: vec2<f32>,
    time: f32,
    _padding: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var t_content: texture_2d<f32>;
@group(1) @binding(1)
var s_content: sampler;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let x = (model.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let y = 1.0 - (model.position.y / uniforms.screen_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = model.uv;
    out.color = model.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // Drop shadow pass (passed with color.a <= 0.4)
    if (in.color.a <= 0.40) {
        let edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
        let alpha = smoothstep(0.0, 0.05, edge_dist) * in.color.a;
        return vec4<f32>(0.0, 0.0, 0.0, alpha);
    }

    var col = in.color;
    let titlebar_v = 0.08;

    // If in terminal body: sample the rendered text & content texture!
    if (uv.y >= titlebar_v) {
        // Map v from titlebar_v..1.0 into 0.0..1.0
        let text_uv = vec2<f32>(uv.x, (uv.y - titlebar_v) / (1.0 - titlebar_v));
        let content_sample = textureSample(t_content, s_content, text_uv);
        col = content_sample;
    } else {
        // Window controls (Red, Yellow, Green circles on top-left of titlebar)
        let aspect = uniforms.screen_size.x / uniforms.screen_size.y;
        let center_y = 0.040;
        let radius = 0.014;

        // Dot 1: Red (Close)
        let d1 = length(vec2<f32>((uv.x - 0.035) * aspect, uv.y - center_y));
        if (d1 < radius) {
            col = vec4<f32>(0.96, 0.44, 0.44, 1.0);
        }

        // Dot 2: Yellow (Minimize)
        let d2 = length(vec2<f32>((uv.x - 0.058) * aspect, uv.y - center_y));
        if (d2 < radius) {
            col = vec4<f32>(0.98, 0.75, 0.44, 1.0);
        }

        // Dot 3: Green (Maximize)
        let d3 = length(vec2<f32>((uv.x - 0.081) * aspect, uv.y - center_y));
        if (d3 < radius) {
            col = vec4<f32>(0.65, 0.89, 0.63, 1.0);
        }
    }

    // Titlebar bottom dividing line
    let sep_dist = abs(uv.y - titlebar_v);
    if (sep_dist < 0.002) {
        col = mix(col, vec4<f32>(0.35, 0.38, 0.52, 0.85), 0.70);
    }

    // Outer border highlight (cyan / sapphire neon outline)
    let border_x = min(uv.x, 1.0 - uv.x);
    let border_y = min(uv.y, 1.0 - uv.y);
    let border_dist = min(border_x, border_y);

    if (border_dist < 0.007) {
        let border_color = vec4<f32>(0.54, 0.71, 0.98, 0.95);
        col = mix(col, border_color, 0.85);
    }

    return col;
}
