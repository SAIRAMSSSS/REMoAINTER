#![allow(dead_code)]
use super::anchor::AnchorPoint;
use super::brush::{sample_brush_alpha, Brush};
use super::filter::{
    apply_levels_scalar, gaussian_blur_f32, gaussian_blur_rgba, hsl_to_rgb, rgb_to_hsl,
    sharpen_f32, sharpen_rgba, FilterEffect, FilterTargetChannel,
};
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
            let ch = |b: f32, t: f32| if b < 0.5 { 2.0 * b * t } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - t) };
            [ch(base[0], top[0]), ch(base[1], top[1]), ch(base[2], top[2])]
        }
        BlendMode::Add => [(base[0] + top[0]).min(1.0), (base[1] + top[1]).min(1.0), (base[2] + top[2]).min(1.0)],
        BlendMode::Subtract => [(base[0] - top[0]).max(0.0), (base[1] - top[1]).max(0.0), (base[2] - top[2]).max(0.0)],
        BlendMode::Darken => [base[0].min(top[0]), base[1].min(top[1]), base[2].min(top[2])],
        BlendMode::Lighten => [base[0].max(top[0]), base[1].max(top[1]), base[2].max(top[2])],
        BlendMode::ColorDodge => {
            let ch = |b: f32, t: f32| if t >= 1.0 { 1.0 } else { (b / (1.0 - t)).min(1.0) };
            [ch(base[0], top[0]), ch(base[1], top[1]), ch(base[2], top[2])]
        }
        BlendMode::ColorBurn => {
            let ch = |b: f32, t: f32| if t <= 0.0 { 0.0 } else { (1.0 - (1.0 - b) / t).max(0.0) };
            [ch(base[0], top[0]), ch(base[1], top[1]), ch(base[2], top[2])]
        }
        BlendMode::LinearBurn => [
            (base[0] + top[0] - 1.0).max(0.0),
            (base[1] + top[1] - 1.0).max(0.0),
            (base[2] + top[2] - 1.0).max(0.0),
        ],
        BlendMode::HardLight => {
            let ch = |b: f32, t: f32| if t < 0.5 { 2.0 * b * t } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - t) };
            [ch(base[0], top[0]), ch(base[1], top[1]), ch(base[2], top[2])]
        }
        BlendMode::SoftLight => {
            let ch = |b: f32, t: f32| {
                if t < 0.5 {
                    b - (1.0 - 2.0 * t) * b * (1.0 - b)
                } else {
                    b + (2.0 * t - 1.0) * (b.sqrt() - b)
                }
            };
            [ch(base[0], top[0]), ch(base[1], top[1]), ch(base[2], top[2])]
        }
        BlendMode::Difference => [(base[0] - top[0]).abs(), (base[1] - top[1]).abs(), (base[2] - top[2]).abs()],
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
    }
}

