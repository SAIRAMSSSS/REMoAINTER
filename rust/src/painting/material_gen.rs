use super::layer::{BlendMode, Layer, LayerStack, LayerType};
use super::mask::{LayerMask, MaskType};
use std::f32::consts::PI;

/// Deterministic fast pseudo-random hash for procedural PBR textures
#[inline(always)]
fn hash2(x: f32, y: f32) -> f32 {
    let n = (x * 12.9898 + y * 78.233).sin() * 43758.5453;
    n.fract().abs()
}

/// Smooth bilinear noise
fn smooth_noise(x: f32, y: f32) -> f32 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;

    // Smoothstep interpolation
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);

    let n00 = hash2(ix, iy);
    let n10 = hash2(ix + 1.0, iy);
    let n01 = hash2(ix, iy + 1.0);
    let n11 = hash2(ix + 1.0, iy + 1.0);

    let nx0 = n00 * (1.0 - sx) + n10 * sx;
    let nx1 = n01 * (1.0 - sx) + n11 * sx;

    nx0 * (1.0 - sy) + nx1 * sy
}

/// Multi-octave fractal Brownian motion (fBm) for organic micro-texture
fn fbm(x: f32, y: f32, octaves: usize) -> f32 {
    let mut val = 0.0f32;
    let mut amp = 0.5f32;
    let mut freq = 1.0f32;
    for _ in 0..octaves {
        val += smooth_noise(x * freq, y * freq) * amp;
        freq *= 2.05;
        amp *= 0.5;
    }
    val
}

