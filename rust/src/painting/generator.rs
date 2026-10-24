use super::mask::fbm_noise;
use serde::{Deserialize, Serialize};

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

/// Configuration and parameters for procedural generator layers and smart masks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorConfig {
    pub name: String,
    pub wear_level: f32,
    pub contrast: f32,
    pub dirt_amount: f32,
    pub invert: bool,
    pub use_ao: bool,
    pub use_curvature: bool,
    pub anchor_name: Option<String>,

    #[serde(default = "default_grunge_amount")]
    pub grunge_amount: f32,
    #[serde(default = "default_grunge_scale")]
    pub grunge_scale: f32,
    #[serde(default)]
    pub axis: u8, // 0: X, 1: Y, 2: Z for Position Gradient
    #[serde(default = "default_normal_dir")]
    pub normal_direction: [f32; 3], // for World Normal (default [0.0, 1.0, 0.0])
    #[serde(default = "default_target_color")]
    pub target_color: [f32; 4], // for ID selection
    #[serde(default = "default_tolerance")]
    pub tolerance: f32, // for ID selection
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self::metal_edge_wear()
    }
}

impl GeneratorConfig {
    /// Metal Edge Wear preset: highlights convex mesh curvature with optional AO occlusion.
    pub fn metal_edge_wear() -> Self {
        Self {
            name: "Metal Edge Wear".into(),
            wear_level: 0.8,
            contrast: 1.2,
            dirt_amount: 0.0,
            invert: false,
            use_ao: true,
            use_curvature: true,
            anchor_name: None,
            grunge_amount: 0.35,
            grunge_scale: 6.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        }
    }

    /// Dirt Crevices preset: accumulates dark soil in cavities and concave creases using AO & Curvature.
    pub fn dirt_crevices() -> Self {
        Self {
            name: "Dirt Crevices".into(),
            wear_level: 0.5,
            contrast: 1.3,
            dirt_amount: 0.8,
            invert: false,
            use_ao: true,
            use_curvature: true,
            anchor_name: None,
            grunge_amount: 0.4,
            grunge_scale: 8.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        }
    }

    /// Dust Accumulation preset: deposits dust particles onto upward-facing surfaces with cavity occlusion.
    pub fn dust_accumulation() -> Self {
        Self {
            name: "Dust Accumulation".into(),
            wear_level: 0.7,
            contrast: 1.1,
            dirt_amount: 0.3,
            invert: false,
            use_ao: true,
            use_curvature: false,
            anchor_name: None,
            grunge_amount: 0.25,
            grunge_scale: 5.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        }
    }

    /// Position Gradient preset: linear height/width ramp along mesh bounding box X, Y, or Z axis.
    pub fn position_gradient(axis: u8) -> Self {
        Self {
            name: "Position Gradient".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: None,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: axis.min(2),
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        }
    }

    /// World Normal Orientation preset: directional surface orientation alignment (e.g. snow, sun bleach).
    pub fn world_normal(direction: [f32; 3]) -> Self {
        let len = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt()
            .max(1e-6);
        Self {
            name: "World Normal Orientation".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: None,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [direction[0] / len, direction[1] / len, direction[2] / len],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        }
    }

    /// Thickness / Subsurface preset: inverse thickness scattering factor for translucent or worn edges.
    pub fn thickness() -> Self {
        Self {
            name: "Thickness / Subsurface".into(),
            wear_level: 0.8,
            contrast: 1.2,
            dirt_amount: 0.5,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: None,
            grunge_amount: 0.2,
            grunge_scale: 4.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: [1.0, 0.0, 0.0, 1.0],
            tolerance: 0.2,
        }
    }

    /// Color / ID Selection preset: isolates a specific baked ID color with distance tolerance.
    pub fn color_id_selection(target: [f32; 4], tolerance: f32) -> Self {
        Self {
            name: "Color / ID Selection".into(),
            wear_level: 1.0,
            contrast: 1.0,
            dirt_amount: 0.0,
            invert: false,
            use_ao: false,
            use_curvature: false,
            anchor_name: None,
            grunge_amount: 0.0,
            grunge_scale: 1.0,
            axis: 1,
            normal_direction: [0.0, 1.0, 0.0],
            target_color: target,
            tolerance: tolerance.max(0.001),
        }
    }
}

