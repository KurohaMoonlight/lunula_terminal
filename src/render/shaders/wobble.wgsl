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
    srgb_target: f32,  // 1.0 when the swapchain is *Srgb (palette must be linearised)
    // Reactive wave (driven by window drag): x = energy 0..1.5, y = horizontal slosh px,
    // z = vertical bob px, w = extra wave clock (s)
    wave: vec4<f32>,
    // x = surface tilt -1..1 (water piles up opposite to the motion)
    wave2: vec4<f32>,
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

// ─────────────────────────────────────────────────────────────────────────────
// Colour helpers — palette is written as the real Kanagawa hex values (0..255)
// and converted to the swapchain's working space here.
// ─────────────────────────────────────────────────────────────────────────────
fn s2l(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn hex(r: f32, g: f32, b: f32) -> vec3<f32> {
    let c = vec3<f32>(r, g, b) / 255.0;
    if (uniforms.srgb_target > 0.5) {
        return s2l(c);
    }
    return c;
}

// ─────────────────────────────────────────────────────────────────────────────
// Cheap value-noise / fbm — gives us ink bleed, paper fibre and dry-brush breakup
// ─────────────────────────────────────────────────────────────────────────────
fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q = q + dot(q, q + vec2<f32>(45.32, 45.32));
    return fract(q.x * q.y);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 3; i = i + 1) {
        v = v + a * vnoise(q);
        q = q * 2.03 + vec2<f32>(17.0, 9.0);
        a = a * 0.5;
    }
    return v; // ~0.0 .. 0.875
}

// Hokusai-style asymmetric swell: long gentle back, steep curling front.
// returns (height 0..amp, front-mask 0..1 where the crest is breaking)
fn swell(x: f32, t: f32, period: f32, amp: f32, speed: f32, phase0: f32) -> vec2<f32> {
    let s = fract(x / period - t * speed + phase0);
    let rise = pow(s, 2.2);
    let fallen = 1.0 - smoothstep(0.86, 0.99, s);
    let h = rise * fallen * amp;
    let front = smoothstep(0.68, 0.88, s) * fallen;
    return vec2<f32>(h, front);
}