/// Generates a seamless procedural tiling PBR material layer with authentic micro-grain, normal & height
pub fn generate_procedural_material(
    id: usize,
    name: &str,
    base_color: [f32; 4],
    base_metallic: f32,
    base_roughness: f32,
    width: u32,
    height: u32,
) -> Layer {
    let mut layer = Layer::new_fill(id, name, base_color, base_metallic, base_roughness, width, height);

    let lower = name.to_lowercase();
    let is_painted = lower.contains("paint") || lower.contains("coated") || lower.contains("enamel") || lower.contains("lacquer");
    let is_metal = (lower.contains("steel") || lower.contains("iron") || lower.contains("metal")
        || lower.contains("aluminium") || lower.contains("aluminum") || lower.contains("chrome")
        || lower.contains("gold") || lower.contains("copper") || lower.contains("bronze") || lower.contains("brass"))
        && !is_painted;
    let is_carbon = lower.contains("carbon");
    let is_leather = lower.contains("leather");
    let is_wood = lower.contains("wood");
    let is_concrete = lower.contains("concrete") || lower.contains("stone") || lower.contains("asphalt") || lower.contains("rock");
    let is_fabric = lower.contains("fabric") || lower.contains("denim") || lower.contains("cotton") || lower.contains("wool");
    let is_rust = lower.contains("rust") || lower.contains("corroded");

    // Procedural PBR texture evaluation across 1024x1024 grid
    let w_f = width as f32;
    let h_f = height as f32;

    for y in 0..height {
        let v = y as f32 / h_f;
        for x in 0..width {
            let u = x as f32 / w_f;
            let idx = (y * width + x) as usize;

            // Compute procedural surface pattern
            let (col_mod, rough_mod, met_mod, h_val) = if is_carbon {
                // 2x2 Carbon Fiber Twill Weave (16x frequency)
                let tile = 24.0;
                let tu = (u * tile).fract();
                let tv = (v * tile).fract();
                let diag = ((tu + tv) * 2.0).floor() as i32 % 2;
                let weave = if diag == 0 { 0.85f32 } else { 0.65f32 };
                let strand = ((tu * 4.0).sin().abs() * (tv * 4.0).cos().abs()) * 0.15;
                let pattern = weave + strand;
                (pattern, (0.12 + strand * 0.2).clamp(0.04, 0.95), 0.0f32, (pattern - 0.75) * 0.5)
            } else if is_metal && !is_rust {
                // Brushed Metal with continuous high-frequency hairline micro-scratches
                let grain = (smooth_noise(u * 200.0, v * 200.0) - 0.5) * 0.08;
                let hairline = (smooth_noise(u * 400.0, v * 80.0) - 0.5) * 0.06;
                let pattern = 1.0 + grain + hairline;
                let r = (base_roughness + grain * 0.3).clamp(0.05, 0.95);
                (pattern, r, base_metallic, (grain + hairline) * 0.6)
            } else if is_rust {
                // Coarse pitted rust and oxidation
                let coarse = fbm(u * 16.0, v * 16.0, 4);
                let fine_pits = hash2((u * 128.0).floor(), (v * 128.0).floor()) * 0.25;
                let pattern = (coarse * 0.75 + fine_pits).clamp(0.0, 1.0);
                let r = (0.75 + pattern * 0.20).clamp(0.05, 0.98);
                (0.85 + pattern * 0.35, r, 0.0f32, (pattern - 0.5) * 1.5)
            } else if is_leather {
                // Voronoi-like cellular pebble grain
                let cell = fbm(u * 32.0, v * 32.0, 3);
                let grain = smooth_noise(u * 64.0, v * 64.0) * 0.2;
                let pattern = cell + grain;
                (0.92 + pattern * 0.15, (base_roughness + pattern * 0.1).clamp(0.2, 0.8), 0.0, (pattern - 0.5) * 0.8)
            } else if is_wood {
                // Directional wood annual growth rings
                let ring_coord = (v * 12.0 + fbm(u * 4.0, v * 4.0, 3) * 2.5).sin();
                let fiber = smooth_noise(u * 120.0, v * 8.0) * 0.12;
                let pattern = (ring_coord * 0.15 + fiber + 0.90).clamp(0.6, 1.4);
                (pattern, (base_roughness + fiber * 0.5).clamp(0.15, 0.85), 0.0, (pattern - 1.0) * 0.6)
            } else if is_concrete {
                // Multi-scale concrete aggregate and pores
                let aggregate = fbm(u * 20.0, v * 20.0, 4);
                let pores = if hash2((u * 80.0).floor(), (v * 80.0).floor()) > 0.82 { -0.3f32 } else { 0.0f32 };
                let pattern = (aggregate + pores).clamp(0.0, 1.0);
                (0.85 + pattern * 0.25, (base_roughness + pattern * 0.15).clamp(0.3, 0.95), 0.0, (pattern - 0.5) * 1.2)
            } else if is_fabric {
                // Crossed twill textile weave
                let tu = (u * 48.0).sin();
                let tv = (v * 48.0).cos();
                let weave = (tu * tv * 0.12 + 0.90).clamp(0.7, 1.2);
                (weave, (base_roughness + weave * 0.05).clamp(0.3, 0.9), 0.0, (weave - 0.9) * 0.5)
            } else {
                // Enamel paint with subtle orange-peel micro-relief
                let orange_peel = fbm(u * 64.0, v * 64.0, 3) * 0.05;
                let pattern = 1.0 + orange_peel;
                (pattern, (base_roughness + orange_peel * 0.25).clamp(0.05, 0.95), base_metallic, orange_peel * 0.3)
            };

            // Apply base color modulation
            let r = (base_color[0] * col_mod).clamp(0.0, 1.0);
            let g = (base_color[1] * col_mod).clamp(0.0, 1.0);
            let b = (base_color[2] * col_mod).clamp(0.0, 1.0);

            layer.color_buffer[idx] = [r, g, b, 1.0];
            layer.metallic_buffer[idx] = met_mod;
            layer.roughness_buffer[idx] = rough_mod;
            layer.height_buffer[idx] = h_val;
        }
    }

    // Evaluate authentic Tangent-Space Normal Map using Sobel filter on height displacement
    let w_i = width as i32;
    let h_i = height as i32;
    let strength = 2.4f32;

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

    layer
}

