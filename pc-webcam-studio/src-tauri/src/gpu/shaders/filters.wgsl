// DirectX 12 WGSL Filter Shader for USB Mobile Webcam

struct FilterUniforms {
    brightness: f32,       // -1.0 to 1.0 (default 0.0)
    contrast: f32,         // 0.0 to 2.0 (default 1.0)
    saturation: f32,       // 0.0 to 2.0 (default 1.0)
    warmth: f32,           // -1.0 to 1.0 (default 0.0)
    sharpness: f32,        // 0.0 to 2.0 (default 0.0)
    vignette: f32,         // 0.0 to 1.0 (default 0.0)
    lut_preset: u32,       // 0..6
    mirror_h: u32,         // 0 or 1
    flip_v: u32,           // 0 or 1
    rotation: u32,         // 0, 90, 180, 270
    zoom_crop: f32,        // 1.0 to 4.0
    width: f32,
    height: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> uniforms: FilterUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var texture_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    // Fullscreen quad generated from a single oversized triangle
    var x = f32(i32(vertex_index & 1u) * 4 - 1);
    var y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// Convert RGB to HSL / Luminance
fn get_luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Color temperature / warmth adjustment
fn apply_warmth(color: vec3<f32>, warmth: f32) -> vec3<f32> {
    if (warmth == 0.0) {
        return color;
    }
    var adjusted = color;
    if (warmth > 0.0) {
        // Warmer: boost reds/yellows, slightly reduce blue
        adjusted.r += warmth * 0.15;
        adjusted.g += warmth * 0.06;
        adjusted.b -= warmth * 0.12;
    } else {
        // Cooler: boost blues, slightly reduce reds
        let cool = -warmth;
        adjusted.r -= cool * 0.10;
        adjusted.g -= cool * 0.04;
        adjusted.b += cool * 0.15;
    }
    return clamp(adjusted, vec3<f32>(0.0), vec3<f32>(1.0));
}

// 7 Built-In Cinematic Look LUT Presets
fn apply_lut_preset(color: vec3<f32>, preset: u32) -> vec3<f32> {
    let lum = get_luminance(color);

    switch preset {
        case 1u: {
            // Studio Warm: soft skin tones, slight golden lift
            return vec3<f32>(
                pow(color.r, 0.92) * 1.05,
                pow(color.g, 0.96) * 1.02,
                pow(color.b, 1.05) * 0.95
            );
        }
        case 2u: {
            // Crisp B&W: high dynamic range monochrome
            let bw = pow(lum, 1.1);
            return vec3<f32>(bw, bw, bw);
        }
        case 3u: {
            // Cinematic Teal & Orange: blue shadows, warm highlights
            var c = color;
            c.r = mix(c.r, pow(c.r, 0.85) * 1.15, lum);
            c.b = mix(pow(c.b, 0.90) * 1.2, c.b * 0.85, lum);
            return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        case 4u: {
            // Vintage Sepia
            return vec3<f32>(
                lum * 1.15 + 0.05,
                lum * 0.95 + 0.02,
                lum * 0.75
            );
        }
        case 5u: {
            // High-Contrast Noir
            let noir = clamp((lum - 0.5) * 1.6 + 0.5, 0.0, 1.0);
            return vec3<f32>(noir, noir, noir);
        }
        case 6u: {
            // Edge Soft / Dreamy Glow
            return mix(color, vec3<f32>(lum), 0.25) * 1.08;
        }
        default: {
            // 0 = Natural / None
            return color;
        }
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = in.uv;

    // 1. Digital Zoom / Center Crop
    if (uniforms.zoom_crop > 1.0) {
        let zoom = uniforms.zoom_crop;
        uv = (uv - 0.5) / zoom + 0.5;
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    }

    // 2. Geometry Transforms (Mirror & Flip)
    if (uniforms.mirror_h == 1u) {
        uv.x = 1.0 - uv.x;
    }
    if (uniforms.flip_v == 1u) {
        uv.y = 1.0 - uv.y;
    }

    // 3. Rotation (0, 90, 180, 270)
    if (uniforms.rotation == 90u) {
        uv = vec2<f32>(uv.y, 1.0 - uv.x);
    } else if (uniforms.rotation == 180u) {
        uv = vec2<f32>(1.0 - uv.x, 1.0 - uv.y);
    } else if (uniforms.rotation == 270u) {
        uv = vec2<f32>(1.0 - uv.y, uv.x);
    }

    // 4. Base Color Sample
    var color = textureSample(input_texture, texture_sampler, uv).rgb;

    // 5. Unsharp Mask Sharpness Convolution (3x3 kernel)
    if (uniforms.sharpness > 0.0) {
        let dx = 1.0 / uniforms.width;
        let dy = 1.0 / uniforms.height;

        let north = textureSample(input_texture, texture_sampler, uv + vec2<f32>(0.0, -dy)).rgb;
        let south = textureSample(input_texture, texture_sampler, uv + vec2<f32>(0.0, dy)).rgb;
        let east  = textureSample(input_texture, texture_sampler, uv + vec2<f32>(dx, 0.0)).rgb;
        let west  = textureSample(input_texture, texture_sampler, uv + vec2<f32>(-dx, 0.0)).rgb;

        let blur = (north + south + east + west) * 0.25;
        color = color + (color - blur) * uniforms.sharpness;
        color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    }

    // 6. Brightness & Contrast
    color = (color - 0.5) * uniforms.contrast + 0.5 + uniforms.brightness;
    color = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));

    // 7. Saturation
    let gray = vec3<f32>(get_luminance(color));
    color = mix(gray, color, uniforms.saturation);

    // 8. Color Warmth / Tint Shift
    color = apply_warmth(color, uniforms.warmth);

    // 9. Built-in LUT Preset
    color = apply_lut_preset(color, uniforms.lut_preset);

    // 10. Radial Vignette
    if (uniforms.vignette > 0.0) {
        let dist = distance(in.uv, vec2<f32>(0.5, 0.5));
        let vig = (1.0 - smoothstep(0.35, 0.75, dist * (0.8 + uniforms.vignette * 1.2)));
        color = color * vig;
    }

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
