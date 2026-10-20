#![allow(dead_code)]
use super::anchor::AnchorPoint;
use super::brush::{AlphaShape, Brush};
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
    Multiply,
    Screen,
    Overlay,
    PassThrough,
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

    // Masking & Anchor Points
    pub mask: Option<LayerMask>,
    pub anchor_point: Option<AnchorPoint>,

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
            mask: None,
            anchor_point: None,
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
            mask: None,
            anchor_point: None,
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

            for y in 0..height {
                let sy = ((y as f64 / height as f64) * (img_h as f64)) as u32 % img_h;
                for x in 0..width {
                    let sx = ((x as f64 / width as f64) * (img_w as f64)) as u32 % img_w;
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
        if let Ok(img) = image::open(mask_path) {
            let grey = img.to_luma8();
            let (img_w, img_h) = grey.dimensions();

            let mut mask = LayerMask::new(MaskType::Black, self.width, self.height);
            for y in 0..self.height {
                let sy = ((y as f64 / self.height as f64) * (img_h as f64)) as u32 % img_h;
                for x in 0..self.width {
                    let sx = ((x as f64 / self.width as f64) * (img_w as f64)) as u32 % img_w;
                    let p = grey.get_pixel(sx, sy);
                    let val = p[0] as f32 / 255.0;
                    let idx = (y * self.width + x) as usize;

                    let curv = baked_curvature.and_then(|c| c.get(idx)).copied().unwrap_or(0.5);
                    let ao = baked_ao.and_then(|a| a.get(idx)).copied().unwrap_or(1.0);

                    let edge_factor = ((curv - 0.5).abs() * 2.0).clamp(0.0, 1.0);
                    let cavity_factor = (1.0 - ao).clamp(0.0, 1.0);

                    let combined = (val * 0.70 + edge_factor * 0.20 + cavity_factor * 0.10).clamp(0.0, 1.0);
                    mask.buffer[idx] = combined;
                }
            }
            self.mask = Some(mask);
        }
    }

    pub fn new_folder(id: usize, name: impl Into<String>, width: u32, height: u32) -> Self {
        let mut l = Self::new_paint(id, name, width, height);
        l.layer_type = LayerType::Folder;
        l.blend_mode = BlendMode::PassThrough;
        l
    }

    pub fn add_black_mask(&mut self) {
        let mut mask = LayerMask::new(MaskType::Black, self.width, self.height);
        mask.recompute(None, None);
        self.mask = Some(mask);
    }

    pub fn add_white_mask(&mut self) {
        let mut mask = LayerMask::new(MaskType::White, self.width, self.height);
        mask.recompute(None, None);
        self.mask = Some(mask);
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

    pub fn paint_stamp(&mut self, uv: [f32; 2], brush: &Brush, on_mask: bool) -> [i32; 4] {
        let px_center = (uv[0] * self.width as f32) as i32;
        let py_center = (uv[1] * self.height as f32) as i32;
        let radius = (brush.size * 0.5) as i32;

        let w = self.width as i32;
        let h = self.height as i32;

        let min_x = px_center - radius;
        let max_x = px_center + radius;
        let min_y = py_center - radius;
        let max_y = py_center + radius;

        let hardness = brush.hardness.clamp(0.0, 0.999);
        let rad_f = radius as f32;

        for y in min_y..=max_y {
            let py = y.rem_euclid(h);
            let dy = (y - py_center) as f32;

            for x in min_x..=max_x {
                let px = x.rem_euclid(w);
                let dx = (x - px_center) as f32;

                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= rad_f {
                    let norm_dist = dist / rad_f;
                    let brush_alpha = if norm_dist <= hardness {
                        1.0
                    } else {
                        1.0 - (norm_dist - hardness) / (1.0 - hardness)
                    };

                    let shape_alpha = if let Some(ref ca) = brush.custom_alpha {
                        let au = ((dx / rad_f + 1.0) * 0.5 * (ca.width as f32)) as u32;
                        let av = ((dy / rad_f + 1.0) * 0.5 * (ca.height as f32)) as u32;
                        if au < ca.width && av < ca.height {
                            ca.data[(av * ca.width + au) as usize] as f32 / 255.0
                        } else {
                            0.0
                        }
                    } else {
                        match brush.alpha_shape {
                            AlphaShape::Circle => 1.0,
                            AlphaShape::Square => 1.0,
                            AlphaShape::SoftRound => (1.0 - norm_dist).powf(2.0),
                            _ => 1.0,
                        }
                    };

                    let alpha = (brush_alpha * shape_alpha * brush.opacity * brush.flow).clamp(0.0, 1.0);

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
        }
        if min_x <= max_x && min_y <= max_y {
            [min_x, min_y, max_x, max_y]
        } else {
            [0, 0, 0, 0]
        }
    }

    pub fn paint_line(&mut self, from_uv: [f32; 2], to_uv: [f32; 2], brush: &Brush, on_mask: bool) -> [i32; 4] {
        let mut dx = to_uv[0] - from_uv[0];
        let mut dy = to_uv[1] - from_uv[1];

        if dx > 0.5 { dx -= 1.0; }
        else if dx < -0.5 { dx += 1.0; }

        if dy > 0.5 { dy -= 1.0; }
        else if dy < -0.5 { dy += 1.0; }

        let pixel_dist = (dx * dx * (self.width as f32).powi(2) + dy * dy * (self.height as f32).powi(2)).sqrt();
        let step_size = (brush.size * brush.spacing).max(1.0);
        let steps = (pixel_dist / step_size).ceil().max(1.0) as usize;

        let mut bbox = [self.width as i32, self.height as i32, 0, 0];
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let mut u = from_uv[0] + dx * t;
            let mut v = from_uv[1] + dy * t;

            u = u.rem_euclid(1.0);
            v = v.rem_euclid(1.0);

            let s_box = self.paint_stamp([u, v], brush, on_mask);
            if s_box[2] >= s_box[0] && s_box[3] >= s_box[1] {
                bbox[0] = bbox[0].min(s_box[0]);
                bbox[1] = bbox[1].min(s_box[1]);
                bbox[2] = bbox[2].max(s_box[2]);
                bbox[3] = bbox[3].max(s_box[3]);
            }
        }
        bbox
    }
}

pub struct LayerStack {
    pub layers: Vec<Layer>,
    pub active_layer_index: usize,
    pub width: u32,
    pub height: u32,

    pub composite_color: Vec<[f32; 4]>,
    pub composite_metallic: Vec<f32>,
    pub composite_roughness: Vec<f32>,
    pub composite_normal: Vec<[f32; 3]>,
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
            composite_lit: vec![[0.4, 0.4, 0.45, 1.0]; total],
        };

        // Initialize with authentic Substance starter material if available
        let starter_mat = Path::new("/mnt/shared/REMoAINTER/assets/shelf/smart_materials/Steel Painted Scraped Green.webp");
        if starter_mat.exists() {
            stack.layers.push(Layer::from_texture_asset(
                1,
                "Base (Steel Painted Scraped)",
                starter_mat,
                1,
                ([0.28, 0.42, 0.32, 1.0], 0.75, 0.35),
                width,
                height,
            ));
        } else {
            stack.layers.push(Layer::new_fill(1, "Base Steel", [0.45, 0.48, 0.52, 1.0], 0.8, 0.3, width, height));
        }

        stack.layers.push(Layer::new_paint(2, "Paint Layer 1", width, height));
        stack.active_layer_index = 1;
        stack.composite(None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
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
        self.composite(None, None, EnvironmentPreset::StudioTomoco, 0.0, 1.0);
    }

    /// Full PBR Stack Composite with Blend Modes, Channel Masking, and Rayon Multi-threaded Global Illumination
    pub fn composite(
        &mut self,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        env_preset: EnvironmentPreset,
        env_rotation_rad: f32,
        light_intensity: f32,
    ) {
        let total = (self.width * self.height) as usize;
        self.composite_color.fill([0.15, 0.15, 0.16, 1.0]);
        self.composite_metallic.fill(0.0);
        self.composite_roughness.fill(0.5);
        self.composite_normal.fill([0.5, 0.5, 1.0]);

        for layer in &mut self.layers {
            if !layer.visible {
                continue;
            }

            if let Some(ref mut mask) = layer.mask {
                mask.recompute(baked_curvature, baked_ao);
            }

            let layer_op = layer.opacity;

            for i in 0..total {
                let mask_val = if let Some(ref mask) = layer.mask {
                    mask.buffer.get(i).copied().unwrap_or(1.0)
                } else {
                    1.0
                };
                let effective_alpha = (layer.color_buffer[i][3] * layer_op * mask_val).clamp(0.0, 1.0);

                if effective_alpha > 0.001 {
                    let base_c = self.composite_color[i];
                    let top_c = layer.color_buffer[i];

                    let blended_c = match layer.blend_mode {
                        BlendMode::Normal | BlendMode::PassThrough => top_c,
                        BlendMode::Multiply => [base_c[0] * top_c[0], base_c[1] * top_c[1], base_c[2] * top_c[2], top_c[3]],
                        BlendMode::Screen => [
                            1.0 - (1.0 - base_c[0]) * (1.0 - top_c[0]),
                            1.0 - (1.0 - base_c[1]) * (1.0 - top_c[1]),
                            1.0 - (1.0 - base_c[2]) * (1.0 - top_c[2]),
                            top_c[3],
                        ],
                        BlendMode::Overlay => [
                            if base_c[0] < 0.5 { 2.0 * base_c[0] * top_c[0] } else { 1.0 - 2.0 * (1.0 - base_c[0]) * (1.0 - top_c[0]) },
                            if base_c[1] < 0.5 { 2.0 * base_c[1] * top_c[1] } else { 1.0 - 2.0 * (1.0 - base_c[1]) * (1.0 - top_c[1]) },
                            if base_c[2] < 0.5 { 2.0 * base_c[2] * top_c[2] } else { 1.0 - 2.0 * (1.0 - base_c[2]) * (1.0 - top_c[2]) },
                            top_c[3],
                        ],
                    };

                    self.composite_color[i] = [
                        base_c[0] * (1.0 - effective_alpha) + blended_c[0] * effective_alpha,
                        base_c[1] * (1.0 - effective_alpha) + blended_c[1] * effective_alpha,
                        base_c[2] * (1.0 - effective_alpha) + blended_c[2] * effective_alpha,
                        1.0,
                    ];

                    self.composite_metallic[i] = self.composite_metallic[i] * (1.0 - effective_alpha) + layer.metallic_buffer[i] * effective_alpha;
                    self.composite_roughness[i] = self.composite_roughness[i] * (1.0 - effective_alpha) + layer.roughness_buffer[i] * effective_alpha;

                    if layer.normal_buffer[i] != [0.5, 0.5, 1.0] {
                        let base_n = self.composite_normal[i];
                        let top_n = layer.normal_buffer[i];
                        self.composite_normal[i] = [
                            base_n[0] * (1.0 - effective_alpha) + top_n[0] * effective_alpha,
                            base_n[1] * (1.0 - effective_alpha) + top_n[1] * effective_alpha,
                            base_n[2] * (1.0 - effective_alpha) + top_n[2] * effective_alpha,
                        ];
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

                let nx = n_ts[0] * 2.0 - 1.0;
                let ny = n_ts[1] * 2.0 - 1.0;
                let nz = (n_ts[2] * 2.0 - 1.0).max(0.1);
                let n_len = (nx * nx + ny * ny + nz * nz).sqrt().max(0.001);
                let n_vec = [nx / n_len, ny / n_len, nz / n_len];

                let ao = baked_ao.and_then(|a| a.get(i)).copied().unwrap_or(1.0);
                let view_dir = [0.0f32, 0.0, 1.0];

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

        for layer in &self.layers {
            if !layer.visible {
                continue;
            }
            let mask_val = if let Some(ref mask) = layer.mask {
                mask.buffer.get(i).copied().unwrap_or(1.0)
            } else {
                1.0
            };
            let effective_alpha = (layer.color_buffer[i][3] * layer.opacity * mask_val).clamp(0.0, 1.0);
            if effective_alpha > 0.001 {
                let top_c = layer.color_buffer[i];
                let blended_c = match layer.blend_mode {
                    BlendMode::Normal | BlendMode::PassThrough => top_c,
                    BlendMode::Multiply => [base_c[0] * top_c[0], base_c[1] * top_c[1], base_c[2] * top_c[2], top_c[3]],
                    BlendMode::Screen => [
                        1.0 - (1.0 - base_c[0]) * (1.0 - top_c[0]),
                        1.0 - (1.0 - base_c[1]) * (1.0 - top_c[1]),
                        1.0 - (1.0 - base_c[2]) * (1.0 - top_c[2]),
                        top_c[3],
                    ],
                    BlendMode::Overlay => [
                        if base_c[0] < 0.5 { 2.0 * base_c[0] * top_c[0] } else { 1.0 - 2.0 * (1.0 - base_c[0]) * (1.0 - top_c[0]) },
                        if base_c[1] < 0.5 { 2.0 * base_c[1] * top_c[1] } else { 1.0 - 2.0 * (1.0 - base_c[1]) * (1.0 - top_c[1]) },
                        if base_c[2] < 0.5 { 2.0 * base_c[2] * top_c[2] } else { 1.0 - 2.0 * (1.0 - base_c[2]) * (1.0 - top_c[2]) },
                        top_c[3],
                    ],
                };

                base_c = [
                    base_c[0] * (1.0 - effective_alpha) + blended_c[0] * effective_alpha,
                    base_c[1] * (1.0 - effective_alpha) + blended_c[1] * effective_alpha,
                    base_c[2] * (1.0 - effective_alpha) + blended_c[2] * effective_alpha,
                    1.0,
                ];
                base_met = base_met * (1.0 - effective_alpha) + layer.metallic_buffer[i] * effective_alpha;
                base_rough = base_rough * (1.0 - effective_alpha) + layer.roughness_buffer[i] * effective_alpha;

                if layer.normal_buffer[i] != [0.5, 0.5, 1.0] {
                    let top_n = layer.normal_buffer[i];
                    base_norm = [
                        base_norm[0] * (1.0 - effective_alpha) + top_n[0] * effective_alpha,
                        base_norm[1] * (1.0 - effective_alpha) + top_n[1] * effective_alpha,
                        base_norm[2] * (1.0 - effective_alpha) + top_n[2] * effective_alpha,
                    ];
                }
            }
        }

        self.composite_color[i] = base_c;
        self.composite_metallic[i] = base_met;
        self.composite_roughness[i] = base_rough;
        self.composite_normal[i] = base_norm;

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
            self.composite(None, baked_ao, env_preset, env_rotation_rad, light_intensity);
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