/// Applies an authentic Substance 3-Tier Smart Material Hierarchy (Base Metal + Cavity Grime + Chipped Topcoat)
pub fn apply_smart_material_hierarchy(
    stack: &mut LayerStack,
    name: &str,
    color: [f32; 4],
    metallic: f32,
    roughness: f32,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
) {
    let w = stack.width;
    let h = stack.height;
    let total = (w * h) as usize;

    let lower = name.to_lowercase();

    // Determine color schemes and metal substrate based on Smart Material name
    let (top_color, base_metal_color, is_colored_paint) = if lower.contains("green") {
        ([0.22f32, 0.42, 0.26, 1.0], [0.72f32, 0.74, 0.78, 1.0], true)
    } else if lower.contains("red") {
        ([0.72f32, 0.18, 0.15, 1.0], [0.70f32, 0.72, 0.75, 1.0], true)
    } else if lower.contains("blue") {
        ([0.18f32, 0.36, 0.65, 1.0], [0.72f32, 0.74, 0.78, 1.0], true)
    } else if lower.contains("yellow") || lower.contains("machinery") {
        ([0.82f32, 0.68, 0.16, 1.0], [0.55f32, 0.58, 0.62, 1.0], true)
    } else if lower.contains("gold") {
        ([0.98f32, 0.84, 0.28, 1.0], [0.65f32, 0.45, 0.20, 1.0], false)
    } else if lower.contains("copper") {
        ([0.95f32, 0.62, 0.48, 1.0], [0.50f32, 0.28, 0.20, 1.0], false)
    } else if lower.contains("bronze") {
        ([0.78f32, 0.55, 0.34, 1.0], [0.25f32, 0.55, 0.48, 1.0], false) // Bronze with verdigris patina
    } else if lower.contains("iron") || lower.contains("old") {
        ([0.25f32, 0.26, 0.28, 1.0], [0.42f32, 0.24, 0.15, 1.0], false) // Old iron with rust
    } else {
        (color, [0.70f32, 0.72, 0.75, 1.0], metallic < 0.5) // User-selected / inferred tint
    };

    stack.layers.clear();

    // 1. TIER 1: BASE METAL LAYER (Exposed under scratches and chipped edges)
    let base_layer = generate_procedural_material(
        1,
        "Base Metal (Steel)",
        base_metal_color,
        0.95, // High metallic
        0.28, // Polished brushed metal roughness
        w,
        h,
    );
    stack.layers.push(base_layer);

    // 2. TIER 2: CAVITY GRIME & RUST LAYER (Accumulates in corners, bolts, seams & crevices via AO)
    let mut grime_layer = generate_procedural_material(
        2,
        "Cavity Grime & Rust",
        [0.28f32, 0.16, 0.11, 1.0], // Dark burnt umber rust
        0.0, // Non-metallic rust
        0.88, // Rough matte
        w,
        h,
    );

    // Mask grime layer strictly into AO crevices
    let mut grime_mask = LayerMask::new(MaskType::Black, w, h);
    for i in 0..total {
        let ao = baked_ao.and_then(|a| a.get(i)).copied().unwrap_or(1.0);
        // AO < 0.85 means cavity/crevice -> deposit dirt and rust
        let cavity = (1.0 - ao).clamp(0.0, 1.0);
        let dirt = (cavity * 1.8).clamp(0.0, 1.0);
        grime_mask.buffer[i] = dirt;
    }
    grime_layer.mask = Some(grime_mask);
    stack.layers.push(grime_layer);

    // 3. TIER 3: TOP SURFACE FINISH / PAINT COAT (Worn off at sharp edges via Curvature)
    let top_coat_name = format!("Top Coat ({})", name);
    let mut top_layer = generate_procedural_material(
        3,
        &top_coat_name,
        top_color,
        if is_colored_paint { metallic.min(0.2) } else { metallic },
        if is_colored_paint { roughness.clamp(0.2, 0.45) } else { roughness },
        w,
        h,
    );

    // Mask top layer with Curvature Edge Wear:
    // Convex ridges (curv > 0.52) chip away the paint, revealing base metal!
    let mut wear_mask = LayerMask::new(MaskType::White, w, h);
    for i in 0..total {
        let curv = baked_curvature.and_then(|c| c.get(i)).copied().unwrap_or(0.5);
        let edge_sharpness = ((curv - 0.51).max(0.0) * 4.0).clamp(0.0, 1.0);
        // Subtle micro-chipping noise along edges
        let u = (i % w as usize) as f32 / w as f32;
        let v = (i / w as usize) as f32 / h as f32;
        let chip_noise = fbm(u * 64.0, v * 64.0, 3) * 0.35;
        let wear = (edge_sharpness * 0.85 + chip_noise * edge_sharpness).clamp(0.0, 1.0);
        wear_mask.buffer[i] = (1.0 - wear).clamp(0.0, 1.0);
    }
    top_layer.mask = Some(wear_mask);
    stack.layers.push(top_layer);

    // 4. TIER 4: ACTIVE PAINT LAYER FOR USER INTERACTION
    let paint_layer = Layer::new_paint(4, "Paint Layer 1", w, h);
    stack.layers.push(paint_layer);

    stack.active_layer_index = 3; // Top paint layer active
}

