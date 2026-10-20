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
        name: String, // e.g. "Metal Edge Wear", "Dirt Crevices"
        wear_level: f32,
        contrast: f32,
        dirt_amount: f32,
        invert: bool,
        use_ao: bool,
        use_curvature: bool,
        anchor_name: Option<String>,
        blend_mode: MaskBlendMode,
        opacity: f32,
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
        self.recompute(None, None);
    }

    /// Evaluates the mask effects stack (Fill, Paint, Generator, Levels, Invert)
    pub fn recompute(&mut self, baked_curvature: Option<&[f32]>, baked_ao: Option<&[f32]>) {
        let _total = (self.width * self.height) as usize;
        let initial = match self.mask_type {
            MaskType::White => 1.0f32,
            MaskType::Black => 0.0f32,
        };
        self.buffer.fill(initial);

        for effect in &self.effects {
            match effect {
                MaskEffect::Fill { value, noise_frequency, blend_mode, opacity } => {
                    let w = self.width;
                    let h = self.height;
                    for y in 0..h {
                        for x in 0..w {
                            let idx = (y * w + x) as usize;
                            let mut sample = *value;
                            if *noise_frequency > 0.01 {
                                let nx = (x as f32 * noise_frequency * 0.05).sin();
                                let ny = (y as f32 * noise_frequency * 0.05).cos();
                                sample = (sample * 0.7 + (nx * ny * 0.5 + 0.5) * 0.3).clamp(0.0, 1.0);
                            }
                            self.buffer[idx] = blend_scalar(self.buffer[idx], sample, *blend_mode, *opacity);
                        }
                    }
                }
                MaskEffect::Paint { blend_mode: _, opacity: _ } => {
                    // Paint buffer contents are preserved
                }
                MaskEffect::Generator { wear_level, contrast, dirt_amount, invert, use_ao, use_curvature, blend_mode, opacity, .. } => {
                    let w = self.width;
                    let h = self.height;
                    for y in 0..h {
                        for x in 0..w {
                            let idx = (y * w + x) as usize;
                            let mut gen_val = 0.0f32;

                            // Read baked curvature (highlights edges)
                            if *use_curvature {
                                if let Some(curv) = baked_curvature {
                                    if idx < curv.len() {
                                        let c = curv[idx];
                                        // Curvature > 0.5 is convex edge ridge
                                        if c > 0.5 {
                                            gen_val += (c - 0.5) * 2.0 * wear_level;
                                        }
                                    }
                                }
                            }

                            // Read baked Ambient Occlusion (darkens crevices)
                            if *use_ao {
                                if let Some(ao) = baked_ao {
                                    if idx < ao.len() {
                                        let a = ao[idx];
                                        gen_val += (1.0 - a) * dirt_amount;
                                    }
                                }
                            }

                            // Apply contrast
                            gen_val = ((gen_val - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
                            if *invert {
                                gen_val = 1.0 - gen_val;
                            }

                            self.buffer[idx] = blend_scalar(self.buffer[idx], gen_val, *blend_mode, *opacity);
                        }
                    }
                }
                MaskEffect::Levels { black_in, white_in, gamma } => {
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
