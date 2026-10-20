#![allow(dead_code)]
use super::anchor::AnchorPoint;
use super::brush::{AlphaShape, Brush};
use super::mask::{LayerMask, MaskType};
use serde::{Deserialize, Serialize};

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
        }
    }

    /// Continuous Anti-Aliased Brush Splat with Toroidal UV Seam Wrapping
    pub fn paint_stamp(&mut self, uv: [f32; 2], brush: &Brush, on_mask: bool) -> [i32; 4] {
        let w = self.width as i32;
        let h = self.height as i32;
        let cx = (uv[0] * w as f32) as i32;
        let cy = (uv[1] * h as f32) as i32;
        let radius = brush.size * 0.5;
        let r_int = (radius + 1.0).ceil() as i32;
        let mut min_x = w;
        let mut min_y = h;
        let mut max_x = 0;
        let mut max_y = 0;

        for dy in -r_int..=r_int {
            for dx in -r_int..=r_int {
                let dist_sq = (dx * dx + dy * dy) as f32;
                if dist_sq <= radius * radius {
                    let dist = dist_sq.sqrt();
                    let norm_dist = dist / radius;

                    // Evaluate alpha falloff
                    let alpha = match brush.alpha_shape {
                        AlphaShape::Circle => (1.0 - norm_dist).clamp(0.0, 1.0),
                        AlphaShape::SoftRound => (1.0 - norm_dist).powf(1.8).clamp(0.0, 1.0),
                        AlphaShape::Gaussian => (-norm_dist * norm_dist * 3.0).exp(),
                        AlphaShape::Square => 1.0,
                        AlphaShape::Dirt | AlphaShape::Grunge => {
                            let noise = ((dx as f32 * 0.4).sin() * (dy as f32 * 0.4).cos()).abs();
                            (1.0 - norm_dist) * (0.6 + noise * 0.4)
                        }
                        AlphaShape::Concrete | AlphaShape::Scratches => {
                            let n = ((dx as f32 * 0.8).sin() + (dy as f32 * 0.2).cos()).abs();
                            (1.0 - norm_dist) * (0.4 + n * 0.6)
                        }
                    } * brush.flow * brush.opacity;

                    if alpha > 0.001 {
                        // Toroidal UV wrap around edges
                        let mut px = (cx + dx) % w;
                        if px < 0 { px += w; }
                        let mut py = (cy + dy) % h;
                        if py < 0 { py += h; }
                        min_x = min_x.min(px);
                        max_x = max_x.max(px);
                        min_y = min_y.min(py);
                        max_y = max_y.max(py);
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

    /// Continuous line interpolation with Shortest-Path Toroidal UV wrapping
    pub fn paint_line(&mut self, from_uv: [f32; 2], to_uv: [f32; 2], brush: &Brush, on_mask: bool) -> [i32; 4] {
        // Compute delta with toroidal wrap
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

            // Toroidal wrap
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
}

impl LayerStack {
    pub fn new(width: u32, height: u32) -> Self {
        let mut stack = Self {
            layers: Vec::new(),
            active_layer_index: 0,
            width,
            height,
            composite_color: vec![[0.85, 0.82, 0.78, 1.0]; (width * height) as usize],
            composite_metallic: vec![0.0; (width * height) as usize],
            composite_roughness: vec![0.35; (width * height) as usize],
            composite_normal: vec![[0.5, 0.5, 1.0]; (width * height) as usize],
        };

        // Initialize default layers matching Substance Painter reference
        stack.layers.push(Layer::new_fill(1, "Base", [0.82, 0.82, 0.84, 1.0], 0.0, 0.45, width, height));
        stack.layers.push(Layer::new_folder(2, "Scattering", width, height));
        stack.layers.push(Layer::new_folder(3, "Color", width, height));
        stack.layers.push(Layer::new_folder(4, "Roughness", width, height));
        stack.layers.push(Layer::new_folder(5, "Final Touches", width, height));
        stack.layers.push(Layer::new_paint(6, "Layer 1", width, height));

        stack.active_layer_index = 5; // Layer 1 active
        stack.composite(None, None);
        stack
    }

    pub fn resize(&mut self, new_w: u32, new_h: u32) {
        self.width = new_w;
        self.height = new_h;
        let total = (new_w * new_h) as usize;
        self.composite_color = vec![[0.85, 0.82, 0.78, 1.0]; total];
        self.composite_metallic = vec![0.0; total];
        self.composite_roughness = vec![0.35; total];
        self.composite_normal = vec![[0.5, 0.5, 1.0]; total];

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
        self.composite(None, None);
    }

    /// Full PBR Stack Composite with Blend Modes, Channel Masking, and Anchor Resolution
    pub fn composite(&mut self, baked_curvature: Option<&[f32]>, baked_ao: Option<&[f32]>) {
        let total = (self.width * self.height) as usize;
        self.composite_color.fill([0.15, 0.15, 0.16, 1.0]);
        self.composite_metallic.fill(0.0);
        self.composite_roughness.fill(0.5);
        self.composite_normal.fill([0.5, 0.5, 1.0]);

        for layer in &mut self.layers {
            if !layer.visible {
                continue;
            }

            // Update mask if present
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

                    // Blend base color
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
    }

    #[inline(always)]
    pub fn composite_pixel_at(&mut self, i: usize) {
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
    }

    pub fn composite_dirty(&mut self, bbox: [i32; 4]) {
        let w = self.width as i32;
        let h = self.height as i32;
        if bbox[2] < bbox[0] || bbox[3] < bbox[1] {
            return;
        }
        if (bbox[2] - bbox[0]) > w / 2 || (bbox[3] - bbox[1]) > h / 2 {
            self.composite(None, None);
            return;
        }

        let x0 = bbox[0].clamp(0, w - 1) as usize;
        let x1 = (bbox[2] + 1).clamp(0, w) as usize;
        let y0 = bbox[1].clamp(0, h - 1) as usize;
        let y1 = (bbox[3] + 1).clamp(0, h) as usize;

        for y in y0..y1 {
            let row_offset = y * self.width as usize;
            for x in x0..x1 {
                self.composite_pixel_at(row_offset + x);
            }
        }
    }
}
