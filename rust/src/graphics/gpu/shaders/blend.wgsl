// GPU per-pixel RGBA blend for the layer compositor.
//
// Buffers are row-major RGBA f32 in [0,1] — identical to the CPU engine's
// `Vec<[f32; 4]>` ImageRgba convention (painting::graph).
//
// RGB is blended with the exact arithmetic of `painting::layer::blend_colors`
// (branch structure, clamps and guards included) so the GPU path is parity
// with the CPU compositor. The output alpha carries the *top* buffer's alpha
// unchanged; the layer-stack alpha lerp (opacity * mask) stays on the CPU
// until the full per-channel pipeline moves to compute shaders.

struct Params {
    width: u32,
    height: u32,
    mode: u32,
    reserved: u32,
};

@group(0) @binding(0) var<storage, read> base_buf: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> top_buf: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> out_buf: array<vec4<f32>>;
@group(0) @binding(3) var<uniform> params: Params;

// These codes MUST match `mode_code()` in graphics/gpu/compositor.rs.
const MODE_NORMAL: u32 = 0u;
const MODE_MULTIPLY: u32 = 1u;
const MODE_SCREEN: u32 = 2u;
const MODE_OVERLAY: u32 = 3u;
const MODE_ADD: u32 = 4u;
const MODE_SUBTRACT: u32 = 5u;
const MODE_DARKEN: u32 = 6u;
const MODE_LIGHTEN: u32 = 7u;
const MODE_COLOR_DODGE: u32 = 8u;
const MODE_COLOR_BURN: u32 = 9u;
const MODE_LINEAR_BURN: u32 = 10u;
const MODE_HARD_LIGHT: u32 = 11u;
const MODE_SOFT_LIGHT: u32 = 12u;
const MODE_DIFFERENCE: u32 = 13u;
const MODE_EXCLUSION: u32 = 14u;
const MODE_VIVID_LIGHT: u32 = 15u;
const MODE_LINEAR_LIGHT: u32 = 16u;
const MODE_PIN_LIGHT: u32 = 17u;
const MODE_HARD_MIX: u32 = 18u;
const MODE_DIVIDE: u32 = 19u;
const MODE_DARKER_COLOR: u32 = 20u;
const MODE_LIGHTER_COLOR: u32 = 21u;

fn op_overlay(b: f32, t: f32) -> f32 {
    if (b < 0.5) {
        return 2.0 * b * t;
    }
    return 1.0 - 2.0 * (1.0 - b) * (1.0 - t);
}

fn op_color_dodge(b: f32, t: f32) -> f32 {
    if (t >= 1.0) {
        return 1.0;
    }
    return min(b / (1.0 - t), 1.0);
}

fn op_color_burn(b: f32, t: f32) -> f32 {
    if (t <= 0.0) {
        return 0.0;
    }
    return max(1.0 - (1.0 - b) / t, 0.0);
}

fn op_hard_light(b: f32, t: f32) -> f32 {
    if (t < 0.5) {
        return 2.0 * b * t;
    }
    return 1.0 - 2.0 * (1.0 - b) * (1.0 - t);
}

fn op_soft_light(b: f32, t: f32) -> f32 {
    if (t < 0.5) {
        return b - (1.0 - 2.0 * t) * b * (1.0 - b);
    }
    return b + (2.0 * t - 1.0) * (sqrt(b) - b);
}

// Dark half: Color Burn with 2t; light half: Color Dodge with 2t - 1.
fn vivid(b: f32, t: f32) -> f32 {
    if (t <= 0.0) {
        return 0.0;
    }
    if (t < 0.5) {
        return max(1.0 - (1.0 - b) / (2.0 * t), 0.0);
    }
    if (t >= 1.0) {
        return 1.0;
    }
    return min(b / (2.0 * (1.0 - t)), 1.0);
}

fn op_linear_light(b: f32, t: f32) -> f32 {
    return clamp(b + 2.0 * t - 1.0, 0.0, 1.0);
}

fn op_pin_light(b: f32, t: f32) -> f32 {
    if (t <= 0.5) {
        return min(b, 2.0 * t);
    }
    return max(b, 2.0 * t - 1.0);
}

fn op_hard_mix(b: f32, t: f32) -> f32 {
    if (vivid(b, t) >= 0.5) {
        return 1.0;
    }
    return 0.0;
}

fn op_divide(b: f32, t: f32) -> f32 {
    // Dividing by black yields white (Photoshop semantics, same guard as CPU).
    if (t <= 0.0) {
        return 1.0;
    }
    return min(b / t, 1.0);
}

// Per-channel modes go through this switch; the whole-color comparison modes
// (Darker/Lighter Color) are handled on the full vector in `blend_rgb`.
fn channel_op(mode: u32, b: f32, t: f32) -> f32 {
    switch mode {
        case MODE_NORMAL: {
            return t;
        }
        case MODE_MULTIPLY: {
            return b * t;
        }
        case MODE_SCREEN: {
            return 1.0 - (1.0 - b) * (1.0 - t);
        }
        case MODE_OVERLAY: {
            return op_overlay(b, t);
        }
        case MODE_ADD: {
            return min(b + t, 1.0);
        }
        case MODE_SUBTRACT: {
            return max(b - t, 0.0);
        }
        case MODE_DARKEN: {
            return min(b, t);
        }
        case MODE_LIGHTEN: {
            return max(b, t);
        }
        case MODE_COLOR_DODGE: {
            return op_color_dodge(b, t);
        }
        case MODE_COLOR_BURN: {
            return op_color_burn(b, t);
        }
        case MODE_LINEAR_BURN: {
            return max(b + t - 1.0, 0.0);
        }
        case MODE_HARD_LIGHT: {
            return op_hard_light(b, t);
        }
        case MODE_SOFT_LIGHT: {
            return op_soft_light(b, t);
        }
        case MODE_DIFFERENCE: {
            return abs(b - t);
        }
        case MODE_EXCLUSION: {
            return b + t - 2.0 * b * t;
        }
        case MODE_VIVID_LIGHT: {
            return vivid(b, t);
        }
        case MODE_LINEAR_LIGHT: {
            return op_linear_light(b, t);
        }
        case MODE_PIN_LIGHT: {
            return op_pin_light(b, t);
        }
        case MODE_HARD_MIX: {
            return op_hard_mix(b, t);
        }
        case MODE_DIVIDE: {
            return op_divide(b, t);
        }
        default: {
            return t;
        }
    }
}

fn blend_rgb(mode: u32, base: vec3<f32>, top: vec3<f32>) -> vec3<f32> {
    // Whole-color comparison in RGB space (sum of channels), not per-channel.
    if (mode == MODE_DARKER_COLOR) {
        let base_sum = base[0] + base[1] + base[2];
        let top_sum = top[0] + top[1] + top[2];
        if (top_sum < base_sum) {
            return top;
        }
        return base;
    }
    if (mode == MODE_LIGHTER_COLOR) {
        let base_sum = base[0] + base[1] + base[2];
        let top_sum = top[0] + top[1] + top[2];
        if (top_sum > base_sum) {
            return top;
        }
        return base;
    }
    return vec3<f32>(
        channel_op(mode, base[0], top[0]),
        channel_op(mode, base[1], top[1]),
        channel_op(mode, base[2], top[2]),
    );
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Padding guard for sizes that are not a multiple of the workgroup.
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = gid.y * params.width + gid.x;
    let base_px = base_buf[idx];
    let top_px = top_buf[idx];
    out_buf[idx] = vec4<f32>(blend_rgb(params.mode, base_px.rgb, top_px.rgb), top_px.a);
}