// ─────────────────────────────────────────────────────────────────────────────
// Title bar background: sumi-e ink wash + Great Wave off Kanagawa
// ─────────────────────────────────────────────────────────────────────────────
fn kanagawa_ink_wave(px: f32, py: f32, t: f32, total_w: f32, th: f32) -> vec4<f32> {
    // Kanagawa (rebelot) palette
    let sumi0       = hex(22.0, 22.0, 29.0);     // #16161D
    let sumi3       = hex(31.0, 31.0, 40.0);     // #1F1F28
    let sumi4       = hex(42.0, 42.0, 55.0);     // #2A2A37
    let wave1       = hex(34.0, 50.0, 73.0);     // #223249 waveBlue1 (indigo)
    let wave2       = hex(45.0, 79.0, 103.0);    // #2D4F67 waveBlue2 (prussian)
    let aqua1       = hex(106.0, 149.0, 137.0);  // #6A9589 waveAqua1
    let aqua2       = hex(122.0, 168.0, 159.0);  // #7AA89F waveAqua2
    let spring_blue = hex(127.0, 180.0, 202.0);  // #7FB4CA springBlue
    let old_white   = hex(200.0, 192.0, 147.0);  // #C8C093 oldWhite
    let fuji_white  = hex(220.0, 215.0, 186.0);  // #DCD7BA fujiWhite

    let p = vec2<f32>(px, py);

    // ── Reactive sea state (all zero at rest → identical to the calm wave) ──
    let en    = uniforms.wave.x;
    let en1   = min(en, 1.0);
    let slosh = uniforms.wave.y;
    let bob   = uniforms.wave.z;
    let wt    = t + uniforms.wave.w;   // wave clock: runs faster / reverses while dragging
    let tilt  = uniforms.wave2.x;

    // ── Paper & wash ────────────────────────────────────────────────────────
    // dark sumi stone gradient, mottled by slow ink-wash clouds
    let wash = fbm(p * vec2<f32>(0.010, 0.040) + vec2<f32>(t * 0.015, 0.0));
    var col = mix(sumi0, sumi3, smoothstep(0.0, th, py) * 0.7);
    col = mix(col, wave1, wash * 0.38);
    // washi fibre grain (static, very subtle)
    col = col * (0.93 + 0.14 * hash21(floor(p)));

    // ── Crescent moon (Lunula) bleeding through the wash ────────────────────
    let moon_c = vec2<f32>(max(total_w * 0.22, 170.0), 15.0);
    let jitter = (fbm(p * 0.35) - 0.4) * 2.0;
    let d_outer = length(p - moon_c) + jitter;
    let d_inner = length(p - (moon_c + vec2<f32>(3.6, -2.2))) + jitter;
    let disc  = (1.0 - smoothstep(7.0, 8.5, d_outer));
    let cut   = (1.0 - smoothstep(6.0, 7.4, d_inner));
    let moon  = clamp(disc - cut, 0.0, 1.0);
    let halo  = exp(-max(d_outer - 8.0, 0.0) * 0.12) * 0.10;
    col = mix(col, wave2, halo);
    col = mix(col, mix(old_white, fuji_white, 0.35), moon * 0.62);

    // ── Far swell: misty indigo wash behind the great wave ──────────────────
    let far = swell(px - slosh * 0.4, wt, 760.0, 9.0 * (1.0 + 0.5 * en1), 0.010, 0.62);
    let far_y = th * 0.60 - far.x - (fbm(vec2<f32>(px * 0.03, 2.0)) - 0.4) * 3.0 + bob * 0.3;
    let far_dc = py - far_y;
    if (far_dc < 0.0) {
        col = mix(col, wave1, exp(far_dc / 4.0) * 0.30);
    } else {
        let far_depth = clamp(far_dc / 22.0, 0.0, 1.0);
        let far_col = mix(wave1 * 1.25, sumi3, far_depth * 0.7);
        col = mix(col, far_col, smoothstep(0.0, 3.5, far_dc) * 0.80);
    }

    // ── The Great Wave ──────────────────────────────────────────────────────
    // Dragging swells the wave, shoves it sideways (slosh), tilts the surface and
    // steepens the curling front — like water in a tray that is being pushed.
    let sw = swell(px - slosh, wt, 520.0, 21.0 * (1.0 + 0.40 * en1), 0.020, 0.30);
    let rough = (fbm(vec2<f32>(px * 0.05, wt * 0.10 + 3.7)) - 0.44) * 3.2 * (1.0 + 0.8 * en);
    let tilt_y = tilt * (px / total_w - 0.5) * 14.0;
    let crest_y = max(
        th * 0.66 - sw.x - sin(px * 0.07 - wt * 0.9) * (1.1 + 1.2 * en) - rough - tilt_y + bob,
        1.5
    );
    let front = clamp(sw.y * (1.0 + 0.6 * en), 0.0, 1.0);
    let dc = py - crest_y;

    if (dc < 0.0) {
        // ink bleed just above the brush edge
        col = mix(col, wave2, exp(dc / 5.0) * (0.20 + 0.20 * wash));

        // spray: sparse fujiWhite droplets flung from the breaking front,
        // travelling with the swell (period * speed = 10.4 px/s)
        let sp = vec2<f32>(px - wt * 10.4, py) / 2.6;
        let r = hash21(floor(sp));
        let cell = fract(sp) - vec2<f32>(0.5, 0.5);
        let dist = length(cell) * 2.6;
        let drop_on = step(0.90 - 0.14 * en1, r) * (1.0 - smoothstep(0.35, 1.0, dist));
        let spray_mask = front * smoothstep(-14.0 - 12.0 * en1, -1.5, dc);
        col = mix(col, fuji_white, drop_on * spray_mask * 0.85);
    } else {
        let depth = clamp(dc / 22.0, 0.0, 1.0);

        // Hokusai prussian gradient: pale at the crest, deep indigo below
        var body = mix(wave2, spring_blue, 0.28);
        body = mix(body, wave1, smoothstep(0.0, 0.85, depth));
        body = mix(body, sumi0, smoothstep(0.55, 1.0, depth) * 0.55);

        // woodblock water-grain streaks following the crest line
        let flow = fbm(vec2<f32>(px * 0.03, dc * 0.20)) * 6.0;
        let streak = smoothstep(0.88, 1.0, sin(dc * 0.85 + flow - wt * 0.4));
        body = mix(body, aqua2, streak * 0.20 * (1.0 - depth * 0.6));

        // ink pooling / mottling inside the body
        body = body * (0.80 + 0.40 * fbm(vec2<f32>(px * 0.02, py * 0.08) + vec2<f32>(5.0, 5.0)));

        col = mix(col, body, smoothstep(0.0, 1.2, dc) * 0.97);

        // sumi contour — brush pressure swells on the curling front
        let lw = 1.0 + 0.9 * front + 0.6 * vnoise(vec2<f32>(px * 0.2, 1.0));
        let line = (1.0 - smoothstep(lw * 0.4, lw, abs(dc - lw * 0.3)));
        col = mix(col, sumi0 * 0.55, line * 0.92);

        // dry-brush foam under the contour (washi white, broken by noise)
        let fd = dc - lw * 0.8;
        let foam_band = (1.0 - smoothstep(0.0, 2.5 + 5.0 * front + 2.5 * en1, fd)) * step(0.0, fd);
        let bn = fbm(vec2<f32>(px * 0.22, py * 0.60 + wt * 0.15));
        let foam_a = foam_band * smoothstep(0.38, 0.62, bn + front * 0.25);
        col = mix(col, mix(old_white, fuji_white, bn), foam_a * 0.90);
    }

    // ── Foreground ripple hairline, broken like a dry brush stroke ──────────
    let r_y = th * 0.90 + sin(px * 0.025 + wt * 0.6) * (1.4 + 1.6 * en1);
    let r_line = (1.0 - smoothstep(0.2, 0.9, abs(py - r_y)));
    let r_break = smoothstep(0.35, 0.60, vnoise(vec2<f32>(px * 0.06, t * 0.05)));
    col = mix(col, aqua1, r_line * r_break * 0.22 * (1.0 + en1));

    // ── Ink pooling at the bottom edge so it melts into the tab bar ─────────
    col = mix(col, sumi0, smoothstep(th - 7.0, th, py) * 0.55);

    // ── Heavier ink behind the window controls and slight edge vignette ─────
    let left = smoothstep(0.0, 150.0, px);
    col = mix(sumi0, col, 0.35 + 0.65 * left);
    let right = smoothstep(0.0, 40.0, total_w - px);
    col = mix(sumi0, col, 0.70 + 0.30 * right);

    return vec4<f32>(col, 0.98);
}

