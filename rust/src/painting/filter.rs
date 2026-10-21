// ─────────────────────────────────────────────────────────────────────────────
// REMoAINTER — Non-Destructive Filter & Effects Stack (Substance Painter Parity)
// Features:
// - Levels (min/max input, gamma, min/max output)
// - Gaussian Blur (Separable 2-pass convolution)
// - Sharpen (Unsharp masking)
// - HSL (Hue shift, saturation multiplier, lightness offset)
// - Invert
// - Gradient Map (Luminance to RGBA gradient ramp)
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterTargetChannel {
    All,
    BaseColor,
    Metallic,
    Roughness,
    Height,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterEffect {
    Levels {
        min_in: f32,
        max_in: f32,
        gamma: f32,
        min_out: f32,
        max_out: f32,
        channel: FilterTargetChannel,
    },
    Blur {
        radius: f32,
        channel: FilterTargetChannel,
    },
    Sharpen {
        strength: f32,
        channel: FilterTargetChannel,
    },
    HSL {
        hue_shift: f32,    // -180.0 .. 180.0
        sat_mult: f32,     // 0.0 .. 3.0
        light_offset: f32, // -1.0 .. 1.0
    },
    Invert {
        channel: FilterTargetChannel,
    },
    GradientMap {
        color_start: [f32; 4],
        color_end: [f32; 4],
    },
}

impl FilterEffect {
    pub fn name(&self) -> &'static str {
        match self {
            FilterEffect::Levels { .. } => "Levels",
            FilterEffect::Blur { .. } => "Gaussian Blur",
            FilterEffect::Sharpen { .. } => "Sharpen",
            FilterEffect::HSL { .. } => "HSL Adjustment",
            FilterEffect::Invert { .. } => "Invert",
            FilterEffect::GradientMap { .. } => "Gradient Map",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Filter Algorithms
// ─────────────────────────────────────────────────────────────────────────────

pub fn apply_levels_scalar(
    v: f32,
    min_in: f32,
    max_in: f32,
    gamma: f32,
    min_out: f32,
    max_out: f32,
) -> f32 {
    let range_in = (max_in - min_in).max(0.001);
    let norm = ((v - min_in) / range_in).clamp(0.0, 1.0);
    let g = norm.powf(1.0 / gamma.max(0.01));
    (min_out + g * (max_out - min_out)).clamp(0.0, 1.0)
}

pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;

    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < 1e-6 {
        let seg = (g - b) / d;
        if g < b {
            seg + 6.0
        } else {
            seg
        }
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;

    (h, s, l)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 {
        return (l, l, l);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hk = h / 360.0;

    let tr = (hk + 1.0 / 3.0).rem_euclid(1.0);
    let tg = hk.rem_euclid(1.0);
    let tb = (hk - 1.0 / 3.0).rem_euclid(1.0);

    let channel = |tc: f32| -> f32 {
        if tc < 1.0 / 6.0 {
            p + (q - p) * 6.0 * tc
        } else if tc < 1.0 / 2.0 {
            q
        } else if tc < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - tc) * 6.0
        } else {
            p
        }
    };

    (channel(tr), channel(tg), channel(tb))
}

pub fn gaussian_blur_f32(data: &[f32], width: usize, height: usize, radius: f32) -> Vec<f32> {
    if radius < 0.5 || width == 0 || height == 0 {
        return data.to_vec();
    }

    let r = radius.round() as usize;
    let sigma = radius * 0.5;
    let mut kernel = Vec::with_capacity(r * 2 + 1);
    let mut weight_sum = 0.0f32;

    for i in 0..=(r * 2) {
        let x = i as f32 - r as f32;
        let w = (-x * x / (2.0 * sigma * sigma)).exp();
        kernel.push(w);
        weight_sum += w;
    }
    for w in &mut kernel {
        *w /= weight_sum;
    }

    // Horizontal pass
    let mut temp = vec![0.0f32; width * height];
    for y in 0..height {
        let row_offset = y * width;
        for x in 0..width {
            let mut sum = 0.0f32;
            for (ki, &kw) in kernel.iter().enumerate() {
                let sx =
                    (x as isize + ki as isize - r as isize).clamp(0, width as isize - 1) as usize;
                sum += data[row_offset + sx] * kw;
            }
            temp[row_offset + x] = sum;
        }
    }

    // Vertical pass
    let mut out = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0f32;
            for (ki, &kw) in kernel.iter().enumerate() {
                let sy =
                    (y as isize + ki as isize - r as isize).clamp(0, height as isize - 1) as usize;
                sum += temp[sy * width + x] * kw;
            }
            out[y * width + x] = sum;
        }
    }

    out
}

