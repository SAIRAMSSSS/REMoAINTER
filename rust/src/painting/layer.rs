#![allow(dead_code)]
use super::anchor::AnchorPoint;
use super::brush::{sample_brush_alpha, Brush};
use super::filter::{
    apply_levels_scalar, apply_point_effect_pixel, gaussian_blur_f32, gaussian_blur_rgba,
    hsl_to_rgb, rgb_to_hsl, sharpen_f32, sharpen_rgba, FilterEffect, FilterTargetChannel,
};
use super::generator::{evaluate_generator_pixel, GeneratorConfig};
use super::history::HistoryManager;
use super::mask::{LayerMask, MaskType};
use crate::graphics::lighting::{pbr_shade, EnvironmentPreset};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerType {
    Paint,
    Fill,
    Folder,
    Adjustment,
    Generator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    PassThrough,
    Multiply,
    Screen,
    Overlay,
    Add,
    Subtract,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    LinearBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Color,
    Luminosity,
    Hue,
    Saturation,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    DarkerColor,
    LighterColor,
    Divide,
    RNM,
    Whiteout,
}

impl BlendMode {
    pub fn name(&self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::PassThrough => "Pass Through",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Add => "Linear Dodge (Add)",
            BlendMode::Subtract => "Subtract",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::LinearBurn => "Linear Burn",
            BlendMode::HardLight => "Hard Light",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::VividLight => "Vivid Light",
            BlendMode::LinearLight => "Linear Light",
            BlendMode::PinLight => "Pin Light",
            BlendMode::HardMix => "Hard Mix",
            BlendMode::DarkerColor => "Darker Color",
            BlendMode::LighterColor => "Lighter Color",
            BlendMode::Divide => "Divide",
            BlendMode::RNM => "RNM (Reoriented Normal)",
            BlendMode::Whiteout => "Whiteout",
        }
    }
}