/// Decodes world space normals from either [0.0, 1.0] texture-encoded RGB space
/// (as output by the baking engine) or signed [-1.0, 1.0] direction vectors.
#[inline]
pub fn decode_wsn_normal(raw: [f32; 3]) -> [f32; 3] {
    if raw[0] < 0.0 || raw[1] < 0.0 || raw[2] < 0.0 {
        return raw;
    }
    let len_raw_sq = raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2];
    let dec = [raw[0] * 2.0 - 1.0, raw[1] * 2.0 - 1.0, raw[2] * 2.0 - 1.0];
    let len_dec_sq = dec[0] * dec[0] + dec[1] * dec[1] + dec[2] * dec[2];
    if (len_dec_sq - 1.0).abs() < (len_raw_sq - 1.0).abs() {
        dec
    } else {
        raw
    }
}

/// Evaluates a procedural generator's intensity in [0.0, 1.0] at a single pixel.
#[inline]
pub fn evaluate_generator_pixel(
    config: &GeneratorConfig,
    idx: usize,
    u_coord: f32,
    v_coord: f32,
    pos_min: [f32; 3],
    pos_max: [f32; 3],
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    baked_wsn: Option<&[[f32; 3]]>,
    baked_pos: Option<&[[f32; 3]]>,
    baked_thickness: Option<&[f32]>,
    baked_id: Option<&[[f32; 4]]>,
    anchor_signal: Option<f32>,
) -> f32 {
    let gen_lower = config.name.to_lowercase();
    let is_dust = gen_lower.contains("dust");
    let is_gradient = gen_lower.contains("gradient") || gen_lower.contains("position");
    let is_orientation = gen_lower.contains("normal") || gen_lower.contains("world");
    let is_thickness = gen_lower.contains("thickness");
    let is_id = gen_lower.contains("id") || gen_lower.contains("color selection");
    let is_dirt =
        gen_lower.contains("dirt") || gen_lower.contains("cavity") || gen_lower.contains("crevice");

    let mut gen_val = 0.0f32;

    // 1. Position Gradient along mesh bounding box axis
    if is_gradient {
        let t = if let Some(pos_slice) = baked_pos {
            if idx < pos_slice.len() {
                let ax = (config.axis as usize).min(2);
                let range = (pos_max[ax] - pos_min[ax]).max(1e-5);
                (pos_slice[idx][ax] - pos_min[ax]) / range
            } else {
                match config.axis {
                    0 => u_coord,
                    1 => 1.0 - v_coord,
                    _ => (u_coord + v_coord) * 0.5,
                }
            }
        } else {
            match config.axis {
                0 => u_coord,
                1 => 1.0 - v_coord,
                _ => (u_coord + v_coord) * 0.5,
            }
        };
        gen_val = t.clamp(0.0, 1.0);
    }
    // 2. World Space Normal Orientation (dot product)
    else if is_orientation {
        let n = if let Some(wsn) = baked_wsn {
            wsn.get(idx)
                .map(|&v| decode_wsn_normal(v))
                .unwrap_or([0.0, 1.0, 0.0])
        } else {
            [0.0, 1.0, 0.0]
        };
        let dir = config.normal_direction;
        let dir_len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let norm_dir = if dir_len > 1e-5 {
            [dir[0] / dir_len, dir[1] / dir_len, dir[2] / dir_len]
        } else {
            [0.0, 1.0, 0.0]
        };
        let dot = n[0] * norm_dir[0] + n[1] * norm_dir[1] + n[2] * norm_dir[2];
        gen_val = (dot * 0.5 + 0.5).clamp(0.0, 1.0);
    }
    // 3. Dust Accumulation (Upward facing + ambient occlusion factor)
    else if is_dust {
        let ny = if let Some(wsn) = baked_wsn {
            wsn.get(idx)
                .map(|&v| decode_wsn_normal(v)[1])
                .unwrap_or(1.0)
        } else {
            1.0
        };
        let up_facing = ny.max(0.0);
        let ao_fac = if let Some(ao) = baked_ao {
            ao.get(idx).copied().unwrap_or(1.0)
        } else {
            1.0
        };
        gen_val = up_facing * (0.6 + 0.4 * (1.0 - ao_fac)) * config.wear_level;
    }
    // 4. Thickness / Subsurface
    else if is_thickness {
        let th = if let Some(thick) = baked_thickness {
            thick.get(idx).copied().unwrap_or(0.5)
        } else {
            0.5
        };
        gen_val = (1.0 - th) * config.dirt_amount + th * config.wear_level;
    }
    // 5. Color / ID Selection
    else if is_id {
        if let Some(ids) = baked_id {
            if idx < ids.len() {
                let c = ids[idx];
                let dr = c[0] - config.target_color[0];
                let dg = c[1] - config.target_color[1];
                let db = c[2] - config.target_color[2];
                let dist = (dr * dr + dg * dg + db * db).sqrt();
                let tol = config.tolerance.max(0.001);
                if dist <= tol {
                    gen_val = (1.0 - dist / tol).clamp(0.0, 1.0);
                }
            }
        }
    }
    // 6. Dirt Crevices
    else if is_dirt {
        if config.use_ao {
            if let Some(ao) = baked_ao {
                if idx < ao.len() {
                    let a = ao[idx];
                    gen_val += (1.0 - a) * config.dirt_amount;
                }
            }
        }
        if config.use_curvature {
            if let Some(curv) = baked_curvature {
                if idx < curv.len() {
                    let c = curv[idx];
                    if c < 0.5 {
                        gen_val += (0.5 - c) * 2.0 * config.wear_level;
                    }
                }
            }
        }
    }
    // 7. Metal Edge Wear (Default)
    else {
        if config.use_curvature {
            if let Some(curv) = baked_curvature {
                if idx < curv.len() {
                    let c = curv[idx];
                    if c > 0.5 {
                        gen_val += (c - 0.5) * 2.0 * config.wear_level;
                    }
                }
            }
        }
        if config.use_ao {
            if let Some(ao) = baked_ao {
                if idx < ao.len() {
                    let a = ao[idx];
                    gen_val *= a;
                }
            }
        }
    }

    // Anchor Point modulation / injection if referenced
    if let Some(asig) = anchor_signal {
        gen_val = (gen_val + asig * config.wear_level).clamp(0.0, 1.0);
    }

    // Modulate with multi-octave procedural grunge noise
    if config.grunge_amount > 0.001 && !is_id {
        let g_scale = config.grunge_scale;
        let noise = fbm_noise(u_coord * g_scale, v_coord * g_scale, 3);
        let mod_factor = (1.0 - config.grunge_amount) + noise * config.grunge_amount * 1.5;
        gen_val *= mod_factor;
    }

    // Apply Contrast Curve
    gen_val = ((gen_val - 0.5) * config.contrast + 0.5).clamp(0.0, 1.0);
    if config.invert {
        gen_val = 1.0 - gen_val;
    }

    gen_val.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metal_edge_wear_evaluation() {
        let config = GeneratorConfig::metal_edge_wear();
        let curv = [0.9, 0.5, 0.1]; // pixel 0: convex edge, 1: flat, 2: concave
        let ao = [1.0, 1.0, 0.3];

        let p0 = evaluate_generator_pixel(
            &config,
            0,
            0.1,
            0.1,
            [0.0; 3],
            [1.0; 3],
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            None,
        );
        let p1 = evaluate_generator_pixel(
            &config,
            1,
            0.5,
            0.5,
            [0.0; 3],
            [1.0; 3],
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            None,
        );
        let p2 = evaluate_generator_pixel(
            &config,
            2,
            0.9,
            0.9,
            [0.0; 3],
            [1.0; 3],
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            None,
        );

        assert!(
            p0 > p1,
            "Convex edge should produce higher wear than flat surface"
        );
        assert!(
            p1 >= p2,
            "Flat surface should produce higher/equal wear than crevice in edge wear"
        );
    }

    #[test]
    fn test_dirt_crevices_evaluation() {
        let config = GeneratorConfig::dirt_crevices();
        let curv = [0.9, 0.5, 0.1]; // pixel 0: convex, 1: flat, 2: concave crevice
        let ao = [1.0, 0.8, 0.1]; // pixel 2: deep occlusion

        let p0 = evaluate_generator_pixel(
            &config,
            0,
            0.1,
            0.1,
            [0.0; 3],
            [1.0; 3],
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            None,
        );
        let p2 = evaluate_generator_pixel(
            &config,
            2,
            0.9,
            0.9,
            [0.0; 3],
            [1.0; 3],
            Some(&curv),
            Some(&ao),
            None,
            None,
            None,
            None,
            None,
        );

        assert!(
            p2 > p0,
            "Concave crevice with deep occlusion must accumulate more dirt than convex edge"
        );
    }

    #[test]
    fn test_position_gradient_and_axis() {
        let config_y = GeneratorConfig::position_gradient(1);
        let pos = [[0.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 10.0, 0.0]];

        let val_bottom = evaluate_generator_pixel(
            &config_y,
            0,
            0.0,
            0.0,
            [0.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            None,
            None,
            None,
            Some(&pos),
            None,
            None,
            None,
        );
        let val_mid = evaluate_generator_pixel(
            &config_y,
            1,
            0.0,
            0.5,
            [0.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            None,
            None,
            None,
            Some(&pos),
            None,
            None,
            None,
        );
        let val_top = evaluate_generator_pixel(
            &config_y,
            2,
            0.0,
            1.0,
            [0.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            None,
            None,
            None,
            Some(&pos),
            None,
            None,
            None,
        );

        assert_eq!(val_bottom, 0.0);
        assert!((val_mid - 0.5).abs() < 1e-4);
        assert_eq!(val_top, 1.0);
    }

    #[test]
    fn test_world_normal_dust_accumulation() {
        let config_dust = GeneratorConfig::dust_accumulation();
        let wsn = [
            [0.0, 1.0, 0.0],  // facing straight up
            [1.0, 0.0, 0.0],  // facing horizontal side
            [0.0, -1.0, 0.0], // facing downwards
        ];

        let val_up = evaluate_generator_pixel(
            &config_dust,
            0,
            0.1,
            0.1,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            Some(&wsn),
            None,
            None,
            None,
            None,
        );
        let val_down = evaluate_generator_pixel(
            &config_dust,
            2,
            0.9,
            0.9,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            Some(&wsn),
            None,
            None,
            None,
            None,
        );

        assert!(val_up > 0.0, "Upward surface should accumulate dust");
        assert_eq!(
            val_down, 0.0,
            "Downward facing surface should accumulate no dust"
        );
    }

    #[test]
    fn test_color_id_selection() {
        let config_id = GeneratorConfig::color_id_selection([1.0, 0.0, 0.0, 1.0], 0.1);
        let id_map = [
            [1.0, 0.0, 0.0, 1.0], // exact match
            [0.0, 1.0, 0.0, 1.0], // green mismatch
        ];

        let val_match = evaluate_generator_pixel(
            &config_id,
            0,
            0.0,
            0.0,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            None,
            None,
            None,
            Some(&id_map),
            None,
        );
        let val_mismatch = evaluate_generator_pixel(
            &config_id,
            1,
            0.5,
            0.5,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            None,
            None,
            None,
            Some(&id_map),
            None,
        );

        assert_eq!(val_match, 1.0);
        assert_eq!(val_mismatch, 0.0);
    }

    #[test]
    fn test_encoded_baker_world_space_normal_dust_and_orientation() {
        // Baker encodes normals into [0.0, 1.0] texture space: (norm * 0.5 + 0.5)
        let config_dust = GeneratorConfig::dust_accumulation();
        let encoded_wsn = [
            [0.5, 1.0, 0.5], // encoded [0.0, 1.0, 0.0] facing straight up
            [1.0, 0.5, 0.5], // encoded [1.0, 0.0, 0.0] vertical wall
            [0.5, 0.0, 0.5], // encoded [0.0, -1.0, 0.0] ceiling facing down
        ];

        let val_up = evaluate_generator_pixel(
            &config_dust,
            0,
            0.1,
            0.1,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            Some(&encoded_wsn),
            None,
            None,
            None,
            None,
        );
        let val_wall = evaluate_generator_pixel(
            &config_dust,
            1,
            0.5,
            0.5,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            Some(&encoded_wsn),
            None,
            None,
            None,
            None,
        );
        let val_down = evaluate_generator_pixel(
            &config_dust,
            2,
            0.9,
            0.9,
            [0.0; 3],
            [1.0; 3],
            None,
            None,
            Some(&encoded_wsn),
            None,
            None,
            None,
            None,
        );

        assert!(val_up > 0.0, "Upward surface must accumulate dust");
        assert_eq!(val_wall, 0.0, "Vertical wall must accumulate zero dust");
        assert_eq!(val_down, 0.0, "Ceiling must accumulate zero dust");
    }
}
