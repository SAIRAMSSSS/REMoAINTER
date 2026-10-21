use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskType {
    White, // 1.0 everywhere
    Black, // 0.0 everywhere
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Subtract,
}

fn default_grunge_amount() -> f32 {
    0.35
}
fn default_grunge_scale() -> f32 {
    6.0
}
fn default_normal_dir() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}
fn default_target_color() -> [f32; 4] {
    [1.0, 0.0, 0.0, 1.0]
}
fn default_tolerance() -> f32 {
    0.2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaskEffect {
    Fill {
        value: f32,
        noise_frequency: f32,
        blend_mode: MaskBlendMode,
        opacity: f32,
    },
    Paint {
        blend_mode: MaskBlendMode,
        opacity: f32,
    },
    Generator {
        name: String, // "Metal Edge Wear", "Dirt Crevices", "Dust Accumulation", "Position Gradient", "World Normal", "Thickness", "ID Selection"
        wear_level: f32,
        contrast: f32,
        dirt_amount: f32,
        invert: bool,
        use_ao: bool,
        use_curvature: bool,
        anchor_name: Option<String>,
        blend_mode: MaskBlendMode,
        opacity: f32,

        #[serde(default = "default_grunge_amount")]
        grunge_amount: f32,
        #[serde(default = "default_grunge_scale")]
        grunge_scale: f32,
        #[serde(default)]
        axis: u8, // 0: X, 1: Y, 2: Z for Position Gradient
        #[serde(default = "default_normal_dir")]
        normal_direction: [f32; 3], // for World Normal (default [0.0, 1.0, 0.0])
        #[serde(default = "default_target_color")]
        target_color: [f32; 4], // for ID selection
        #[serde(default = "default_tolerance")]
        tolerance: f32, // for ID selection
    },
    Levels {
        black_in: f32,
        white_in: f32,
        gamma: f32,
    },
    Invert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMask {
    pub mask_type: MaskType,
    pub width: u32,
    pub height: u32,
    pub effects: Vec<MaskEffect>,
    #[serde(skip)]
    pub buffer: Vec<f32>,
}

impl LayerMask {
    pub fn new(mask_type: MaskType, width: u32, height: u32) -> Self {
        let initial_val = match mask_type {
            MaskType::White => 1.0f32,
            MaskType::Black => 0.0f32,
        };
        let buffer = vec![initial_val; (width * height) as usize];
        Self {
            mask_type,
            width,
            height,
            effects: Vec::new(),
            buffer,
        }
    }

    pub fn resize(&mut self, new_w: u32, new_h: u32) {
        self.width = new_w;
        self.height = new_h;
        let initial_val = match self.mask_type {
            MaskType::White => 1.0f32,
            MaskType::Black => 0.0f32,
        };
        self.buffer = vec![initial_val; (new_w * new_h) as usize];
        self.recompute(None, None, None, None, None, None);
    }

    pub fn invert(&mut self) {
        for v in &mut self.buffer {
            *v = (1.0 - *v).clamp(0.0, 1.0);
        }
    }

    pub fn clear(&mut self) {
        let initial_val = match self.mask_type {
            MaskType::White => 1.0f32,
            MaskType::Black => 0.0f32,
        };
        self.buffer.fill(initial_val);
    }

    /// Evaluates the mask effects stack (Fill, Paint, Generator, Levels, Invert)
    /// Driven by all available mesh maps: Curvature, AO, World Space Normal, Position, Thickness, ID
    pub fn recompute(
        &mut self,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
    ) {
        self.recompute_with_anchors(
            baked_curvature,
            baked_ao,
            baked_wsn,
            baked_pos,
            baked_thickness,
            baked_id,
            None,
        );
    }

    /// Evaluates the mask effects stack with optional Anchor Points referenced across layers
    pub fn recompute_with_anchors(
        &mut self,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
        anchor_maps: Option<&std::collections::HashMap<String, Vec<f32>>>,
    ) {
        let _total = (self.width * self.height) as usize;
        let initial = match self.mask_type {
            MaskType::White => 1.0f32,
            MaskType::Black => 0.0f32,
        };
        self.buffer.fill(initial);

        // Calculate Position bounding box if position gradient is used
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

        let w = self.width;
        let h = self.height;
        let w_f = w.max(1) as f32;
        let h_f = h.max(1) as f32;

        for effect in &self.effects {
            match effect {
                MaskEffect::Fill {
                    value,
                    noise_frequency,
                    blend_mode,
                    opacity,
                } => {
                    for y in 0..h {
                        for x in 0..w {
                            let idx = (y * w + x) as usize;
                            let mut sample = *value;
                            if *noise_frequency > 0.01 {
                                let nx = (x as f32 * noise_frequency * 0.05).sin();
                                let ny = (y as f32 * noise_frequency * 0.05).cos();
                                sample =
                                    (sample * 0.7 + (nx * ny * 0.5 + 0.5) * 0.3).clamp(0.0, 1.0);
                            }
                            self.buffer[idx] =
                                blend_scalar(self.buffer[idx], sample, *blend_mode, *opacity);
                        }
                    }
                }
                MaskEffect::Paint {
                    blend_mode: _,
                    opacity: _,
                } => {
                    // Paint buffer contents are preserved
                }
                MaskEffect::Generator {
                    name,
                    wear_level,
                    contrast,
                    dirt_amount,
                    invert,
                    use_ao,
                    use_curvature,
                    grunge_amount,
                    grunge_scale,
                    axis,
                    normal_direction,
                    target_color,
                    tolerance,
                    blend_mode,
                    opacity,
                    anchor_name,
                    ..
                } => {
                    let gen_lower = name.to_lowercase();
                    let is_dust = gen_lower.contains("dust");
                    let is_gradient =
                        gen_lower.contains("gradient") || gen_lower.contains("position");
                    let is_orientation =
                        gen_lower.contains("normal") || gen_lower.contains("world");
                    let is_thickness = gen_lower.contains("thickness");
                    let is_id = gen_lower.contains("id") || gen_lower.contains("color selection");
                    let is_dirt = gen_lower.contains("dirt")
                        || gen_lower.contains("cavity")
                        || gen_lower.contains("crevice");

                    for y in 0..h {
                        let v_coord = y as f32 / h_f;
                        let row_offset = (y * w) as usize;

                        for x in 0..w {
                            let u_coord = x as f32 / w_f;
                            let idx = row_offset + x as usize;
                            let mut gen_val = 0.0f32;

                            // 1. Position Gradient
                            if is_gradient {
                                let t = if let Some(pos_slice) = baked_pos {
                                    if idx < pos_slice.len() {
                                        let ax = (*axis as usize).min(2);
                                        let range = (pos_max[ax] - pos_min[ax]).max(1e-5);
                                        (pos_slice[idx][ax] - pos_min[ax]) / range
                                    } else {
                                        match *axis {
                                            0 => u_coord,
                                            1 => 1.0 - v_coord,
                                            _ => (u_coord + v_coord) * 0.5,
                                        }
                                    }
                                } else {
                                    match *axis {
                                        0 => u_coord,
                                        1 => 1.0 - v_coord,
                                        _ => (u_coord + v_coord) * 0.5,
                                    }
                                };
                                gen_val = t.clamp(0.0, 1.0);
                            }
                            // 2. World Space Normal Orientation
                            else if is_orientation {
                                let n = if let Some(wsn) = baked_wsn {
                                    wsn.get(idx).copied().unwrap_or([0.0, 1.0, 0.0])
                                } else {
                                    [0.0, 1.0, 0.0]
                                };
                                let dot = n[0] * normal_direction[0]
                                    + n[1] * normal_direction[1]
                                    + n[2] * normal_direction[2];
                                gen_val = (dot * 0.5 + 0.5).clamp(0.0, 1.0);
                            }
                            // 3. Dust Accumulation (Top/Upward Facing + Cavity Occlusion)
                            else if is_dust {
                                let ny = if let Some(wsn) = baked_wsn {
                                    wsn.get(idx).map(|n| n[1]).unwrap_or(1.0)
                                } else {
                                    1.0
                                };
                                let up_facing = ny.max(0.0);
                                let ao_fac = if let Some(ao) = baked_ao {
                                    ao.get(idx).copied().unwrap_or(1.0)
                                } else {
                                    1.0
                                };
                                gen_val = up_facing * (0.6 + 0.4 * (1.0 - ao_fac)) * wear_level;
                            }
                            // 4. Thickness / Subsurface
                            else if is_thickness {
                                let th = if let Some(thick) = baked_thickness {
                                    thick.get(idx).copied().unwrap_or(0.5)
                                } else {
                                    0.5
                                };
                                gen_val = (1.0 - th) * dirt_amount + th * wear_level;
                            }
                            // 5. Color / ID Selection
                            else if is_id {
                                if let Some(ids) = baked_id {
                                    if idx < ids.len() {
                                        let c = ids[idx];
                                        let dr = c[0] - target_color[0];
                                        let dg = c[1] - target_color[1];
                                        let db = c[2] - target_color[2];
                                        let dist = (dr * dr + dg * dg + db * db).sqrt();
                                        let tol = tolerance.max(0.001);
                                        if dist <= tol {
                                            gen_val = (1.0 - dist / tol).clamp(0.0, 1.0);
                                        }
                                    }
                                }
                            }
                            // 6. Dirt Crevices
                            else if is_dirt {
                                if *use_ao {
                                    if let Some(ao) = baked_ao {
                                        if idx < ao.len() {
                                            let a = ao[idx];
                                            gen_val += (1.0 - a) * dirt_amount;
                                        }
                                    }
                                }
                                if *use_curvature {
                                    if let Some(curv) = baked_curvature {
                                        if idx < curv.len() {
                                            let c = curv[idx];
                                            if c < 0.5 {
                                                gen_val += (0.5 - c) * 2.0 * wear_level;
                                            }
                                        }
                                    }
                                }
                            }
                            // 7. Metal Edge Wear (Default)
                            else {
                                if *use_curvature {
                                    if let Some(curv) = baked_curvature {
                                        if idx < curv.len() {
                                            let c = curv[idx];
                                            if c > 0.5 {
                                                gen_val += (c - 0.5) * 2.0 * wear_level;
                                            }
                                        }
                                    }
                                }
                                if *use_ao {
                                    if let Some(ao) = baked_ao {
                                        if idx < ao.len() {
                                            let a = ao[idx];
                                            gen_val *= a;
                                        }
                                    }
                                }
                            }

                            // Anchor Point modulation / injection if referenced
                            if let Some(ref aname) = anchor_name {
                                if let Some(anchors) = anchor_maps {
                                    if let Some(abuf) = anchors.get(aname) {
                                        if let Some(&asig) = abuf.get(idx) {
                                            gen_val = (gen_val + asig * wear_level).clamp(0.0, 1.0);
                                        }
                                    }
                                }
                            }

                            // Modulate with multi-octave procedural grunge noise
                            if *grunge_amount > 0.001 && !is_id {
                                let g_scale = *grunge_scale;
                                let noise = fbm_noise(u_coord * g_scale, v_coord * g_scale, 3);
                                let mod_factor =
                                    (1.0 - grunge_amount) + noise * grunge_amount * 1.5;
                                gen_val *= mod_factor;
                            }

                            // Apply Contrast Curve
                            gen_val = ((gen_val - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
                            if *invert {
                                gen_val = 1.0 - gen_val;
                            }

                            self.buffer[idx] =
                                blend_scalar(self.buffer[idx], gen_val, *blend_mode, *opacity);
                        }
                    }
                }
                MaskEffect::Levels {
                    black_in,
                    white_in,
                    gamma,
                } => {
                    let range = (white_in - black_in).max(0.001);
                    for v in &mut self.buffer {
                        let normalized = ((*v - black_in) / range).clamp(0.0, 1.0);
                        *v = normalized.powf(1.0 / gamma.max(0.01));
                    }
                }
                MaskEffect::Invert => {
                    for v in &mut self.buffer {
                        *v = 1.0 - *v;
                    }
                }
            }
        }
    }
}

/// Deterministic 2D procedural pseudo-random noise
fn pseudo_noise(x: f32, y: f32) -> f32 {
    let s = (x * 12.9898 + y * 78.233).sin() * 43758.5453;
    s - s.floor()
}

/// Smooth bilinear interpolated value noise
fn smooth_noise(x: f32, y: f32) -> f32 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;

    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);

    let n00 = pseudo_noise(ix, iy);
    let n10 = pseudo_noise(ix + 1.0, iy);
    let n01 = pseudo_noise(ix, iy + 1.0);
    let n11 = pseudo_noise(ix + 1.0, iy + 1.0);

    let nx0 = n00 * (1.0 - ux) + n10 * ux;
    let nx1 = n01 * (1.0 - ux) + n11 * ux;
    nx0 * (1.0 - uy) + nx1 * uy
}

/// Fast Multi-Octave Fractal Brownian Motion (FBM) Grunge Noise
pub fn fbm_noise(x: f32, y: f32, octaves: usize) -> f32 {
    let mut val = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    for _ in 0..octaves {
        val += smooth_noise(x * freq, y * freq) * amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    val
}

fn blend_scalar(base: f32, blend: f32, mode: MaskBlendMode, opacity: f32) -> f32 {
    let result = match mode {
        MaskBlendMode::Normal => blend,
        MaskBlendMode::Multiply => base * blend,
        MaskBlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - blend),
        MaskBlendMode::Overlay => {
            if base < 0.5 {
                2.0 * base * blend
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - blend)
            }
        }
        MaskBlendMode::Add => (base + blend).min(1.0),
        MaskBlendMode::Subtract => (base - blend).max(0.0),
    };
    base * (1.0 - opacity) + result * opacity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_wear_and_dirt_generators() {
        let mut mask = LayerMask::new(MaskType::Black, 2, 2);
        // Convex edge in pixel 0
        let curvature = vec![0.9, 0.5, 0.5, 0.1];
        let ao = vec![1.0, 1.0, 0.2, 0.2];

        // 1. Metal Edge Wear
        mask.effects.push(MaskEffect::Generator {
            name: "Metal Edge Wear".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: true,
            anchor_name: None,
            blend_mode: MaskBlendMode::Normal,
            opacity: 1.0,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        });

        mask.recompute(Some(&curvature), Some(&ao), None, None, None, None);
        assert!(mask.buffer[0] > 0.5, "Pixel 0 should have high edge wear");
        assert!(
            mask.buffer[1] <= 0.5,
            "Flat surface should not have high edge wear"
        );

        // 2. Dirt Crevices
        let mut dirt_mask = LayerMask::new(MaskType::Black, 2, 2);
        dirt_mask.effects.push(MaskEffect::Generator {
            name: "Dirt Crevices".into(),
            wear_level: 0.0,
            contrast: 1.0,
            dirt_amount: 1.0,
            invert: false,
            use_ao: true,
            use_curvature: false,
            anchor_name: None,
            blend_mode: MaskBlendMode::Normal,
            opacity: 1.0,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        });
        dirt_mask.recompute(Some(&curvature), Some(&ao), None, None, None, None);
        assert!(
            dirt_mask.buffer[2] > 0.5,
            "Crevice in pixel 2 should accumulate dirt"
        );
    }

    #[test]
    fn test_position_gradient_and_id_selection() {
        let mut mask = LayerMask::new(MaskType::Black, 2, 2);
        let pos = vec![
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 10.0, 0.0],
        ];
        let id_map = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];

        // Position Gradient along Y (axis = 1)
        mask.effects.push(MaskEffect::Generator {
            name: "Position Gradient".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: None,
            blend_mode: MaskBlendMode::Normal,
            opacity: 1.0,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.1,
        });

        mask.recompute(None, None, None, Some(&pos), None, None);
        assert_eq!(mask.buffer[0], 0.0, "Bottom pixel should be 0.0");
        assert_eq!(mask.buffer[3], 1.0, "Top pixel should be 1.0");

        // ID Selection (Target red [1.0, 0.0, 0.0, 1.0])
        let mut id_mask = LayerMask::new(MaskType::Black, 2, 2);
        id_mask.effects.push(MaskEffect::Generator {
            name: "Color Selection".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: None,
            blend_mode: MaskBlendMode::Normal,
            opacity: 1.0,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.1,
        });
        id_mask.recompute(None, None, None, None, None, Some(&id_map));
        assert_eq!(id_mask.buffer[0], 1.0, "Pixel 0 matches red target exactly");
        assert_eq!(id_mask.buffer[1], 0.0, "Pixel 1 is green, should not match");
    }

    #[test]
    fn test_anchor_point_generator_referencing() {
        let mut mask = LayerMask::new(MaskType::Black, 2, 2);
        mask.effects.push(MaskEffect::Generator {
            name: "Metal Edge Wear".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: Some("MyDecalAnchor".into()),
            blend_mode: MaskBlendMode::Normal,
            opacity: 1.0,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.1,
        });

        let mut anchors = std::collections::HashMap::new();
        anchors.insert("MyDecalAnchor".to_string(), vec![0.8, 0.0, 0.5, 1.0]);

        mask.recompute_with_anchors(None, None, None, None, None, None, Some(&anchors));
        assert!((mask.buffer[0] - 0.8).abs() < 1e-4);
        assert!((mask.buffer[1] - 0.0).abs() < 1e-4);
        assert!((mask.buffer[2] - 0.5).abs() < 1e-4);
        assert!((mask.buffer[3] - 1.0).abs() < 1e-4);
    }
}