pub fn blend_colors(base: [f32; 3], top: [f32; 3], mode: BlendMode) -> [f32; 3] {
    match mode {
        BlendMode::Normal | BlendMode::PassThrough => top,
        BlendMode::Multiply => [base[0] * top[0], base[1] * top[1], base[2] * top[2]],
        BlendMode::Screen => [
            1.0 - (1.0 - base[0]) * (1.0 - top[0]),
            1.0 - (1.0 - base[1]) * (1.0 - top[1]),
            1.0 - (1.0 - base[2]) * (1.0 - top[2]),
        ],
        BlendMode::Overlay => {
            let ch = |b: f32, t: f32| {
                if b < 0.5 {
                    2.0 * b * t
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - t)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::Add => [
            (base[0] + top[0]).min(1.0),
            (base[1] + top[1]).min(1.0),
            (base[2] + top[2]).min(1.0),
        ],
        BlendMode::Subtract => [
            (base[0] - top[0]).max(0.0),
            (base[1] - top[1]).max(0.0),
            (base[2] - top[2]).max(0.0),
        ],
        BlendMode::Darken => [
            base[0].min(top[0]),
            base[1].min(top[1]),
            base[2].min(top[2]),
        ],
        BlendMode::Lighten => [
            base[0].max(top[0]),
            base[1].max(top[1]),
            base[2].max(top[2]),
        ],
        BlendMode::ColorDodge => {
            let ch = |b: f32, t: f32| {
                if t >= 1.0 {
                    1.0
                } else {
                    (b / (1.0 - t)).min(1.0)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::ColorBurn => {
            let ch = |b: f32, t: f32| {
                if t <= 0.0 {
                    0.0
                } else {
                    (1.0 - (1.0 - b) / t).max(0.0)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::LinearBurn => [
            (base[0] + top[0] - 1.0).max(0.0),
            (base[1] + top[1] - 1.0).max(0.0),
            (base[2] + top[2] - 1.0).max(0.0),
        ],
        BlendMode::HardLight => {
            let ch = |b: f32, t: f32| {
                if t < 0.5 {
                    2.0 * b * t
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - t)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::SoftLight => {
            let ch = |b: f32, t: f32| {
                if t < 0.5 {
                    b - (1.0 - 2.0 * t) * b * (1.0 - b)
                } else {
                    b + (2.0 * t - 1.0) * (b.sqrt() - b)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::Difference => [
            (base[0] - top[0]).abs(),
            (base[1] - top[1]).abs(),
            (base[2] - top[2]).abs(),
        ],
        BlendMode::Exclusion => [
            base[0] + top[0] - 2.0 * base[0] * top[0],
            base[1] + top[1] - 2.0 * base[1] * top[1],
            base[2] + top[2] - 2.0 * base[2] * top[2],
        ],
        BlendMode::Color => {
            let (_hb, _sb, lb) = rgb_to_hsl(base[0], base[1], base[2]);
            let (ht, st, _lt) = rgb_to_hsl(top[0], top[1], top[2]);
            let (r, g, b) = hsl_to_rgb(ht, st, lb);
            [r, g, b]
        }
        BlendMode::Luminosity => {
            let (hb, sb, _lb) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_ht, _st, lt) = rgb_to_hsl(top[0], top[1], top[2]);
            let (r, g, b) = hsl_to_rgb(hb, sb, lt);
            [r, g, b]
        }
        BlendMode::Hue => {
            let (_hb, sb, lb) = rgb_to_hsl(base[0], base[1], base[2]);
            let (ht, _st, _lt) = rgb_to_hsl(top[0], top[1], top[2]);
            let (r, g, b) = hsl_to_rgb(ht, sb, lb);
            [r, g, b]
        }
        BlendMode::Saturation => {
            let (hb, _sb, lb) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_ht, st, _lt) = rgb_to_hsl(top[0], top[1], top[2]);
            let (r, g, b) = hsl_to_rgb(hb, st, lb);
            [r, g, b]
        }
        BlendMode::VividLight => {
            // Dark half: Color Burn with 2t, light half: Color Dodge with 2t - 1.
            let ch = |b: f32, t: f32| {
                if t <= 0.0 {
                    0.0
                } else if t < 0.5 {
                    (1.0 - (1.0 - b) / (2.0 * t)).max(0.0)
                } else if t >= 1.0 {
                    1.0
                } else {
                    (b / (2.0 * (1.0 - t))).min(1.0)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::LinearLight => {
            // Linear Burn with 2t below, Linear Dodge with 2t - 1 above -> simple clamp form.
            [
                (base[0] + 2.0 * top[0] - 1.0).clamp(0.0, 1.0),
                (base[1] + 2.0 * top[1] - 1.0).clamp(0.0, 1.0),
                (base[2] + 2.0 * top[2] - 1.0).clamp(0.0, 1.0),
            ]
        }
        BlendMode::PinLight => {
            // Min with 2t below, Max with 2t - 1 above.
            let ch = |b: f32, t: f32| {
                if t <= 0.5 {
                    b.min(2.0 * t)
                } else {
                    b.max(2.0 * t - 1.0)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::HardMix => {
            // Thresholded Vivid Light: quantize each channel to 0 or 1.
            let vivid = |b: f32, t: f32| {
                if t <= 0.0 {
                    0.0
                } else if t < 0.5 {
                    (1.0 - (1.0 - b) / (2.0 * t)).max(0.0)
                } else if t >= 1.0 {
                    1.0
                } else {
                    (b / (2.0 * (1.0 - t))).min(1.0)
                }
            };
            let ch = |b: f32, t: f32| if vivid(b, t) >= 0.5 { 1.0 } else { 0.0 };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        BlendMode::DarkerColor => {
            // Whole-color comparison in RGB space (sum of channels), not per-channel.
            let base_sum = base[0] + base[1] + base[2];
            let top_sum = top[0] + top[1] + top[2];
            if top_sum < base_sum {
                top
            } else {
                base
            }
        }
        BlendMode::LighterColor => {
            let base_sum = base[0] + base[1] + base[2];
            let top_sum = top[0] + top[1] + top[2];
            if top_sum > base_sum {
                top
            } else {
                base
            }
        }
        BlendMode::Divide => {
            // Guard the zero denominator: dividing by black yields white (Photoshop semantics).
            let ch = |b: f32, t: f32| {
                if t <= 0.0 {
                    1.0
                } else {
                    (b / t).min(1.0)
                }
            };
            [
                ch(base[0], top[0]),
                ch(base[1], top[1]),
                ch(base[2], top[2]),
            ]
        }
        // RNM and Whiteout are normal-map modes: they are only meaningful through
        // blend_normal_channels (wired into the normal channel compositing) and
        // degrade to Normal on the color path.
        BlendMode::RNM | BlendMode::Whiteout => top,
    }
}

pub fn blend_scalar_channel(base: f32, top: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal | BlendMode::PassThrough => top,
        BlendMode::Multiply => base * top,
        BlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - top),
        BlendMode::Overlay => {
            if base < 0.5 {
                2.0 * base * top
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - top)
            }
        }
        BlendMode::Add => (base + top).min(1.0),
        BlendMode::Subtract => (base - top).max(0.0),
        BlendMode::Darken => base.min(top),
        BlendMode::Lighten => base.max(top),
        BlendMode::Difference => (base - top).abs(),
        BlendMode::VividLight => {
            if top <= 0.0 {
                0.0
            } else if top < 0.5 {
                (1.0 - (1.0 - base) / (2.0 * top)).max(0.0)
            } else if top >= 1.0 {
                1.0
            } else {
                (base / (2.0 * (1.0 - top))).min(1.0)
            }
        }
        BlendMode::LinearLight => (base + 2.0 * top - 1.0).clamp(0.0, 1.0),
        BlendMode::PinLight => {
            if top <= 0.5 {
                base.min(2.0 * top)
            } else {
                base.max(2.0 * top - 1.0)
            }
        }
        BlendMode::HardMix => {
            let vivid = if top <= 0.0 {
                0.0
            } else if top < 0.5 {
                (1.0 - (1.0 - base) / (2.0 * top)).max(0.0)
            } else if top >= 1.0 {
                1.0
            } else {
                (base / (2.0 * (1.0 - top))).min(1.0)
            };
            if vivid >= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        BlendMode::Divide => {
            if top <= 0.0 {
                1.0
            } else {
                (base / top).min(1.0)
            }
        }
        BlendMode::DarkerColor => {
            if top < base {
                top
            } else {
                base
            }
        }
        BlendMode::LighterColor => {
            if top > base {
                top
            } else {
                base
            }
        }
        _ => top,
    }
}

/// Blends two tangent-space normal map values, each encoded as [0..1] with the
/// neutral (0.5, 0.5, 1.0). Only RNM and Whiteout implement actual normal-map
/// recombination; all other modes degenerate to the top normal (the callers
/// lerp against the result with the layer's effective alpha, which reproduces
/// the legacy Normal behavior for them).
///
/// RNM uses the classic Reoriented Normal Mapping cross-term: the detail's
/// lateral xy is countered by the base's z and vice versa, with
/// z = z_base * z_detail, then the result is re-normalized.
/// Whiteout stacks the detail lateral at half weight and reconstructs z from
/// sqrt(1 - x^2 - y^2), as defined by the classic Whiteout blend.
pub fn blend_normal_channels(base: [f32; 3], top: [f32; 3], mode: BlendMode) -> [f32; 3] {
    match mode {
        BlendMode::Whiteout => {
            // Decode to signed tangent space.
            let bx = base[0] * 2.0 - 1.0;
            let by = base[1] * 2.0 - 1.0;
            let sx = top[0] * 2.0 - 1.0;
            let sy = top[1] * 2.0 - 1.0;
            // Detail lateral at half weight, z reconstructed (guarded against negative).
            let x = bx + sx * 0.5;
            let y = by + sy * 0.5;
            let z = (1.0 - x * x - y * y).max(1e-4).sqrt();
            [x * 0.5 + 0.5, y * 0.5 + 0.5, z * 0.5 + 0.5]
        }
        BlendMode::RNM => {
            let b = [
                base[0] * 2.0 - 1.0,
                base[1] * 2.0 - 1.0,
                (base[2] * 2.0 - 1.0).max(1e-4),
            ];
            let s = [
                top[0] * 2.0 - 1.0,
                top[1] * 2.0 - 1.0,
                (top[2] * 2.0 - 1.0).max(1e-4),
            ];
            // Classic RNM transition: xy cross-bonded, z multiplicative.
            let mut x = b[0] * s[2] + s[0] * b[2];
            let mut y = b[1] * s[2] + s[1] * b[2];
            let mut z = b[2] * s[2];
            let len = (x * x + y * y + z * z).sqrt().max(1e-6);
            x /= len;
            y /= len;
            z /= len;
            [x * 0.5 + 0.5, y * 0.5 + 0.5, z * 0.5 + 0.5]
        }
        _ => top,
    }
}

/// Computes tangent-space normal from a height field at (x, y) using a 3x3 Sobel gradient operator.
/// Returns tangent-space normal encoded in [0.0, 1.0] color space (where [0.5, 0.5, 1.0] is flat neutral).
#[inline]
pub fn normal_from_height_at(
    heights: &[f32],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    strength: f32,
) -> [f32; 3] {
    if strength <= 0.0001 || heights.is_empty() || w <= 0 || h <= 0 {
        return [0.5, 0.5, 1.0];
    }
    let get_h = |px: i32, py: i32| -> f32 {
        let cx = px.rem_euclid(w) as usize;
        let cy = py.rem_euclid(h) as usize;
        let idx = cy * w as usize + cx;
        heights.get(idx).copied().unwrap_or(0.0)
    };

    let tl = get_h(x - 1, y - 1);
    let l = get_h(x - 1, y);
    let bl = get_h(x - 1, y + 1);
    let tr = get_h(x + 1, y - 1);
    let r = get_h(x + 1, y);
    let br = get_h(x + 1, y + 1);
    let t = get_h(x, y - 1);
    let b = get_h(x, y + 1);

    // dx: horizontal gradient (right - left)
    let dx = (tr + 2.0 * r + br) - (tl + 2.0 * l + bl);
    // dy: vertical gradient (top - bottom for OpenGL Y+ up tangent space)
    let dy = (tl + 2.0 * t + tr) - (bl + 2.0 * b + br);

    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
        return [0.5, 0.5, 1.0];
    }

    let nx = -dx * strength * 0.125;
    let ny = dy * strength * 0.125;
    let nz = 1.0f32;
    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);

    [
        (nx / len) * 0.5 + 0.5,
        (ny / len) * 0.5 + 0.5,
        (nz / len) * 0.5 + 0.5,
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: usize,
    pub name: String,
    pub layer_type: LayerType,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub folder_expanded: bool,
    pub parent_folder_id: Option<usize>,
    pub effects: Vec<FilterEffect>,

    // Per-Layer Channel Overrides & Blends (reference PBR suite Parity)
    pub channel_color_enabled: bool,
    pub channel_metallic_enabled: bool,
    pub channel_roughness_enabled: bool,
    pub channel_normal_enabled: bool,
    pub channel_height_enabled: bool,

    pub channel_color_blend: BlendMode,
    pub channel_metallic_blend: BlendMode,
    pub channel_roughness_blend: BlendMode,
    pub channel_normal_blend: BlendMode,
    pub channel_height_blend: BlendMode,

    pub channel_color_opacity: f32,
    pub channel_metallic_opacity: f32,
    pub channel_roughness_opacity: f32,
    pub channel_normal_opacity: f32,
    pub channel_height_opacity: f32,

    // Masking & Anchor Points
    pub mask: Option<LayerMask>,
    pub anchor_point: Option<AnchorPoint>,
    pub color_tag: Option<u8>,

    // Procedural Generator Config (for Generator layers)
    #[serde(default)]
    pub generator: Option<GeneratorConfig>,

    // Fill settings (for Fill & Generator layers)
    pub fill_color: [f32; 4],
    pub fill_metallic: f32,
    pub fill_roughness: f32,
    pub fill_height: f32,

    // Channel Pixel Buffers
    #[serde(skip)]
    pub color_buffer: Vec<[f32; 4]>,
    #[serde(skip)]
    pub metallic_buffer: Vec<f32>,
    #[serde(skip)]
    pub roughness_buffer: Vec<f32>,
    #[serde(skip)]
    pub normal_buffer: Vec<[f32; 3]>,
    #[serde(skip)]
    pub height_buffer: Vec<f32>,

    pub width: u32,
    pub height: u32,
}

/// Immutable snapshot of texture buffers a tool samples from (smudge "Sample Merged", clone).
pub struct SampleSource<'a> {
    pub color: &'a [[f32; 4]],
    pub metallic: &'a [f32],
    pub roughness: &'a [f32],
    pub normal: &'a [[f32; 3]],
    pub height: &'a [f32],
    pub w: u32,
    pub h: u32,
}

impl<'a> SampleSource<'a> {
    /// Nearest sample at texture index with clamped coordinates.
    #[inline]
    pub fn sample_idx(&self, idx: usize) -> ([f32; 4], f32, f32, [f32; 3], f32) {
        let i = idx.min(self.color.len().saturating_sub(1));
        (
            self.color[i],
            self.metallic[i.min(self.metallic.len().saturating_sub(1))],
            self.roughness[i.min(self.roughness.len().saturating_sub(1))],
            self.normal[i.min(self.normal.len().saturating_sub(1))],
            self.height[i.min(self.height.len().saturating_sub(1))],
        )
    }

    /// Nearest sample at a UV coordinate (clamped to texture bounds).
    #[inline]
    pub fn sample_uv(&self, u: f32, v: f32) -> ([f32; 4], f32, f32, [f32; 3], f32) {
        let px = (u.clamp(0.0, 1.0) * (self.w as f32 - 1.0)).round() as i32;
        let py = (v.clamp(0.0, 1.0) * (self.h as f32 - 1.0)).round() as i32;
        let idx = (py * self.w as i32 + px).max(0) as usize;
        self.sample_idx(idx)
    }
}

/// Local tangent-frame basis (t, b) from a surface normal. The baker handles mirrored-UV
/// handedness separately via the UV determinant.
pub fn tangent_basis(n: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let up = if n[0].abs() > 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let c = [
        n[1] * up[2] - n[2] * up[1],
        n[2] * up[0] - n[0] * up[2],
        n[0] * up[1] - n[1] * up[0],
    ];
    let cl = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1e-9);
    let t = [c[0] / cl, c[1] / cl, c[2] / cl];
    let b = [
        n[1] * t[2] - n[2] * t[1],
        n[2] * t[0] - n[0] * t[2],
        n[0] * t[1] - n[1] * t[0],
    ];
    (t, b)
}

#[inline]
fn blend_rgb(cur: [f32; 4], src: [f32; 3], alpha: f32) -> [f32; 4] {
    [
        cur[0] * (1.0 - alpha) + src[0] * alpha,
        cur[1] * (1.0 - alpha) + src[1] * alpha,
        cur[2] * (1.0 - alpha) + src[2] * alpha,
        cur[3],
    ]
}

/// (cos, sin, mirror_x, mirror_y, scale) for the clone stamp pattern transform.
#[inline]
fn clone_transform(brush: &Brush) -> (f32, f32, bool, bool, f32) {
    let a = brush.clone_rotation.to_radians();
    (
        a.cos(),
        a.sin(),
        brush.clone_mirror_x,
        brush.clone_mirror_y,
        brush.clone_scale.max(0.05),
    )
}

impl Layer {
    pub fn new_paint(id: usize, name: impl Into<String>, width: u32, height: u32) -> Self {
        let total = (width * height) as usize;
        Self {
            id,
            name: name.into(),
            layer_type: LayerType::Paint,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            folder_expanded: true,
            parent_folder_id: None,
            effects: Vec::new(),

            channel_color_enabled: true,
            channel_metallic_enabled: true,
            channel_roughness_enabled: true,
            channel_normal_enabled: true,
            channel_height_enabled: true,

            channel_color_blend: BlendMode::Normal,
            channel_metallic_blend: BlendMode::Normal,
            channel_roughness_blend: BlendMode::Normal,
            channel_normal_blend: BlendMode::Normal,
            channel_height_blend: BlendMode::Add,

            channel_color_opacity: 1.0,
            channel_metallic_opacity: 1.0,
            channel_roughness_opacity: 1.0,
            channel_normal_opacity: 1.0,
            channel_height_opacity: 1.0,

            mask: None,
            anchor_point: None,
            color_tag: None,
            generator: None,
            fill_color: [0.8, 0.8, 0.8, 1.0],
            fill_metallic: 0.0,
            fill_roughness: 0.5,
            fill_height: 0.0,
            color_buffer: vec![[0.0, 0.0, 0.0, 0.0]; total],
            metallic_buffer: vec![0.0; total],
            roughness_buffer: vec![0.5; total],
            normal_buffer: vec![[0.5, 0.5, 1.0]; total],
            height_buffer: vec![0.0; total],
            width,
            height,
        }
    }

    pub fn new_fill(
        id: usize,
        name: impl Into<String>,
        color: [f32; 4],
        metallic: f32,
        roughness: f32,
        width: u32,
        height: u32,
    ) -> Self {
        let total = (width * height) as usize;
        Self {
            id,
            name: name.into(),
            layer_type: LayerType::Fill,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            folder_expanded: true,
            parent_folder_id: None,
            effects: Vec::new(),

            channel_color_enabled: true,
            channel_metallic_enabled: true,
            channel_roughness_enabled: true,
            channel_normal_enabled: true,
            channel_height_enabled: true,

            channel_color_blend: BlendMode::Normal,
            channel_metallic_blend: BlendMode::Normal,
            channel_roughness_blend: BlendMode::Normal,
            channel_normal_blend: BlendMode::Normal,
            channel_height_blend: BlendMode::Add,

            channel_color_opacity: 1.0,
            channel_metallic_opacity: 1.0,
            channel_roughness_opacity: 1.0,
            channel_normal_opacity: 1.0,
            channel_height_opacity: 1.0,

            mask: None,
            anchor_point: None,
            color_tag: None,
            generator: None,
            fill_color: color,
            fill_metallic: metallic,
            fill_roughness: roughness,
            fill_height: 0.0,
            color_buffer: vec![color; total],
            metallic_buffer: vec![metallic; total],
            roughness_buffer: vec![roughness; total],
            normal_buffer: vec![[0.5, 0.5, 1.0]; total],
            height_buffer: vec![0.0; total],
            width,
            height,
        }
    }

    pub fn apply_effects(&mut self) {
        for effect in &self.effects {
            match effect {
                FilterEffect::Levels {
                    min_in,
                    max_in,
                    gamma,
                    min_out,
                    max_out,
                    channel,
                } => {
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::BaseColor
                    {
                        for p in &mut self.color_buffer {
                            p[0] = apply_levels_scalar(
                                p[0], *min_in, *max_in, *gamma, *min_out, *max_out,
                            );
                            p[1] = apply_levels_scalar(
                                p[1], *min_in, *max_in, *gamma, *min_out, *max_out,
                            );
                            p[2] = apply_levels_scalar(
                                p[2], *min_in, *max_in, *gamma, *min_out, *max_out,
                            );
                        }
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Metallic
                    {
                        for v in &mut self.metallic_buffer {
                            *v = apply_levels_scalar(
                                *v, *min_in, *max_in, *gamma, *min_out, *max_out,
                            );
                        }
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Roughness
                    {
                        for v in &mut self.roughness_buffer {
                            *v = apply_levels_scalar(
                                *v, *min_in, *max_in, *gamma, *min_out, *max_out,
                            );
                        }
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Height
                    {
                        for v in &mut self.height_buffer {
                            *v = apply_levels_scalar(
                                *v, *min_in, *max_in, *gamma, *min_out, *max_out,
                            );
                        }
                    }
                }
                FilterEffect::Blur { radius, channel } => {
                    let w = self.width as usize;
                    let h = self.height as usize;
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::BaseColor
                    {
                        self.color_buffer = gaussian_blur_rgba(&self.color_buffer, w, h, *radius);
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Metallic
                    {
                        self.metallic_buffer =
                            gaussian_blur_f32(&self.metallic_buffer, w, h, *radius);
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Roughness
                    {
                        self.roughness_buffer =
                            gaussian_blur_f32(&self.roughness_buffer, w, h, *radius);
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Height
                    {
                        self.height_buffer = gaussian_blur_f32(&self.height_buffer, w, h, *radius);
                    }
                }
                FilterEffect::Sharpen { strength, channel } => {
                    let w = self.width as usize;
                    let h = self.height as usize;
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::BaseColor
                    {
                        self.color_buffer = sharpen_rgba(&self.color_buffer, w, h, *strength);
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Metallic
                    {
                        self.metallic_buffer = sharpen_f32(&self.metallic_buffer, w, h, *strength);
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Roughness
                    {
                        self.roughness_buffer =
                            sharpen_f32(&self.roughness_buffer, w, h, *strength);
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Height
                    {
                        self.height_buffer = sharpen_f32(&self.height_buffer, w, h, *strength);
                    }
                }
                FilterEffect::HSL {
                    hue_shift,
                    sat_mult,
                    light_offset,
                } => {
                    for p in &mut self.color_buffer {
                        let (h, s, l) = rgb_to_hsl(p[0], p[1], p[2]);
                        let new_h = (h + hue_shift).rem_euclid(360.0);
                        let new_s = (s * sat_mult).clamp(0.0, 1.0);
                        let new_l = (l + light_offset).clamp(0.0, 1.0);
                        let (r, g, b) = hsl_to_rgb(new_h, new_s, new_l);
                        p[0] = r;
                        p[1] = g;
                        p[2] = b;
                    }
                }
                FilterEffect::Invert { channel } => {
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::BaseColor
                    {
                        for p in &mut self.color_buffer {
                            p[0] = (1.0 - p[0]).clamp(0.0, 1.0);
                            p[1] = (1.0 - p[1]).clamp(0.0, 1.0);
                            p[2] = (1.0 - p[2]).clamp(0.0, 1.0);
                        }
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Metallic
                    {
                        for v in &mut self.metallic_buffer {
                            *v = (1.0 - *v).clamp(0.0, 1.0);
                        }
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Roughness
                    {
                        for v in &mut self.roughness_buffer {
                            *v = (1.0 - *v).clamp(0.0, 1.0);
                        }
                    }
                    if *channel == FilterTargetChannel::All
                        || *channel == FilterTargetChannel::Height
                    {
                        for v in &mut self.height_buffer {
                            *v = -*v;
                        }
                    }
                }
                FilterEffect::GradientMap {
                    color_start,
                    color_end,
                } => {
                    for p in &mut self.color_buffer {
                        let lum = (p[0] * 0.299 + p[1] * 0.587 + p[2] * 0.114).clamp(0.0, 1.0);
                        p[0] = color_start[0] * (1.0 - lum) + color_end[0] * lum;
                        p[1] = color_start[1] * (1.0 - lum) + color_end[1] * lum;
                        p[2] = color_start[2] * (1.0 - lum) + color_end[2] * lum;
                    }
                }
            }
        }
    }

    pub fn update_fill(&mut self) {
        if self.layer_type == LayerType::Fill {
            let total = (self.width * self.height) as usize;
            for i in 0..total {
                self.color_buffer[i] = self.fill_color;
                self.metallic_buffer[i] = self.fill_metallic;
                self.roughness_buffer[i] = self.fill_roughness;
                self.height_buffer[i] = self.fill_height;
            }
        }
    }

    /// Loads authentic PBR texture asset (Smart Material or Material) with Base Color, Metallic, Roughness, Height & Normal
    pub fn from_texture_asset(
        id: usize,
        name: impl Into<String>,
        path: &Path,
        _category: usize,
        pbr_hint: ([f32; 4], f32, f32),
        width: u32,
        height: u32,
    ) -> Self {
        let name_str = name.into();
        let mut layer = Self::new_fill(
            id, name_str, pbr_hint.0, pbr_hint.1, pbr_hint.2, width, height,
        );

        if let Ok(img) = image::open(path) {
            let rgba_img = img.to_rgba8();
            let (img_w, img_h) = rgba_img.dimensions();
            let tiling = 2.0f64;

            for y in 0..height {
                let sy = (((y as f64 / height as f64) * tiling * (img_h as f64)) as u32) % img_h;
                for x in 0..width {
                    let sx = (((x as f64 / width as f64) * tiling * (img_w as f64)) as u32) % img_w;
                    let p = rgba_img.get_pixel(sx, sy);
                    let idx = (y * width + x) as usize;

                    let r = p[0] as f32 / 255.0;
                    let g = p[1] as f32 / 255.0;
                    let b = p[2] as f32 / 255.0;
                    let lum = r * 0.299 + g * 0.587 + b * 0.114;

                    layer.color_buffer[idx] = [r, g, b, 1.0];

                    let base_met = pbr_hint.1;
                    if base_met > 0.35 {
                        layer.metallic_buffer[idx] = (base_met * (0.8 + 0.2 * lum)).clamp(0.0, 1.0);
                    } else {
                        layer.metallic_buffer[idx] = 0.0;
                    }

                    let base_rough = pbr_hint.2;
                    let micro_rough = (base_rough + (lum - 0.5) * 0.35).clamp(0.04, 0.96);
                    layer.roughness_buffer[idx] = micro_rough;

                    let h_val = (lum - 0.5) * 2.0;
                    layer.height_buffer[idx] = h_val;
                }
            }

            // Normal map Sobel filter from displacement
            let w_i = width as i32;
            let h_i = height as i32;
            let strength = 2.2f32;

            for y in 0..h_i {
                for x in 0..w_i {
                    let idx = (y * w_i + x) as usize;
                    let x_prev = (x - 1).rem_euclid(w_i) as usize;
                    let x_next = (x + 1).rem_euclid(w_i) as usize;
                    let y_prev = (y - 1).rem_euclid(h_i) as usize;
                    let y_next = (y + 1).rem_euclid(h_i) as usize;

                    let hl = layer.height_buffer[y as usize * width as usize + x_prev];
                    let hr = layer.height_buffer[y as usize * width as usize + x_next];
                    let hd = layer.height_buffer[y_prev * width as usize + x as usize];
                    let hu = layer.height_buffer[y_next * width as usize + x as usize];

                    let dx = (hr - hl) * strength;
                    let dy = (hu - hd) * strength;
                    let len = (dx * dx + dy * dy + 1.0).sqrt();

                    let nx = -dx / len;
                    let ny = -dy / len;
                    let nz = 1.0 / len;

                    layer.normal_buffer[idx] = [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5];
                }
            }
        }
        layer
    }

    /// Applies an authentic reference Smart Mask (.spmsk / .webp) with baked curvature & AO wear
    pub fn apply_smart_mask_file(
        &mut self,
        mask_path: &Path,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
    ) {
        let mask_name = mask_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Smart Mask");
        let mask = super::material_gen::generate_smart_mask(
            mask_name,
            baked_curvature,
            baked_ao,
            self.width,
            self.height,
        );
        self.mask = Some(mask);
    }

    /// Attaches or updates a first-class Smart Mask preset driven by baked mesh maps.
    pub fn apply_smart_mask_preset(
        &mut self,
        preset: super::material_gen::SmartMaskPreset,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
    ) {
        let mask = preset.create_mask(
            self.width,
            self.height,
            baked_curvature,
            baked_ao,
            baked_wsn,
            baked_pos,
            baked_thickness,
            baked_id,
        );
        self.mask = Some(mask);
    }

    pub fn new_folder(id: usize, name: impl Into<String>, width: u32, height: u32) -> Self {
        let mut l = Self::new_paint(id, name, width, height);
        l.layer_type = LayerType::Folder;
        l.blend_mode = BlendMode::PassThrough;
        l
    }

    pub fn new_adjustment(
        id: usize,
        name: impl Into<String>,
        effect: FilterEffect,
        width: u32,
        height: u32,
    ) -> Self {
        let total = (width * height) as usize;
        let mut l = Self::new_paint(id, name, width, height);
        l.layer_type = LayerType::Adjustment;
        l.blend_mode = BlendMode::Normal;
        l.channel_height_blend = BlendMode::Normal;
        l.effects = vec![effect];
        l.color_tag = Some(7);
        l.color_buffer = vec![[0.0, 0.0, 0.0, 0.0]; total];
        l
    }

    /// Creates a first-class procedural generator layer driving multi-channel PBR attributes
    /// dynamically from baked mesh maps and noise.
    pub fn new_generator(
        id: usize,
        name: impl Into<String>,
        config: GeneratorConfig,
        color: [f32; 4],
        metallic: f32,
        roughness: f32,
        height: f32,
        width: u32,
        height_px: u32,
    ) -> Self {
        let mut l = Self::new_paint(id, name, width, height_px);
        l.layer_type = LayerType::Generator;
        l.generator = Some(config);
        l.fill_color = color;
        l.fill_metallic = metallic;
        l.fill_roughness = roughness;
        l.fill_height = height;
        l.color_tag = Some(3); // Amber/orange badge for generator
        l.channel_color_blend = BlendMode::Normal;
        l.channel_metallic_blend = BlendMode::Normal;
        l.channel_roughness_blend = BlendMode::Normal;
        l.channel_normal_blend = BlendMode::Normal;
        l.channel_height_blend = BlendMode::Add;
        l
    }

    pub fn new_generator_edge_wear(id: usize, width: u32, height: u32) -> Self {
        Self::new_generator(
            id,
            "Metal Edge Wear",
            GeneratorConfig::metal_edge_wear(),
            [0.85, 0.85, 0.85, 1.0],
            1.0,
            0.2,
            0.0,
            width,
            height,
        )
    }

    pub fn new_generator_dirt_crevices(id: usize, width: u32, height: u32) -> Self {
        Self::new_generator(
            id,
            "Dirt Crevices",
            GeneratorConfig::dirt_crevices(),
            [0.18, 0.12, 0.08, 1.0],
            0.0,
            0.9,
            -0.1,
            width,
            height,
        )
    }

    pub fn new_generator_dust(id: usize, width: u32, height: u32) -> Self {
        Self::new_generator(
            id,
            "Dust Accumulation",
            GeneratorConfig::dust_accumulation(),
            [0.65, 0.62, 0.58, 1.0],
            0.0,
            0.95,
            0.02,
            width,
            height,
        )
    }

    pub fn new_generator_position_gradient(id: usize, axis: u8, width: u32, height: u32) -> Self {
        Self::new_generator(
            id,
            "Position Gradient",
            GeneratorConfig::position_gradient(axis),
            [0.9, 0.9, 0.9, 1.0],
            0.0,
            0.5,
            0.0,
            width,
            height,
        )
    }

    pub fn new_generator_world_normal(id: usize, dir: [f32; 3], width: u32, height: u32) -> Self {
        Self::new_generator(
            id,
            "World Normal Orientation",
            GeneratorConfig::world_normal(dir),
            [0.9, 0.9, 0.9, 1.0],
            0.0,
            0.5,
            0.0,
            width,
            height,
        )
    }

    pub fn new_generator_thickness(id: usize, width: u32, height: u32) -> Self {
        Self::new_generator(
            id,
            "Thickness / Subsurface",
            GeneratorConfig::thickness(),
            [0.8, 0.2, 0.1, 1.0],
            0.0,
            0.6,
            0.0,
            width,
            height,
        )
    }

    pub fn new_generator_id_selection(
        id: usize,
        target: [f32; 4],
        tol: f32,
        width: u32,
        height: u32,
    ) -> Self {
        Self::new_generator(
            id,
            "Color / ID Selection",
            GeneratorConfig::color_id_selection(target, tol),
            [1.0, 0.3, 0.1, 1.0],
            0.0,
            0.5,
            0.0,
            width,
            height,
        )
    }

    /// Evaluates the procedural generator into the layer's channel buffers non-destructively
    /// from baked mesh maps and noise.
    pub fn evaluate_generator(
        &mut self,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
        anchor_maps: Option<&std::collections::HashMap<String, Vec<f32>>>,
    ) {
        let Some(ref config) = self.generator else {
            return;
        };
        if self.layer_type != LayerType::Generator {
            return;
        }

        let w = self.width;
        let h = self.height;
        let total = (w * h) as usize;
        if self.color_buffer.len() != total {
            self.color_buffer.resize(total, self.fill_color);
            self.metallic_buffer.resize(total, self.fill_metallic);
            self.roughness_buffer.resize(total, self.fill_roughness);
            self.height_buffer.resize(total, self.fill_height);
            self.normal_buffer.resize(total, [0.5, 0.5, 1.0]);
        }

        let (pos_min, pos_max) = if let Some(pos_slice) = baked_pos {
            let mut pmin = [f32::MAX, f32::MAX, f32::MAX];
            let mut pmax = [f32::MIN, f32::MIN, f32::MIN];
            for p in pos_slice {
                pmin[0] = pmin[0].min(p[0]);
                pmin[1] = pmin[1].min(p[1]);
                pmin[2] = pmin[2].min(p[2]);
                pmax[0] = pmax[0].max(p[0]);
                pmax[1] = pmax[1].max(p[1]);
                pmax[2] = pmax[2].max(p[2]);
            }
            (pmin, pmax)
        } else {
            ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
        };

        let anchor_buf = config
            .anchor_name
            .as_ref()
            .and_then(|aname| anchor_maps.and_then(|maps| maps.get(aname)));

        let w_f = w.max(1) as f32;
        let h_f = h.max(1) as f32;

        for y in 0..h {
            let v_coord = y as f32 / h_f;
            let row_offset = (y * w) as usize;
            for x in 0..w {
                let u_coord = x as f32 / w_f;
                let idx = row_offset + x as usize;
                let asig = anchor_buf.and_then(|abuf| abuf.get(idx).copied());

                let gen_val = evaluate_generator_pixel(
                    config,
                    idx,
                    u_coord,
                    v_coord,
                    pos_min,
                    pos_max,
                    baked_curvature,
                    baked_ao,
                    baked_wsn,
                    baked_pos,
                    baked_thickness,
                    baked_id,
                    asig,
                );

                self.color_buffer[idx] = [
                    self.fill_color[0],
                    self.fill_color[1],
                    self.fill_color[2],
                    (self.fill_color[3] * gen_val).clamp(0.0, 1.0),
                ];
                self.metallic_buffer[idx] = self.fill_metallic;
                self.roughness_buffer[idx] = self.fill_roughness;
                self.height_buffer[idx] = self.fill_height * gen_val;
                self.normal_buffer[idx] = [0.5, 0.5, 1.0];
            }
        }

        if !self.effects.is_empty() {
            self.apply_effects();
        }
    }

    pub fn add_black_mask(&mut self) {
        let mut mask = LayerMask::new(MaskType::Black, self.width, self.height);
        mask.recompute(None, None, None, None, None, None);
        self.mask = Some(mask);
    }

    pub fn add_white_mask(&mut self) {
        let mut mask = LayerMask::new(MaskType::White, self.width, self.height);
        mask.recompute(None, None, None, None, None, None);
        self.mask = Some(mask);
    }

    pub fn add_bitmap_mask(&mut self, path: &std::path::Path) {
        if let Ok(img) = image::open(path) {
            let gray = img.to_luma8();
            let (w, h) = (self.width as usize, self.height as usize);
            let mut mask = LayerMask::new(MaskType::White, self.width, self.height);
            let mut buf = vec![1.0f32; w * h];
            let (iw, ih) = gray.dimensions();
            for y in 0..self.height {
                for x in 0..self.width {
                    let sx = (x * iw / self.width).min(iw - 1);
                    let sy = (y * ih / self.height).min(ih - 1);
                    let lum = gray.get_pixel(sx, sy)[0] as f32 / 255.0;
                    buf[(y * self.width + x) as usize] = lum;
                }
            }
            mask.buffer = buf;
            self.mask = Some(mask);
        }
    }

    pub fn clear(&mut self) {
        if self.layer_type == LayerType::Paint {
            self.color_buffer.fill([0.0, 0.0, 0.0, 0.0]);
            self.metallic_buffer.fill(0.0);
            self.roughness_buffer.fill(0.5);
            self.normal_buffer.fill([0.5, 0.5, 1.0]);
            self.height_buffer.fill(0.0);
        } else if self.layer_type == LayerType::Fill {
            self.color_buffer.fill(self.fill_color);
            self.metallic_buffer.fill(self.fill_metallic);
            self.roughness_buffer.fill(self.fill_roughness);
            self.normal_buffer.fill([0.5, 0.5, 1.0]);
            self.height_buffer.fill(self.fill_height);
        } else if self.layer_type == LayerType::Generator {
            self.color_buffer.fill([0.0, 0.0, 0.0, 0.0]);
            self.metallic_buffer.fill(self.fill_metallic);
            self.roughness_buffer.fill(self.fill_roughness);
            self.normal_buffer.fill([0.5, 0.5, 1.0]);
            self.height_buffer.fill(0.0);
        }
    }

    pub fn paint_stamp(
        &mut self,
        uv: [f32; 2],
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if !on_mask
            && (self.layer_type == LayerType::Folder
                || self.layer_type == LayerType::Adjustment
                || self.layer_type == LayerType::Generator)
        {
            return [0, 0, 0, 0];
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let px_center = (uv[0].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        let py_center = (uv[1].clamp(0.0, 1.0) * (h - 1) as f32).round() as i32;
        let radius = (brush.size * 0.5).round() as i32;

        let min_x = (px_center - radius).clamp(0, w - 1);
        let max_x = (px_center + radius).clamp(0, w - 1);
        let min_y = (py_center - radius).clamp(0, h - 1);
        let max_y = (py_center + radius).clamp(0, h - 1);

        if min_x > max_x || min_y > max_y {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(self, [min_x, min_y, max_x, max_y]);
        }

        let hardness = brush.hardness.clamp(0.0, 0.999);
        let rad_f = radius.max(1) as f32;

        for y in min_y..=max_y {
            let py = y;
            let dy = (y - py_center) as f32;

            for x in min_x..=max_x {
                let px = x;
                let dx = (x - px_center) as f32;

                let alpha_base =
                    sample_brush_alpha(dx, dy, rad_f, hardness, brush.angle.to_radians(), brush);
                let alpha = (alpha_base * brush.opacity * brush.flow).clamp(0.0, 1.0);

                if alpha > 0.001 {
                    let idx = (py * w + px) as usize;

                    if on_mask {
                        if let Some(ref mut mask) = self.mask {
                            if idx < mask.buffer.len() {
                                mask.buffer[idx] =
                                    (mask.buffer[idx] * (1.0 - alpha) + alpha).clamp(0.0, 1.0);
                            }
                        }
                    } else {
                        if brush.paint_color && idx < self.color_buffer.len() {
                            let cur = self.color_buffer[idx];
                            let target = brush.color;
                            self.color_buffer[idx] = [
                                cur[0] * (1.0 - alpha) + target[0] * alpha,
                                cur[1] * (1.0 - alpha) + target[1] * alpha,
                                cur[2] * (1.0 - alpha) + target[2] * alpha,
                                (cur[3] + alpha).min(1.0),
                            ];
                        }
                        if brush.paint_metallic && idx < self.metallic_buffer.len() {
                            self.metallic_buffer[idx] =
                                self.metallic_buffer[idx] * (1.0 - alpha) + brush.metallic * alpha;
                        }
                        if brush.paint_roughness && idx < self.roughness_buffer.len() {
                            self.roughness_buffer[idx] = self.roughness_buffer[idx] * (1.0 - alpha)
                                + brush.roughness * alpha;
                        }
                        if brush.paint_normal && idx < self.normal_buffer.len() {
                            let cur = self.normal_buffer[idx];
                            self.normal_buffer[idx] = [
                                cur[0] * (1.0 - alpha) + brush.normal[0] * alpha,
                                cur[1] * (1.0 - alpha) + brush.normal[1] * alpha,
                                cur[2] * (1.0 - alpha) + brush.normal[2] * alpha,
                            ];
                        }
                        if brush.paint_height && idx < self.height_buffer.len() {
                            self.height_buffer[idx] =
                                self.height_buffer[idx] * (1.0 - alpha) + brush.height * alpha;
                        }
                    }
                }
            }
        }
        if min_x <= max_x && min_y <= max_y {
            [min_x, min_y, max_x, max_y]
        } else {
            [0, 0, 0, 0]
        }
    }

    pub fn paint_line(
        &mut self,
        from_uv: [f32; 2],
        to_uv: [f32; 2],
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        let dx = to_uv[0] - from_uv[0];
        let dy = to_uv[1] - from_uv[1];

        // If distance across UV space is large (e.g. crossing seams or separate UV shells), avoid drawing an interpolated smear
        if dx.abs() > 0.25 || dy.abs() > 0.25 {
            return self.paint_stamp(to_uv, brush, on_mask, history);
        }

        let pixel_dist =
            (dx * dx * (self.width as f32).powi(2) + dy * dy * (self.height as f32).powi(2)).sqrt();
        let step_size = (brush.size * brush.spacing).max(1.0);
        let steps = (pixel_dist / step_size).ceil().max(1.0) as usize;

        let mut bbox = [self.width as i32, self.height as i32, 0, 0];
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let u = (from_uv[0] + dx * t).clamp(0.0, 1.0);
            let v = (from_uv[1] + dy * t).clamp(0.0, 1.0);

            let s_box = self.paint_stamp([u, v], brush, on_mask, history.as_deref_mut());
            if s_box[2] >= s_box[0] && s_box[3] >= s_box[1] {
                bbox[0] = bbox[0].min(s_box[0]);
                bbox[1] = bbox[1].min(s_box[1]);
                bbox[2] = bbox[2].max(s_box[2]);
                bbox[3] = bbox[3].max(s_box[3]);
            }
        }
        bbox
    }

    pub fn erase_stamp(
        &mut self,
        uv: [f32; 2],
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if !on_mask
            && (self.layer_type == LayerType::Folder
                || self.layer_type == LayerType::Adjustment
                || self.layer_type == LayerType::Generator)
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let px_center = (uv[0].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        let py_center = (uv[1].clamp(0.0, 1.0) * (h - 1) as f32).round() as i32;
        let radius = (brush.size * 0.5).round() as i32;

        let min_x = (px_center - radius).clamp(0, w - 1);
        let max_x = (px_center + radius).clamp(0, w - 1);
        let min_y = (py_center - radius).clamp(0, h - 1);
        let max_y = (py_center + radius).clamp(0, h - 1);

        if min_x > max_x || min_y > max_y {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(self, [min_x, min_y, max_x, max_y]);
        }

        let hardness = brush.hardness.clamp(0.0, 0.999);
        let rad_f = radius.max(1) as f32;

        for y in min_y..=max_y {
            let py = y;
            let dy = (y - py_center) as f32;

            for x in min_x..=max_x {
                let px = x;
                let dx = (x - px_center) as f32;

                let alpha_base =
                    sample_brush_alpha(dx, dy, rad_f, hardness, brush.angle.to_radians(), brush);
                let alpha = (alpha_base * brush.opacity * brush.flow).clamp(0.0, 1.0);
                if alpha > 0.001 {
                    let idx = (py * w + px) as usize;
                    if on_mask {
                        if let Some(ref mut mask) = self.mask {
                            if idx < mask.buffer.len() {
                                mask.buffer[idx] = (mask.buffer[idx] - alpha).max(0.0);
                            }
                        }
                    } else if idx < self.color_buffer.len() {
                        self.color_buffer[idx][3] = (self.color_buffer[idx][3] - alpha).max(0.0);
                    }
                }
            }
        }
        [min_x, min_y, max_x, max_y]
    }

    pub fn erase_line(
        &mut self,
        from_uv: [f32; 2],
        to_uv: [f32; 2],
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        let dx = to_uv[0] - from_uv[0];
        let dy = to_uv[1] - from_uv[1];

        if dx.abs() > 0.25 || dy.abs() > 0.25 {
            return self.erase_stamp(to_uv, brush, on_mask, history);
        }

        let pixel_dist =
            (dx * dx * (self.width as f32).powi(2) + dy * dy * (self.height as f32).powi(2)).sqrt();
        let step_size = (brush.size * brush.spacing).max(1.0);
        let steps = (pixel_dist / step_size).ceil().max(1.0) as usize;

        let mut bbox = [self.width as i32, self.height as i32, 0, 0];
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let u = (from_uv[0] + dx * t).clamp(0.0, 1.0);
            let v = (from_uv[1] + dy * t).clamp(0.0, 1.0);

            let s_box = self.erase_stamp([u, v], brush, on_mask, history.as_deref_mut());
            if s_box[2] >= s_box[0] && s_box[3] >= s_box[1] {
                bbox[0] = bbox[0].min(s_box[0]);
                bbox[1] = bbox[1].min(s_box[1]);
                bbox[2] = bbox[2].max(s_box[2]);
                bbox[3] = bbox[3].max(s_box[3]);
            }
        }
        bbox
    }

    pub fn flood_fill(&mut self, brush: &Brush, mut history: Option<&mut HistoryManager>) {
        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(self, [0, 0, self.width as i32, self.height as i32]);
        }
        self.color_buffer.fill(brush.color);
        self.metallic_buffer.fill(brush.metallic);
        self.roughness_buffer.fill(brush.roughness);
        self.normal_buffer.fill(brush.normal);
        self.height_buffer.fill(brush.height);
    }

    /// Fills a specific triangle in UV space with current brush material
    pub fn fill_triangle(
        &mut self,
        uv0: [f32; 2],
        uv1: [f32; 2],
        uv2: [f32; 2],
        brush: &Brush,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;

        let min_u = uv0[0].min(uv1[0]).min(uv2[0]).clamp(0.0, 1.0);
        let max_u = uv0[0].max(uv1[0]).max(uv2[0]).clamp(0.0, 1.0);
        let min_v = uv0[1].min(uv1[1]).min(uv2[1]).clamp(0.0, 1.0);
        let max_v = uv0[1].max(uv1[1]).max(uv2[1]).clamp(0.0, 1.0);

        let px_min = ((min_u * (w_f - 1.0)).floor() as i32).clamp(0, w - 1);
        let px_max = ((max_u * (w_f - 1.0)).ceil() as i32).clamp(0, w - 1);
        let py_min = ((min_v * (h_f - 1.0)).floor() as i32).clamp(0, h - 1);
        let py_max = ((max_v * (h_f - 1.0)).ceil() as i32).clamp(0, h - 1);

        if px_min > px_max || py_min > py_max {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(self, [px_min, py_min, px_max, py_max]);
        }

        let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
            .max(1e-7);

        for py in py_min..=py_max {
            let v_px = py as f32 / (h_f - 1.0);
            let row_offset = py as usize * self.width as usize;
            for px in px_min..=px_max {
                let u_px = px as f32 / (w_f - 1.0);
                let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                    + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                    / denom;
                let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                    + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                    / denom;
                let b2 = 1.0 - b0 - b1;

                if b0 >= -0.01 && b1 >= -0.01 && b2 >= -0.01 {
                    let idx = row_offset + px as usize;
                    if brush.paint_color && idx < self.color_buffer.len() {
                        self.color_buffer[idx] = brush.color;
                    }
                    if brush.paint_metallic && idx < self.metallic_buffer.len() {
                        self.metallic_buffer[idx] = brush.metallic;
                    }
                    if brush.paint_roughness && idx < self.roughness_buffer.len() {
                        self.roughness_buffer[idx] = brush.roughness;
                    }
                    if brush.paint_normal && idx < self.normal_buffer.len() {
                        self.normal_buffer[idx] = brush.normal;
                    }
                    if brush.paint_height && idx < self.height_buffer.len() {
                        self.height_buffer[idx] = brush.height;
                    }
                }
            }
        }
        [px_min, py_min, px_max, py_max]
    }

    /// 3D Surface Projection Painting: Evaluates Euclidean distance on the 3D mesh surface.
    /// Paints seamlessly across UV seams, singularities/poles, and disconnected shells with zero distortion.
    pub fn paint_3d_surface(
        &mut self,
        mesh: &crate::graphics::mesh::Mesh,
        hit_pos: [f32; 3],
        hit_normal: [f32; 3],
        radius_3d: f32,
        brush: &Brush,
        is_eraser: bool,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if !on_mask
            && (self.layer_type == LayerType::Folder
                || self.layer_type == LayerType::Adjustment
                || self.layer_type == LayerType::Generator)
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;
        let num_tris = mesh.indices.len() / 3;
        let r2 = radius_3d * radius_3d;
        let hardness = brush.hardness.clamp(0.0, 0.999);
        // Stamp-local tangent frame: per-pixel alpha now honors alpha shapes, custom alphas,
        // stencils and brush angle in 3D (previously 3D painting used a plain distance falloff)
        let (tan_v, bin_v) = tangent_basis(hit_normal);
        let angle_rad = brush.angle.to_radians();

        let mut bbox = [w, h, 0, 0];
        let mut any_painted = false;

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;

            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                continue;
            }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
                continue;
            }

            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];

            // 3D Triangle AABB check against the brush sphere
            let tri_min_x = v0[0].min(v1[0]).min(v2[0]);
            let tri_max_x = v0[0].max(v1[0]).max(v2[0]);
            let tri_min_y = v0[1].min(v1[1]).min(v2[1]);
            let tri_max_y = v0[1].max(v1[1]).max(v2[1]);
            let tri_min_z = v0[2].min(v1[2]).min(v2[2]);
            let tri_max_z = v0[2].max(v1[2]).max(v2[2]);

            if hit_pos[0] + radius_3d < tri_min_x
                || hit_pos[0] - radius_3d > tri_max_x
                || hit_pos[1] + radius_3d < tri_min_y
                || hit_pos[1] - radius_3d > tri_max_y
                || hit_pos[2] + radius_3d < tri_min_z
                || hit_pos[2] - radius_3d > tri_max_z
            {
                continue;
            }

            // Normal check: back-face culling to prevent bleeding through to the backside
            let n0 = mesh.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n1 = mesh.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n2 = mesh.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
            let avg_n = [
                (n0[0] + n1[0] + n2[0]) * 0.3333,
                (n0[1] + n1[1] + n2[1]) * 0.3333,
                (n0[2] + n1[2] + n2[2]) * 0.3333,
            ];
            let dot =
                avg_n[0] * hit_normal[0] + avg_n[1] * hit_normal[1] + avg_n[2] * hit_normal[2];
            // Paint-through modifier (PaintThroughModifier parity): paint back-facing geometry too
            if dot < 0.05 && !brush.paint_through {
                continue;
            }

            let uv0 = mesh.uvs[i0];
            let uv1 = mesh.uvs[i1];
            let uv2 = mesh.uvs[i2];

            let min_u = uv0[0].min(uv1[0]).min(uv2[0]).clamp(0.0, 1.0);
            let max_u = uv0[0].max(uv1[0]).max(uv2[0]).clamp(0.0, 1.0);
            let min_v = uv0[1].min(uv1[1]).min(uv2[1]).clamp(0.0, 1.0);
            let max_v = uv0[1].max(uv1[1]).max(uv2[1]).clamp(0.0, 1.0);

            let px_min = ((min_u * (w_f - 1.0)).floor() as i32).clamp(0, w - 1);
            let px_max = ((max_u * (w_f - 1.0)).ceil() as i32).clamp(0, w - 1);
            let py_min = ((min_v * (h_f - 1.0)).floor() as i32).clamp(0, h - 1);
            let py_max = ((max_v * (h_f - 1.0)).ceil() as i32).clamp(0, h - 1);

            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0])
                + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
                .max(1e-7);

            if let Some(ref mut hist) = history {
                hist.record_bbox_before_modify(self, [px_min, py_min, px_max, py_max]);
            }

            for py in py_min..=py_max {
                let v_px = py as f32 / (h_f - 1.0);
                let row_offset = py as usize * self.width as usize;

                for px in px_min..=px_max {
                    let u_px = px as f32 / (w_f - 1.0);

                    let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                        + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                        / denom;
                    let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                        + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                        / denom;
                    let b2 = 1.0 - b0 - b1;

                    if b0 >= -0.05 && b1 >= -0.05 && b2 >= -0.05 {
                        let p3d_x = b0 * v0[0] + b1 * v1[0] + b2 * v2[0];
                        let p3d_y = b0 * v0[1] + b1 * v1[1] + b2 * v2[1];
                        let p3d_z = b0 * v0[2] + b1 * v1[2] + b2 * v2[2];

                        let dx = p3d_x - hit_pos[0];
                        let dy = p3d_y - hit_pos[1];
                        let dz = p3d_z - hit_pos[2];
                        let dist2 = dx * dx + dy * dy + dz * dz;

                        if dist2 <= r2 {
                            let inv_r = 1.0 / radius_3d;
                            let lt =
                                ((dx * tan_v[0] + dy * tan_v[1] + dz * tan_v[2]) * inv_r) * 64.0;
                            let lb =
                                ((dx * bin_v[0] + dy * bin_v[1] + dz * bin_v[2]) * inv_r) * 64.0;
                            let brush_alpha =
                                sample_brush_alpha(lt, lb, 64.0, hardness, angle_rad, brush);
                            let alpha = (brush_alpha * brush.opacity * brush.flow).clamp(0.0, 1.0);

                            if alpha > 0.001 {
                                let idx = row_offset + px as usize;
                                if on_mask {
                                    if let Some(ref mut mask) = self.mask {
                                        if idx < mask.buffer.len() {
                                            if is_eraser {
                                                mask.buffer[idx] =
                                                    (mask.buffer[idx] - alpha).max(0.0);
                                            } else {
                                                mask.buffer[idx] =
                                                    (mask.buffer[idx] * (1.0 - alpha) + alpha)
                                                        .clamp(0.0, 1.0);
                                            }
                                        }
                                    }
                                } else if is_eraser {
                                    if idx < self.color_buffer.len() {
                                        self.color_buffer[idx][3] =
                                            (self.color_buffer[idx][3] - alpha).max(0.0);
                                    }
                                } else {
                                    if brush.paint_color && idx < self.color_buffer.len() {
                                        let cur = self.color_buffer[idx];
                                        let target = brush.color;
                                        self.color_buffer[idx] = [
                                            cur[0] * (1.0 - alpha) + target[0] * alpha,
                                            cur[1] * (1.0 - alpha) + target[1] * alpha,
                                            cur[2] * (1.0 - alpha) + target[2] * alpha,
                                            (cur[3] + alpha).min(1.0),
                                        ];
                                    }
                                    if brush.paint_metallic && idx < self.metallic_buffer.len() {
                                        self.metallic_buffer[idx] = self.metallic_buffer[idx]
                                            * (1.0 - alpha)
                                            + brush.metallic * alpha;
                                    }
                                    if brush.paint_roughness && idx < self.roughness_buffer.len() {
                                        self.roughness_buffer[idx] = self.roughness_buffer[idx]
                                            * (1.0 - alpha)
                                            + brush.roughness * alpha;
                                    }
                                    if brush.paint_normal && idx < self.normal_buffer.len() {
                                        let cur = self.normal_buffer[idx];
                                        self.normal_buffer[idx] = [
                                            cur[0] * (1.0 - alpha) + brush.normal[0] * alpha,
                                            cur[1] * (1.0 - alpha) + brush.normal[1] * alpha,
                                            cur[2] * (1.0 - alpha) + brush.normal[2] * alpha,
                                        ];
                                    }
                                    if brush.paint_height && idx < self.height_buffer.len() {
                                        self.height_buffer[idx] = self.height_buffer[idx]
                                            * (1.0 - alpha)
                                            + brush.height * alpha;
                                    }
                                }

                                bbox[0] = bbox[0].min(px);
                                bbox[1] = bbox[1].min(py);
                                bbox[2] = bbox[2].max(px);
                                bbox[3] = bbox[3].max(py);
                                any_painted = true;
                            }
                        }
                    }
                }
            }
        }

        if any_painted && bbox[2] >= bbox[0] && bbox[3] >= bbox[1] {
            bbox
        } else {
            [0, 0, 0, 0]
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayerStack {
    pub layers: Vec<Layer>,
    pub active_layer_index: usize,
    pub width: u32,
    pub height: u32,

    pub composite_color: Vec<[f32; 4]>,
    pub composite_metallic: Vec<f32>,
    pub composite_roughness: Vec<f32>,
    pub composite_normal: Vec<[f32; 3]>,
    pub composite_height: Vec<f32>,
    pub composite_lit: Vec<[f32; 4]>,
    pub height_to_normal_scale: f32,
}

impl LayerStack {
    /// Alpha-falloff edge term shared by the stamp functions.
    #[inline]
    fn edge_alpha(norm_dist: f32, hardness: f32) -> f32 {
        if norm_dist <= hardness {
            1.0
        } else {
            (1.0 - (norm_dist - hardness) / (1.0 - hardness).max(0.001)).clamp(0.0, 1.0)
        }
    }

    /// Writes one pixel of paint/erase into a layer (shared by 2D/3D/projection stamps).
    #[inline]
    fn apply_paint_px(
        layer: &mut Layer,
        idx: usize,
        alpha: f32,
        brush: &Brush,
        is_eraser: bool,
        on_mask: bool,
    ) {
        if !on_mask
            && (layer.layer_type == LayerType::Folder
                || layer.layer_type == LayerType::Adjustment
                || layer.layer_type == LayerType::Generator)
        {
            return;
        }
        if on_mask {
            if let Some(ref mut mask) = layer.mask {
                if idx < mask.buffer.len() {
                    if is_eraser {
                        mask.buffer[idx] = (mask.buffer[idx] - alpha).max(0.0);
                    } else {
                        mask.buffer[idx] =
                            (mask.buffer[idx] * (1.0 - alpha) + alpha).clamp(0.0, 1.0);
                    }
                }
            }
            return;
        }
        if is_eraser {
            if idx < layer.color_buffer.len() {
                layer.color_buffer[idx][3] = (layer.color_buffer[idx][3] - alpha).max(0.0);
            }
            return;
        }
        if brush.paint_color && idx < layer.color_buffer.len() {
            let cur = layer.color_buffer[idx];
            layer.color_buffer[idx] = [
                cur[0].mul_add(1.0 - alpha, brush.color[0] * alpha),
                cur[1].mul_add(1.0 - alpha, brush.color[1] * alpha),
                cur[2].mul_add(1.0 - alpha, brush.color[2] * alpha),
                (cur[3] + alpha).min(1.0),
            ];
        }
        if brush.paint_metallic && idx < layer.metallic_buffer.len() {
            layer.metallic_buffer[idx] =
                layer.metallic_buffer[idx].mul_add(1.0 - alpha, brush.metallic * alpha);
        }
        if brush.paint_roughness && idx < layer.roughness_buffer.len() {
            layer.roughness_buffer[idx] =
                layer.roughness_buffer[idx].mul_add(1.0 - alpha, brush.roughness * alpha);
        }
        if brush.paint_normal && idx < layer.normal_buffer.len() {
            let cur = layer.normal_buffer[idx];
            layer.normal_buffer[idx] = [
                cur[0].mul_add(1.0 - alpha, brush.normal[0] * alpha),
                cur[1].mul_add(1.0 - alpha, brush.normal[1] * alpha),
                cur[2].mul_add(1.0 - alpha, brush.normal[2] * alpha),
            ];
        }
        if brush.paint_height && idx < layer.height_buffer.len() {
            layer.height_buffer[idx] =
                layer.height_buffer[idx].mul_add(1.0 - alpha, brush.height * alpha);
        }
    }

    /// Per-pixel clone write: copies full channel state from the sampled composite region.
    #[inline]
    fn apply_clone_px(
        layer: &mut Layer,
        idx: usize,
        alpha: f32,
        src: ([f32; 4], f32, f32, [f32; 3], f32),
        brush: &Brush,
        on_mask: bool,
    ) {
        let (sc, s_met, s_rough, s_norm, s_height) = src;
        if on_mask {
            if let Some(ref mut mask) = layer.mask {
                if idx < mask.buffer.len() {
                    let lum = (sc[0] * 0.299 + sc[1] * 0.587 + sc[2] * 0.114) * sc[3];
                    mask.buffer[idx] =
                        (mask.buffer[idx] * (1.0 - alpha) + lum * alpha).clamp(0.0, 1.0);
                }
            }
            return;
        }
        if idx < layer.color_buffer.len() {
            let cur = layer.color_buffer[idx];
            layer.color_buffer[idx] = [
                cur[0].mul_add(1.0 - alpha, sc[0] * alpha),
                cur[1].mul_add(1.0 - alpha, sc[1] * alpha),
                cur[2].mul_add(1.0 - alpha, sc[2] * alpha),
                (cur[3] + alpha * sc[3]).min(1.0),
            ];
        }
        if idx < layer.metallic_buffer.len() {
            layer.metallic_buffer[idx] =
                layer.metallic_buffer[idx].mul_add(1.0 - alpha, s_met * alpha);
        }
        if idx < layer.roughness_buffer.len() {
            layer.roughness_buffer[idx] =
                layer.roughness_buffer[idx].mul_add(1.0 - alpha, s_rough * alpha);
        }
        if brush.paint_normal && idx < layer.normal_buffer.len() {
            let cur = layer.normal_buffer[idx];
            layer.normal_buffer[idx] = [
                cur[0].mul_add(1.0 - alpha, s_norm[0] * alpha),
                cur[1].mul_add(1.0 - alpha, s_norm[1] * alpha),
                cur[2].mul_add(1.0 - alpha, s_norm[2] * alpha),
            ];
        }
        if brush.paint_height && idx < layer.height_buffer.len() {
            layer.height_buffer[idx] =
                layer.height_buffer[idx].mul_add(1.0 - alpha, s_height * alpha);
        }
    }

    /// Transformed clone-source pixel for a destination local offset (dx, dy) in texture pixels.
    /// Rotation/scale/mirror are applied around each waypoint's center.
    #[inline]
    fn clone_source_px(
        dx: f32,
        dy: f32,
        src_center: [f32; 2],
        w: i32,
        h: i32,
        brush: &Brush,
    ) -> Option<usize> {
        let a = brush.clone_rotation.to_radians();
        let (ca, sa) = (a.cos(), a.sin());
        let sc = brush.clone_scale.max(0.05);
        let mut tx = (dx * ca - dy * sa) / sc;
        let mut ty = (dx * sa + dy * ca) / sc;
        if brush.clone_mirror_x {
            tx = -tx;
        }
        if brush.clone_mirror_y {
            ty = -ty;
        }
        let spx = (src_center[0] + tx).round() as i32;
        let spy = (src_center[1] + ty).round() as i32;
        if spx < 0 || spx >= w || spy < 0 || spy >= h {
            return None;
        }
        Some((spy * w + spx) as usize)
    }

    #[inline]
    fn sample_composite_src(&self, idx: usize) -> ([f32; 4], f32, f32, [f32; 3], f32) {
        let n = self.composite_color.len();
        let i = idx.min(n.saturating_sub(1));
        (
            self.composite_color[i],
            self.composite_metallic[i.min(self.composite_metallic.len().saturating_sub(1))],
            self.composite_roughness[i.min(self.composite_roughness.len().saturating_sub(1))],
            self.composite_normal[i.min(self.composite_normal.len().saturating_sub(1))],
            self.composite_height[i.min(self.composite_height.len().saturating_sub(1))],
        )
    }

    // ───────────────────── Smudge (EngineActionSmudge parity) ─────────────────────

    /// 2D UV displacement smudge. `delta` = current − previous texture-space stroke motion.
    /// Pulls content from behind the stroke from the layer's own pre-stamp state, or from the
    /// merged composite when `brush.smudge_sample_merged` is set. Finger Painting lays down
    /// the brush color instead.
    pub fn smudge_stamp_at(
        &mut self,
        uv: [f32; 2],
        delta: [f32; 2],
        brush: &Brush,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if self.layers.is_empty() {
            return [0, 0, 0, 0];
        }
        let li = self.active_layer_index.min(self.layers.len() - 1);
        if self.layers[li].layer_type == LayerType::Folder
            || self.layers[li].layer_type == LayerType::Adjustment
            || self.layers[li].layer_type == LayerType::Generator
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let px_center = (uv[0].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        let py_center = (uv[1].clamp(0.0, 1.0) * (h - 1) as f32).round() as i32;
        let radius = (brush.size * 0.5).round() as i32;
        let min_x = (px_center - radius).clamp(0, w - 1);
        let max_x = (px_center + radius).clamp(0, w - 1);
        let min_y = (py_center - radius).clamp(0, h - 1);
        let max_y = (py_center + radius).clamp(0, h - 1);
        if min_x > max_x || min_y > max_y {
            return [0, 0, 0, 0];
        }

        let hardness = brush.hardness.clamp(0.0, 0.999);
        let rad_f = radius.max(1) as f32;
        // Displacement opposite to stroke motion: new(p) += old(p - Δ') blended by flow
        let dxp = -delta[0] * (w - 1) as f32;
        let dyp = -delta[1] * (h - 1) as f32;
        let finger = brush.smudge_finger_painting;
        let merged = brush.smudge_sample_merged;
        let brush_rgb = [brush.color[0], brush.color[1], brush.color[2]];

        struct Pulled {
            idx: usize,
            rgb: [f32; 3],
            met: f32,
            rough: f32,
            alpha: f32,
            has_src: bool,
        }
        let mut pulls: Vec<Pulled> = Vec::new();
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = (px - px_center) as f32;
                let dy = (py - py_center) as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > rad_f {
                    continue;
                }
                let alpha = (Self::edge_alpha(dist / rad_f, hardness) * brush.flow).clamp(0.0, 1.0);
                if alpha < 0.001 {
                    continue;
                }
                let idx = (py * w + px) as usize;
                let sx = ((px as f32 + dxp).round() as i32).clamp(0, w - 1);
                let sy = ((py as f32 + dyp).round() as i32).clamp(0, h - 1);
                let sidx = (sy * w + sx) as usize;
                let (rgb, met, rough, has_src) = if finger {
                    (brush_rgb, brush.metallic, brush.roughness, true)
                } else if merged {
                    let c = self.composite_color[sidx.min(self.composite_color.len())
                        - if sidx < self.composite_color.len() {
                            0
                        } else {
                            1
                        }];
                    (
                        [c[0], c[1], c[2]],
                        self.composite_metallic[sidx.min(self.composite_metallic.len() - 1)],
                        self.composite_roughness[sidx.min(self.composite_roughness.len() - 1)],
                        c[3] >= 0.01,
                    )
                } else {
                    let buf = &self.layers[li].color_buffer;
                    let c = buf[sidx.min(buf.len() - 1)];
                    (
                        [c[0], c[1], c[2]],
                        self.layers[li].metallic_buffer
                            [sidx.min(self.layers[li].metallic_buffer.len() - 1)],
                        self.layers[li].roughness_buffer
                            [sidx.min(self.layers[li].roughness_buffer.len() - 1)],
                        c[3] >= 0.01,
                    )
                };
                pulls.push(Pulled {
                    idx,
                    rgb,
                    met,
                    rough,
                    alpha,
                    has_src,
                });
            }
        }
        if pulls.is_empty() {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(&self.layers[li], [min_x, min_y, max_x, max_y]);
        }

        let layer = &mut self.layers[li];
        for p in &pulls {
            if !p.has_src {
                continue;
            }
            let cur = layer.color_buffer[p.idx];
            layer.color_buffer[p.idx] = [
                cur[0].mul_add(1.0 - p.alpha, p.rgb[0] * p.alpha),
                cur[1].mul_add(1.0 - p.alpha, p.rgb[1] * p.alpha),
                cur[2].mul_add(1.0 - p.alpha, p.rgb[2] * p.alpha),
                if finger {
                    (cur[3] + p.alpha * 0.35).min(1.0)
                } else {
                    cur[3]
                },
            ];
            if p.idx < layer.metallic_buffer.len() {
                layer.metallic_buffer[p.idx] =
                    layer.metallic_buffer[p.idx].mul_add(1.0 - p.alpha, p.met * p.alpha);
            }
            if p.idx < layer.roughness_buffer.len() {
                layer.roughness_buffer[p.idx] =
                    layer.roughness_buffer[p.idx].mul_add(1.0 - p.alpha, p.rough * p.alpha);
            }
        }
        [min_x, min_y, max_x, max_y]
    }

    /// 3D surface displacement smudge. Same semantics as `smudge_stamp_at`, only the stamp is
    /// rasterized across the tangent-space projection of the 3D brush sphere (`delta_uv` is the
    /// texture-space motion of the hit).
    pub fn smudge_3d_at(
        &mut self,
        mesh: &crate::graphics::mesh::Mesh,
        hit_pos: [f32; 3],
        hit_normal: [f32; 3],
        radius_3d: f32,
        delta_uv: [f32; 2],
        brush: &Brush,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if self.layers.is_empty() {
            return [0, 0, 0, 0];
        }
        let li = self.active_layer_index.min(self.layers.len() - 1);
        if self.layers[li].layer_type == LayerType::Folder
            || self.layers[li].layer_type == LayerType::Adjustment
            || self.layers[li].layer_type == LayerType::Generator
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;
        let num_tris = mesh.indices.len() / 3;
        let r2 = radius_3d * radius_3d;
        let rad_f = radius_3d.max(1e-6);
        let hardness = brush.hardness.clamp(0.0, 0.999);
        let (tan_v, bin_v) = tangent_basis(hit_normal);
        let finger = brush.smudge_finger_painting;
        let merged = brush.smudge_sample_merged;
        let brush_rgb = [brush.color[0], brush.color[1], brush.color[2]];

        struct Pulled {
            idx: usize,
            rgb: [f32; 3],
            met: f32,
            rough: f32,
            alpha: f32,
            has_src: bool,
        }
        let mut pulls: Vec<Pulled> = Vec::new();
        let mut bbox = [w, h, 0, 0];

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                continue;
            }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
                continue;
            }
            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];
            if hit_pos[0] + radius_3d < v0[0].min(v1[0]).min(v2[0])
                || hit_pos[0] - radius_3d > v0[0].max(v1[0]).max(v2[0])
                || hit_pos[1] + radius_3d < v0[1].min(v1[1]).min(v2[1])
                || hit_pos[1] - radius_3d > v0[1].max(v1[1]).max(v2[1])
                || hit_pos[2] + radius_3d < v0[2].min(v1[2]).min(v2[2])
                || hit_pos[2] - radius_3d > v0[2].max(v1[2]).max(v2[2])
            {
                continue;
            }
            let n0 = mesh.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n1 = mesh.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n2 = mesh.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
            let avg_n = [
                (n0[0] + n1[0] + n2[0]) * 0.3333,
                (n0[1] + n1[1] + n2[1]) * 0.3333,
                (n0[2] + n1[2] + n2[2]) * 0.3333,
            ];
            if avg_n[0] * hit_normal[0] + avg_n[1] * hit_normal[1] + avg_n[2] * hit_normal[2] < 0.05
            {
                continue;
            }

            let uv0 = mesh.uvs[i0];
            let uv1 = mesh.uvs[i1];
            let uv2 = mesh.uvs[i2];
            let px_min =
                ((uv0[0].min(uv1[0]).min(uv2[0]) * (w_f - 1.0)).floor() as i32).clamp(0, w - 1);
            let px_max =
                ((uv0[0].max(uv1[0]).max(uv2[0]) * (w_f - 1.0)).ceil() as i32).clamp(0, w - 1);
            let py_min =
                ((uv0[1].min(uv1[1]).min(uv2[1]) * (h_f - 1.0)).floor() as i32).clamp(0, h - 1);
            let py_max =
                ((uv0[1].max(uv1[1]).max(uv2[1]) * (h_f - 1.0)).ceil() as i32).clamp(0, h - 1);
            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0])
                + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
                .max(1e-7);

            for py in py_min..=py_max {
                let v_px = py as f32 / (h_f - 1.0);
                for px in px_min..=px_max {
                    let u_px = px as f32 / (w_f - 1.0);
                    let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                        + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                        / denom;
                    let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                        + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                        / denom;
                    let b2 = 1.0 - b0 - b1;
                    if b0 < -0.05 || b1 < -0.05 || b2 < -0.05 {
                        continue;
                    }
                    let p3d = [
                        b0 * v0[0] + b1 * v1[0] + b2 * v2[0],
                        b0 * v0[1] + b1 * v1[1] + b2 * v2[1],
                        b0 * v0[2] + b1 * v1[2] + b2 * v2[2],
                    ];
                    let dp = [
                        p3d[0] - hit_pos[0],
                        p3d[1] - hit_pos[1],
                        p3d[2] - hit_pos[2],
                    ];
                    if dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2] > r2 {
                        continue;
                    }
                    // Shape/falloff in stamp-local tangent space (matches the 2D stamp and honors
                    // alpha shapes, custom alphas, brushes' angle and stencils)
                    let lt =
                        ((dp[0] * tan_v[0] + dp[1] * tan_v[1] + dp[2] * tan_v[2]) / rad_f) * 64.0;
                    let lb =
                        ((dp[0] * bin_v[0] + dp[1] * bin_v[1] + dp[2] * bin_v[2]) / rad_f) * 64.0;
                    let shape =
                        sample_brush_alpha(lt, lb, 64.0, hardness, brush.angle.to_radians(), brush);
                    let alpha = (shape * brush.flow).clamp(0.0, 1.0);
                    if alpha < 0.001 {
                        continue;
                    }
                    let idx = (py * w + px) as usize;
                    if idx < self.layers[li].color_buffer.len() {
                        let su = (u_px - delta_uv[0]).clamp(0.0, 1.0);
                        let sv = (v_px - delta_uv[1]).clamp(0.0, 1.0);
                        let sidx = (((sv * (h_f - 1.0)).round() as i32).clamp(0, h - 1) * w
                            + ((su * (w_f - 1.0)).round() as i32).clamp(0, w - 1))
                            as usize;
                        let (rgb, met, rough, has_src) = if finger {
                            (brush_rgb, brush.metallic, brush.roughness, true)
                        } else if merged {
                            let c = self.composite_color[sidx.min(self.composite_color.len() - 1)];
                            (
                                [c[0], c[1], c[2]],
                                self.composite_metallic
                                    [sidx.min(self.composite_metallic.len() - 1)],
                                self.composite_roughness
                                    [sidx.min(self.composite_roughness.len() - 1)],
                                c[3] >= 0.01,
                            )
                        } else {
                            let lb2 = &self.layers[li];
                            let c = lb2.color_buffer[sidx.min(lb2.color_buffer.len() - 1)];
                            (
                                [c[0], c[1], c[2]],
                                lb2.metallic_buffer[sidx.min(lb2.metallic_buffer.len() - 1)],
                                lb2.roughness_buffer[sidx.min(lb2.roughness_buffer.len() - 1)],
                                c[3] >= 0.01,
                            )
                        };
                        pulls.push(Pulled {
                            idx,
                            rgb,
                            met,
                            rough,
                            alpha,
                            has_src,
                        });
                        bbox[0] = bbox[0].min(px);
                        bbox[1] = bbox[1].min(py);
                        bbox[2] = bbox[2].max(px);
                        bbox[3] = bbox[3].max(py);
                    }
                }
            }
        }
        if pulls.is_empty() || bbox[2] < bbox[0] {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(&self.layers[li], bbox);
        }

        let layer = &mut self.layers[li];
        for p in &pulls {
            if !p.has_src {
                continue;
            }
            let cur = layer.color_buffer[p.idx];
            layer.color_buffer[p.idx] = [
                cur[0].mul_add(1.0 - p.alpha, p.rgb[0] * p.alpha),
                cur[1].mul_add(1.0 - p.alpha, p.rgb[1] * p.alpha),
                cur[2].mul_add(1.0 - p.alpha, p.rgb[2] * p.alpha),
                if finger {
                    (cur[3] + p.alpha * 0.35).min(1.0)
                } else {
                    cur[3]
                },
            ];
            if p.idx < layer.metallic_buffer.len() {
                layer.metallic_buffer[p.idx] =
                    layer.metallic_buffer[p.idx].mul_add(1.0 - p.alpha, p.met * p.alpha);
            }
            if p.idx < layer.roughness_buffer.len() {
                layer.roughness_buffer[p.idx] =
                    layer.roughness_buffer[p.idx].mul_add(1.0 - p.alpha, p.rough * p.alpha);
            }
        }
        bbox
    }

    // ───────────────────── Clone stamp v2 (full-parity aligned clone) ─────────────────────

    /// UV-space clone stamp: copies full channel state (color, alpha, metallic, roughness and,
    /// when enabled, normal/height) from the merged composite at `src_uv` into the active
    /// layer at `dst_uv`, honoring rotation/scale/mirror pattern transforms.
    pub fn clone_stamp_at(
        &mut self,
        dst_uv: [f32; 2],
        src_uv: [f32; 2],
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if self.layers.is_empty() {
            return [0, 0, 0, 0];
        }
        let li = self.active_layer_index.min(self.layers.len() - 1);
        if !on_mask
            && (self.layers[li].layer_type == LayerType::Folder
                || self.layers[li].layer_type == LayerType::Adjustment
                || self.layers[li].layer_type == LayerType::Generator)
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let px_center = (dst_uv[0].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        let py_center = (dst_uv[1].clamp(0.0, 1.0) * (h - 1) as f32).round() as i32;
        let radius = (brush.size * 0.5).round() as i32;
        let min_x = (px_center - radius).clamp(0, w - 1);
        let max_x = (px_center + radius).clamp(0, w - 1);
        let min_y = (py_center - radius).clamp(0, h - 1);
        let max_y = (py_center + radius).clamp(0, h - 1);
        if min_x > max_x || min_y > max_y {
            return [0, 0, 0, 0];
        }

        let hardness = brush.hardness.clamp(0.0, 0.999);
        let rad_f = radius.max(1) as f32;
        let src_center = [
            src_uv[0].clamp(0.0, 1.0) * (w - 1) as f32,
            src_uv[1].clamp(0.0, 1.0) * (h - 1) as f32,
        ];

        struct CloneOp {
            idx: usize,
            alpha: f32,
            src: ([f32; 4], f32, f32, [f32; 3], f32),
        }
        let mut ops: Vec<CloneOp> = Vec::new();
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = (px - px_center) as f32;
                let dy = (py - py_center) as f32;
                let shape = sample_brush_alpha(dx, dy, rad_f, hardness, 0.0, brush);
                let alpha = (shape * brush.opacity * brush.flow).clamp(0.0, 1.0);
                if alpha < 0.001 {
                    continue;
                }
                if let Some(sidx) = Self::clone_source_px(dx, dy, src_center, w, h, brush) {
                    let src = self.sample_composite_src(sidx);
                    let idx = (py * w + px) as usize;
                    if idx < self.layers[li].color_buffer.len() {
                        ops.push(CloneOp { idx, alpha, src });
                    }
                }
            }
        }
        if ops.is_empty() {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(&self.layers[li], [min_x, min_y, max_x, max_y]);
        }

        let layer = &mut self.layers[li];
        for op in &ops {
            Self::apply_clone_px(layer, op.idx, op.alpha, op.src, brush, on_mask);
        }
        [min_x, min_y, max_x, max_y]
    }

    /// 3D clone stamp: rasterizes over the touched mesh surface and clones from the merged
    /// composite with the same pattern transform as `clone_stamp_at`.
    pub fn clone_stamp_3d_at(
        &mut self,
        mesh: &crate::graphics::mesh::Mesh,
        hit_pos: [f32; 3],
        hit_normal: [f32; 3],
        hit_uv: [f32; 2],
        radius_3d: f32,
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if self.layers.is_empty() {
            return [0, 0, 0, 0];
        }
        let li = self.active_layer_index.min(self.layers.len() - 1);
        if !on_mask
            && (self.layers[li].layer_type == LayerType::Folder
                || self.layers[li].layer_type == LayerType::Adjustment
                || self.layers[li].layer_type == LayerType::Generator)
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;
        let num_tris = mesh.indices.len() / 3;
        let r2 = radius_3d * radius_3d;
        let rad_f = radius_3d.max(1e-6);
        let hardness = brush.hardness.clamp(0.0, 0.999);
        let (tan_v, bin_v) = tangent_basis(hit_normal);
        let src_center = [
            hit_uv[0].clamp(0.0, 1.0) * (w - 1) as f32,
            hit_uv[1].clamp(0.0, 1.0) * (h - 1) as f32,
        ];

        struct CloneOp {
            idx: usize,
            alpha: f32,
            src: ([f32; 4], f32, f32, [f32; 3], f32),
        }
        let mut ops: Vec<CloneOp> = Vec::new();
        let mut bbox = [w, h, 0, 0];

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                continue;
            }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
                continue;
            }
            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];
            if hit_pos[0] + radius_3d < v0[0].min(v1[0]).min(v2[0])
                || hit_pos[0] - radius_3d > v0[0].max(v1[0]).max(v2[0])
                || hit_pos[1] + radius_3d < v0[1].min(v1[1]).min(v2[1])
                || hit_pos[1] - radius_3d > v0[1].max(v1[1]).max(v2[1])
                || hit_pos[2] + radius_3d < v0[2].min(v1[2]).min(v2[2])
                || hit_pos[2] - radius_3d > v0[2].max(v1[2]).max(v2[2])
            {
                continue;
            }
            let n0 = mesh.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n1 = mesh.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n2 = mesh.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
            let avg_n = [
                (n0[0] + n1[0] + n2[0]) * 0.3333,
                (n0[1] + n1[1] + n2[1]) * 0.3333,
                (n0[2] + n1[2] + n2[2]) * 0.3333,
            ];
            if avg_n[0] * hit_normal[0] + avg_n[1] * hit_normal[1] + avg_n[2] * hit_normal[2] < 0.05
            {
                continue;
            }

            let uv0 = mesh.uvs[i0];
            let uv1 = mesh.uvs[i1];
            let uv2 = mesh.uvs[i2];
            let px_min =
                ((uv0[0].min(uv1[0]).min(uv2[0]) * (w_f - 1.0)).floor() as i32).clamp(0, w - 1);
            let px_max =
                ((uv0[0].max(uv1[0]).max(uv2[0]) * (w_f - 1.0)).ceil() as i32).clamp(0, w - 1);
            let py_min =
                ((uv0[1].min(uv1[1]).min(uv2[1]) * (h_f - 1.0)).floor() as i32).clamp(0, h - 1);
            let py_max =
                ((uv0[1].max(uv1[1]).max(uv2[1]) * (h_f - 1.0)).ceil() as i32).clamp(0, h - 1);
            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0])
                + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
                .max(1e-7);

            for py in py_min..=py_max {
                let v_px = py as f32 / (h_f - 1.0);
                for px in px_min..=px_max {
                    let u_px = px as f32 / (w_f - 1.0);
                    let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                        + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                        / denom;
                    let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                        + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                        / denom;
                    let b2 = 1.0 - b0 - b1;
                    if b0 < -0.05 || b1 < -0.05 || b2 < -0.05 {
                        continue;
                    }
                    let p3d = [
                        b0 * v0[0] + b1 * v1[0] + b2 * v2[0],
                        b0 * v0[1] + b1 * v1[1] + b2 * v2[1],
                        b0 * v0[2] + b1 * v1[2] + b2 * v2[2],
                    ];
                    let dp = [
                        p3d[0] - hit_pos[0],
                        p3d[1] - hit_pos[1],
                        p3d[2] - hit_pos[2],
                    ];
                    if dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2] > r2 {
                        continue;
                    }
                    let lt =
                        ((dp[0] * tan_v[0] + dp[1] * tan_v[1] + dp[2] * tan_v[2]) / rad_f) * 64.0;
                    let lb =
                        ((dp[0] * bin_v[0] + dp[1] * bin_v[1] + dp[2] * bin_v[2]) / rad_f) * 64.0;
                    let shape =
                        sample_brush_alpha(lt, lb, 64.0, hardness, brush.angle.to_radians(), brush);
                    let alpha = (shape * brush.opacity * brush.flow).clamp(0.0, 1.0);
                    if alpha < 0.001 {
                        continue;
                    }
                    let idx = (py * w + px) as usize;
                    if idx < self.layers[li].color_buffer.len() {
                        // Pattern transform in texture space, anchored at the hit texel
                        let dx = (u_px - hit_uv[0]) * (w - 1) as f32;
                        let dy = (v_px - hit_uv[1]) * (h - 1) as f32;
                        if let Some(sidx) = Self::clone_source_px(dx, dy, src_center, w, h, brush) {
                            let src = self.sample_composite_src(sidx);
                            ops.push(CloneOp { idx, alpha, src });
                            bbox[0] = bbox[0].min(px);
                            bbox[1] = bbox[1].min(py);
                            bbox[2] = bbox[2].max(px);
                            bbox[3] = bbox[3].max(py);
                        }
                    }
                }
            }
        }
        if ops.is_empty() || bbox[2] < bbox[0] {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(&self.layers[li], bbox);
        }

        let layer = &mut self.layers[li];
        for op in &ops {
            Self::apply_clone_px(layer, op.idx, op.alpha, op.src, brush, on_mask);
        }
        bbox
    }

    // ───────────────────── Projection tool (screen-space projection stamping) ─────────────────────

    /// Screen-space projection painting: `screen_alpha` maps a 3D surface point to stamp alpha
    /// (screen-space brush shape × depth window), like reference's projection paint with a
    /// constrained `udepthscale` window.
    pub fn paint_3d_projection_at(
        &mut self,
        mesh: &crate::graphics::mesh::Mesh,
        screen_alpha: &dyn Fn([f32; 3]) -> f32,
        brush: &Brush,
        is_eraser: bool,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        if self.layers.is_empty() {
            return [0, 0, 0, 0];
        }
        let li = self.active_layer_index.min(self.layers.len() - 1);
        if !on_mask
            && (self.layers[li].layer_type == LayerType::Folder
                || self.layers[li].layer_type == LayerType::Adjustment
                || self.layers[li].layer_type == LayerType::Generator)
        {
            return [0, 0, 0, 0];
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;
        let num_tris = mesh.indices.len() / 3;

        struct ProjOp {
            idx: usize,
            alpha: f32,
        }
        let mut ops: Vec<ProjOp> = Vec::new();
        let mut bbox = [w, h, 0, 0];

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                continue;
            }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
                continue;
            }
            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];
            let uv0 = mesh.uvs[i0];
            let uv1 = mesh.uvs[i1];
            let uv2 = mesh.uvs[i2];

            // Cheap screen-space culling: skip triangles that project fully outside the stamp
            let a0 = screen_alpha(v0);
            let a1 = screen_alpha(v1);
            let a2 = screen_alpha(v2);
            if a0 <= 0.0 && a1 <= 0.0 && a2 <= 0.0 {
                continue;
            }

            let px_min =
                ((uv0[0].min(uv1[0]).min(uv2[0]) * (w_f - 1.0)).floor() as i32).clamp(0, w - 1);
            let px_max =
                ((uv0[0].max(uv1[0]).max(uv2[0]) * (w_f - 1.0)).ceil() as i32).clamp(0, w - 1);
            let py_min =
                ((uv0[1].min(uv1[1]).min(uv2[1]) * (h_f - 1.0)).floor() as i32).clamp(0, h - 1);
            let py_max =
                ((uv0[1].max(uv1[1]).max(uv2[1]) * (h_f - 1.0)).ceil() as i32).clamp(0, h - 1);
            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0])
                + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
                .max(1e-7);

            for py in py_min..=py_max {
                let v_px = py as f32 / (h_f - 1.0);
                for px in px_min..=px_max {
                    let u_px = px as f32 / (w_f - 1.0);
                    let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                        + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                        / denom;
                    let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                        + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                        / denom;
                    let b2 = 1.0 - b0 - b1;
                    if b0 < -0.05 || b1 < -0.05 || b2 < -0.05 {
                        continue;
                    }
                    let p3d = [
                        b0 * v0[0] + b1 * v1[0] + b2 * v2[0],
                        b0 * v0[1] + b1 * v1[1] + b2 * v2[1],
                        b0 * v0[2] + b1 * v1[2] + b2 * v2[2],
                    ];
                    let shape = screen_alpha(p3d);
                    let alpha = (shape * brush.opacity * brush.flow).clamp(0.0, 1.0);
                    if alpha < 0.001 {
                        continue;
                    }
                    let idx = (py * w + px) as usize;
                    if idx < self.layers[li].color_buffer.len() {
                        ops.push(ProjOp { idx, alpha });
                        bbox[0] = bbox[0].min(px);
                        bbox[1] = bbox[1].min(py);
                        bbox[2] = bbox[2].max(px);
                        bbox[3] = bbox[3].max(py);
                    }
                }
            }
        }
        if ops.is_empty() || bbox[2] < bbox[0] {
            return [0, 0, 0, 0];
        }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(&self.layers[li], bbox);
        }

        let layer = &mut self.layers[li];
        let bbox_guard = bbox;
        for op in &ops {
            Self::apply_paint_px(layer, op.idx, op.alpha, brush, is_eraser, on_mask);
        }
        let _ = bbox_guard;
        bbox
    }

    /// Returns the next available unique layer ID (strictly greater than any existing layer ID).
    pub fn next_id(&self) -> usize {
        self.layers.iter().map(|l| l.id).max().unwrap_or(0) + 1
    }

    /// Inserts a complete Smart Material folder hierarchy non-destructively above the active layer.
    pub fn insert_smart_material(
        &mut self,
        preset: super::material_gen::SmartMaterialPreset,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
    ) -> usize {
        let start_id = self.next_id();
        let hierarchy = super::material_gen::create_smart_material_hierarchy(
            preset,
            start_id,
            self.width,
            self.height,
            baked_curvature,
            baked_ao,
            baked_wsn,
            baked_pos,
            baked_thickness,
            baked_id,
        );

        let insert_idx = if self.layers.is_empty() {
            0
        } else {
            (self.active_layer_index + 1).min(self.layers.len())
        };
        let folder_idx = insert_idx + hierarchy.len() - 1;

        for (i, layer) in hierarchy.into_iter().enumerate() {
            self.layers.insert(insert_idx + i, layer);
        }
        self.active_layer_index = folder_idx;
        folder_idx
    }

    pub fn new(width: u32, height: u32) -> Self {
        let total = (width * height) as usize;
        let mut stack = Self {
            layers: Vec::new(),
            active_layer_index: 0,
            width,
            height,
            composite_color: vec![[0.22, 0.23, 0.26, 1.0]; total],
            composite_metallic: vec![0.75; total],
            composite_roughness: vec![0.32; total],
            composite_normal: vec![[0.5, 0.5, 1.0]; total],
            composite_height: vec![0.0; total],
            composite_lit: vec![[0.4, 0.4, 0.45, 1.0]; total],
            height_to_normal_scale: 2.0,
        };

        // Initialize with authentic procedural PBR base layer
        let base_metal = super::material_gen::generate_procedural_material(
            1,
            "Base Metal (Steel)",
            [0.65, 0.68, 0.72, 1.0],
            0.92,
            0.28,
            width,
            height,
        );
        stack.layers.push(base_metal);
        stack
            .layers
            .push(Layer::new_paint(2, "Paint Layer 1", width, height));
        stack.active_layer_index = 1;
        stack.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
        stack
    }

    pub fn resize(&mut self, new_w: u32, new_h: u32) {
        self.width = new_w;
        self.height = new_h;
        let total = (new_w * new_h) as usize;
        self.composite_color.resize(total, [0.22, 0.23, 0.26, 1.0]);
        self.composite_metallic.resize(total, 0.75);
        self.composite_roughness.resize(total, 0.32);
        self.composite_normal.resize(total, [0.5, 0.5, 1.0]);
        self.composite_height.resize(total, 0.0);
        self.composite_lit.resize(total, [0.4, 0.4, 0.45, 1.0]);

        for layer in &mut self.layers {
            layer.width = new_w;
            layer.height = new_h;
            layer.color_buffer.resize(total, layer.fill_color);
            layer.metallic_buffer.resize(total, layer.fill_metallic);
            layer.roughness_buffer.resize(total, layer.fill_roughness);
            layer.normal_buffer.resize(total, [0.5, 0.5, 1.0]);
            layer.height_buffer.resize(total, 0.0);
            if let Some(ref mut mask) = layer.mask {
                mask.resize(new_w, new_h);
            }
        }
        self.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
    }

    /// Recomputes the composite buffers using default environment
    pub fn recompute_composite(&mut self) {
        self.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
    }

    /// Evaluates an adjustment layer non-destructively onto accumulated composite channels.
    pub fn apply_adjustment_layer(&mut self, layer_idx: usize, folder_opacity: f32) {
        let LayerStack {
            ref layers,
            ref mut composite_color,
            ref mut composite_metallic,
            ref mut composite_roughness,
            ref mut composite_height,
            width,
            height,
            ..
        } = *self;

        let layer = &layers[layer_idx];
        if !layer.visible || layer.opacity <= 0.001 || folder_opacity <= 0.001 {
            return;
        }

        let effects = &layer.effects;
        if effects.is_empty() {
            return;
        }

        let layer_opacity = layer.opacity * folder_opacity;
        let mask_slice = layer.mask.as_ref().map(|m| m.buffer.as_slice());

        let col_enabled = layer.channel_color_enabled;
        let col_blend = layer.channel_color_blend;
        let col_op = layer.channel_color_opacity;

        let met_enabled = layer.channel_metallic_enabled;
        let met_blend = layer.channel_metallic_blend;
        let met_op = layer.channel_metallic_opacity;

        let rough_enabled = layer.channel_roughness_enabled;
        let rough_blend = layer.channel_roughness_blend;
        let rough_op = layer.channel_roughness_opacity;

        let height_enabled = layer.channel_height_enabled;
        let height_blend = layer.channel_height_blend;
        let height_op = layer.channel_height_opacity;

        let total = (width * height) as usize;
        let w = width as usize;
        let h = height as usize;
        let has_spatial = effects.iter().any(|e| e.is_spatial());

        if has_spatial {
            // Spatial pipeline: execute 2D convolution filters across target channel working copies
            let mut work_color = composite_color.clone();
            let mut work_met = composite_metallic.clone();
            let mut work_rough = composite_roughness.clone();
            let mut work_height = composite_height.clone();

            for effect in effects {
                match effect {
                    FilterEffect::Blur { radius, channel } => {
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::BaseColor
                        {
                            work_color = gaussian_blur_rgba(&work_color, w, h, *radius);
                        }
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::Metallic
                        {
                            work_met = gaussian_blur_f32(&work_met, w, h, *radius);
                        }
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::Roughness
                        {
                            work_rough = gaussian_blur_f32(&work_rough, w, h, *radius);
                        }
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::Height
                        {
                            work_height = gaussian_blur_f32(&work_height, w, h, *radius);
                        }
                    }
                    FilterEffect::Sharpen { strength, channel } => {
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::BaseColor
                        {
                            work_color = sharpen_rgba(&work_color, w, h, *strength);
                        }
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::Metallic
                        {
                            work_met = sharpen_f32(&work_met, w, h, *strength);
                        }
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::Roughness
                        {
                            work_rough = sharpen_f32(&work_rough, w, h, *strength);
                        }
                        if *channel == FilterTargetChannel::All
                            || *channel == FilterTargetChannel::Height
                        {
                            work_height = sharpen_f32(&work_height, w, h, *strength);
                        }
                    }
                    point_eff => {
                        for i in 0..total {
                            apply_point_effect_pixel(
                                point_eff,
                                &mut work_color[i],
                                &mut work_met[i],
                                &mut work_rough[i],
                                &mut work_height[i],
                            );
                        }
                    }
                }
            }

            for i in 0..total {
                let mask_val = if let Some(m) = mask_slice {
                    m.get(i).copied().unwrap_or(1.0)
                } else {
                    1.0
                };
                let eff_a = (layer_opacity * mask_val).clamp(0.0, 1.0);
                if eff_a <= 0.001 {
                    continue;
                }

                if col_enabled {
                    let a = (eff_a * col_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let b = [
                            composite_color[i][0],
                            composite_color[i][1],
                            composite_color[i][2],
                        ];
                        let t = [work_color[i][0], work_color[i][1], work_color[i][2]];
                        let blended = blend_colors(b, t, col_blend);
                        composite_color[i] = [
                            b[0] * (1.0 - a) + blended[0] * a,
                            b[1] * (1.0 - a) + blended[1] * a,
                            b[2] * (1.0 - a) + blended[2] * a,
                            1.0,
                        ];
                    }
                }

                if met_enabled {
                    let a = (eff_a * met_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let blended =
                            blend_scalar_channel(composite_metallic[i], work_met[i], met_blend);
                        composite_metallic[i] = composite_metallic[i] * (1.0 - a) + blended * a;
                    }
                }

                if rough_enabled {
                    let a = (eff_a * rough_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let blended = blend_scalar_channel(
                            composite_roughness[i],
                            work_rough[i],
                            rough_blend,
                        );
                        composite_roughness[i] = composite_roughness[i] * (1.0 - a) + blended * a;
                    }
                }

                if height_enabled {
                    let a = (eff_a * height_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let blended =
                            blend_scalar_channel(composite_height[i], work_height[i], height_blend);
                        composite_height[i] = composite_height[i] * (1.0 - a) + blended * a;
                    }
                }
            }
        } else {
            // Pure point filter pipeline: zero buffer re-allocations
            for i in 0..total {
                let mask_val = if let Some(m) = mask_slice {
                    m.get(i).copied().unwrap_or(1.0)
                } else {
                    1.0
                };
                let eff_a = (layer_opacity * mask_val).clamp(0.0, 1.0);
                if eff_a <= 0.001 {
                    continue;
                }

                let orig_c = composite_color[i];
                let orig_met = composite_metallic[i];
                let orig_rough = composite_roughness[i];
                let orig_height = composite_height[i];

                let mut filt_c = orig_c;
                let mut filt_met = orig_met;
                let mut filt_rough = orig_rough;
                let mut filt_height = orig_height;

                for eff in effects {
                    apply_point_effect_pixel(
                        eff,
                        &mut filt_c,
                        &mut filt_met,
                        &mut filt_rough,
                        &mut filt_height,
                    );
                }

                if col_enabled {
                    let a = (eff_a * col_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let b = [orig_c[0], orig_c[1], orig_c[2]];
                        let t = [filt_c[0], filt_c[1], filt_c[2]];
                        let blended = blend_colors(b, t, col_blend);
                        composite_color[i] = [
                            b[0] * (1.0 - a) + blended[0] * a,
                            b[1] * (1.0 - a) + blended[1] * a,
                            b[2] * (1.0 - a) + blended[2] * a,
                            1.0,
                        ];
                    }
                }

                if met_enabled {
                    let a = (eff_a * met_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let blended = blend_scalar_channel(orig_met, filt_met, met_blend);
                        composite_metallic[i] = orig_met * (1.0 - a) + blended * a;
                    }
                }

                if rough_enabled {
                    let a = (eff_a * rough_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let blended = blend_scalar_channel(orig_rough, filt_rough, rough_blend);
                        composite_roughness[i] = orig_rough * (1.0 - a) + blended * a;
                    }
                }

                if height_enabled {
                    let a = (eff_a * height_op).clamp(0.0, 1.0);
                    if a > 0.001 {
                        let blended = blend_scalar_channel(orig_height, filt_height, height_blend);
                        composite_height[i] = orig_height * (1.0 - a) + blended * a;
                    }
                }
            }
        }
    }

    /// Full PBR Stack Composite with Blend Modes, Channel Masking, and Rayon Multi-threaded Global Illumination
    pub fn composite(
        &mut self,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
        env_preset: EnvironmentPreset,
        env_rotation_rad: f32,
        light_intensity: f32,
    ) {
        let total = (self.width * self.height) as usize;
        self.composite_color.fill([0.15, 0.15, 0.16, 1.0]);
        self.composite_metallic.fill(0.0);
        self.composite_roughness.fill(0.5);
        self.composite_normal.fill([0.5, 0.5, 1.0]);
        if self.composite_height.len() != total {
            self.composite_height.resize(total, 0.0);
        }
        self.composite_height.fill(0.0);

        let layers_meta: Vec<(usize, bool, f32, Option<usize>)> = self
            .layers
            .iter()
            .map(|l| (l.id, l.visible, l.opacity, l.parent_folder_id))
            .collect();

        // Harvest available anchor points across layers
        let mut anchor_maps: std::collections::HashMap<String, Vec<f32>> =
            std::collections::HashMap::new();
        for l in &self.layers {
            if let Some(ref ap) = l.anchor_point {
                let buf: Vec<f32> = match ap.channel {
                    crate::painting::anchor::AnchorChannel::MaskOnly => {
                        if let Some(ref m) = l.mask {
                            m.buffer.clone()
                        } else {
                            vec![1.0; total]
                        }
                    }
                    crate::painting::anchor::AnchorChannel::Height => l.height_buffer.clone(),
                    crate::painting::anchor::AnchorChannel::Roughness => l.roughness_buffer.clone(),
                    crate::painting::anchor::AnchorChannel::Metallic => l.metallic_buffer.clone(),
                    _ => l
                        .color_buffer
                        .iter()
                        .map(|c| (c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114) * c[3])
                        .collect(),
                };
                anchor_maps.insert(ap.name.clone(), buf);
            }
        }

        for layer_idx in 0..self.layers.len() {
            let (is_folder, is_visible, parent_folder_id) = {
                let layer = &self.layers[layer_idx];
                (
                    layer.layer_type == LayerType::Folder,
                    layer.visible,
                    layer.parent_folder_id,
                )
            };
            if is_folder || !is_visible {
                continue;
            }

            // Check parent folder visibility and opacity hierarchy
            let mut folder_visible = true;
            let mut folder_opacity = 1.0f32;
            let mut curr_parent = parent_folder_id;
            while let Some(parent_id) = curr_parent {
                if let Some(parent) = layers_meta.iter().find(|m| m.0 == parent_id) {
                    if !parent.1 {
                        folder_visible = false;
                        break;
                    }
                    folder_opacity *= parent.2;
                    curr_parent = parent.3;
                } else {
                    break;
                }
            }
            if !folder_visible {
                continue;
            }

            if let Some(ref mut mask) = self.layers[layer_idx].mask {
                if !mask.effects.is_empty() {
                    mask.recompute_with_anchors(
                        baked_curvature,
                        baked_ao,
                        baked_wsn,
                        baked_pos,
                        baked_thickness,
                        baked_id,
                        Some(&anchor_maps),
                    );
                }
            }

            if self.layers[layer_idx].layer_type == LayerType::Adjustment {
                self.apply_adjustment_layer(layer_idx, folder_opacity);
                continue;
            }

            if self.layers[layer_idx].layer_type == LayerType::Generator {
                self.layers[layer_idx].evaluate_generator(
                    baked_curvature,
                    baked_ao,
                    baked_wsn,
                    baked_pos,
                    baked_thickness,
                    baked_id,
                    Some(&anchor_maps),
                );
            }

            let layer = &self.layers[layer_idx];
            let layer_op = layer.opacity * folder_opacity;

            for i in 0..total {
                let mask_val = if let Some(ref mask) = layer.mask {
                    mask.buffer.get(i).copied().unwrap_or(1.0)
                } else {
                    1.0
                };
                let base_alpha = (layer.color_buffer[i][3] * layer_op * mask_val).clamp(0.0, 1.0);

                if base_alpha > 0.001 {
                    // 1. Color channel
                    if layer.channel_color_enabled {
                        let eff_a = (base_alpha * layer.channel_color_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let base_c = [
                                self.composite_color[i][0],
                                self.composite_color[i][1],
                                self.composite_color[i][2],
                            ];
                            let top_c = [
                                layer.color_buffer[i][0],
                                layer.color_buffer[i][1],
                                layer.color_buffer[i][2],
                            ];
                            let blended = blend_colors(base_c, top_c, layer.channel_color_blend);
                            self.composite_color[i] = [
                                base_c[0] * (1.0 - eff_a) + blended[0] * eff_a,
                                base_c[1] * (1.0 - eff_a) + blended[1] * eff_a,
                                base_c[2] * (1.0 - eff_a) + blended[2] * eff_a,
                                1.0,
                            ];
                        }
                    }

                    // 2. Metallic channel
                    if layer.channel_metallic_enabled {
                        let eff_a = (base_alpha * layer.channel_metallic_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let blended = blend_scalar_channel(
                                self.composite_metallic[i],
                                layer.metallic_buffer[i],
                                layer.channel_metallic_blend,
                            );
                            self.composite_metallic[i] =
                                self.composite_metallic[i] * (1.0 - eff_a) + blended * eff_a;
                        }
                    }

                    // 3. Roughness channel
                    if layer.channel_roughness_enabled {
                        let eff_a = (base_alpha * layer.channel_roughness_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let blended = blend_scalar_channel(
                                self.composite_roughness[i],
                                layer.roughness_buffer[i],
                                layer.channel_roughness_blend,
                            );
                            self.composite_roughness[i] =
                                self.composite_roughness[i] * (1.0 - eff_a) + blended * eff_a;
                        }
                    }

                    // 4. Height channel
                    if layer.channel_height_enabled {
                        let eff_a = (base_alpha * layer.channel_height_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let blended = blend_scalar_channel(
                                self.composite_height[i],
                                layer.height_buffer[i],
                                layer.channel_height_blend,
                            );
                            self.composite_height[i] =
                                self.composite_height[i] * (1.0 - eff_a) + blended * eff_a;
                        }
                    }

                    // 5. Normal channel
                    if layer.channel_normal_enabled && layer.normal_buffer[i] != [0.5, 0.5, 1.0] {
                        let eff_a = (base_alpha * layer.channel_normal_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let top_n = layer.normal_buffer[i];
                            let mode = layer.channel_normal_blend;
                            let base_n = self.composite_normal[i];
                            let blended_n = if matches!(mode, BlendMode::RNM | BlendMode::Whiteout)
                            {
                                blend_normal_channels(base_n, top_n, mode)
                            } else {
                                top_n
                            };
                            self.composite_normal[i] = [
                                base_n[0] * (1.0 - eff_a) + blended_n[0] * eff_a,
                                base_n[1] * (1.0 - eff_a) + blended_n[1] * eff_a,
                                base_n[2] * (1.0 - eff_a) + blended_n[2] * eff_a,
                            ];
                        }
                    }
                }
            }
        }

        // Dynamic Height-to-Normal Relief Pass (Substance Painter Parity)
        // Evaluates 3x3 Sobel height gradient and blends onto composite_normal via RNM
        if self.height_to_normal_scale > 0.0001 && !self.composite_height.is_empty() {
            let w = self.width as i32;
            let h = self.height as i32;
            let scale = self.height_to_normal_scale;
            let heights = &self.composite_height;

            self.composite_normal
                .par_iter_mut()
                .enumerate()
                .for_each(|(i, norm)| {
                    let x = (i % w as usize) as i32;
                    let y = (i / w as usize) as i32;
                    let h_norm = normal_from_height_at(heights, x, y, w, h, scale);
                    if h_norm != [0.5, 0.5, 1.0] {
                        *norm = blend_normal_channels(*norm, h_norm, BlendMode::RNM);
                    }
                });
        }

        // Rayon Multi-threaded Cook-Torrance GGX PBR Global Illumination pass
        if self.composite_lit.len() != total {
            self.composite_lit.resize(total, [0.0, 0.0, 0.0, 1.0]);
        }

        let col = &self.composite_color;
        let met = &self.composite_metallic;
        let rgh = &self.composite_roughness;
        let nrm = &self.composite_normal;

        self.composite_lit
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, out_pixel)| {
                let base_c = col[i];
                let m = met[i];
                let r = rgh[i];
                let n_ts = nrm[i];

                let nx_ts = n_ts[0] * 2.0 - 1.0;
                let ny_ts = n_ts[1] * 2.0 - 1.0;
                let nz_ts = n_ts[2] * 2.0 - 1.0;

                let (nx, ny, nz) = if let Some(wsn_map) = baked_wsn {
                    let w = wsn_map.get(i).copied().unwrap_or([0.5, 0.5, 1.0]);
                    let wx = w[0] * 2.0 - 1.0;
                    let wy = w[1] * 2.0 - 1.0;
                    let wz = w[2] * 2.0 - 1.0;
                    (
                        wx + nx_ts * 0.35,
                        wy + ny_ts * 0.35,
                        wz + (nz_ts - 1.0) * 0.35,
                    )
                } else {
                    (nx_ts, ny_ts, nz_ts.max(0.1))
                };

                let n_len = (nx * nx + ny * ny + nz * nz).sqrt().max(0.001);
                let n_vec = [nx / n_len, ny / n_len, nz / n_len];

                let ao = baked_ao.and_then(|a| a.get(i)).copied().unwrap_or(1.0);
                let view_dir = [0.35f32, 0.35, 0.87];

                *out_pixel = pbr_shade(
                    base_c,
                    m,
                    r,
                    n_vec,
                    view_dir,
                    ao,
                    env_preset,
                    env_rotation_rad,
                    light_intensity,
                );
            });
    }

    #[inline(always)]
    pub fn composite_channels_at(
        &mut self,
        i: usize,
        layers_meta: &[(usize, bool, f32, Option<usize>)],
    ) {
        let mut base_c = [0.15f32, 0.15, 0.16, 1.0];
        let mut base_met = 0.0f32;
        let mut base_rough = 0.5f32;
        let mut base_norm = [0.5f32, 0.5, 1.0];
        let mut base_height = 0.0f32;

        for layer in &self.layers {
            if layer.layer_type == LayerType::Folder || !layer.visible {
                continue;
            }

            let mut folder_visible = true;
            let mut folder_opacity = 1.0f32;
            let mut curr_parent = layer.parent_folder_id;
            while let Some(parent_id) = curr_parent {
                if let Some(parent) = layers_meta.iter().find(|m| m.0 == parent_id) {
                    if !parent.1 {
                        folder_visible = false;
                        break;
                    }
                    folder_opacity *= parent.2;
                    curr_parent = parent.3;
                } else {
                    break;
                }
            }
            if !folder_visible {
                continue;
            }

            if layer.layer_type == LayerType::Adjustment {
                let mask_val = if let Some(ref mask) = layer.mask {
                    mask.buffer.get(i).copied().unwrap_or(1.0)
                } else {
                    1.0
                };
                let eff_a = (layer.opacity * folder_opacity * mask_val).clamp(0.0, 1.0);
                if eff_a > 0.001 {
                    let orig_c = base_c;
                    let orig_met = base_met;
                    let orig_rough = base_rough;
                    let orig_height = base_height;

                    let mut filt_c = orig_c;
                    let mut filt_met = orig_met;
                    let mut filt_rough = orig_rough;
                    let mut filt_height = orig_height;

                    for eff in &layer.effects {
                        apply_point_effect_pixel(
                            eff,
                            &mut filt_c,
                            &mut filt_met,
                            &mut filt_rough,
                            &mut filt_height,
                        );
                    }

                    if layer.channel_color_enabled {
                        let a = (eff_a * layer.channel_color_opacity).clamp(0.0, 1.0);
                        if a > 0.001 {
                            let b = [orig_c[0], orig_c[1], orig_c[2]];
                            let t = [filt_c[0], filt_c[1], filt_c[2]];
                            let blended = blend_colors(b, t, layer.channel_color_blend);
                            base_c = [
                                b[0] * (1.0 - a) + blended[0] * a,
                                b[1] * (1.0 - a) + blended[1] * a,
                                b[2] * (1.0 - a) + blended[2] * a,
                                1.0,
                            ];
                        }
                    }

                    if layer.channel_metallic_enabled {
                        let a = (eff_a * layer.channel_metallic_opacity).clamp(0.0, 1.0);
                        if a > 0.001 {
                            let blended = blend_scalar_channel(
                                orig_met,
                                filt_met,
                                layer.channel_metallic_blend,
                            );
                            base_met = orig_met * (1.0 - a) + blended * a;
                        }
                    }

                    if layer.channel_roughness_enabled {
                        let a = (eff_a * layer.channel_roughness_opacity).clamp(0.0, 1.0);
                        if a > 0.001 {
                            let blended = blend_scalar_channel(
                                orig_rough,
                                filt_rough,
                                layer.channel_roughness_blend,
                            );
                            base_rough = orig_rough * (1.0 - a) + blended * a;
                        }
                    }

                    if layer.channel_height_enabled {
                        let a = (eff_a * layer.channel_height_opacity).clamp(0.0, 1.0);
                        if a > 0.001 {
                            let blended = blend_scalar_channel(
                                orig_height,
                                filt_height,
                                layer.channel_height_blend,
                            );
                            base_height = orig_height * (1.0 - a) + blended * a;
                        }
                    }
                }
                continue;
            }

            let mask_val = if let Some(ref mask) = layer.mask {
                mask.buffer.get(i).copied().unwrap_or(1.0)
            } else {
                1.0
            };
            let base_alpha = (layer.color_buffer[i][3] * layer.opacity * folder_opacity * mask_val)
                .clamp(0.0, 1.0);
            if base_alpha > 0.001 {
                if layer.channel_color_enabled {
                    let eff_a = (base_alpha * layer.channel_color_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let b_c = [base_c[0], base_c[1], base_c[2]];
                        let top_c = [
                            layer.color_buffer[i][0],
                            layer.color_buffer[i][1],
                            layer.color_buffer[i][2],
                        ];
                        let blended = blend_colors(b_c, top_c, layer.channel_color_blend);
                        base_c = [
                            base_c[0] * (1.0 - eff_a) + blended[0] * eff_a,
                            base_c[1] * (1.0 - eff_a) + blended[1] * eff_a,
                            base_c[2] * (1.0 - eff_a) + blended[2] * eff_a,
                            1.0,
                        ];
                    }
                }

                if layer.channel_metallic_enabled {
                    let eff_a = (base_alpha * layer.channel_metallic_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let blended = blend_scalar_channel(
                            base_met,
                            layer.metallic_buffer[i],
                            layer.channel_metallic_blend,
                        );
                        base_met = base_met * (1.0 - eff_a) + blended * eff_a;
                    }
                }

                if layer.channel_roughness_enabled {
                    let eff_a = (base_alpha * layer.channel_roughness_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let blended = blend_scalar_channel(
                            base_rough,
                            layer.roughness_buffer[i],
                            layer.channel_roughness_blend,
                        );
                        base_rough = base_rough * (1.0 - eff_a) + blended * eff_a;
                    }
                }

                if layer.channel_height_enabled {
                    let eff_a = (base_alpha * layer.channel_height_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let blended = blend_scalar_channel(
                            base_height,
                            layer.height_buffer[i],
                            layer.channel_height_blend,
                        );
                        base_height = base_height * (1.0 - eff_a) + blended * eff_a;
                    }
                }

                if layer.channel_normal_enabled && layer.normal_buffer[i] != [0.5, 0.5, 1.0] {
                    let eff_a = (base_alpha * layer.channel_normal_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let top_n = layer.normal_buffer[i];
                        let mode = layer.channel_normal_blend;
                        let blended_n = if matches!(mode, BlendMode::RNM | BlendMode::Whiteout) {
                            blend_normal_channels(base_norm, top_n, mode)
                        } else {
                            top_n
                        };
                        base_norm = [
                            base_norm[0] * (1.0 - eff_a) + blended_n[0] * eff_a,
                            base_norm[1] * (1.0 - eff_a) + blended_n[1] * eff_a,
                            base_norm[2] * (1.0 - eff_a) + blended_n[2] * eff_a,
                        ];
                    }
                }
            }
        }

        self.composite_color[i] = base_c;
        self.composite_metallic[i] = base_met;
        self.composite_roughness[i] = base_rough;
        if self.composite_height.len() > i {
            self.composite_height[i] = base_height;
        }
        self.composite_normal[i] = base_norm;
    }

    pub fn composite_pixel_lighting_at(
        &mut self,
        i: usize,
        baked_ao: Option<&[f32]>,
        env_preset: EnvironmentPreset,
        env_rotation_rad: f32,
        light_intensity: f32,
    ) {
        let mut base_norm = self.composite_normal[i];

        if self.height_to_normal_scale > 0.0001 && !self.composite_height.is_empty() {
            let w = self.width as i32;
            let h = self.height as i32;
            let x = (i % w as usize) as i32;
            let y = (i / w as usize) as i32;
            let h_norm = normal_from_height_at(
                &self.composite_height,
                x,
                y,
                w,
                h,
                self.height_to_normal_scale,
            );
            if h_norm != [0.5, 0.5, 1.0] {
                base_norm = blend_normal_channels(base_norm, h_norm, BlendMode::RNM);
                self.composite_normal[i] = base_norm;
            }
        }

        let base_c = self.composite_color[i];
        let base_met = self.composite_metallic[i];
        let base_rough = self.composite_roughness[i];

        let nx = base_norm[0] * 2.0 - 1.0;
        let ny = base_norm[1] * 2.0 - 1.0;
        let nz = (base_norm[2] * 2.0 - 1.0).max(0.1);
        let n_len = (nx * nx + ny * ny + nz * nz).sqrt().max(0.001);
        let n_vec = [nx / n_len, ny / n_len, nz / n_len];

        let ao = baked_ao.and_then(|a| a.get(i)).copied().unwrap_or(1.0);
        let view_dir = [0.0f32, 0.0, 1.0];

        self.composite_lit[i] = pbr_shade(
            base_c,
            base_met,
            base_rough,
            n_vec,
            view_dir,
            ao,
            env_preset,
            env_rotation_rad,
            light_intensity,
        );
    }

    pub fn composite_pixel_at(
        &mut self,
        i: usize,
        baked_ao: Option<&[f32]>,
        env_preset: EnvironmentPreset,
        env_rotation_rad: f32,
        light_intensity: f32,
    ) {
        let layers_meta: Vec<(usize, bool, f32, Option<usize>)> = self
            .layers
            .iter()
            .map(|l| (l.id, l.visible, l.opacity, l.parent_folder_id))
            .collect();
        self.composite_channels_at(i, &layers_meta);
        self.composite_pixel_lighting_at(
            i,
            baked_ao,
            env_preset,
            env_rotation_rad,
            light_intensity,
        );
    }

    pub fn composite_dirty(
        &mut self,
        bbox: [i32; 4],
        baked_ao: Option<&[f32]>,
        env_preset: EnvironmentPreset,
        env_rotation_rad: f32,
        light_intensity: f32,
    ) {
        let w = self.width as i32;
        let h = self.height as i32;
        if bbox[2] < bbox[0] || bbox[3] < bbox[1] {
            return;
        }

        let has_spatial_adjustment = self.layers.iter().any(|l| {
            l.visible
                && l.layer_type == LayerType::Adjustment
                && l.effects.iter().any(|e| e.is_spatial())
        });

        if has_spatial_adjustment || (bbox[2] - bbox[0]) > w / 2 || (bbox[3] - bbox[1]) > h / 2 {
            self.composite(
                None,
                baked_ao,
                None,
                None,
                None,
                None,
                env_preset,
                env_rotation_rad,
                light_intensity,
            );
            return;
        }

        let x0 = (bbox[0] - 1).clamp(0, w - 1) as usize;
        let x1 = (bbox[2] + 2).clamp(0, w) as usize;
        let y0 = (bbox[1] - 1).clamp(0, h - 1) as usize;
        let y1 = (bbox[3] + 2).clamp(0, h) as usize;

        let layers_meta: Vec<(usize, bool, f32, Option<usize>)> = self
            .layers
            .iter()
            .map(|l| (l.id, l.visible, l.opacity, l.parent_folder_id))
            .collect();

        // Pass 1: Composite base channels across entire expanded dirty footprint (guarantees valid height field)
        for y in y0..y1 {
            let row_offset = y * self.width as usize;
            for x in x0..x1 {
                self.composite_channels_at(row_offset + x, &layers_meta);
            }
        }

        // Pass 2: Spatial height-to-normal Sobel filter and PBR lighting (all neighbor heights are updated)
        for y in y0..y1 {
            let row_offset = y * self.width as usize;
            for x in x0..x1 {
                self.composite_pixel_lighting_at(
                    row_offset + x,
                    baked_ao,
                    env_preset,
                    env_rotation_rad,
                    light_intensity,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_blend_modes() {
        let base = [0.5, 0.4, 0.3];
        let top = [0.8, 0.2, 0.6];
        let modes = [
            BlendMode::Normal,
            BlendMode::PassThrough,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::LinearBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
            BlendMode::Color,
            BlendMode::Luminosity,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::VividLight,
            BlendMode::LinearLight,
            BlendMode::PinLight,
            BlendMode::HardMix,
            BlendMode::DarkerColor,
            BlendMode::LighterColor,
            BlendMode::Divide,
            BlendMode::RNM,
            BlendMode::Whiteout,
        ];
        for mode in modes {
            let c = blend_colors(base, top, mode);
            assert!(
                c[0] >= 0.0 && c[0] <= 1.0,
                "Mode {:?} out of bounds: {:?}",
                mode,
                c
            );
            assert!(
                c[1] >= 0.0 && c[1] <= 1.0,
                "Mode {:?} out of bounds: {:?}",
                mode,
                c
            );
            assert!(
                c[2] >= 0.0 && c[2] <= 1.0,
                "Mode {:?} out of bounds: {:?}",
                mode,
                c
            );

            let s = blend_scalar_channel(0.5, 0.8, mode);
            assert!(
                (0.0..=1.0).contains(&s),
                "Scalar mode {:?} out of bounds: {}",
                mode,
                s
            );
        }
    }

    #[test]
    fn test_folder_hierarchy_and_channel_overrides() {
        let mut stack = LayerStack::new(4, 4);
        stack.layers.clear();

        // Folder with id 10
        let folder = Layer::new_folder(10, "Base Folder", 4, 4);
        stack.layers.push(folder);

        // Child fill layer with parent_folder_id = 10
        let mut child = Layer::new_fill(11, "Red Child", [1.0, 0.0, 0.0, 1.0], 1.0, 0.1, 4, 4);
        child.parent_folder_id = Some(10);
        child.channel_roughness_enabled = false; // Override: do not contribute roughness
        stack.layers.push(child);

        stack.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
        // Child color should be visible because folder is visible
        assert_eq!(stack.composite_color[0], [1.0, 0.0, 0.0, 1.0]);
        // Metallic should be contributed
        assert_eq!(stack.composite_metallic[0], 1.0);
        // Roughness channel was disabled, so it should stay default (0.5)
        assert_eq!(stack.composite_roughness[0], 0.5);

        // Now hide the folder
        stack.layers[0].visible = false;
        stack.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
        // Since parent folder is hidden, child should not contribute
        assert_eq!(stack.composite_color[0], [0.15, 0.15, 0.16, 1.0]);
        assert_eq!(stack.composite_metallic[0], 0.0);
    }

    // ───────────────── Tool-engine v2 tests (smudge / clone / projection / paint-through) ─────────────────

    fn test_brush() -> Brush {
        let mut b = Brush::default();
        b.size = 8.0;
        b.hardness = 0.99;
        b.opacity = 1.0;
        b.flow = 1.0;
        b.smudge_sample_merged = false;
        b
    }

    fn paint_stack(w: u32, h: u32) -> LayerStack {
        let mut ls = LayerStack::new(w, h);
        ls.layers.clear();
        ls.layers.push(Layer::new_paint(1, "t", w, h));
        ls.active_layer_index = 0;
        ls
    }

    fn test_mesh_flat_quad_z() -> crate::graphics::mesh::Mesh {
        crate::graphics::mesh::Mesh {
            name: "t".into(),
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            tangents: vec![[1.0, 0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            submeshes: vec![],
        }
    }

    #[test]
    fn test_smudge_pulls_content_behind_stroke() {
        let mut ls = paint_stack(32, 32);
        for i in 0..ls.layers[0].color_buffer.len() {
            ls.layers[0].color_buffer[i] = [0.1, 0.1, 0.1, 1.0];
        }
        // Bright texel at (8, 16)
        ls.layers[0].color_buffer[16 * 32 + 8] = [1.0, 0.0, 0.0, 1.0];

        // Stamp at (16, 16) while the stroke moved +8 px along +u
        let brush = test_brush();
        let bbox = ls.smudge_stamp_at([16.0 / 31.0, 16.0 / 31.0], [0.25, 0.0], &brush, None);
        assert!(
            bbox[2] > bbox[0] && bbox[3] > bbox[1],
            "smudge stamp touched pixels: {:?}",
            bbox
        );
        let got = ls.layers[0].color_buffer[16 * 32 + 16];
        assert!(
            got[0] > 0.9 && got[1] < 0.1,
            "smudge pulled red behind the stroke: {:?}",
            got
        );
        // Smudge copies rather than erases: the source texel is only partially drained
        let orig = ls.layers[0].color_buffer[16 * 32 + 8];
        assert_eq!(orig[0], 1.0, "source must keep its color: {:?}", orig);
    }

    #[test]
    fn test_smudge_finger_painting_lays_brush_color() {
        let mut ls = paint_stack(32, 32);
        for i in 0..ls.layers[0].color_buffer.len() {
            ls.layers[0].color_buffer[i] = [0.0, 0.0, 0.0, 0.0];
        }
        let mut brush = test_brush();
        brush.smudge_finger_painting = true;
        brush.color = [0.0, 1.0, 0.0, 1.0];
        let delta = [0.25, 0.0]; // source has nothing painted: finger painting still lays color
        let _ = ls.smudge_stamp_at([16.0 / 31.0, 16.0 / 31.0], delta, &brush, None);
        let got = ls.layers[0].color_buffer[16 * 32 + 16];
        assert!(
            got[1] > 0.9,
            "finger painting deposits brush color: {:?}",
            got
        );
    }

    #[test]
    fn test_smudge_sample_merged_reads_composite() {
        let mut ls = paint_stack(32, 32);
        for i in 0..ls.layers[0].color_buffer.len() {
            ls.layers[0].color_buffer[i] = [0.0, 0.0, 0.0, 0.0]; // active layer empty
        }
        for i in 0..ls.composite_color.len() {
            ls.composite_color[i] = [0.0, 0.0, 1.0, 1.0]; // merged composite is blue
        }
        ls.composite_color[16 * 32 + 8] = [1.0, 0.0, 0.0, 1.0];
        let mut brush = test_brush();
        brush.smudge_sample_merged = true;
        let _ = ls.smudge_stamp_at([16.0 / 31.0, 16.0 / 31.0], [0.25, 0.0], &brush, None);
        let got = ls.layers[0].color_buffer[16 * 32 + 16];
        assert!(
            got[0] > 0.9 && got[2] < 0.1,
            "Sample Merged must pull from the composite: {:?}",
            got
        );
    }

    #[test]
    fn test_clone_copies_pattern_with_full_channels() {
        let mut ls = paint_stack(32, 32);
        for i in 0..ls.layers[0].color_buffer.len() {
            ls.layers[0].color_buffer[i] = [0.0, 0.0, 0.0, 0.0];
        }
        for i in 0..ls.composite_color.len() {
            ls.composite_color[i] = [0.0, 0.0, 0.0, 0.0];
        }
        // A pattern in the composite source area
        for y in 2..7 {
            for x in 2..7 {
                ls.composite_color[y * 32 + x] = [0.9, 0.2, 0.7, 1.0];
            }
        }
        ls.composite_roughness[4 * 32 + 4] = 0.95;

        let mut brush = test_brush();
        brush.clone_rotation = 0.0;
        brush.clone_scale = 1.0;
        let bbox = ls.clone_stamp_at(
            [16.0 / 31.0, 16.0 / 31.0],
            [4.0 / 31.0, 4.0 / 31.0],
            &brush,
            false,
            None,
        );
        assert!(bbox[2] > bbox[0]);
        let inner = ls.layers[0].color_buffer[16 * 32 + 18]; // dst (18,16) inside the cloned pattern
        let outside = ls.layers[0].color_buffer[16 * 32 + 27]; // far from the stamp in-row
        assert!(
            inner[0] > 0.8 && inner[2] > 0.6 && inner[3] > 0.9,
            "clone copied color+alpha, got {:?}",
            inner
        );
        assert_eq!(outside[3], 0.0, "clone must not paint outside the stamp");
        // Full channel fidelity: dst (18,16) ← src (6,4)
        let met_area = ls.layers[0].metallic_buffer[16 * 32 + 18];
        let rough_area = ls.layers[0].roughness_buffer[16 * 32 + 18];
        assert!(
            (met_area - ls.composite_metallic[4 * 32 + 6]).abs() < 0.02,
            "metallic cloned"
        );
        assert!(
            (rough_area - ls.composite_roughness[4 * 32 + 6]).abs() < 0.02,
            "roughness cloned: {} vs {}",
            rough_area,
            ls.composite_roughness[4 * 32 + 6]
        );
    }

    #[test]
    fn test_clone_pattern_mirror_transform() {
        let mut ls = paint_stack(32, 32);
        for i in 0..ls.composite_color.len() {
            ls.composite_color[i] = [0.0, 0.0, 0.0, 0.0];
        }
        // Asymmetric pattern: only src px (3, 4) is inked
        ls.composite_color[4 * 32 + 3] = [1.0, 1.0, 1.0, 1.0];

        let mut brush = test_brush();
        brush.clone_mirror_x = true;
        // Dst center (17,4), src center (4,4): mirrored local +1 → src px 3 (inked) lands at dst 18
        let _ = ls.clone_stamp_at(
            [17.0 / 31.0, 4.0 / 31.0],
            [4.0 / 31.0, 4.0 / 31.0],
            &brush,
            false,
            None,
        );
        let mirror_on = ls.layers[0].color_buffer[4 * 32 + 18];
        assert!(
            mirror_on[3] > 0.9,
            "mirrored clone pulls px (3,4), got {:?}",
            mirror_on
        );

        // Without mirroring, local +1 maps to src 5 which is empty
        let mut ls2 = paint_stack(32, 32);
        ls2.composite_color = ls.composite_color.clone();
        let mut brush2 = test_brush();
        brush2.clone_mirror_x = false;
        let _ = ls2.clone_stamp_at(
            [17.0 / 31.0, 4.0 / 31.0],
            [4.0 / 31.0, 4.0 / 31.0],
            &brush2,
            false,
            None,
        );
        assert!(
            ls2.layers[0].color_buffer[4 * 32 + 18][3] < 0.9,
            "unmirrored clone pulls empty src (5,4)"
        );
    }

    #[test]
    fn test_paint_through_backface_toggle() {
        let mut ls = paint_stack(16, 16);
        let mesh = test_mesh_flat_quad_z();

        let mut brush = test_brush();
        brush.paint_through = false;
        // Hit from the back side: normal flipped
        let b0 = ls.layers[0].paint_3d_surface(
            &mesh,
            [0.5, 0.5, 0.0],
            [0.0, 0.0, -1.0],
            0.35,
            &brush,
            false,
            false,
            None,
        );
        assert!(b0[2] <= b0[0], "backface culled by default");

        brush.paint_through = true;
        let b1 = ls.layers[0].paint_3d_surface(
            &mesh,
            [0.5, 0.5, 0.0],
            [0.0, 0.0, -1.0],
            0.35,
            &brush,
            false,
            false,
            None,
        );
        assert!(
            b1[2] > b1[0] && b1[3] > b1[1],
            "paint-through paints back-facing geometry"
        );
        let painted = ls.layers[0]
            .color_buffer
            .iter()
            .filter(|c| c[3] > 0.5)
            .count();
        assert!(painted > 0, "pixels received paint");
    }

    #[test]
    fn test_projection_closure_alpha_paints() {
        let mut ls = paint_stack(16, 16);
        let mesh = test_mesh_flat_quad_z();
        // Screen-space closure: alpha 1 over the plan corner area (x<0.6, y<0.6)
        let screen_alpha = |p3: [f32; 3]| -> f32 {
            if p3[0] <= 0.6 && p3[1] <= 0.6 {
                1.0
            } else {
                0.0
            }
        };
        let brush = test_brush();
        let _ = ls.paint_3d_projection_at(&mesh, &screen_alpha, &brush, false, false, None);
        let painted: usize = ls.layers[0]
            .color_buffer
            .iter()
            .filter(|c| c[3] > 0.8)
            .count();
        assert!(
            painted > 16,
            "projection painted the screen-space region: {}",
            painted
        );
        // The corner texel inside the closure region is painted
        assert!(ls.layers[0].color_buffer[3 * 16 + 3][3] > 0.8);
    }

    #[test]
    fn test_normal_from_height_flat_is_neutral() {
        let w = 8;
        let h = 8;
        let heights = vec![0.5f32; (w * h) as usize];
        let n = normal_from_height_at(&heights, 4, 4, w, h, 2.0);
        assert!(
            (n[0] - 0.5).abs() < 1e-4,
            "flat nx should be 0.5, got {}",
            n[0]
        );
        assert!(
            (n[1] - 0.5).abs() < 1e-4,
            "flat ny should be 0.5, got {}",
            n[1]
        );
        assert!(
            (n[2] - 1.0).abs() < 1e-4,
            "flat nz should be 1.0, got {}",
            n[2]
        );
    }

    #[test]
    fn test_normal_from_height_horizontal_ramp() {
        let w = 8;
        let h = 8;
        // Height increases with x (slope facing left, normal tilts left: nx < 0.5)
        let mut heights = vec![0.0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                heights[(y * w + x) as usize] = x as f32 * 0.1;
            }
        }
        let n = normal_from_height_at(&heights, 4, 4, w, h, 2.0);
        assert!(
            n[0] < 0.49,
            "nx should deflect negative (< 0.5), got {}",
            n[0]
        );
        assert!(
            (n[1] - 0.5).abs() < 1e-4,
            "ny should remain neutral, got {}",
            n[1]
        );
        assert!(n[2] > 0.5, "nz should point up, got {}", n[2]);
    }

    #[test]
    fn test_normal_from_height_vertical_ramp() {
        let w = 8;
        let h = 8;
        // Height decreases with y (top y=0 is high, bottom y=7 is low -> slopes towards bottom (+V) -> ny > 0.5)
        let mut heights = vec![0.0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                heights[(y * w + x) as usize] = (h - 1 - y) as f32 * 0.1;
            }
        }
        let n = normal_from_height_at(&heights, 4, 4, w, h, 2.0);
        assert!(
            (n[0] - 0.5).abs() < 1e-4,
            "nx should remain neutral, got {}",
            n[0]
        );
        assert!(
            n[1] > 0.51,
            "ny should deflect positive (> 0.5) for OpenGL Y+, got {}",
            n[1]
        );
        assert!(n[2] > 0.5, "nz should point up, got {}", n[2]);

        // Zero / negative dimension guard test (must not panic)
        let safe = normal_from_height_at(&heights, 0, 0, 0, 0, 1.0);
        assert_eq!(safe, [0.5, 0.5, 1.0]);
    }

    #[test]
    fn test_dynamic_height_to_normal_compositing() {
        let mut ls = LayerStack::new(16, 16);
        ls.layers.clear();
        let mut layer = Layer::new_fill(
            1,
            "Height Bump Layer",
            [0.5, 0.5, 0.5, 1.0],
            0.0,
            0.5,
            16,
            16,
        );
        // Paint a sharp height bump in the center
        for y in 6..10 {
            for x in 6..10 {
                layer.height_buffer[y * 16 + x] = 1.0;
            }
        }
        ls.layers.push(layer);
        ls.height_to_normal_scale = 2.0;
        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // At the edge of the bump (x=5, y=7), height steps up from 0 to 1 -> normal should deflect
        let edge_norm = ls.composite_normal[7 * 16 + 5];
        assert!(
            edge_norm != [0.5, 0.5, 1.0],
            "edge of height bump must deflect normal from neutral, got {:?}",
            edge_norm
        );
        assert!(edge_norm[0] < 0.5, "slope facing left deflects nx negative");

        // At flat center (x=8, y=8), heights are equal -> normal remains neutral
        let center_norm = ls.composite_normal[8 * 16 + 8];
        assert!(
            (center_norm[0] - 0.5).abs() < 0.05 && (center_norm[1] - 0.5).abs() < 0.05,
            "flat center must remain near neutral, got {:?}",
            center_norm
        );
    }

    #[test]
    fn test_adjustment_layer_invert_non_destructive() {
        let mut ls = LayerStack::new(4, 4);
        ls.layers.clear();

        let base_col = [0.8, 0.2, 0.1, 1.0];
        let fill = Layer::new_fill(1, "Base Fill", base_col, 0.0, 0.5, 4, 4);
        ls.layers.push(fill);

        // Baseline composite
        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
        for c in &ls.composite_color {
            assert!((c[0] - 0.8).abs() < 0.01);
            assert!((c[1] - 0.2).abs() < 0.01);
            assert!((c[2] - 0.1).abs() < 0.01);
        }

        // Add Adjustment Layer with Invert filter
        let adj = Layer::new_adjustment(
            2,
            "Invert Adjustment",
            FilterEffect::Invert {
                channel: FilterTargetChannel::All,
            },
            4,
            4,
        );
        ls.layers.push(adj);

        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // Verify composite color is now inverted
        for c in &ls.composite_color {
            assert!((c[0] - 0.2).abs() < 0.01, "expected r~0.2, got {}", c[0]);
            assert!((c[1] - 0.8).abs() < 0.01, "expected g~0.8, got {}", c[1]);
            assert!((c[2] - 0.9).abs() < 0.01, "expected b~0.9, got {}", c[2]);
        }

        // Verify raw fill layer strokes/buffers are 100% UNCHANGED (non-destructive)
        for c in &ls.layers[0].color_buffer {
            assert!((c[0] - 0.8).abs() < 1e-5);
            assert!((c[1] - 0.2).abs() < 1e-5);
            assert!((c[2] - 0.1).abs() < 1e-5);
        }

        // Disable adjustment layer visibility -> baseline look is restored immediately
        ls.layers[1].visible = false;
        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
        for c in &ls.composite_color {
            assert!((c[0] - 0.8).abs() < 0.01);
            assert!((c[1] - 0.2).abs() < 0.01);
            assert!((c[2] - 0.1).abs() < 0.01);
        }
    }

    #[test]
    fn test_adjustment_layer_with_mask() {
        let mut ls = LayerStack::new(4, 4);
        ls.layers.clear();

        let base_col = [0.8, 0.2, 0.1, 1.0];
        let fill = Layer::new_fill(1, "Base Fill", base_col, 0.0, 0.5, 4, 4);
        ls.layers.push(fill);

        let mut adj = Layer::new_adjustment(
            2,
            "Invert Masked",
            FilterEffect::Invert {
                channel: FilterTargetChannel::All,
            },
            4,
            4,
        );

        // Mask: left 2 columns (x=0,1) unmasked (1.0), right 2 columns (x=2,3) masked (0.0)
        let mut mask = LayerMask::new(MaskType::Black, 4, 4);
        for y in 0..4 {
            for x in 0..2 {
                mask.buffer[y * 4 + x] = 1.0;
            }
        }
        adj.mask = Some(mask);
        ls.layers.push(adj);

        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // Left side is inverted
        for y in 0..4 {
            for x in 0..2 {
                let c = ls.composite_color[y * 4 + x];
                assert!((c[0] - 0.2).abs() < 0.01, "left side should be inverted");
                assert!((c[1] - 0.8).abs() < 0.01);
            }
        }

        // Right side is untouched
        for y in 0..4 {
            for x in 2..4 {
                let c = ls.composite_color[y * 4 + x];
                assert!(
                    (c[0] - 0.8).abs() < 0.01,
                    "right side should remain original"
                );
                assert!((c[1] - 0.2).abs() < 0.01);
            }
        }
    }

    #[test]
    fn test_adjustment_layer_blur_spatial() {
        let mut ls = LayerStack::new(8, 8);
        ls.layers.clear();

        // Layer with sharp white left half, black right half
        let mut paint = Layer::new_paint(1, "Step", 8, 8);
        for y in 0..8 {
            for x in 0..4 {
                paint.color_buffer[y * 8 + x] = [1.0, 1.0, 1.0, 1.0];
            }
            for x in 4..8 {
                paint.color_buffer[y * 8 + x] = [0.0, 0.0, 0.0, 1.0];
            }
        }
        ls.layers.push(paint);

        let blur_adj = Layer::new_adjustment(
            2,
            "Blur Adjustment",
            FilterEffect::Blur {
                radius: 2.0,
                channel: FilterTargetChannel::All,
            },
            8,
            8,
        );
        ls.layers.push(blur_adj);

        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // The step transition at x=3 and x=4 should now be smoothly blurred
        let c3 = ls.composite_color[4 * 8 + 3][0];
        let c4 = ls.composite_color[4 * 8 + 4][0];
        assert!(
            c3 < 0.99 && c3 > 0.01,
            "pixel at border should be blurred, got {}",
            c3
        );
        assert!(
            c4 < 0.99 && c4 > 0.01,
            "pixel at border should be blurred, got {}",
            c4
        );
    }

    #[test]
    fn test_adjustment_layer_height_blend_normal_not_doubled() {
        let mut ls = LayerStack::new(4, 4);
        ls.layers.clear();

        // Base layer with 0.35 height
        let mut fill = Layer::new_fill(1, "Base Bump", [0.5, 0.5, 0.5, 1.0], 0.0, 0.5, 4, 4);
        fill.fill_height = 0.35;
        fill.update_fill();
        ls.layers.push(fill);

        // Add Invert Adjustment layer targeting BaseColor only
        let adj = Layer::new_adjustment(
            2,
            "Invert Color Only",
            FilterEffect::Invert {
                channel: FilterTargetChannel::BaseColor,
            },
            4,
            4,
        );
        ls.layers.push(adj);

        ls.composite(
            None,
            None,
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // Underlying height must remain exactly 0.35, not doubled to 0.70 via Add blend!
        for h in &ls.composite_height {
            assert!(
                (h - 0.35).abs() < 1e-4,
                "adjustment layer must not double underlying height, expected 0.35 got {}",
                h
            );
        }
    }

    #[test]
    fn test_generator_layer_metal_edge_wear_composite() {
        let mut ls = LayerStack::new(2, 2);
        ls.layers.clear();

        // Base green paint layer (metallic 0.0, roughness 0.8)
        let base = Layer::new_fill(1, "Base Paint", [0.1, 0.6, 0.2, 1.0], 0.0, 0.8, 2, 2);
        ls.layers.push(base);

        // Generator layer: Metal Edge Wear
        let mut gen = Layer::new_generator_edge_wear(2, 2, 2);
        // Disable noise for exact test determinism
        if let Some(ref mut cfg) = gen.generator {
            cfg.grunge_amount = 0.0;
        }
        ls.layers.push(gen);

        // Curvature: pixel 0 convex (0.9), pixel 1 flat (0.5), pixel 2 crevice (0.1), pixel 3 flat (0.5)
        let curv = vec![0.9f32, 0.5, 0.1, 0.5];
        let ao = vec![1.0f32, 1.0, 1.0, 1.0];

        ls.composite(
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // Pixel 0 (convex edge) should have high metallic wear and bright color
        assert!(
            ls.composite_metallic[0] > 0.4,
            "Pixel 0 should be metallic wear, got {}",
            ls.composite_metallic[0]
        );
        assert!(
            ls.composite_roughness[0] < 0.6,
            "Pixel 0 roughness should be polished wear (< 0.6), got {}",
            ls.composite_roughness[0]
        );

        // Pixel 1 (flat) should remain green base paint
        assert!(
            ls.composite_metallic[1] < 0.1,
            "Pixel 1 should have minimal/zero metallic wear, got {}",
            ls.composite_metallic[1]
        );
        assert!(
            (ls.composite_color[1][1] - 0.6).abs() < 0.1,
            "Pixel 1 color should remain predominantly green base"
        );
    }

    #[test]
    fn test_generator_layer_dirt_crevices_composite() {
        let mut ls = LayerStack::new(2, 2);
        ls.layers.clear();

        // Base shiny white ceramic layer
        let base = Layer::new_fill(1, "Ceramic", [0.9, 0.9, 0.9, 1.0], 0.0, 0.1, 2, 2);
        ls.layers.push(base);

        // Generator layer: Dirt Crevices
        let mut gen = Layer::new_generator_dirt_crevices(2, 2, 2);
        if let Some(ref mut cfg) = gen.generator {
            cfg.grunge_amount = 0.0;
        }
        ls.layers.push(gen);

        // Curvature: pixel 2 is crevice (0.1), others flat (0.5)
        let curv = vec![0.5f32, 0.5, 0.1, 0.5];
        // AO: pixel 2 is deeply occluded (0.2), others unoccluded (1.0)
        let ao = vec![1.0f32, 1.0, 0.2, 1.0];

        ls.composite(
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // Pixel 2 (crevice) should accumulate dirt: darker color, rougher, and recessed height
        assert!(
            ls.composite_color[2][0] < 0.6,
            "Pixel 2 crevice should be darkened by dirt, got r={}",
            ls.composite_color[2][0]
        );
        assert!(
            ls.composite_roughness[2] > 0.4,
            "Pixel 2 crevice roughness should increase from dirt, got {}",
            ls.composite_roughness[2]
        );
        assert!(
            ls.composite_height[2] < 0.0,
            "Pixel 2 crevice height should be recessed (< 0), got {}",
            ls.composite_height[2]
        );

        // Pixel 0 (flat unoccluded) should remain bright white shiny ceramic
        assert!(
            ls.composite_color[0][0] > 0.8,
            "Pixel 0 should remain bright ceramic, got {}",
            ls.composite_color[0][0]
        );
        assert!(
            ls.composite_roughness[0] < 0.2,
            "Pixel 0 should remain smooth, got {}",
            ls.composite_roughness[0]
        );
    }

    #[test]
    fn test_generator_layer_paint_stamp_guarded_and_mask_painting() {
        let mut gen = Layer::new_generator_edge_wear(1, 4, 4);
        let brush = Brush::default();

        // 1. Direct paint stamp on generator layer must be rejected
        let bbox = gen.paint_stamp([0.5, 0.5], &brush, false, None);
        assert_eq!(
            bbox,
            [0, 0, 0, 0],
            "paint stamp directly on generator channels must return empty bbox"
        );

        // 2. Add white mask and paint with eraser on mask
        gen.add_white_mask();
        assert!(gen.mask.is_some());
        let mask_bbox = gen.paint_stamp([0.5, 0.5], &brush, true, None);
        assert_ne!(
            mask_bbox,
            [0, 0, 0, 0],
            "paint stamp on generator mask must succeed"
        );
    }

    #[test]
    fn test_smart_material_hierarchy_compositing_edge_wear() {
        use super::super::material_gen::{SmartMaskPreset, SmartMaterialPreset};

        let mut stack = LayerStack::new(2, 2);
        // Clear initial default layers so we only test the smart material
        stack.layers.clear();

        // Pixel 0: Sharp convex edge (curv = 1.0)
        // Pixel 1: Flat surface (curv = 0.5)
        // Pixel 2: Crevice (ao = 0.2, curv = 0.2)
        // Pixel 3: Flat surface (curv = 0.5)
        let curv = vec![1.0, 0.5, 0.2, 0.5];
        let ao = vec![1.0, 1.0, 0.2, 1.0];

        let folder_idx = stack.insert_smart_material(
            SmartMaterialPreset::SteelPaintedWorn,
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
        );

        assert_eq!(stack.active_layer_index, folder_idx);
        let folder = &stack.layers[folder_idx];
        assert_eq!(folder.layer_type, LayerType::Folder);
        let folder_id = folder.id;

        // Verify parenting
        for layer in &stack.layers[..folder_idx] {
            assert_eq!(layer.parent_folder_id, Some(folder_id));
        }

        stack.composite(
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );

        // Pixel 1 (flat): Weathered navy paint coat is intact (metallic = 0.0)
        assert!(
            stack.composite_metallic[1] < 0.2,
            "Pixel 1 (flat) must show non-metallic paint coat, got metallic={}",
            stack.composite_metallic[1]
        );

        // Pixel 0 (sharp edge): Paint is stripped, exposing bare steel substrate (metallic > 0.7)
        assert!(
            stack.composite_metallic[0] > 0.7,
            "Pixel 0 (convex edge) must expose bare metal substrate, got metallic={}",
            stack.composite_metallic[0]
        );

        // Pixel 2 (crevice): Dirt grime darkened
        assert!(
            stack.composite_roughness[2] > 0.6,
            "Pixel 2 (crevice) must be rough from cavity grime, got roughness={}",
            stack.composite_roughness[2]
        );

        // Folder visibility test: turning off folder disables the entire smart material
        stack.layers[folder_idx].visible = false;
        stack.composite(
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            EnvironmentPreset::StudioTomoco,
            0.0,
            1.0,
        );
        // All child layers skipped -> default backdrop
        assert_eq!(stack.composite_metallic[0], 0.0);

        // Layer apply smart mask preset test
        let mut paint_layer = Layer::new_paint(999, "Test Coat", 2, 2);
        paint_layer.apply_smart_mask_preset(
            SmartMaskPreset::MetalEdgeWear,
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
        );
        assert!(paint_layer.mask.is_some());
        let mask = paint_layer.mask.as_ref().unwrap();
        assert_eq!(mask.effects.len(), 1);
        // Convex edge (pixel 0) should have high edge wear value
        assert!(
            mask.buffer[0] > mask.buffer[1],
            "Edge wear mask must be higher at convex edge than flat surface"
        );
    }
}