/// Procedural mesh-aware Smart Mask generation (Curvature, AO, Noise, Edges, Cavities)
pub fn generate_smart_mask(
    name: &str,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    width: u32,
    height: u32,
) -> LayerMask {
    let lower = name.to_lowercase();
    let is_edge = lower.contains("edge") || lower.contains("damage") || lower.contains("wear") || lower.contains("scrape") || lower.contains("chip");
    let is_cavity = lower.contains("cavity") || lower.contains("occlusion") || lower.contains("crevice");
    let is_dust = lower.contains("dust");
    let is_scratch = lower.contains("scratch");
    let is_dirt = lower.contains("dirt") || lower.contains("ground") || lower.contains("rust") || lower.contains("leak") || lower.contains("stain");

    let mut mask = LayerMask::new(MaskType::Black, width, height);
    let w_f = width as f32;
    let h_f = height as f32;

    for y in 0..height {
        let v = y as f32 / h_f;
        for x in 0..width {
            let u = x as f32 / w_f;
            let idx = (y * width + x) as usize;

            let curv = baked_curvature.and_then(|c| c.get(idx)).copied().unwrap_or(0.5);
            let ao = baked_ao.and_then(|a| a.get(idx)).copied().unwrap_or(1.0);

            let edge_val = ((curv - 0.50).max(0.0) * 3.2).clamp(0.0, 1.0);
            let cavity_val = ((1.0 - ao) * 2.2).clamp(0.0, 1.0);
            let noise_fine = fbm(u * 48.0, v * 48.0, 3);
            let noise_coarse = fbm(u * 12.0, v * 12.0, 3);

            let mask_val = if is_edge {
                // Focus on ridges and corners with organic chipping
                let chipped = edge_val * (0.6 + noise_fine * 0.7);
                chipped.clamp(0.0, 1.0)
            } else if is_cavity {
                // Crevice accumulation
                let crevice = cavity_val * (0.7 + noise_coarse * 0.5);
                crevice.clamp(0.0, 1.0)
            } else if is_dirt {
                // Cavities + splashes and drips
                let drip = (v * 16.0 + noise_coarse * 4.0).sin().max(0.0) * 0.3;
                let dirt = cavity_val * 0.65 + noise_fine * 0.35 + drip * (1.0 - ao);
                dirt.clamp(0.0, 1.0)
            } else if is_dust {
                // Soft settling in occluded areas with fine grain
                (cavity_val * 0.5 + noise_fine * 0.3 + 0.1).clamp(0.0, 1.0)
            } else if is_scratch {
                // High frequency sharp scratch lines
                let scratch_line = (smooth_noise(u * 120.0, v * 8.0) * 10.0).fract();
                let scratch = if scratch_line > 0.88 { 0.9f32 } else { 0.0f32 };
                (scratch * (edge_val * 0.5 + 0.5)).clamp(0.0, 1.0)
            } else {
                // General smart weathering
                (edge_val * 0.5 + cavity_val * 0.5 + (noise_coarse - 0.5) * 0.3).clamp(0.0, 1.0)
            };

            mask.buffer[idx] = mask_val;
        }
    }

    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_procedural_material() {
        let mat = generate_procedural_material(
            1,
            "Steel Brushed",
            [0.7, 0.7, 0.72, 1.0],
            0.9,
            0.3,
            64,
            64,
        );
        assert_eq!(mat.color_buffer.len(), 64 * 64);
        assert_eq!(mat.normal_buffer.len(), 64 * 64);
        assert_eq!(mat.roughness_buffer.len(), 64 * 64);
        assert_eq!(mat.metallic_buffer.len(), 64 * 64);
        // Normal vectors should be normalized and in 0..1 range
        for n in &mat.normal_buffer {
            assert!(n[2] > 0.0 && n[2] <= 1.0);
        }
    }

    #[test]
    fn test_apply_smart_material_hierarchy() {
        let mut stack = LayerStack::new(64, 64);
        let curv = vec![0.7f32; 64 * 64];
        let ao = vec![0.5f32; 64 * 64];
        apply_smart_material_hierarchy(
            &mut stack,
            "Steel Painted Scraped Green",
            [0.2, 0.4, 0.2, 1.0],
            0.05,
            0.3,
            Some(&curv),
            Some(&ao),
        );
        assert_eq!(stack.layers.len(), 4);
        // Tier 3 (top coat) must have wear mask
        assert!(stack.layers[2].mask.is_some());
        // Tier 2 (crevice grime) must have cavity mask
        assert!(stack.layers[1].mask.is_some());
    }
}