pub fn gaussian_blur_rgba(
    data: &[[f32; 4]],
    width: usize,
    height: usize,
    radius: f32,
) -> Vec<[f32; 4]> {
    if radius < 0.5 || width == 0 || height == 0 {
        return data.to_vec();
    }

    let r = radius.round() as usize;
    let sigma = radius * 0.5;
    let mut kernel = Vec::with_capacity(r * 2 + 1);
    let mut weight_sum = 0.0f32;

    for i in 0..=(r * 2) {
        let x = i as f32 - r as f32;
        let w = (-x * x / (2.0 * sigma * sigma)).exp();
        kernel.push(w);
        weight_sum += w;
    }
    for w in &mut kernel {
        *w /= weight_sum;
    }

    let mut temp = vec![[0.0f32; 4]; width * height];
    for y in 0..height {
        let row_offset = y * width;
        for x in 0..width {
            let mut sum = [0.0f32; 4];
            for (ki, &kw) in kernel.iter().enumerate() {
                let sx =
                    (x as isize + ki as isize - r as isize).clamp(0, width as isize - 1) as usize;
                let c = data[row_offset + sx];
                sum[0] += c[0] * kw;
                sum[1] += c[1] * kw;
                sum[2] += c[2] * kw;
                sum[3] += c[3] * kw;
            }
            temp[row_offset + x] = sum;
        }
    }

    let mut out = vec![[0.0f32; 4]; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 4];
            for (ki, &kw) in kernel.iter().enumerate() {
                let sy =
                    (y as isize + ki as isize - r as isize).clamp(0, height as isize - 1) as usize;
                let c = temp[sy * width + x];
                sum[0] += c[0] * kw;
                sum[1] += c[1] * kw;
                sum[2] += c[2] * kw;
                sum[3] += c[3] * kw;
            }
            out[y * width + x] = sum;
        }
    }

    out
}

pub fn sharpen_f32(data: &[f32], width: usize, height: usize, strength: f32) -> Vec<f32> {
    let blurred = gaussian_blur_f32(data, width, height, 1.5);
    data.iter()
        .zip(blurred.iter())
        .map(|(&orig, &blur)| (orig + strength * (orig - blur)).clamp(0.0, 1.0))
        .collect()
}

pub fn sharpen_rgba(
    data: &[[f32; 4]],
    width: usize,
    height: usize,
    strength: f32,
) -> Vec<[f32; 4]> {
    let blurred = gaussian_blur_rgba(data, width, height, 1.5);
    data.iter()
        .zip(blurred.iter())
        .map(|(&orig, &blur)| {
            [
                (orig[0] + strength * (orig[0] - blur[0])).clamp(0.0, 1.0),
                (orig[1] + strength * (orig[1] - blur[1])).clamp(0.0, 1.0),
                (orig[2] + strength * (orig[2] - blur[2])).clamp(0.0, 1.0),
                orig[3],
            ]
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levels_scalar() {
        let v0 = apply_levels_scalar(0.5, 0.0, 1.0, 1.0, 0.0, 1.0);
        assert!((v0 - 0.5).abs() < 1e-4);

        // Clamping min_in
        let v1 = apply_levels_scalar(0.2, 0.3, 1.0, 1.0, 0.0, 1.0);
        assert_eq!(v1, 0.0);

        // Invert via min_out=1.0, max_out=0.0
        let v_inv = apply_levels_scalar(0.8, 0.0, 1.0, 1.0, 1.0, 0.0);
        assert!((v_inv - 0.2).abs() < 1e-4);
    }

    #[test]
    fn test_rgb_hsl_roundtrip() {
        let (r, g, b) = (0.75, 0.35, 0.12);
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let (r2, g2, b2) = hsl_to_rgb(h, s, l);
        assert!((r - r2).abs() < 1e-3);
        assert!((g - g2).abs() < 1e-3);
        assert!((b - b2).abs() < 1e-3);
    }

    #[test]
    fn test_gaussian_blur_and_sharpen() {
        let data = vec![
            0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0,
            1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        ];
        let blurred = gaussian_blur_f32(&data, 5, 5, 1.0);
        assert_eq!(blurred.len(), 25);
        // Center should be smoothed and neighbor non-zero
        assert!(blurred[12] < 1.0);
        assert!(blurred[11] > 0.0);

        let sharpened = sharpen_f32(&data, 5, 5, 0.5);
        assert_eq!(sharpened.len(), 25);
    }
}
