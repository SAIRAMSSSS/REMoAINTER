use super::generator::GeneratorConfig;
use super::layer::{Layer, LayerStack};
use super::mask::{LayerMask, MaskBlendMode, MaskEffect, MaskType};
use super::texture_importer::PbrTextureSet;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Finds the directory containing real PBR materials
pub fn find_materials_dir() -> PathBuf {
    crate::paths::find_assets_subdir("materials").unwrap_or_default()
}

/// Built-in Smart Mask Presets driven by baked mesh maps and noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartMaskPreset {
    MetalEdgeWear,
    DirtCrevices,
    DustAccumulation,
    PaintChipping,
    CavityRust,
    GroundDirtGradient,
    SubsurfaceThickness,
}

impl SmartMaskPreset {
    pub const ALL: [SmartMaskPreset; 7] = [
        SmartMaskPreset::MetalEdgeWear,
        SmartMaskPreset::DirtCrevices,
        SmartMaskPreset::DustAccumulation,
        SmartMaskPreset::PaintChipping,
        SmartMaskPreset::CavityRust,
        SmartMaskPreset::GroundDirtGradient,
        SmartMaskPreset::SubsurfaceThickness,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            SmartMaskPreset::MetalEdgeWear => "Metal Edge Wear",
            SmartMaskPreset::DirtCrevices => "Dirt Crevices",
            SmartMaskPreset::DustAccumulation => "Dust Accumulation",
            SmartMaskPreset::PaintChipping => "Paint Chipping",
            SmartMaskPreset::CavityRust => "Cavity Rust",
            SmartMaskPreset::GroundDirtGradient => "Ground Dirt Gradient",
            SmartMaskPreset::SubsurfaceThickness => "Subsurface Thickness",
        }
    }

    pub fn to_generator_config(&self) -> GeneratorConfig {
        match self {
            SmartMaskPreset::MetalEdgeWear => GeneratorConfig::metal_edge_wear(),
            SmartMaskPreset::DirtCrevices => GeneratorConfig::dirt_crevices(),
            SmartMaskPreset::DustAccumulation => GeneratorConfig::dust_accumulation(),
            SmartMaskPreset::PaintChipping => GeneratorConfig {
                name: "Paint Chipping".into(),
                wear_level: 0.75,
                contrast: 1.5,
                dirt_amount: 0.1,
                invert: false,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.55,
                grunge_scale: 14.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            SmartMaskPreset::CavityRust => GeneratorConfig {
                name: "Cavity Rust".into(),
                wear_level: 0.6,
                contrast: 1.35,
                dirt_amount: 0.85,
                invert: false,
                use_ao: true,
                use_curvature: false,
                anchor_name: None,
                grunge_amount: 0.45,
                grunge_scale: 7.5,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            SmartMaskPreset::GroundDirtGradient => GeneratorConfig {
                name: "Ground Dirt Gradient".into(),
                wear_level: 0.8,
                contrast: 1.2,
                dirt_amount: 0.5,
                invert: true,
                use_ao: false,
                use_curvature: false,
                anchor_name: None,
                grunge_amount: 0.3,
                grunge_scale: 4.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            SmartMaskPreset::SubsurfaceThickness => GeneratorConfig::thickness(),
        }
    }

    pub fn from_name_fuzzy(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("chip") || lower.contains("peel") || lower.contains("flake") {
            SmartMaskPreset::PaintChipping
        } else if lower.contains("rust") || lower.contains("corro") {
            SmartMaskPreset::CavityRust
        } else if lower.contains("dust") || lower.contains("sand") || lower.contains("snow") {
            SmartMaskPreset::DustAccumulation
        } else if lower.contains("ground") || lower.contains("gradient") || lower.contains("height")
        {
            SmartMaskPreset::GroundDirtGradient
        } else if lower.contains("thick") || lower.contains("subsurface") || lower.contains("sss") {
            SmartMaskPreset::SubsurfaceThickness
        } else if lower.contains("cavity")
            || lower.contains("crevice")
            || lower.contains("dirt")
            || lower.contains("grime")
        {
            SmartMaskPreset::DirtCrevices
        } else {
            SmartMaskPreset::MetalEdgeWear
        }
    }

    pub fn create_mask(
        &self,
        width: u32,
        height: u32,
        baked_curvature: Option<&[f32]>,
        baked_ao: Option<&[f32]>,
        baked_wsn: Option<&[[f32; 3]]>,
        baked_pos: Option<&[[f32; 3]]>,
        baked_thickness: Option<&[f32]>,
        baked_id: Option<&[[f32; 4]]>,
    ) -> LayerMask {
        let config = self.to_generator_config();
        let mut mask = LayerMask::new(MaskType::Black, width, height);
        mask.effects.push(MaskEffect::Generator {
            name: config.name.clone(),
            wear_level: config.wear_level,
            contrast: config.contrast,
            dirt_amount: config.dirt_amount,
            invert: config.invert,
            use_ao: config.use_ao,
            use_curvature: config.use_curvature,
            anchor_name: config.anchor_name,
            blend_mode: MaskBlendMode::Normal,
            opacity: 1.0,
            grunge_amount: config.grunge_amount,
            grunge_scale: config.grunge_scale,
            axis: config.axis,
            normal_direction: config.normal_direction,
            target_color: config.target_color,
            tolerance: config.tolerance,
        });
        mask.recompute(
            baked_curvature,
            baked_ao,
            baked_wsn,
            baked_pos,
            baked_thickness,
            baked_id,
        );
        mask
    }
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

/// Generates a Smart Mask driven by baked Curvature and AO maps (backward-compatible)
pub fn generate_smart_mask(
    name: &str,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    width: u32,
    height: u32,
) -> LayerMask {
    let preset = SmartMaskPreset::from_name_fuzzy(name);
    preset.create_mask(
        width,
        height,
        baked_curvature,
        baked_ao,
        None,
        None,
        None,
        None,
    )
}

/// Generates a Smart Mask driven by all available baked mesh maps
pub fn generate_smart_mask_full(
    name: &str,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    baked_wsn: Option<&[[f32; 3]]>,
    baked_pos: Option<&[[f32; 3]]>,
    baked_thickness: Option<&[f32]>,
    baked_id: Option<&[[f32; 4]]>,
    width: u32,
    height: u32,
) -> LayerMask {
    let preset = SmartMaskPreset::from_name_fuzzy(name);
    preset.create_mask(
        width,
        height,
        baked_curvature,
        baked_ao,
        baked_wsn,
        baked_pos,
        baked_thickness,
        baked_id,
    )
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
        let shelf_dirs: Vec<PathBuf> = ["shelf/materials", "shelf/smart_materials"]
            .iter()
            .filter_map(|sub| crate::paths::find_assets_subdir(sub))
            .collect();
        for sdir in &shelf_dirs {
            let p_png = sdir.join(format!("{}.png", clean_name));
            let p_webp = sdir.join(format!("{}.webp", clean_name));
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

/// Built-in Smart Material Presets bundling multi-layer folder hierarchies
/// (substrates, wear masks, cavity dirt generators, and highlight coats).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartMaterialPreset {
    SteelPaintedWorn,
    MachineryHeavyDuty,
    BrassAntique,
    BronzeArmor,
    AluminiumAnodizedRed,
    IronRoughRust,
    PlasticScratchedRough,
    GoldAntiqueWorn,
    WoodVarnishedVintage,
}

impl SmartMaterialPreset {
    pub const ALL: [SmartMaterialPreset; 9] = [
        SmartMaterialPreset::SteelPaintedWorn,
        SmartMaterialPreset::MachineryHeavyDuty,
        SmartMaterialPreset::BrassAntique,
        SmartMaterialPreset::BronzeArmor,
        SmartMaterialPreset::AluminiumAnodizedRed,
        SmartMaterialPreset::IronRoughRust,
        SmartMaterialPreset::PlasticScratchedRough,
        SmartMaterialPreset::GoldAntiqueWorn,
        SmartMaterialPreset::WoodVarnishedVintage,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            SmartMaterialPreset::SteelPaintedWorn => "Steel Painted Worn",
            SmartMaterialPreset::MachineryHeavyDuty => "Machinery Heavy Duty",
            SmartMaterialPreset::BrassAntique => "Brass Antique",
            SmartMaterialPreset::BronzeArmor => "Bronze Armor",
            SmartMaterialPreset::AluminiumAnodizedRed => "Aluminium Anodized Red",
            SmartMaterialPreset::IronRoughRust => "Iron Rough Rust",
            SmartMaterialPreset::PlasticScratchedRough => "Plastic Scratched Rough",
            SmartMaterialPreset::GoldAntiqueWorn => "Gold Antique Worn",
            SmartMaterialPreset::WoodVarnishedVintage => "Wood Varnished Vintage",
        }
    }

    pub fn from_name_fuzzy(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("machin")
            || lower.contains("heavy")
            || lower.contains("caterpillar")
            || lower.contains("yellow")
        {
            SmartMaterialPreset::MachineryHeavyDuty
        } else if lower.contains("brass") {
            SmartMaterialPreset::BrassAntique
        } else if lower.contains("bronze") || lower.contains("armor") || lower.contains("armour") {
            SmartMaterialPreset::BronzeArmor
        } else if lower.contains("alumin") || lower.contains("anodiz") {
            SmartMaterialPreset::AluminiumAnodizedRed
        } else if lower.contains("rust") || lower.contains("iron") {
            SmartMaterialPreset::IronRoughRust
        } else if lower.contains("plastic") || lower.contains("polymer") {
            SmartMaterialPreset::PlasticScratchedRough
        } else if lower.contains("gold") {
            SmartMaterialPreset::GoldAntiqueWorn
        } else if lower.contains("wood") || lower.contains("varnish") || lower.contains("walnut") {
            SmartMaterialPreset::WoodVarnishedVintage
        } else if lower.contains("copper") || lower.contains("patina") {
            SmartMaterialPreset::BronzeArmor
        } else {
            SmartMaterialPreset::SteelPaintedWorn
        }
    }
}

/// Builds an authentic multi-layer Smart Material folder hierarchy
/// returning layers in stack order (lowest child first, folder container on top).
pub fn create_smart_material_hierarchy(
    preset: SmartMaterialPreset,
    start_id: usize,
    width: u32,
    height: u32,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    baked_wsn: Option<&[[f32; 3]]>,
    baked_pos: Option<&[[f32; 3]]>,
    baked_thickness: Option<&[f32]>,
    baked_id: Option<&[[f32; 4]]>,
) -> Vec<Layer> {
    let mut layers = Vec::new();

    // Determine if preset has an optional 4th child (surface dust)
    let has_dust = matches!(
        preset,
        SmartMaterialPreset::MachineryHeavyDuty
            | SmartMaterialPreset::PlasticScratchedRough
            | SmartMaterialPreset::GoldAntiqueWorn
    );
    let child_count = if has_dust { 4 } else { 3 };
    let folder_id = start_id + child_count;

    // Preset configurations:
    let (
        sub_name,
        sub_color,
        sub_met,
        sub_rough,
        coat_name,
        coat_color,
        coat_met,
        coat_rough,
        coat_height,
        coat_mask_cfg,
        dirt_name,
        dirt_color,
        dirt_met,
        dirt_rough,
        dirt_height,
        dirt_cfg,
        dust_info,
    ) = match preset {
        SmartMaterialPreset::SteelPaintedWorn => (
            "Steel Substrate",
            [0.68, 0.70, 0.72, 1.0],
            0.95,
            0.28,
            "Weathered Paint Coat",
            [0.18, 0.26, 0.38, 1.0], // Dark industrial slate navy
            0.0,
            0.58,
            0.02,
            GeneratorConfig {
                name: "Metal Edge Wear".into(),
                wear_level: 0.95,
                contrast: 1.8,
                dirt_amount: 0.0,
                invert: true, // Coat is present on flats, stripped on convex edges
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.3,
                grunge_scale: 8.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Cavity Grime",
            [0.12, 0.10, 0.08, 1.0],
            0.0,
            0.92,
            -0.05,
            GeneratorConfig::dirt_crevices(),
            None,
        ),
        SmartMaterialPreset::MachineryHeavyDuty => (
            "Cast Iron Substrate",
            [0.45, 0.46, 0.48, 1.0],
            0.90,
            0.42,
            "Machinery Yellow Coat",
            [0.92, 0.65, 0.08, 1.0], // Warning yellow
            0.0,
            0.48,
            0.03,
            GeneratorConfig {
                name: "Metal Edge Wear".into(),
                wear_level: 0.85,
                contrast: 1.4,
                dirt_amount: 0.0,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.4,
                grunge_scale: 7.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Grease & Crevice Grime",
            [0.06, 0.05, 0.04, 1.0],
            0.1,
            0.75,
            -0.04,
            GeneratorConfig::dirt_crevices(),
            Some((
                "Factory Dust",
                [0.60, 0.55, 0.48, 1.0],
                0.0,
                0.95,
                0.01,
                GeneratorConfig::dust_accumulation(),
                0.4,
            )),
        ),
        SmartMaterialPreset::BrassAntique => (
            "Oxidized Patina Base",
            [0.25, 0.22, 0.16, 1.0],
            0.85,
            0.65,
            "Polished Brass Surface",
            [0.88, 0.72, 0.35, 1.0],
            0.92,
            0.22,
            0.01,
            GeneratorConfig {
                name: "Edge Highlight".into(),
                wear_level: 0.8,
                contrast: 1.25,
                dirt_amount: 0.0,
                invert: false, // Bright brass shines on exposed convex edges and high-wear spots
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.3,
                grunge_scale: 6.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Verdigris Tarnish",
            [0.15, 0.30, 0.24, 1.0],
            0.25,
            0.85,
            -0.03,
            GeneratorConfig::dirt_crevices(),
            None,
        ),
        SmartMaterialPreset::BronzeArmor => (
            "Dark Copper Substrate",
            [0.32, 0.18, 0.12, 1.0],
            0.88,
            0.58,
            "Hammered Bronze Coat",
            [0.78, 0.52, 0.30, 1.0],
            0.90,
            0.32,
            0.02,
            GeneratorConfig {
                name: "Metal Edge Wear".into(),
                wear_level: 0.78,
                contrast: 1.3,
                dirt_amount: 0.0,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.35,
                grunge_scale: 7.5,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Ancient Cavity Dirt",
            [0.10, 0.08, 0.06, 1.0],
            0.0,
            0.95,
            -0.06,
            GeneratorConfig::dirt_crevices(),
            None,
        ),
        SmartMaterialPreset::AluminiumAnodizedRed => (
            "Brushed Aluminium Base",
            [0.91, 0.92, 0.93, 1.0],
            0.96,
            0.20,
            "Anodized Red Surface",
            [0.82, 0.08, 0.12, 1.0],
            0.85,
            0.30,
            0.015,
            GeneratorConfig {
                name: "Paint Chipping".into(),
                wear_level: 0.78,
                contrast: 1.5,
                dirt_amount: 0.1,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.55,
                grunge_scale: 14.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Fine Crevice Dirt",
            [0.14, 0.12, 0.11, 1.0],
            0.0,
            0.90,
            -0.03,
            GeneratorConfig::dirt_crevices(),
            None,
        ),
        SmartMaterialPreset::IronRoughRust => (
            "Heavy Rust Substrate",
            [0.45, 0.18, 0.08, 1.0],
            0.15,
            0.88,
            "Rough Iron Surface",
            [0.38, 0.39, 0.40, 1.0],
            0.85,
            0.45,
            0.03,
            GeneratorConfig {
                name: "Metal Edge Wear".into(),
                wear_level: 0.7,
                contrast: 1.4,
                dirt_amount: 0.0,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.45,
                grunge_scale: 8.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Crevice Corrosion",
            [0.28, 0.10, 0.04, 1.0],
            0.05,
            0.95,
            -0.08,
            GeneratorConfig::dirt_crevices(),
            None,
        ),
        SmartMaterialPreset::PlasticScratchedRough => (
            "Dark Polymer Substrate",
            [0.12, 0.12, 0.13, 1.0],
            0.0,
            0.82,
            "Gloss Polymer Coat",
            [0.10, 0.50, 0.85, 1.0], // Vibrant Cyan Blue
            0.0,
            0.25,
            0.02,
            GeneratorConfig {
                name: "Edge Wear".into(),
                wear_level: 0.72,
                contrast: 1.35,
                dirt_amount: 0.0,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.45,
                grunge_scale: 10.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Cavity Grime",
            [0.08, 0.08, 0.08, 1.0],
            0.0,
            0.90,
            -0.03,
            GeneratorConfig::dirt_crevices(),
            Some((
                "Surface Dust",
                [0.55, 0.52, 0.48, 1.0],
                0.0,
                0.95,
                0.01,
                GeneratorConfig::dust_accumulation(),
                0.45,
            )),
        ),
        SmartMaterialPreset::GoldAntiqueWorn => (
            "Burnished Bronze Base",
            [0.52, 0.40, 0.18, 1.0],
            0.90,
            0.50,
            "High-Polish Gold Surface",
            [1.00, 0.82, 0.32, 1.0],
            0.98,
            0.15,
            0.015,
            GeneratorConfig {
                name: "Metal Edge Wear".into(),
                wear_level: 0.75,
                contrast: 1.3,
                dirt_amount: 0.0,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.3,
                grunge_scale: 6.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Antique Crevice Patina",
            [0.12, 0.09, 0.05, 1.0],
            0.20,
            0.92,
            -0.04,
            GeneratorConfig::dirt_crevices(),
            Some((
                "Museum Dust",
                [0.62, 0.58, 0.52, 1.0],
                0.0,
                0.95,
                0.01,
                GeneratorConfig::dust_accumulation(),
                0.35,
            )),
        ),
        SmartMaterialPreset::WoodVarnishedVintage => (
            "Natural Walnut Substrate",
            [0.32, 0.18, 0.09, 1.0],
            0.0,
            0.78,
            "Amber Varnish Surface",
            [0.55, 0.28, 0.10, 1.0],
            0.0,
            0.22,
            0.02,
            GeneratorConfig {
                name: "Varnish Edge Wear".into(),
                wear_level: 0.78,
                contrast: 1.35,
                dirt_amount: 0.0,
                invert: true,
                use_ao: true,
                use_curvature: true,
                anchor_name: None,
                grunge_amount: 0.4,
                grunge_scale: 8.0,
                axis: 1,
                normal_direction: [0.0, 1.0, 0.0],
                target_color: [1.0, 0.0, 0.0, 1.0],
                tolerance: 0.2,
            },
            "Wax & Crevice Grime",
            [0.08, 0.05, 0.03, 1.0],
            0.0,
            0.90,
            -0.05,
            GeneratorConfig::dirt_crevices(),
            None,
        ),
    };

    // 1. Child 1: Substrate Base Metal/Material
    let mut sub_layer = load_real_pbr_material(
        start_id, sub_name, sub_color, sub_met, sub_rough, width, height,
    );
    sub_layer.parent_folder_id = Some(folder_id);
    sub_layer.color_tag = Some(2); // Orange badge for smart material components
    layers.push(sub_layer);

    // 2. Child 2: Surface Coat with Smart Mask (Edge Wear / Paint Chipping)
    let mut coat_layer = Layer::new_fill(
        start_id + 1,
        coat_name,
        coat_color,
        coat_met,
        coat_rough,
        width,
        height,
    );
    coat_layer.fill_height = coat_height;
    coat_layer.parent_folder_id = Some(folder_id);
    coat_layer.color_tag = Some(2);

    let mut coat_mask = LayerMask::new(MaskType::Black, width, height);
    coat_mask.effects.push(MaskEffect::Generator {
        name: coat_mask_cfg.name.clone(),
        wear_level: coat_mask_cfg.wear_level,
        contrast: coat_mask_cfg.contrast,
        dirt_amount: coat_mask_cfg.dirt_amount,
        invert: coat_mask_cfg.invert,
        use_ao: coat_mask_cfg.use_ao,
        use_curvature: coat_mask_cfg.use_curvature,
        anchor_name: coat_mask_cfg.anchor_name,
        blend_mode: MaskBlendMode::Normal,
        opacity: 1.0,
        grunge_amount: coat_mask_cfg.grunge_amount,
        grunge_scale: coat_mask_cfg.grunge_scale,
        axis: coat_mask_cfg.axis,
        normal_direction: coat_mask_cfg.normal_direction,
        target_color: coat_mask_cfg.target_color,
        tolerance: coat_mask_cfg.tolerance,
    });
    coat_mask.recompute(
        baked_curvature,
        baked_ao,
        baked_wsn,
        baked_pos,
        baked_thickness,
        baked_id,
    );
    coat_layer.mask = Some(coat_mask);
    layers.push(coat_layer);

    // 3. Child 3: Cavity Dirt Grime Generator Layer
    let mut dirt_layer = Layer::new_generator(
        start_id + 2,
        dirt_name,
        dirt_cfg,
        dirt_color,
        dirt_met,
        dirt_rough,
        dirt_height,
        width,
        height,
    );
    dirt_layer.parent_folder_id = Some(folder_id);
    dirt_layer.color_tag = Some(2);
    dirt_layer.evaluate_generator(
        baked_curvature,
        baked_ao,
        baked_wsn,
        baked_pos,
        baked_thickness,
        baked_id,
        None,
    );
    layers.push(dirt_layer);

    // 4. Optional Child 4: Surface Dust Accumulation Generator Layer
    if let Some((d_name, d_col, d_met, d_rough, d_h, d_cfg, d_op)) = dust_info {
        let mut dust_layer = Layer::new_generator(
            start_id + 3,
            d_name,
            d_cfg,
            d_col,
            d_met,
            d_rough,
            d_h,
            width,
            height,
        );
        dust_layer.parent_folder_id = Some(folder_id);
        dust_layer.opacity = d_op;
        dust_layer.color_tag = Some(2);
        dust_layer.evaluate_generator(
            baked_curvature,
            baked_ao,
            baked_wsn,
            baked_pos,
            baked_thickness,
            baked_id,
            None,
        );
        layers.push(dust_layer);
    }

    // 5. Group Folder Container (Placed at top of the hierarchy slice)
    let mut folder = Layer::new_folder(folder_id, preset.name(), width, height);
    folder.parent_folder_id = None;
    folder.folder_expanded = true;
    folder.color_tag = Some(2); // Orange badge
    layers.push(folder);

    layers
}

/// Applies an authentic Smart Material hierarchy non-destructively onto the LayerStack
/// without wiping existing project layers.
pub fn apply_smart_material_hierarchy(
    stack: &mut LayerStack,
    name: &str,
    color: [f32; 4],
    metallic: f32,
    roughness: f32,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
) {
    apply_smart_material_hierarchy_full(
        stack,
        name,
        color,
        metallic,
        roughness,
        baked_curvature,
        baked_ao,
        None,
        None,
        None,
        None,
    );
}

/// Applies a Smart Material hierarchy non-destructively with all baked mesh maps.
pub fn apply_smart_material_hierarchy_full(
    stack: &mut LayerStack,
    name: &str,
    color: [f32; 4],
    metallic: f32,
    roughness: f32,
    baked_curvature: Option<&[f32]>,
    baked_ao: Option<&[f32]>,
    baked_wsn: Option<&[[f32; 3]]>,
    baked_pos: Option<&[[f32; 3]]>,
    baked_thickness: Option<&[f32]>,
    baked_id: Option<&[[f32; 4]]>,
) {
    let preset = SmartMaterialPreset::from_name_fuzzy(name);
    let start_id = stack.next_id();
    let mut hierarchy = create_smart_material_hierarchy(
        preset,
        start_id,
        stack.width,
        stack.height,
        baked_curvature,
        baked_ao,
        baked_wsn,
        baked_pos,
        baked_thickness,
        baked_id,
    );

    // If caller passed custom color overrides (not transparent/zero), apply to coat
    if color[3] > 0.001 {
        if let Some(coat) = hierarchy
            .iter_mut()
            .find(|l| l.name.contains("Coat") || l.name.contains("Surface"))
        {
            coat.fill_color = color;
            coat.fill_metallic = metallic;
            coat.fill_roughness = roughness;
            coat.update_fill();
        }
    }

    let insert_idx = if stack.layers.is_empty() {
        0
    } else {
        (stack.active_layer_index + 1).min(stack.layers.len())
    };
    let folder_idx = insert_idx + hierarchy.len() - 1;

    for (i, layer) in hierarchy.into_iter().enumerate() {
        stack.layers.insert(insert_idx + i, layer);
    }
    stack.active_layer_index = folder_idx;
}

#[cfg(test)]
mod tests {
    use super::super::layer::LayerType;
    use super::*;

    #[test]
    fn test_smart_mask_preset_generation() {
        let curv = vec![1.0, 0.5, 0.0, 0.5];
        let ao = vec![1.0, 1.0, 0.2, 0.8];
        for preset in SmartMaskPreset::ALL {
            let mask = preset.create_mask(2, 2, Some(&curv), Some(&ao), None, None, None, None);
            assert_eq!(mask.width, 2);
            assert_eq!(mask.height, 2);
            assert_eq!(mask.buffer.len(), 4);
            assert_eq!(mask.effects.len(), 1);
        }
    }

    #[test]
    fn test_smart_material_hierarchy_building() {
        let curv = vec![0.5; 16];
        let ao = vec![1.0; 16];
        for preset in SmartMaterialPreset::ALL {
            let layers = create_smart_material_hierarchy(
                preset,
                100,
                4,
                4,
                Some(&curv),
                Some(&ao),
                None,
                None,
                None,
                None,
            );
            assert!(
                layers.len() >= 4,
                "Hierarchy must contain at least 3 children and 1 folder"
            );
            let folder = layers.last().unwrap();
            assert_eq!(folder.layer_type, LayerType::Folder);
            assert_eq!(folder.parent_folder_id, None);
            let folder_id = folder.id;

            // All other layers must be children of this folder
            for child in &layers[..layers.len() - 1] {
                assert_eq!(child.parent_folder_id, Some(folder_id));
                assert_ne!(child.id, folder_id);
            }
        }
    }

    #[test]
    fn test_non_destructive_smart_material_application() {
        let mut stack = LayerStack::new(4, 4);
        let initial_count = stack.layers.len();
        let initial_bottom_id = stack.layers[0].id;

        apply_smart_material_hierarchy(
            &mut stack,
            "Steel Painted Worn",
            [0.2, 0.3, 0.5, 1.0],
            0.0,
            0.5,
            None,
            None,
        );

        assert!(
            stack.layers.len() > initial_count,
            "Smart Material must be inserted into stack"
        );
        assert_eq!(
            stack.layers[0].id, initial_bottom_id,
            "Initial bottom layer must be preserved without clearing"
        );
        let active = &stack.layers[stack.active_layer_index];
        assert_eq!(
            active.layer_type,
            LayerType::Folder,
            "Active layer should be the Smart Material folder"
        );
    }
}
