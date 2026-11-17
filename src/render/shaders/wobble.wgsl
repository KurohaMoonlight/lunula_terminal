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
    window_size: vec2<f32>,
    time: f32,
    is_maximized: f32,
    hover_button: f32, // 0.0=none, 1.0=close, 2.0=minimize, 3.0=maximize
    _pad0: f32,
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
        if (uniforms.is_maximized > 0.5) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }
        let edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
        let alpha = smoothstep(0.0, 0.05, edge_dist) * in.color.a;
        return vec4<f32>(0.0, 0.0, 0.0, alpha);
    }

    // Sample the full-window offscreen content texture (titlebar, tabs, terminal body)
    var col = textureSample(t_content, s_content, uv);

    let titlebar_h = 42.0;
    let px = uv.x * uniforms.window_size.x;
    let py = uv.y * uniforms.window_size.y;

    if (py < titlebar_h) {
        // Window controls (Red, Yellow, Green circles on top-left of titlebar)
        let radius = 6.0;
        let center_y = 21.0;

        // Dot 1: Red (Close)
        let d1 = length(vec2<f32>(px - 30.0, py - center_y));
        if (d1 < radius) {
            col = vec4<f32>(0.96, 0.44, 0.44, 1.0);
            if (uniforms.hover_button > 0.5 && uniforms.hover_button < 1.5) {
                // macOS-style 'X' inside Close button
                let lx = abs(px - 30.0);
                let ly = abs(py - center_y);
                if (abs(lx - ly) < 0.9 && max(lx, ly) < 3.2) {
                    col = vec4<f32>(0.35, 0.08, 0.08, 1.0);
                }
            }
        } else if (d1 < radius + 2.5 && uniforms.hover_button > 0.5 && uniforms.hover_button < 1.5) {
            // Subtle glow ring around Close button
            let glow = smoothstep(radius + 2.5, radius, d1);
            col = mix(col, vec4<f32>(0.96, 0.44, 0.44, 0.6), glow * 0.7);
        }

        // Dot 2: Yellow (Minimize)
        let d2 = length(vec2<f32>(px - 52.0, py - center_y));
        if (d2 < radius) {
            col = vec4<f32>(0.98, 0.75, 0.44, 1.0);
            if (uniforms.hover_button > 1.5 && uniforms.hover_button < 2.5) {
                // '-' inside Minimize button
                if (abs(py - center_y) < 0.9 && abs(px - 52.0) < 3.2) {
                    col = vec4<f32>(0.45, 0.28, 0.05, 1.0);
                }
            }
        } else if (d2 < radius + 2.5 && uniforms.hover_button > 1.5 && uniforms.hover_button < 2.5) {
            // Subtle glow ring around Minimize button
            let glow = smoothstep(radius + 2.5, radius, d2);
            col = mix(col, vec4<f32>(0.98, 0.75, 0.44, 0.6), glow * 0.7);
        }

        // Dot 3: Green (Maximize)
        let d3 = length(vec2<f32>(px - 74.0, py - center_y));
        if (d3 < radius) {
            col = vec4<f32>(0.65, 0.89, 0.63, 1.0);
            if (uniforms.hover_button > 2.5 && uniforms.hover_button < 3.5) {
                // '+' inside Maximize button
                let dx = abs(px - 74.0);
                let dy = abs(py - center_y);
                if ((dx < 0.9 && dy < 3.2) || (dy < 0.9 && dx < 3.2)) {
                    col = vec4<f32>(0.08, 0.35, 0.08, 1.0);
                }
            }
        } else if (d3 < radius + 2.5 && uniforms.hover_button > 2.5 && uniforms.hover_button < 3.5) {
            // Subtle glow ring around Maximize button
            let glow = smoothstep(radius + 2.5, radius, d3);
            col = mix(col, vec4<f32>(0.65, 0.89, 0.63, 0.6), glow * 0.7);
        }
    }

    // Titlebar bottom dividing line
    let sep_dist = abs(py - titlebar_h);
    if (sep_dist < 1.0) {
        col = mix(col, vec4<f32>(0.35, 0.38, 0.52, 0.85), 0.70);
    }

    // Outer border outline around the entire box
    let border_px = min(min(px, uniforms.window_size.x - px), min(py, uniforms.window_size.y - py));
    if (border_px < 1.0) {
        col = mix(col, vec4<f32>(0.35, 0.38, 0.52, 0.85), 0.70);
    }

    return col;
}
