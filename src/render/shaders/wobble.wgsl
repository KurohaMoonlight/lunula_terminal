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

// Kanagawa Sumi-e Ink Wave generator for the titlebar background
fn kanagawa_ink_wave(px: f32, py: f32, time: f32, total_w: f32, th: f32) -> vec4<f32> {
    // Kanagawa Wave color palette
    let sumi_ink0   = vec3<f32>(0.086, 0.086, 0.114); // #16161D (deepest ink stone)
    let sumi_ink1   = vec3<f32>(0.133, 0.196, 0.286); // #223249 (waveBlue1 - deep indigo ocean)
    let wave_blue   = vec3<f32>(0.176, 0.310, 0.404); // #2D4F67 (waveBlue2 - Prussian blue wave body)
    let wave_aqua   = vec3<f32>(0.416, 0.584, 0.537); // #6A9589 (waveAqua1 - sea foam green)
    let spring_blue = vec3<f32>(0.498, 0.706, 0.792); // #7FB4CA (springBlue - wave foam crest)
    let fuji_white  = vec3<f32>(0.863, 0.843, 0.729); // #DCD7BA (fujiWhite - washi paper white tip)

    // Vertical ink wash base gradient
    let grad = clamp(py / th, 0.0, 1.0);
    var base_col = mix(sumi_ink0, sumi_ink1 * 0.7, grad * 0.9);

    // Layer 1: Slow, deep oceanic swell (Layer paling belakang)
    let w1_x = px * 0.006 + time * 0.3;
    let wave1_y = th * 0.60 + sin(w1_x) * 6.0 + cos(w1_x * 0.45 + 1.2) * 3.5;
    if (py > wave1_y) {
        let depth = clamp((py - wave1_y) / (th - wave1_y + 0.1), 0.0, 1.0);
        let w1_col = mix(wave_blue * 0.65, sumi_ink1, depth * 0.55);
        let blend1 = smoothstep(wave1_y, wave1_y + 2.5, py);
        base_col = mix(base_col, w1_col, blend1 * 0.85);
    }

    // Layer 2: Main dynamic curling wave (Badan ombak Hokusai)
    let w2_x = px * 0.012 - time * 0.45;
    let s2 = sin(w2_x);
    // Asymmetric peaked wave shape (sharper curling crests, wider calm troughs)
    let peak2 = pow(max(0.0, s2 * 0.5 + 0.5), 2.2) * 15.0;
    let wave2_y = th * 0.52 - peak2 + cos(w2_x * 1.6 + time * 0.15) * 3.0;
    if (py > wave2_y) {
        let depth = clamp((py - wave2_y) / (th - wave2_y + 0.1), 0.0, 1.0);
        let w2_col = mix(wave_aqua * 0.80, wave_blue, depth * 0.75);
        let blend2 = smoothstep(wave2_y, wave2_y + 2.5, py);
        base_col = mix(base_col, w2_col, blend2 * 0.92);

        // Layer 3: Foam crest & fine spray at the peak
        let foam_dist = abs(py - wave2_y);
        if (foam_dist < 2.5) {
            let foam_mix = smoothstep(2.5, 0.1, foam_dist);
            let foam_col = mix(spring_blue, fuji_white, smoothstep(2.0, 0.0, foam_dist));
            base_col = mix(base_col, foam_col, foam_mix * 0.88);
        }
    }

    // Layer 4: Foreground gentle water ripple (Aksen riak di dasar titlebar)
    let w3_x = px * 0.02 + time * 0.7;
    let wave3_y = th * 0.84 + sin(w3_x) * 2.5 + cos(w3_x * 0.6 - time * 0.3) * 1.5;
    if (py > wave3_y) {
        let blend3 = smoothstep(wave3_y, wave3_y + 1.5, py);
        base_col = mix(base_col, sumi_ink1 * 0.85, blend3 * 0.40);
        if (abs(py - wave3_y) < 1.0) {
            base_col = mix(base_col, spring_blue * 0.7, 0.30);
        }
    }

    return vec4<f32>(base_col, 0.98);
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

    let titlebar_h = 48.0;
    let px = uv.x * uniforms.window_size.x;
    let py = uv.y * uniforms.window_size.y;

    if (py < titlebar_h) {
        // Calculate the Kanagawa Ink Wave in the background (lowest z-index)
        let wave = kanagawa_ink_wave(px, py, uniforms.time, uniforms.window_size.x, titlebar_h);

        // Blend offscreen content (semi-transparent frosted pills & crisp text) OVER the wave
        col = vec4<f32>(mix(wave.rgb, col.rgb, col.a), max(wave.a, col.a));

        // Window controls (Red, Yellow, Green circles on top-left of titlebar with Kanagawa pigments)
        let radius = 6.0;
        let center_y = 24.0;

        // Dot 1: Red (Close / Quit) - samuraiRed #E82424
        let d1 = length(vec2<f32>(px - 30.0, py - center_y));
        if (d1 < radius) {
            col = vec4<f32>(0.91, 0.25, 0.25, 1.0);
            if (uniforms.hover_button > 0.5 && uniforms.hover_button < 1.5) {
                // macOS-style 'X' inside Close button
                let lx = abs(px - 30.0);
                let ly = abs(py - center_y);
                if (abs(lx - ly) < 0.9 && max(lx, ly) < 3.2) {
                    col = vec4<f32>(0.20, 0.05, 0.06, 1.0);
                }
            }
        } else if (d1 < radius + 2.5 && uniforms.hover_button > 0.5 && uniforms.hover_button < 1.5) {
            // Subtle glow ring around Close button
            let glow = smoothstep(radius + 2.5, radius, d1);
            col = mix(col, vec4<f32>(0.91, 0.25, 0.25, 0.6), glow * 0.7);
        }

        // Dot 2: Yellow (Minimize) - boatYellow2 #E6C384
        let d2 = length(vec2<f32>(px - 52.0, py - center_y));
        if (d2 < radius) {
            col = vec4<f32>(0.90, 0.76, 0.52, 1.0);
            if (uniforms.hover_button > 1.5 && uniforms.hover_button < 2.5) {
                // '-' inside Minimize button
                if (abs(py - center_y) < 0.9 && abs(px - 52.0) < 3.2) {
                    col = vec4<f32>(0.25, 0.18, 0.08, 1.0);
                }
            }
        } else if (d2 < radius + 2.5 && uniforms.hover_button > 1.5 && uniforms.hover_button < 2.5) {
            // Subtle glow ring around Minimize button
            let glow = smoothstep(radius + 2.5, radius, d2);
            col = mix(col, vec4<f32>(0.90, 0.76, 0.52, 0.6), glow * 0.7);
        }

        // Dot 3: Green (Maximize) - springGreen #98BB6C
        let d3 = length(vec2<f32>(px - 74.0, py - center_y));
        if (d3 < radius) {
            col = vec4<f32>(0.60, 0.73, 0.42, 1.0);
            if (uniforms.hover_button > 2.5 && uniforms.hover_button < 3.5) {
                // '+' inside Maximize button
                let dx = abs(px - 74.0);
                let dy = abs(py - center_y);
                if ((dx < 0.9 && dy < 3.2) || (dy < 0.9 && dx < 3.2)) {
                    col = vec4<f32>(0.08, 0.20, 0.08, 1.0);
                }
            }
        } else if (d3 < radius + 2.5 && uniforms.hover_button > 2.5 && uniforms.hover_button < 3.5) {
            // Subtle glow ring around Maximize button
            let glow = smoothstep(radius + 2.5, radius, d3);
            col = mix(col, vec4<f32>(0.60, 0.73, 0.42, 0.6), glow * 0.7);
        }
    }

    // Titlebar bottom dividing line
    let sep_dist = abs(py - titlebar_h);
    if (sep_dist < 1.0) {
        col = mix(col, vec4<f32>(0.21, 0.21, 0.27, 0.85), 0.75); // sumiInk5
    }

    // Outer border outline around the entire box
    let border_px = min(min(px, uniforms.window_size.x - px), min(py, uniforms.window_size.y - py));
    if (border_px < 1.0) {
        col = mix(col, vec4<f32>(0.21, 0.21, 0.27, 0.85), 0.75); // sumiInk5
    }

    return col;
}