fn paint_dot(col: vec4<f32>, px: f32, py: f32, cx: f32, cy: f32, r: f32, c: vec3<f32>) -> vec4<f32> {
    let d = length(vec2<f32>(px - cx, py - cy));
    let a = (1.0 - smoothstep(r - 0.7, r + 0.7, d));
    return vec4<f32>(mix(col.rgb, c, a), max(col.a, a));
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

    let border_col = vec4<f32>(hex(54.0, 54.0, 70.0), 0.85); // sumiInk5

    if (py < titlebar_h) {
        // The offscreen target is cleared to 0 and drawn with ALPHA_BLENDING, so it
        // holds premultiplied colour: composite it OVER the ink wave accordingly.
        let wave = kanagawa_ink_wave(px, py, uniforms.time, uniforms.window_size.x, titlebar_h);
        col = vec4<f32>(col.rgb + wave.rgb * (1.0 - col.a), max(wave.a, col.a));

        // Window controls — muted mineral pigments at rest, vivid on hover
        let center_y = 24.0;
        let radius = 6.0;
        let hb = uniforms.hover_button;

        // Close: autumnRed #C34043 -> samuraiRed #E82424
        var c_close = hex(195.0, 64.0, 67.0);
        if (hb > 0.5 && hb < 1.5) { c_close = hex(232.0, 36.0, 36.0); }
        // Minimize: autumnYellow #DCA561 -> carpYellow #E6C384
        var c_min = hex(220.0, 165.0, 97.0);
        if (hb > 1.5 && hb < 2.5) { c_min = hex(230.0, 195.0, 132.0); }
        // Maximize: autumnGreen #76946A -> springGreen #98BB6C
        var c_max = hex(118.0, 148.0, 106.0);
        if (hb > 2.5 && hb < 3.5) { c_max = hex(152.0, 187.0, 108.0); }

        let glyph = hex(22.0, 22.0, 29.0); // sumiInk0

        // soft ink-bleed glow behind the hovered button
        if (hb > 0.5) {
            var hx = 30.0;
            var hc = c_close;
            if (hb > 1.5 && hb < 2.5) { hx = 52.0; hc = c_min; }
            if (hb > 2.5) { hx = 74.0; hc = c_max; }
            let hd = length(vec2<f32>(px - hx, py - center_y));
            col = vec4<f32>(mix(col.rgb, hc, (1.0 - smoothstep(radius, radius + 5.0, hd)) * 0.35), col.a);
        }

        col = paint_dot(col, px, py, 30.0, center_y, radius, c_close);
        col = paint_dot(col, px, py, 52.0, center_y, radius, c_min);
        col = paint_dot(col, px, py, 74.0, center_y, radius, c_max);

        // glyphs on hover
        if (hb > 0.5 && hb < 1.5) {
            let lx = abs(px - 30.0);
            let ly = abs(py - center_y);
            if (abs(lx - ly) < 0.9 && max(lx, ly) < 3.2) { col = vec4<f32>(glyph, 1.0); }
        }
        if (hb > 1.5 && hb < 2.5) {
            if (abs(py - center_y) < 0.9 && abs(px - 52.0) < 3.2) { col = vec4<f32>(glyph, 1.0); }
        }
        if (hb > 2.5 && hb < 3.5) {
            let dx = abs(px - 74.0);
            let dy = abs(py - center_y);
            if ((dx < 0.9 && dy < 3.2) || (dy < 0.9 && dx < 3.2)) { col = vec4<f32>(glyph, 1.0); }
        }
    }

    // Titlebar bottom dividing line
    if (abs(py - titlebar_h) < 1.0) {
        col = mix(col, border_col, 0.75);
    }

    // Outer border outline around the entire box
    let border_px = min(min(px, uniforms.window_size.x - px), min(py, uniforms.window_size.y - py));
    if (border_px < 1.0) {
        col = mix(col, border_col, 0.75);
    }

    return col;
}
