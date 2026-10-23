use super::layer::{Layer, LayerStack};
use super::mask::{LayerMask, MaskBlendMode, MaskEffect, MaskType};
use super::texture_importer::PbrTextureSet;
use std::path::PathBuf;

/// Finds the directory containing real PBR materials
pub fn find_materials_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("/mnt/shared/REMoAINTER/assets/materials"),
        PathBuf::from("assets/materials"),
        PathBuf::from("../assets/materials"),
    ];
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    PathBuf::from("/mnt/shared/REMoAINTER/assets/materials")
}

/// Generates or loads a real PBR material
pub fn generate_procedural_material(
    id: usize,
    name: &str,
    base_color: [f32; 4],
    base_metallic: f32,
    base_roughness: f32,
    width: u32,
    height: u32,
) -> Layer {
    load_real_pbr_material(
        id,
        name,
        base_color,
        base_metallic,
        base_roughness,
        width,
        height,
    )
}

/// Generates a Smart Mask driven by baked Curvature and AO maps
pub fn generate_smart_mask(
    name: &str,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    width: u32,
    height: u32,
) -> LayerMask {
    let mut mask = LayerMask::new(MaskType::Black, width, height);
    let lower = name.to_lowercase();
    let is_cavity = lower.contains("cavity")
        || lower.contains("crevice")
        || lower.contains("dirt")
        || lower.contains("dust");

    mask.effects.push(MaskEffect::Generator {
        name: if is_cavity {
            "Dirt Crevices".into()
        } else {
            "Metal Edge Wear".into()
        },
        wear_level: if is_cavity { 0.5 } else { 1.0 },
        contrast: 1.2,
        dirt_amount: if is_cavity { 1.0 } else { 0.0 },
        invert: false,
        use_ao: is_cavity,
        use_curvature: !is_cavity,
        anchor_name: None,
        blend_mode: MaskBlendMode::Normal,
        opacity: 1.0,
        grunge_amount: 0.35,
        grunge_scale: 6.0,
        axis: 1,
        normal_direction: [0.0, 1.0, 0.0],
        target_color: [1.0, 0.0, 0.0, 1.0],
        tolerance: 0.2,
    });

    mask.recompute(baked_curvature, baked_ao, None, None, None, None);
    mask
}

/// Loads a real PBR material from assets/materials/<name> into a new Layer
pub fn load_real_pbr_material(
    id: usize,
    name: &str,
    default_color: [f32; 4],
    default_metallic: f32,
    default_roughness: f32,
    width: u32,
    height: u32,
) -> Layer {
    let clean_name = name
        .trim_end_matches(".png")
        .trim_end_matches(".webp")
        .trim_end_matches(".spsm")
        .trim_end_matches(".sbsar");
    let mut layer = Layer::new_fill(
        id,
        clean_name,
        default_color,
        default_metallic,
        default_roughness,
        width,
        height,
    );
    let mat_dir = find_materials_dir().join(clean_name);

    let mut loaded = false;

    // 1. Check exact directory in assets/materials
    if mat_dir.exists() {
        let set = PbrTextureSet::from_folder(&mat_dir);
        if set.base_color.is_some() || set.normal.is_some() || set.roughness.is_some() {
            let _ = set.load_into_layer(&mut layer, false);
            loaded = true;
        }
    }

    // 2. Fallback: check case-insensitive match in assets/materials
    if !loaded {
        if let Ok(entries) = std::fs::read_dir(find_materials_dir()) {
            let lower = clean_name.to_lowercase();
            for entry in entries.flatten() {
                let entry_name = entry.file_name().to_string_lossy().to_string();
                let entry_lower = entry_name.to_lowercase();
                if entry_lower == lower
                    || entry_lower.contains(&lower)
                    || lower.contains(&entry_lower)
                {
                    let set = PbrTextureSet::from_folder(&entry.path());
                    if set.base_color.is_some() || set.normal.is_some() || set.roughness.is_some() {
                        let _ = set.load_into_layer(&mut layer, false);
                        loaded = true;
                        break;
                    }
                }
            }
        }
    }

    // 3. Fallback: check shelf materials/smart_materials preview images
    if !loaded {
        let shelf_dirs = [
            "/mnt/shared/REMoAINTER/assets/shelf/materials",
            "/mnt/shared/REMoAINTER/assets/shelf/smart_materials",
            "assets/shelf/materials",
            "assets/shelf/smart_materials",
        ];
        for sdir in &shelf_dirs {
            let p_png = std::path::Path::new(sdir).join(format!("{}.png", clean_name));
            let p_webp = std::path::Path::new(sdir).join(format!("{}.webp", clean_name));
            let found = if p_png.exists() {
                Some(p_png)
            } else if p_webp.exists() {
                Some(p_webp)
            } else {
                None
            };
            if let Some(img_path) = found {
                layer = Layer::from_texture_asset(
                    id,
                    clean_name,
                    &img_path,
                    2,
                    (default_color, default_metallic, default_roughness),
                    width,
                    height,
                );
                loaded = true;
                break;
            }
        }
    }

    // 4. Fallback: Procedural surface noise so NO layer is ever flat uniform color
    if !loaded {
        let _total = (width * height) as usize;
        let w_f = width as f32;
        let h_f = height as f32;

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let x_norm = x as f32 / w_f;
                let y_norm = y as f32 / h_f;

                // Multi-scale procedural surface noise
                let n1 = (x_norm * 48.0).sin() * (y_norm * 48.0).cos();
                let n2 = ((x_norm * 128.0 + y_norm * 96.0).sin() * 0.5 + 0.5) * 0.15;
                let micro = (n1 * 0.08 + n2).clamp(-0.2, 0.2);

                // Modulate base color subtly
                let r = (default_color[0] + micro * 0.15).clamp(0.0, 1.0);
                let g = (default_color[1] + micro * 0.15).clamp(0.0, 1.0);
                let b = (default_color[2] + micro * 0.15).clamp(0.0, 1.0);
                layer.color_buffer[idx] = [r, g, b, 1.0];

                // Modulate roughness
                let rough = (default_roughness + micro * 0.35).clamp(0.04, 0.98);
                layer.roughness_buffer[idx] = rough;

                // Subtle micro-normal bump
                let nx = -micro * 2.0;
                let ny = micro * 2.0;
                let nz = 1.0f32;
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                layer.normal_buffer[idx] = [
                    (nx / len) * 0.5 + 0.5,
                    (ny / len) * 0.5 + 0.5,
                    (nz / len) * 0.5 + 0.5,
                ];
            }
        }
    }

    layer
}