pub fn blend_scalar_channel(base: f32, top: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal | BlendMode::PassThrough => top,
        BlendMode::Multiply => base * top,
        BlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - top),
        BlendMode::Overlay => if base < 0.5 { 2.0 * base * top } else { 1.0 - 2.0 * (1.0 - base) * (1.0 - top) },
        BlendMode::Add => (base + top).min(1.0),
        BlendMode::Subtract => (base - top).max(0.0),
        BlendMode::Darken => base.min(top),
        BlendMode::Lighten => base.max(top),
        BlendMode::Difference => (base - top).abs(),
        _ => top,
    }
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

    // Per-Layer Channel Overrides & Blends (Substance Painter Parity)
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

    // Fill settings (for Fill layers)
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

    pub fn new_fill(id: usize, name: impl Into<String>, color: [f32; 4], metallic: f32, roughness: f32, width: u32, height: u32) -> Self {
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
                FilterEffect::Levels { min_in, max_in, gamma, min_out, max_out, channel } => {
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::BaseColor {
                        for p in &mut self.color_buffer {
                            p[0] = apply_levels_scalar(p[0], *min_in, *max_in, *gamma, *min_out, *max_out);
                            p[1] = apply_levels_scalar(p[1], *min_in, *max_in, *gamma, *min_out, *max_out);
                            p[2] = apply_levels_scalar(p[2], *min_in, *max_in, *gamma, *min_out, *max_out);
                        }
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Metallic {
                        for v in &mut self.metallic_buffer {
                            *v = apply_levels_scalar(*v, *min_in, *max_in, *gamma, *min_out, *max_out);
                        }
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Roughness {
                        for v in &mut self.roughness_buffer {
                            *v = apply_levels_scalar(*v, *min_in, *max_in, *gamma, *min_out, *max_out);
                        }
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Height {
                        for v in &mut self.height_buffer {
                            *v = apply_levels_scalar(*v, *min_in, *max_in, *gamma, *min_out, *max_out);
                        }
                    }
                }
                FilterEffect::Blur { radius, channel } => {
                    let w = self.width as usize;
                    let h = self.height as usize;
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::BaseColor {
                        self.color_buffer = gaussian_blur_rgba(&self.color_buffer, w, h, *radius);
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Metallic {
                        self.metallic_buffer = gaussian_blur_f32(&self.metallic_buffer, w, h, *radius);
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Roughness {
                        self.roughness_buffer = gaussian_blur_f32(&self.roughness_buffer, w, h, *radius);
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Height {
                        self.height_buffer = gaussian_blur_f32(&self.height_buffer, w, h, *radius);
                    }
                }
                FilterEffect::Sharpen { strength, channel } => {
                    let w = self.width as usize;
                    let h = self.height as usize;
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::BaseColor {
                        self.color_buffer = sharpen_rgba(&self.color_buffer, w, h, *strength);
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Metallic {
                        self.metallic_buffer = sharpen_f32(&self.metallic_buffer, w, h, *strength);
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Roughness {
                        self.roughness_buffer = sharpen_f32(&self.roughness_buffer, w, h, *strength);
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Height {
                        self.height_buffer = sharpen_f32(&self.height_buffer, w, h, *strength);
                    }
                }
                FilterEffect::HSL { hue_shift, sat_mult, light_offset } => {
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
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::BaseColor {
                        for p in &mut self.color_buffer {
                            p[0] = (1.0 - p[0]).clamp(0.0, 1.0);
                            p[1] = (1.0 - p[1]).clamp(0.0, 1.0);
                            p[2] = (1.0 - p[2]).clamp(0.0, 1.0);
                        }
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Metallic {
                        for v in &mut self.metallic_buffer {
                            *v = (1.0 - *v).clamp(0.0, 1.0);
                        }
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Roughness {
                        for v in &mut self.roughness_buffer {
                            *v = (1.0 - *v).clamp(0.0, 1.0);
                        }
                    }
                    if *channel == FilterTargetChannel::All || *channel == FilterTargetChannel::Height {
                        for v in &mut self.height_buffer {
                            *v = -*v;
                        }
                    }
                }
                FilterEffect::GradientMap { color_start, color_end } => {
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
        let mut layer = Self::new_fill(id, name_str, pbr_hint.0, pbr_hint.1, pbr_hint.2, width, height);

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

    /// Applies an authentic Substance Smart Mask (.spmsk / .webp) with baked curvature & AO wear
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

    pub fn new_folder(id: usize, name: impl Into<String>, width: u32, height: u32) -> Self {
        let mut l = Self::new_paint(id, name, width, height);
        l.layer_type = LayerType::Folder;
        l.blend_mode = BlendMode::PassThrough;
        l
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
        }
    }

    pub fn paint_stamp(
        &mut self,
        uv: [f32; 2],
        brush: &Brush,
        on_mask: bool,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
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

                let alpha_base = sample_brush_alpha(dx, dy, rad_f, hardness, brush.angle.to_radians(), brush);
                let alpha = (alpha_base * brush.opacity * brush.flow).clamp(0.0, 1.0);

                if alpha > 0.001 {
                    let idx = (py * w + px) as usize;

                    if on_mask {
                        if let Some(ref mut mask) = self.mask {
                            if idx < mask.buffer.len() {
                                mask.buffer[idx] = (mask.buffer[idx] * (1.0 - alpha) + alpha).clamp(0.0, 1.0);
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
                            self.metallic_buffer[idx] = self.metallic_buffer[idx] * (1.0 - alpha) + brush.metallic * alpha;
                        }
                        if brush.paint_roughness && idx < self.roughness_buffer.len() {
                            self.roughness_buffer[idx] = self.roughness_buffer[idx] * (1.0 - alpha) + brush.roughness * alpha;
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
                            self.height_buffer[idx] = self.height_buffer[idx] * (1.0 - alpha) + brush.height * alpha;
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

        let pixel_dist = (dx * dx * (self.width as f32).powi(2) + dy * dy * (self.height as f32).powi(2)).sqrt();
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

                let alpha_base = sample_brush_alpha(dx, dy, rad_f, hardness, brush.angle.to_radians(), brush);
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

        let pixel_dist = (dx * dx * (self.width as f32).powi(2) + dy * dy * (self.height as f32).powi(2)).sqrt();
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

        let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1])).max(1e-7);

        for py in py_min..=py_max {
            let v_px = py as f32 / (h_f - 1.0);
            let row_offset = py as usize * self.width as usize;
            for px in px_min..=px_max {
                let u_px = px as f32 / (w_f - 1.0);
                let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0]) + (uv2[0] - uv1[0]) * (v_px - uv2[1])) / denom;
                let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0]) + (uv0[0] - uv2[0]) * (v_px - uv2[1])) / denom;
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
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;
        let num_tris = mesh.indices.len() / 3;
        let r2 = radius_3d * radius_3d;
        let hardness = brush.hardness.clamp(0.0, 0.999);

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

            if hit_pos[0] + radius_3d < tri_min_x || hit_pos[0] - radius_3d > tri_max_x ||
               hit_pos[1] + radius_3d < tri_min_y || hit_pos[1] - radius_3d > tri_max_y ||
               hit_pos[2] + radius_3d < tri_min_z || hit_pos[2] - radius_3d > tri_max_z {
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
            let dot = avg_n[0] * hit_normal[0] + avg_n[1] * hit_normal[1] + avg_n[2] * hit_normal[2];
            if dot < 0.05 {
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

            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1])).max(1e-7);

            if let Some(ref mut hist) = history {
                hist.record_bbox_before_modify(self, [px_min, py_min, px_max, py_max]);
            }

            for py in py_min..=py_max {
                let v_px = py as f32 / (h_f - 1.0);
                let row_offset = py as usize * self.width as usize;

                for px in px_min..=px_max {
                    let u_px = px as f32 / (w_f - 1.0);

                    let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0]) + (uv2[0] - uv1[0]) * (v_px - uv2[1])) / denom;
                    let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0]) + (uv0[0] - uv2[0]) * (v_px - uv2[1])) / denom;
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
                            let dist = dist2.sqrt();
                            let norm_dist = dist / radius_3d;
                            let brush_alpha = if norm_dist <= hardness {
                                1.0
                            } else {
                                1.0 - (norm_dist - hardness) / (1.0 - hardness)
                            };
                            let alpha = (brush_alpha * brush.opacity * brush.flow).clamp(0.0, 1.0);

                            if alpha > 0.001 {
                                let idx = row_offset + px as usize;
                                if on_mask {
                                    if let Some(ref mut mask) = self.mask {
                                        if idx < mask.buffer.len() {
                                            if is_eraser {
                                                mask.buffer[idx] = (mask.buffer[idx] - alpha).max(0.0);
                                            } else {
                                                mask.buffer[idx] = (mask.buffer[idx] * (1.0 - alpha) + alpha).clamp(0.0, 1.0);
                                            }
                                        }
                                    }
                                } else if is_eraser {
                                    if idx < self.color_buffer.len() {
                                        self.color_buffer[idx][3] = (self.color_buffer[idx][3] - alpha).max(0.0);
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
                                        self.metallic_buffer[idx] = self.metallic_buffer[idx] * (1.0 - alpha) + brush.metallic * alpha;
                                    }
                                    if brush.paint_roughness && idx < self.roughness_buffer.len() {
                                        self.roughness_buffer[idx] = self.roughness_buffer[idx] * (1.0 - alpha) + brush.roughness * alpha;
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
                                        self.height_buffer[idx] = self.height_buffer[idx] * (1.0 - alpha) + brush.height * alpha;
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

    /// 3D Surface Smudge: blends surrounding pixels on the 3D surface
    pub fn smudge_3d_surface(
        &mut self,
        mesh: &crate::graphics::mesh::Mesh,
        hit_pos: [f32; 3],
        hit_normal: [f32; 3],
        radius_3d: f32,
        brush: &Brush,
        mut history: Option<&mut HistoryManager>,
    ) -> [i32; 4] {
        let w = self.width as i32;
        let h = self.height as i32;
        let w_f = self.width as f32;
        let h_f = self.height as f32;
        let num_tris = mesh.indices.len() / 3;
        let r2 = radius_3d * radius_3d;
        let mut bbox = [w, h, 0, 0];
        let mut any_painted = false;

        // Sample center color to blend towards
        let mut center_color = [0.5f32, 0.5, 0.5, 1.0];
        let mut center_met = 0.0f32;
        let mut center_rough = 0.5f32;
        let mut found_center = false;

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() { continue; }
            let uv0 = mesh.uvs[i0];
            let px = ((uv0[0] * (w_f - 1.0)).round() as usize).min(self.width as usize - 1);
            let py = ((uv0[1] * (h_f - 1.0)).round() as usize).min(self.height as usize - 1);
            let idx = py * self.width as usize + px;
            if idx < self.color_buffer.len() {
                center_color = self.color_buffer[idx];
                center_met = self.metallic_buffer[idx];
                center_rough = self.roughness_buffer[idx];
                found_center = true;
                break;
            }
        }
        if !found_center { return [0, 0, 0, 0]; }

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;

            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() { continue; }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() { continue; }

            let v0 = mesh.vertices[i0];
            let v1 = mesh.vertices[i1];
            let v2 = mesh.vertices[i2];

            let tri_min_x = v0[0].min(v1[0]).min(v2[0]);
            let tri_max_x = v0[0].max(v1[0]).max(v2[0]);
            let tri_min_y = v0[1].min(v1[1]).min(v2[1]);
            let tri_max_y = v0[1].max(v1[1]).max(v2[1]);
            let tri_min_z = v0[2].min(v1[2]).min(v2[2]);
            let tri_max_z = v0[2].max(v1[2]).max(v2[2]);

            if hit_pos[0] + radius_3d < tri_min_x || hit_pos[0] - radius_3d > tri_max_x ||
               hit_pos[1] + radius_3d < tri_min_y || hit_pos[1] - radius_3d > tri_max_y ||
               hit_pos[2] + radius_3d < tri_min_z || hit_pos[2] - radius_3d > tri_max_z {
                continue;
            }

            let n0 = mesh.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n1 = mesh.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n2 = mesh.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
            let avg_n = [(n0[0] + n1[0] + n2[0]) * 0.3333, (n0[1] + n1[1] + n2[1]) * 0.3333, (n0[2] + n1[2] + n2[2]) * 0.3333];
            if avg_n[0] * hit_normal[0] + avg_n[1] * hit_normal[1] + avg_n[2] * hit_normal[2] < 0.05 {
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

            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1])).max(1e-7);

            if let Some(ref mut hist) = history {
                hist.record_bbox_before_modify(self, [px_min, py_min, px_max, py_max]);
            }

            for py in py_min..=py_max {
                let v_px = py as f32 / (h_f - 1.0);
                let row_offset = py as usize * self.width as usize;

                for px in px_min..=px_max {
                    let u_px = px as f32 / (w_f - 1.0);
                    let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0]) + (uv2[0] - uv1[0]) * (v_px - uv2[1])) / denom;
                    let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0]) + (uv0[0] - uv2[0]) * (v_px - uv2[1])) / denom;
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
                            let dist = dist2.sqrt();
                            let norm_dist = dist / radius_3d;
                            let alpha = ((1.0 - norm_dist) * brush.flow * 0.35).clamp(0.0, 1.0);
                            let idx = row_offset + px as usize;

                            if idx < self.color_buffer.len() {
                                let cur = self.color_buffer[idx];
                                self.color_buffer[idx] = [
                                    cur[0] * (1.0 - alpha) + center_color[0] * alpha,
                                    cur[1] * (1.0 - alpha) + center_color[1] * alpha,
                                    cur[2] * (1.0 - alpha) + center_color[2] * alpha,
                                    cur[3],
                                ];
                                self.metallic_buffer[idx] = self.metallic_buffer[idx] * (1.0 - alpha) + center_met * alpha;
                                self.roughness_buffer[idx] = self.roughness_buffer[idx] * (1.0 - alpha) + center_rough * alpha;
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

        if any_painted && bbox[2] >= bbox[0] && bbox[3] >= bbox[1] { bbox } else { [0, 0, 0, 0] }
    }

    /// 2D UV Smudge stamp
    pub fn smudge_stamp(&mut self, uv: [f32; 2], brush: &Brush, mut history: Option<&mut HistoryManager>) -> [i32; 4] {
        let w = self.width as i32;
        let h = self.height as i32;
        let px_center = (uv[0].clamp(0.0, 1.0) * (w - 1) as f32).round() as i32;
        let py_center = (uv[1].clamp(0.0, 1.0) * (h - 1) as f32).round() as i32;
        let radius = (brush.size * 0.5).round() as i32;

        let min_x = (px_center - radius).clamp(0, w - 1);
        let max_x = (px_center + radius).clamp(0, w - 1);
        let min_y = (py_center - radius).clamp(0, h - 1);
        let max_y = (py_center + radius).clamp(0, h - 1);

        if min_x > max_x || min_y > max_y { return [0, 0, 0, 0]; }

        if let Some(ref mut hist) = history {
            hist.record_bbox_before_modify(self, [min_x, min_y, max_x, max_y]);
        }

        let center_idx = (py_center as usize * self.width as usize + px_center as usize).min(self.color_buffer.len().saturating_sub(1));
        let c_color = self.color_buffer[center_idx];
        let c_met = self.metallic_buffer[center_idx];
        let c_rough = self.roughness_buffer[center_idx];
        let rad_f = radius.max(1) as f32;

        for y in min_y..=max_y {
            let row_offset = y as usize * self.width as usize;
            let dy = (y - py_center) as f32;
            for x in min_x..=max_x {
                let dx = (x - px_center) as f32;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= rad_f {
                    let norm_d = d / rad_f;
                    let alpha = ((1.0 - norm_d) * brush.flow * 0.35).clamp(0.0, 1.0);
                    let idx = row_offset + x as usize;
                    if idx < self.color_buffer.len() {
                        let cur = self.color_buffer[idx];
                        self.color_buffer[idx] = [
                            cur[0] * (1.0 - alpha) + c_color[0] * alpha,
                            cur[1] * (1.0 - alpha) + c_color[1] * alpha,
                            cur[2] * (1.0 - alpha) + c_color[2] * alpha,
                            cur[3],
                        ];
                        self.metallic_buffer[idx] = self.metallic_buffer[idx] * (1.0 - alpha) + c_met * alpha;
                        self.roughness_buffer[idx] = self.roughness_buffer[idx] * (1.0 - alpha) + c_rough * alpha;
                    }
                }
            }
        }
        [min_x, min_y, max_x, max_y]
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
}

impl LayerStack {
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
        stack.layers.push(Layer::new_paint(2, "Paint Layer 1", width, height));
        stack.active_layer_index = 1;
        stack.composite(None, None, None, None, None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
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
        self.composite(None, None, None, None, None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
    }

    /// Recomputes the composite buffers using default environment
    pub fn recompute_composite(&mut self) {
        self.composite(None, None, None, None, None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
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

        // Apply layer effects
        for layer in &mut self.layers {
            if !layer.effects.is_empty() {
                layer.apply_effects();
            }
        }

        let layers_meta: Vec<(usize, bool, f32, Option<usize>)> = self.layers.iter().map(|l| (l.id, l.visible, l.opacity, l.parent_folder_id)).collect();

        // Harvest available anchor points across layers
        let mut anchor_maps: std::collections::HashMap<String, Vec<f32>> = std::collections::HashMap::new();
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
                    _ => l.color_buffer.iter().map(|c| (c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114) * c[3]).collect(),
                };
                anchor_maps.insert(ap.name.clone(), buf);
            }
        }

        for layer in &mut self.layers {
            if layer.layer_type == LayerType::Folder || !layer.visible {
                continue;
            }

            // Check parent folder visibility and opacity hierarchy
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

            if let Some(ref mut mask) = layer.mask {
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
                            let base_c = [self.composite_color[i][0], self.composite_color[i][1], self.composite_color[i][2]];
                            let top_c = [layer.color_buffer[i][0], layer.color_buffer[i][1], layer.color_buffer[i][2]];
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
                            let blended = blend_scalar_channel(self.composite_metallic[i], layer.metallic_buffer[i], layer.channel_metallic_blend);
                            self.composite_metallic[i] = self.composite_metallic[i] * (1.0 - eff_a) + blended * eff_a;
                        }
                    }

                    // 3. Roughness channel
                    if layer.channel_roughness_enabled {
                        let eff_a = (base_alpha * layer.channel_roughness_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let blended = blend_scalar_channel(self.composite_roughness[i], layer.roughness_buffer[i], layer.channel_roughness_blend);
                            self.composite_roughness[i] = self.composite_roughness[i] * (1.0 - eff_a) + blended * eff_a;
                        }
                    }

                    // 4. Height channel
                    if layer.channel_height_enabled {
                        let eff_a = (base_alpha * layer.channel_height_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let blended = blend_scalar_channel(self.composite_height[i], layer.height_buffer[i], layer.channel_height_blend);
                            self.composite_height[i] = self.composite_height[i] * (1.0 - eff_a) + blended * eff_a;
                        }
                    }

                    // 5. Normal channel
                    if layer.channel_normal_enabled && layer.normal_buffer[i] != [0.5, 0.5, 1.0] {
                        let eff_a = (base_alpha * layer.channel_normal_opacity).clamp(0.0, 1.0);
                        if eff_a > 0.001 {
                            let base_n = self.composite_normal[i];
                            let top_n = layer.normal_buffer[i];
                            self.composite_normal[i] = [
                                base_n[0] * (1.0 - eff_a) + top_n[0] * eff_a,
                                base_n[1] * (1.0 - eff_a) + top_n[1] * eff_a,
                                base_n[2] * (1.0 - eff_a) + top_n[2] * eff_a,
                            ];
                        }
                    }
                }
            }
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
                    (wx + nx_ts * 0.35, wy + ny_ts * 0.35, wz + (nz_ts - 1.0) * 0.35)
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
    pub fn composite_pixel_at(
        &mut self,
        i: usize,
        baked_ao: Option<&[f32]>,
        env_preset: EnvironmentPreset,
        env_rotation_rad: f32,
        light_intensity: f32,
    ) {
        let mut base_c = [0.15f32, 0.15, 0.16, 1.0];
        let mut base_met = 0.0f32;
        let mut base_rough = 0.5f32;
        let mut base_norm = [0.5f32, 0.5, 1.0];
        let mut base_height = 0.0f32;

        let layers_meta: Vec<(usize, bool, f32, Option<usize>)> = self.layers.iter().map(|l| (l.id, l.visible, l.opacity, l.parent_folder_id)).collect();

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

            let mask_val = if let Some(ref mask) = layer.mask {
                mask.buffer.get(i).copied().unwrap_or(1.0)
            } else {
                1.0
            };
            let base_alpha = (layer.color_buffer[i][3] * layer.opacity * folder_opacity * mask_val).clamp(0.0, 1.0);
            if base_alpha > 0.001 {
                if layer.channel_color_enabled {
                    let eff_a = (base_alpha * layer.channel_color_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let b_c = [base_c[0], base_c[1], base_c[2]];
                        let top_c = [layer.color_buffer[i][0], layer.color_buffer[i][1], layer.color_buffer[i][2]];
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
                        let blended = blend_scalar_channel(base_met, layer.metallic_buffer[i], layer.channel_metallic_blend);
                        base_met = base_met * (1.0 - eff_a) + blended * eff_a;
                    }
                }

                if layer.channel_roughness_enabled {
                    let eff_a = (base_alpha * layer.channel_roughness_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let blended = blend_scalar_channel(base_rough, layer.roughness_buffer[i], layer.channel_roughness_blend);
                        base_rough = base_rough * (1.0 - eff_a) + blended * eff_a;
                    }
                }

                if layer.channel_height_enabled {
                    let eff_a = (base_alpha * layer.channel_height_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let blended = blend_scalar_channel(base_height, layer.height_buffer[i], layer.channel_height_blend);
                        base_height = base_height * (1.0 - eff_a) + blended * eff_a;
                    }
                }

                if layer.channel_normal_enabled && layer.normal_buffer[i] != [0.5, 0.5, 1.0] {
                    let eff_a = (base_alpha * layer.channel_normal_opacity).clamp(0.0, 1.0);
                    if eff_a > 0.001 {
                        let top_n = layer.normal_buffer[i];
                        base_norm = [
                            base_norm[0] * (1.0 - eff_a) + top_n[0] * eff_a,
                            base_norm[1] * (1.0 - eff_a) + top_n[1] * eff_a,
                            base_norm[2] * (1.0 - eff_a) + top_n[2] * eff_a,
                        ];
                    }
                }
            }
        }

        self.composite_color[i] = base_c;
        self.composite_metallic[i] = base_met;
        self.composite_roughness[i] = base_rough;
        self.composite_normal[i] = base_norm;
        if self.composite_height.len() > i {
            self.composite_height[i] = base_height;
        }

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
        if (bbox[2] - bbox[0]) > w / 2 || (bbox[3] - bbox[1]) > h / 2 {
            self.composite(None, baked_ao, None, None, None, None, env_preset, env_rotation_rad, light_intensity);
            return;
        }

        let x0 = bbox[0].clamp(0, w - 1) as usize;
        let x1 = (bbox[2] + 1).clamp(0, w) as usize;
        let y0 = bbox[1].clamp(0, h - 1) as usize;
        let y1 = (bbox[3] + 1).clamp(0, h) as usize;

        for y in y0..y1 {
            let row_offset = y * self.width as usize;
            for x in x0..x1 {
                self.composite_pixel_at(
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
    fn test_all_18_blend_modes() {
        let base = [0.5, 0.4, 0.3];
        let top = [0.8, 0.2, 0.6];
        let modes = [
            BlendMode::Normal, BlendMode::PassThrough, BlendMode::Multiply, BlendMode::Screen,
            BlendMode::Overlay, BlendMode::Add, BlendMode::Subtract, BlendMode::Darken,
            BlendMode::Lighten, BlendMode::ColorDodge, BlendMode::ColorBurn, BlendMode::LinearBurn,
            BlendMode::HardLight, BlendMode::SoftLight, BlendMode::Difference, BlendMode::Exclusion,
            BlendMode::Color, BlendMode::Luminosity,
        ];
        for mode in modes {
            let c = blend_colors(base, top, mode);
            assert!(c[0] >= 0.0 && c[0] <= 1.0, "Mode {:?} out of bounds: {:?}", mode, c);
            assert!(c[1] >= 0.0 && c[1] <= 1.0, "Mode {:?} out of bounds: {:?}", mode, c);
            assert!(c[2] >= 0.0 && c[2] <= 1.0, "Mode {:?} out of bounds: {:?}", mode, c);

            let s = blend_scalar_channel(0.5, 0.8, mode);
            assert!(s >= 0.0 && s <= 1.0, "Scalar mode {:?} out of bounds: {}", mode, s);
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

        stack.composite(None, None, None, None, None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
        // Child color should be visible because folder is visible
        assert_eq!(stack.composite_color[0], [1.0, 0.0, 0.0, 1.0]);
        // Metallic should be contributed
        assert_eq!(stack.composite_metallic[0], 1.0);
        // Roughness channel was disabled, so it should stay default (0.5)
        assert_eq!(stack.composite_roughness[0], 0.5);

        // Now hide the folder
        stack.layers[0].visible = false;
        stack.composite(None, None, None, None, None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
        // Since parent folder is hidden, child should not contribute
        assert_eq!(stack.composite_color[0], [0.15, 0.15, 0.16, 1.0]);
        assert_eq!(stack.composite_metallic[0], 0.0);
    }
}