/// Applies an authentic 3-tier reference PBR suite Smart Material hierarchy with real PBR maps and edge wear
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

    stack.layers.clear();

    // 1. Tier 1: Base Metal Substrate
    let base_metal_name = if name.to_lowercase().contains("copper") {
        "Copper Oxidized"
    } else if name.to_lowercase().contains("brass") {
        "Brass Antique"
    } else if name.to_lowercase().contains("gold") {
        "Gold Pure"
    } else if name.to_lowercase().contains("aluminium") || name.to_lowercase().contains("aluminum")
    {
        "Aluminium Anodized Red"
    } else if name.to_lowercase().contains("bronze") {
        "Bronze Armor"
    } else {
        "Iron Rough"
    };

    let base_metal = load_real_pbr_material(
        1,
        base_metal_name,
        [0.72, 0.74, 0.78, 1.0],
        0.95,
        0.28,
        w,
        h,
    );
    stack.layers.push(base_metal);

    // 2. Tier 2: Top Surface Coat (authentically loaded for the clicked Smart Material)
    let mut coat_layer = load_real_pbr_material(2, name, color, metallic, roughness, w, h);
    coat_layer.name = format!("Coat ({})", name);

    // 3. Black Mask with Smart Spatial Curvature Edge Wear & AO Crevices (reference Generator)
    let mut mask = LayerMask::new(MaskType::Black, w, h);
    for i in 0..total {
        let x = (i % w as usize) as f32;
        let y = (i / w as usize) as f32;

        let curv = baked_curvature
            .and_then(|c| c.get(i))
            .copied()
            .unwrap_or(0.5);
        let ao = baked_ao.and_then(|a| a.get(i)).copied().unwrap_or(1.0);

        // High-frequency pseudo-random chipping noise for realistic paint peeling
        let n1 = (x * 0.12).sin() * (y * 0.12).cos() * 0.5 + 0.5;
        let n2 = (((x * 0.31 + y * 0.27) * 12.9898).sin() * 43758.5453)
            .fract()
            .abs();
        let noise = n1 * 0.6 + n2 * 0.4;

        // Curvature > 0.52 indicates exposed convex edges; peel paint at sharp edges
        let edge_factor = if curv > 0.52 {
            ((curv - 0.52) * 7.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let edge_wear = if edge_factor > 0.05 {
            (edge_factor * (0.5 + 0.9 * noise)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Dirt accumulation in concave crevices (AO < 0.65)
        let crevice_dirt = if ao < 0.65 { (0.65 - ao) * 1.8 } else { 0.0 };

        // Mask value: 1.0 = coat visible, 0.0 = coat stripped showing underlying bare metal
        let mask_val = (1.0 - edge_wear + crevice_dirt * 0.25).clamp(0.0, 1.0);
        mask.buffer[i] = mask_val;
    }
    coat_layer.mask = Some(mask);
    stack.layers.push(coat_layer);

    // 4. Tier 3: Paint/Detail Layer for user strokes
    let user_paint = Layer::new_paint(3, "Paint Detail 1", w, h);
    stack.layers.push(user_paint);

    stack.active_layer_index = 2;
}
