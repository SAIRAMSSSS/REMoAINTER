use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::{ZipArchive, ZipWriter};

use crate::baker::engine::{
    decode_scalar_plane, decode_vec3_plane, decode_vec4_plane, encode_scalar_plane,
    encode_vec3_plane, encode_vec4_plane, BakedMapFormat, BakedMaps, BakerOptions,
};
use crate::graphics::mesh::Mesh;
use crate::painting::anchor::AnchorPoint;
use crate::painting::filter::FilterEffect;
use crate::painting::generator::GeneratorConfig;
use crate::painting::layer::{BlendMode, Layer, LayerStack, LayerType};
use crate::painting::mask::{LayerMask, MaskEffect, MaskType};
use crate::painting::texture_set::{TextureChannelDef, TextureSet, TextureSetManager};

pub mod config;
pub use config::AppConfig;

/// Magic bytes for REMoAINTER Project archives
pub const RMP_MAGIC: &str = "REMOAINTER_PROJECT_V1";
pub const CURRENT_FORMAT_VERSION: u32 = 1;
pub const CURRENT_APP_VERSION: &str = "0.2.0";

/// Snapshot information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: usize,
    pub name: String,
    pub timestamp: String,
    pub path: PathBuf,
}

/// Metadata stored in `project.json` inside the .rmp archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub magic: String,
    pub format_version: u32,
    pub app_version: String,
    pub project_name: String,
    pub created_at: String,
    pub modified_at: String,

    pub mesh_name: String,
    pub mesh_file_path: Option<String>,
    pub width: u32,
    pub height: u32,

    pub baker_options: BakerOptionsManifest,

    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub camera_dist: f32,
    pub exposure: f32,
    pub light_intensity: f32,
    pub env_rotation: f32,
    pub selected_env_idx: usize,

    #[serde(default = "default_height_to_normal_scale")]
    pub height_to_normal_scale: f32,

    pub layers: Vec<LayerManifest>,

    #[serde(default)]
    pub texture_sets: Vec<TextureSetManifest>,
    #[serde(default)]
    pub active_texture_set_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureSetManifest {
    pub id: usize,
    pub name: String,
    #[serde(default)]
    pub udim_tile: Option<u32>,
    pub resolution: u32,
    #[serde(default)]
    pub channels: Vec<TextureChannelDef>,
    pub visible: bool,
    pub solo: bool,
    #[serde(default = "default_height_to_normal_scale")]
    pub height_to_normal_scale: f32,
    pub layers: Vec<LayerManifest>,
}

fn default_height_to_normal_scale() -> f32 {
    2.0
}

fn default_low_suffix() -> String {
    "_low".to_string()
}
fn default_high_suffix() -> String {
    "_high".to_string()
}
fn default_one() -> u32 {
    1
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakerOptionsManifest {
    pub resolution: u32,
    pub ao_samples: u32,
    pub ao_max_distance: f32,
    pub ao_spread: f32,
    pub curvature_intensity: f32,
    pub invert_normal_y: bool,
    pub bake_ao: bool,
    pub bake_curvature: bool,
    pub bake_normal: bool,
    pub bake_world_space_normal: bool,
    pub bake_position: bool,
    pub bake_thickness: bool,
    pub bake_id: bool,
    pub max_frontal_distance: f32,
    pub max_rear_distance: f32,
    #[serde(default)]
    pub match_by_name: bool,
    #[serde(default = "default_low_suffix")]
    pub low_poly_suffix: String,
    #[serde(default = "default_high_suffix")]
    pub high_poly_suffix: String,
    #[serde(default = "default_one")]
    pub anti_aliasing: u32,
    #[serde(default)]
    pub relative_to_bbox: bool,
    #[serde(default = "default_true")]
    pub average_normals: bool,
    #[serde(default)]
    pub ignore_backfaces: bool,
    // Skew correction persistence (skew map bitmap itself is not stored in the project)
    #[serde(default)]
    pub use_skew_correction: bool,
    #[serde(default)]
    pub skew_invert: bool,
    #[serde(default = "default_skew_strength")]
    pub skew_strength: f32,
    /// Output depth used when baking + the encoding of the baked planes stored
    /// in this archive (`baked/<map>.u8` / `.f16`, plain `.raw` = f32).
    #[serde(default)]
    pub map_format: BakedMapFormat,
    /// Nearest-shell fallback when mesh-name matching fails to pair a shell.
    #[serde(default)]
    pub match_by_shell: bool,
    /// Target UDIM UV tile (e.g. Some(1001), Some(1002)).
    #[serde(default)]
    pub udim_tile: Option<u32>,
}

pub fn default_skew_strength() -> f32 {
    0.6
}

pub fn default_map_format() -> BakedMapFormat {
    BakedMapFormat::F32
}

/// Archive member name for a baked map at the project's bake output depth.
/// Legacy f32 planes keep the original `baked/<map>.raw` name so older app
/// versions and older projects stay load-compatible.
fn baked_plane_file(base: &str, fmt: BakedMapFormat) -> String {
    match fmt {
        BakedMapFormat::F32 => format!("baked/{}.raw", base),
        BakedMapFormat::U8 => format!("baked/{}.u8", base),
        BakedMapFormat::F16 => format!("baked/{}.f16", base),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerManifest {
    pub id: usize,
    pub name: String,
    pub layer_type: String, // "Paint", "Fill", "Folder"
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: String,
    pub folder_expanded: bool,
    #[serde(default)]
    pub parent_folder_id: Option<usize>,
    #[serde(default)]
    pub effects: Vec<FilterEffect>,
    pub color_tag: Option<u8>,

    pub fill_color: [f32; 4],
    pub fill_metallic: f32,
    pub fill_roughness: f32,
    pub fill_height: f32,

    pub has_mask: bool,
    pub mask_type: Option<String>,
    pub mask_effects: Vec<MaskEffect>,

    pub anchor_point: Option<AnchorPoint>,

    #[serde(default)]
    pub generator: Option<GeneratorConfig>,
}

pub struct ProjectManager;

/// Return type for loading a project with multi-texture sets restored
pub type LoadedProjectWithSets = (
    ProjectManifest,
    Mesh,
    LayerStack,
    BakedMaps,
    BakerOptions,
    Option<TextureSetManager>,
);

impl ProjectManager {
    /// Saves the current project state to a versioned `.rmp` ZIP archive
    pub fn save_project(
        path: &Path,
        project_name: &str,
        mesh: &Mesh,
        mesh_file_path: Option<&str>,
        layer_stack: &LayerStack,
        baked_maps: &BakedMaps,
        baker_options: &BakerOptions,
        camera: (f32, f32, f32),          // yaw, pitch, dist
        lighting: (f32, f32, f32, usize), // exposure, intensity, env_rot, env_idx
    ) -> Result<(), String> {
        Self::save_project_with_texture_sets(
            path,
            project_name,
            mesh,
            mesh_file_path,
            layer_stack,
            baked_maps,
            baker_options,
            camera,
            lighting,
            None,
        )
    }

    /// Converts a Layer to its serializable LayerManifest
    pub fn layer_to_manifest(layer: &Layer) -> LayerManifest {
        let l_type_str = match layer.layer_type {
            LayerType::Paint => "Paint",
            LayerType::Fill => "Fill",
            LayerType::Folder => "Folder",
            LayerType::Adjustment => "Adjustment",
            LayerType::Generator => "Generator",
        };
        let blend_str = match layer.blend_mode {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::PassThrough => "PassThrough",
            BlendMode::Add => "Add",
            BlendMode::Subtract => "Subtract",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::ColorDodge => "ColorDodge",
            BlendMode::ColorBurn => "ColorBurn",
            BlendMode::LinearBurn => "LinearBurn",
            BlendMode::HardLight => "HardLight",
            BlendMode::SoftLight => "SoftLight",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::VividLight => "VividLight",
            BlendMode::LinearLight => "LinearLight",
            BlendMode::PinLight => "PinLight",
            BlendMode::HardMix => "HardMix",
            BlendMode::DarkerColor => "DarkerColor",
            BlendMode::LighterColor => "LighterColor",
            BlendMode::Divide => "Divide",
            BlendMode::RNM => "RNM",
            BlendMode::Whiteout => "Whiteout",
        };

        let (has_mask, mask_type_str, mask_effects) = if let Some(ref m) = layer.mask {
            let m_type = match m.mask_type {
                MaskType::White => "White",
                MaskType::Black => "Black",
            };
            (true, Some(m_type.to_string()), m.effects.clone())
        } else {
            (false, None, Vec::new())
        };

        LayerManifest {
            id: layer.id,
            name: layer.name.clone(),
            layer_type: l_type_str.to_string(),
            visible: layer.visible,
            opacity: layer.opacity,
            blend_mode: blend_str.to_string(),
            folder_expanded: layer.folder_expanded,
            parent_folder_id: layer.parent_folder_id,
            effects: layer.effects.clone(),
            color_tag: layer.color_tag,
            fill_color: layer.fill_color,
            fill_metallic: layer.fill_metallic,
            fill_roughness: layer.fill_roughness,
            fill_height: layer.fill_height,
            has_mask,
            mask_type: mask_type_str,
            mask_effects,
            anchor_point: layer.anchor_point.clone(),
            generator: layer.generator.clone(),
        }
    }

    /// Converts a LayerManifest back to a runtime Layer
    pub fn manifest_to_layer(lm: &LayerManifest, w: u32, h: u32) -> Layer {
        let layer_type = match lm.layer_type.as_str() {
            "Fill" => LayerType::Fill,
            "Folder" => LayerType::Folder,
            "Adjustment" => LayerType::Adjustment,
            "Generator" => LayerType::Generator,
            _ => LayerType::Paint,
        };
        let blend_mode = match lm.blend_mode.as_str() {
            "Multiply" => BlendMode::Multiply,
            "Screen" => BlendMode::Screen,
            "Overlay" => BlendMode::Overlay,
            "PassThrough" => BlendMode::PassThrough,
            "Add" => BlendMode::Add,
            "Subtract" => BlendMode::Subtract,
            "Darken" => BlendMode::Darken,
            "Lighten" => BlendMode::Lighten,
            "ColorDodge" => BlendMode::ColorDodge,
            "ColorBurn" => BlendMode::ColorBurn,
            "LinearBurn" => BlendMode::LinearBurn,
            "HardLight" => BlendMode::HardLight,
            "SoftLight" => BlendMode::SoftLight,
            "Difference" => BlendMode::Difference,
            "Exclusion" => BlendMode::Exclusion,
            "Color" => BlendMode::Color,
            "Luminosity" => BlendMode::Luminosity,
            "Hue" => BlendMode::Hue,
            "Saturation" => BlendMode::Saturation,
            "VividLight" => BlendMode::VividLight,
            "LinearLight" => BlendMode::LinearLight,
            "PinLight" => BlendMode::PinLight,
            "HardMix" => BlendMode::HardMix,
            "DarkerColor" => BlendMode::DarkerColor,
            "LighterColor" => BlendMode::LighterColor,
            "Divide" => BlendMode::Divide,
            "RNM" => BlendMode::RNM,
            "Whiteout" => BlendMode::Whiteout,
            _ => BlendMode::Normal,
        };

        let total_pixels = (w * h) as usize;
        let mut layer = Layer {
            id: lm.id,
            name: lm.name.clone(),
            layer_type,
            visible: lm.visible,
            opacity: lm.opacity,
            blend_mode,
            folder_expanded: lm.folder_expanded,
            parent_folder_id: lm.parent_folder_id,
            effects: lm.effects.clone(),

            channel_color_enabled: true,
            channel_metallic_enabled: true,
            channel_roughness_enabled: true,
            channel_normal_enabled: true,
            channel_height_enabled: true,

            channel_color_blend: BlendMode::Normal,
            channel_metallic_blend: BlendMode::Normal,
            channel_roughness_blend: BlendMode::Normal,
            channel_normal_blend: BlendMode::Normal,
            channel_height_blend: if layer_type == LayerType::Adjustment {
                BlendMode::Normal
            } else {
                BlendMode::Add
            },

            channel_color_opacity: 1.0,
            channel_metallic_opacity: 1.0,
            channel_roughness_opacity: 1.0,
            channel_normal_opacity: 1.0,
            channel_height_opacity: 1.0,

            mask: None,
            anchor_point: lm.anchor_point.clone(),
            color_tag: lm.color_tag,
            generator: lm.generator.clone(),
            fill_color: lm.fill_color,
            fill_metallic: lm.fill_metallic,
            fill_roughness: lm.fill_roughness,
            fill_height: lm.fill_height,
            color_buffer: vec![[0.0, 0.0, 0.0, 0.0]; total_pixels],
            metallic_buffer: vec![0.0; total_pixels],
            roughness_buffer: vec![0.5; total_pixels],
            normal_buffer: vec![[0.5, 0.5, 1.0]; total_pixels],
            height_buffer: vec![0.0; total_pixels],
            width: w,
            height: h,
        };

        if lm.has_mask {
            let m_type = match lm.mask_type.as_deref() {
                Some("White") => MaskType::White,
                _ => MaskType::Black,
            };
            layer.mask = Some(LayerMask {
                mask_type: m_type,
                buffer: vec![if m_type == MaskType::White { 1.0 } else { 0.0 }; total_pixels],
                effects: lm.mask_effects.clone(),
                width: w,
                height: h,
            });
        }

        layer
    }

    /// Writes raw layer channel buffers into the zip archive with optional prefix
    fn write_layer_buffers(
        zip: &mut ZipWriter<File>,
        options: SimpleFileOptions,
        prefix: &str,
        layer_stack: &LayerStack,
    ) -> Result<(), String> {
        for (i, layer) in layer_stack.layers.iter().enumerate() {
            if !layer.color_buffer.is_empty() {
                zip.start_file(format!("{}layers/{}_color.raw", prefix, i), options)
                    .map_err(|e| format!("Zip error: {}", e))?;
                let slice: &[u8] = bytemuck::cast_slice(&layer.color_buffer);
                zip.write_all(slice)
                    .map_err(|e| format!("Zip write error: {}", e))?;
            }
            if !layer.metallic_buffer.is_empty() {
                zip.start_file(format!("{}layers/{}_metallic.raw", prefix, i), options)
                    .map_err(|e| format!("Zip error: {}", e))?;
                let slice: &[u8] = bytemuck::cast_slice(&layer.metallic_buffer);
                zip.write_all(slice)
                    .map_err(|e| format!("Zip write error: {}", e))?;
            }
            if !layer.roughness_buffer.is_empty() {
                zip.start_file(format!("{}layers/{}_roughness.raw", prefix, i), options)
                    .map_err(|e| format!("Zip error: {}", e))?;
                let slice: &[u8] = bytemuck::cast_slice(&layer.roughness_buffer);
                zip.write_all(slice)
                    .map_err(|e| format!("Zip write error: {}", e))?;
            }
            if !layer.normal_buffer.is_empty() {
                zip.start_file(format!("{}layers/{}_normal.raw", prefix, i), options)
                    .map_err(|e| format!("Zip error: {}", e))?;
                let slice: &[u8] = bytemuck::cast_slice(&layer.normal_buffer);
                zip.write_all(slice)
                    .map_err(|e| format!("Zip write error: {}", e))?;
            }
            if !layer.height_buffer.is_empty() {
                zip.start_file(format!("{}layers/{}_height.raw", prefix, i), options)
                    .map_err(|e| format!("Zip error: {}", e))?;
                let slice: &[u8] = bytemuck::cast_slice(&layer.height_buffer);
                zip.write_all(slice)
                    .map_err(|e| format!("Zip write error: {}", e))?;
            }
            if let Some(ref m) = layer.mask {
                if !m.buffer.is_empty() {
                    zip.start_file(format!("{}layers/{}_mask.raw", prefix, i), options)
                        .map_err(|e| format!("Zip error: {}", e))?;
                    let slice: &[u8] = bytemuck::cast_slice(&m.buffer);
                    zip.write_all(slice)
                        .map_err(|e| format!("Zip write error: {}", e))?;
                }
            }
        }
        Ok(())
    }

    /// Reads raw layer channel buffers from the zip archive with optional prefix
    fn read_layer_buffers(
        zip: &mut ZipArchive<File>,
        prefix: &str,
        layer_stack: &mut LayerStack,
    ) -> Result<(), String> {
        let total_pixels = (layer_stack.width * layer_stack.height) as usize;
        for (i, layer) in layer_stack.layers.iter_mut().enumerate() {
            if let Ok(mut zf) = zip.by_name(&format!("{}layers/{}_color.raw", prefix, i)) {
                let mut bytes = Vec::new();
                zf.read_to_end(&mut bytes)
                    .map_err(|e| format!("Failed to read layer {i} color buffer: {e}"))?;
                if bytes.len() == total_pixels * 16 {
                    let slice: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
                    layer.color_buffer.copy_from_slice(slice);
                } else {
                    return Err(format!(
                        "Corrupted color buffer for layer {i}: expected {} bytes, found {}",
                        total_pixels * 16,
                        bytes.len()
                    ));
                }
            }
            if let Ok(mut zf) = zip.by_name(&format!("{}layers/{}_metallic.raw", prefix, i)) {
                let mut bytes = Vec::new();
                zf.read_to_end(&mut bytes)
                    .map_err(|e| format!("Failed to read layer {i} metallic buffer: {e}"))?;
                if bytes.len() == total_pixels * 4 {
                    let slice: &[f32] = bytemuck::cast_slice(&bytes);
                    layer.metallic_buffer.copy_from_slice(slice);
                } else {
                    return Err(format!(
                        "Corrupted metallic buffer for layer {i}: expected {} bytes, found {}",
                        total_pixels * 4,
                        bytes.len()
                    ));
                }
            }
            if let Ok(mut zf) = zip.by_name(&format!("{}layers/{}_roughness.raw", prefix, i)) {
                let mut bytes = Vec::new();
                zf.read_to_end(&mut bytes)
                    .map_err(|e| format!("Failed to read layer {i} roughness buffer: {e}"))?;
                if bytes.len() == total_pixels * 4 {
                    let slice: &[f32] = bytemuck::cast_slice(&bytes);
                    layer.roughness_buffer.copy_from_slice(slice);
                } else {
                    return Err(format!(
                        "Corrupted roughness buffer for layer {i}: expected {} bytes, found {}",
                        total_pixels * 4,
                        bytes.len()
                    ));
                }
            }
            if let Ok(mut zf) = zip.by_name(&format!("{}layers/{}_normal.raw", prefix, i)) {
                let mut bytes = Vec::new();
                zf.read_to_end(&mut bytes)
                    .map_err(|e| format!("Failed to read layer {i} normal buffer: {e}"))?;
                if bytes.len() == total_pixels * 12 {
                    let slice: &[[f32; 3]] = bytemuck::cast_slice(&bytes);
                    layer.normal_buffer.copy_from_slice(slice);
                } else {
                    return Err(format!(
                        "Corrupted normal buffer for layer {i}: expected {} bytes, found {}",
                        total_pixels * 12,
                        bytes.len()
                    ));
                }
            }
            if let Ok(mut zf) = zip.by_name(&format!("{}layers/{}_height.raw", prefix, i)) {
                let mut bytes = Vec::new();
                zf.read_to_end(&mut bytes)
                    .map_err(|e| format!("Failed to read layer {i} height buffer: {e}"))?;
                if bytes.len() == total_pixels * 4 {
                    let slice: &[f32] = bytemuck::cast_slice(&bytes);
                    layer.height_buffer.copy_from_slice(slice);
                } else {
                    return Err(format!(
                        "Corrupted height buffer for layer {i}: expected {} bytes, found {}",
                        total_pixels * 4,
                        bytes.len()
                    ));
                }
            }
            if let Some(ref mut m) = layer.mask {
                if let Ok(mut zf) = zip.by_name(&format!("{}layers/{}_mask.raw", prefix, i)) {
                    let mut bytes = Vec::new();
                    zf.read_to_end(&mut bytes)
                        .map_err(|e| format!("Failed to read layer {i} mask buffer: {e}"))?;
                    if bytes.len() == total_pixels * 4 {
                        let slice: &[f32] = bytemuck::cast_slice(&bytes);
                        m.buffer.copy_from_slice(slice);
                    } else {
                        return Err(format!(
                            "Corrupted mask buffer for layer {i}: expected {} bytes, found {}",
                            total_pixels * 4,
                            bytes.len()
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Writes baked mesh maps into the zip archive with optional prefix
    fn write_baked_maps(
        zip: &mut ZipWriter<File>,
        options: SimpleFileOptions,
        prefix: &str,
        baked_maps: &BakedMaps,
        baked_fmt: BakedMapFormat,
    ) -> Result<(), String> {
        let mut write_plane = |name: &str, bytes: &[u8]| -> Result<(), String> {
            if bytes.is_empty() {
                return Ok(());
            }
            zip.start_file(format!("{}{}", prefix, name), options)
                .map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())
        };
        if let Some(ref ao) = baked_maps.ambient_occlusion {
            write_plane(
                &baked_plane_file("ao", baked_fmt),
                &encode_scalar_plane(ao, baked_fmt),
            )?;
        }
        if let Some(ref curv) = baked_maps.curvature {
            write_plane(
                &baked_plane_file("curvature", baked_fmt),
                &encode_scalar_plane(curv, baked_fmt),
            )?;
        }
        if let Some(ref norm) = baked_maps.normal {
            write_plane(
                &baked_plane_file("normal", baked_fmt),
                &encode_vec3_plane(norm, baked_fmt),
            )?;
        }
        if let Some(ref wsn) = baked_maps.world_space_normal {
            write_plane(
                &baked_plane_file("world_space_normal", baked_fmt),
                &encode_vec3_plane(wsn, baked_fmt),
            )?;
        }
        if let Some(ref pos) = baked_maps.position {
            write_plane(
                &baked_plane_file("position", baked_fmt),
                &encode_vec3_plane(pos, baked_fmt),
            )?;
        }
        if let Some(ref thick) = baked_maps.thickness {
            write_plane(
                &baked_plane_file("thickness", baked_fmt),
                &encode_scalar_plane(thick, baked_fmt),
            )?;
        }
        if let Some(ref id_map) = baked_maps.id {
            write_plane(
                &baked_plane_file("id", baked_fmt),
                &encode_vec4_plane(id_map, baked_fmt),
            )?;
        }
        Ok(())
    }

    /// Reads baked mesh maps from the zip archive with optional prefix
    fn read_baked_maps(
        zip: &mut ZipArchive<File>,
        prefix: &str,
        resolution: u32,
        baked_fmt: BakedMapFormat,
    ) -> BakedMaps {
        let mut baked_maps = BakedMaps {
            resolution,
            ambient_occlusion: None,
            curvature: None,
            normal: None,
            world_space_normal: None,
            position: None,
            thickness: None,
            id: None,
            is_cancelled: false,
        };
        let read_plane = |zip: &mut ZipArchive<File>, base: &str| -> Vec<u8> {
            for name in [
                format!("{}{}", prefix, baked_plane_file(base, baked_fmt)),
                format!("{}baked/{}.raw", prefix, base),
            ] {
                if let Ok(mut zf) = zip.by_name(&name) {
                    let mut bytes = Vec::new();
                    let _ = zf.read_to_end(&mut bytes);
                    return bytes;
                }
            }
            Vec::new()
        };

        let bytes = read_plane(zip, "ao");
        if !bytes.is_empty() {
            baked_maps.ambient_occlusion = Some(decode_scalar_plane(&bytes, baked_fmt));
        }
        let bytes = read_plane(zip, "curvature");
        if !bytes.is_empty() {
            baked_maps.curvature = Some(decode_scalar_plane(&bytes, baked_fmt));
        }
        let bytes = read_plane(zip, "normal");
        if !bytes.is_empty() {
            baked_maps.normal = Some(decode_vec3_plane(&bytes, baked_fmt));
        }
        let bytes = read_plane(zip, "world_space_normal");
        if !bytes.is_empty() {
            baked_maps.world_space_normal = Some(decode_vec3_plane(&bytes, baked_fmt));
        }
        let bytes = read_plane(zip, "position");
        if !bytes.is_empty() {
            baked_maps.position = Some(decode_vec3_plane(&bytes, baked_fmt));
        }
        let bytes = read_plane(zip, "thickness");
        if !bytes.is_empty() {
            baked_maps.thickness = Some(decode_scalar_plane(&bytes, baked_fmt));
        }
        let bytes = read_plane(zip, "id");
        if !bytes.is_empty() {
            baked_maps.id = Some(decode_vec4_plane(&bytes, baked_fmt));
        }
        baked_maps
    }

    /// Saves the current project state, persisting all texture sets (e.g. UDIM or multi-material)
    pub fn save_project_with_texture_sets(
        path: &Path,
        project_name: &str,
        mesh: &Mesh,
        mesh_file_path: Option<&str>,
        layer_stack: &LayerStack,
        baked_maps: &BakedMaps,
        baker_options: &BakerOptions,
        camera: (f32, f32, f32),          // yaw, pitch, dist
        lighting: (f32, f32, f32, usize), // exposure, intensity, env_rot, env_idx
        texture_set_manager: Option<&TextureSetManager>,
    ) -> Result<(), String> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        // Write to temporary file first for atomic transaction safety
        let temp_path = path.with_extension("rmp.tmp");
        let file = File::create(&temp_path)
            .map_err(|e| format!("Failed to create temp project file: {}", e))?;
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        // 1. Build Manifest
        let now = chrono::Local::now().to_rfc3339();

        let (texture_sets_manifest, active_idx, primary_layers, proj_w, proj_h, proj_scale) =
            if let Some(tsm) = texture_set_manager {
                if !tsm.sets.is_empty() {
                    let a_idx = tsm.active_index.min(tsm.sets.len().saturating_sub(1));
                    let mut ts_list = Vec::with_capacity(tsm.sets.len());
                    for set in &tsm.sets {
                        let mut set_lm = Vec::with_capacity(set.layer_stack.layers.len());
                        for layer in &set.layer_stack.layers {
                            set_lm.push(Self::layer_to_manifest(layer));
                        }
                        ts_list.push(TextureSetManifest {
                            id: set.id,
                            name: set.name.clone(),
                            udim_tile: set.udim_tile,
                            resolution: set.resolution,
                            channels: set.channels.clone(),
                            visible: set.visible,
                            solo: set.solo,
                            height_to_normal_scale: set.layer_stack.height_to_normal_scale,
                            layers: set_lm,
                        });
                    }
                    let prim = ts_list[a_idx].layers.clone();
                    let w = tsm.sets[a_idx].layer_stack.width;
                    let h = tsm.sets[a_idx].layer_stack.height;
                    let s = tsm.sets[a_idx].layer_stack.height_to_normal_scale;
                    (ts_list, a_idx, prim, w, h, s)
                } else {
                    let mut set_lm = Vec::with_capacity(layer_stack.layers.len());
                    for layer in &layer_stack.layers {
                        set_lm.push(Self::layer_to_manifest(layer));
                    }
                    (
                        Vec::new(),
                        0,
                        set_lm,
                        layer_stack.width,
                        layer_stack.height,
                        layer_stack.height_to_normal_scale,
                    )
                }
            } else {
                let mut set_lm = Vec::with_capacity(layer_stack.layers.len());
                for layer in &layer_stack.layers {
                    set_lm.push(Self::layer_to_manifest(layer));
                }
                (
                    Vec::new(),
                    0,
                    set_lm,
                    layer_stack.width,
                    layer_stack.height,
                    layer_stack.height_to_normal_scale,
                )
            };

        let manifest = ProjectManifest {
            magic: RMP_MAGIC.to_string(),
            format_version: CURRENT_FORMAT_VERSION,
            app_version: CURRENT_APP_VERSION.to_string(),
            project_name: project_name.to_string(),
            created_at: now.clone(),
            modified_at: now,
            mesh_name: mesh.name.clone(),
            mesh_file_path: mesh_file_path.map(|s| s.to_string()),
            width: proj_w,
            height: proj_h,
            baker_options: BakerOptionsManifest {
                resolution: baker_options.resolution,
                ao_samples: baker_options.ao_samples,
                ao_max_distance: baker_options.ao_max_distance,
                ao_spread: baker_options.ao_spread,
                curvature_intensity: baker_options.curvature_intensity,
                invert_normal_y: baker_options.invert_normal_y,
                bake_ao: baker_options.bake_ao,
                bake_curvature: baker_options.bake_curvature,
                bake_normal: baker_options.bake_normal,
                bake_world_space_normal: baker_options.bake_world_space_normal,
                bake_position: baker_options.bake_position,
                bake_thickness: baker_options.bake_thickness,
                bake_id: baker_options.bake_id,
                max_frontal_distance: baker_options.max_frontal_distance,
                max_rear_distance: baker_options.max_rear_distance,
                match_by_name: baker_options.match_by_name,
                low_poly_suffix: baker_options.low_poly_suffix.clone(),
                high_poly_suffix: baker_options.high_poly_suffix.clone(),
                anti_aliasing: baker_options.anti_aliasing,
                relative_to_bbox: baker_options.relative_to_bbox,
                average_normals: baker_options.average_normals,
                ignore_backfaces: baker_options.ignore_backfaces,
                use_skew_correction: baker_options.use_skew_correction,
                skew_invert: baker_options.skew_invert,
                skew_strength: baker_options.skew_strength,
                map_format: baker_options.map_format,
                match_by_shell: baker_options.match_by_shell,
                udim_tile: baker_options.udim_tile,
            },
            camera_yaw: camera.0,
            camera_pitch: camera.1,
            camera_dist: camera.2,
            exposure: lighting.0,
            light_intensity: lighting.1,
            env_rotation: lighting.2,
            selected_env_idx: lighting.3,
            height_to_normal_scale: proj_scale,
            layers: primary_layers,
            texture_sets: texture_sets_manifest,
            active_texture_set_index: active_idx,
        };

        // Write project.json
        zip.start_file("project.json", options)
            .map_err(|e| format!("Zip error: {}", e))?;
        let json_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("Serialization error: {}", e))?;
        zip.write_all(&json_bytes)
            .map_err(|e| format!("Zip write error: {}", e))?;

        // 2. Write Embedded Mesh (Wavefront OBJ format)
        zip.start_file("mesh/model.obj", options)
            .map_err(|e| format!("Zip error: {}", e))?;
        let obj_content = mesh.export_obj_string();
        zip.write_all(obj_content.as_bytes())
            .map_err(|e| format!("Zip write error: {}", e))?;

        let baked_fmt = baker_options.map_format;

        // 3. Write Root Layers & Baked Maps (for legacy backward compatibility)
        if let Some(tsm) = texture_set_manager {
            if !tsm.sets.is_empty() {
                Self::write_layer_buffers(
                    &mut zip,
                    options,
                    "",
                    &tsm.sets[active_idx].layer_stack,
                )?;
                Self::write_baked_maps(
                    &mut zip,
                    options,
                    "",
                    &tsm.sets[active_idx].baked_maps,
                    baked_fmt,
                )?;

                // 4. Write Per-Set Layers & Baked Maps
                for (s_idx, set) in tsm.sets.iter().enumerate() {
                    let prefix = format!("sets/{}/", s_idx);
                    Self::write_layer_buffers(&mut zip, options, &prefix, &set.layer_stack)?;
                    Self::write_baked_maps(&mut zip, options, &prefix, &set.baked_maps, baked_fmt)?;
                }
            } else {
                Self::write_layer_buffers(&mut zip, options, "", layer_stack)?;
                Self::write_baked_maps(&mut zip, options, "", baked_maps, baked_fmt)?;
            }
        } else {
            Self::write_layer_buffers(&mut zip, options, "", layer_stack)?;
            Self::write_baked_maps(&mut zip, options, "", baked_maps, baked_fmt)?;
        }

        zip.finish()
            .map_err(|e| format!("Zip finish error: {}", e))?;

        // Atomic file replacement
        fs::rename(&temp_path, path)
            .map_err(|e| format!("Failed to finalize project file: {}", e))?;

        Ok(())
    }

    /// Loads a project from a `.rmp` archive
    pub fn load_project(
        path: &Path,
    ) -> Result<(ProjectManifest, Mesh, LayerStack, BakedMaps, BakerOptions), String> {
        let (manifest, mesh, layer_stack, baked_maps, baker_options, _) =
            Self::load_project_with_texture_sets(path)?;
        Ok((manifest, mesh, layer_stack, baked_maps, baker_options))
    }

    /// Loads a project from a `.rmp` archive, restoring all texture sets if present
    pub fn load_project_with_texture_sets(path: &Path) -> Result<LoadedProjectWithSets, String> {
        let file = File::open(path).map_err(|e| format!("Failed to open project file: {}", e))?;
        let mut zip =
            ZipArchive::new(file).map_err(|e| format!("Invalid project archive: {}", e))?;

        // 1. Read Manifest
        let manifest: ProjectManifest = {
            let mut manifest_file = zip
                .by_name("project.json")
                .map_err(|e| format!("Missing project.json: {}", e))?;
            let mut contents = String::new();
            manifest_file
                .read_to_string(&mut contents)
                .map_err(|e| format!("Failed to read project.json: {}", e))?;
            serde_json::from_str(&contents)
                .map_err(|e| format!("Invalid project manifest format: {}", e))?
        };

        // Migration Check
        if manifest.format_version > CURRENT_FORMAT_VERSION {
            return Err(format!(
                "Project format version {} is newer than supported version {}. Please upgrade REMoAINTER.",
                manifest.format_version, CURRENT_FORMAT_VERSION
            ));
        }

        // 2. Read Mesh
        let mesh = if let Ok(mut mesh_file) = zip.by_name("mesh/model.obj") {
            let mut obj_str = String::new();
            mesh_file
                .read_to_string(&mut obj_str)
                .map_err(|e| format!("Failed to read embedded mesh: {}", e))?;
            Mesh::from_obj_string(&obj_str, &manifest.mesh_name)
                .map_err(|e| format!("Corrupted embedded mesh in project: {e}"))?
        } else if let Some(ref mesh_path) = manifest.mesh_file_path {
            Mesh::load_file(mesh_path)
                .map_err(|e| format!("Failed to load linked mesh file at '{mesh_path}': {e}"))?
        } else {
            Mesh::create_material_sphere(64, 64)
        };

        let baked_fmt = manifest.baker_options.map_format;

        // 3. Reconstruct LayerStack(s) and TextureSetManager
        let (primary_layer_stack, primary_baked_maps, tsm_opt) = if !manifest
            .texture_sets
            .is_empty()
        {
            let active_idx = manifest
                .active_texture_set_index
                .min(manifest.texture_sets.len().saturating_sub(1));
            let mut tsm = TextureSetManager {
                sets: Vec::with_capacity(manifest.texture_sets.len()),
                active_index: active_idx,
                next_id: 1,
                is_udim_workflow: manifest.texture_sets.iter().any(|s| s.udim_tile.is_some()),
            };

            for (s_idx, sm) in manifest.texture_sets.iter().enumerate() {
                let w = sm.resolution.max(1);
                let h = sm.resolution.max(1);
                let mut set_stack = LayerStack::new(w, h);
                set_stack.height_to_normal_scale = sm.height_to_normal_scale;
                set_stack.layers.clear();

                for lm in &sm.layers {
                    set_stack.layers.push(Self::manifest_to_layer(lm, w, h));
                }

                let prefix = format!("sets/{}/", s_idx);
                let read_prefix = if s_idx == 0
                    && zip
                        .by_name(&format!("{}layers/0_color.raw", prefix))
                        .is_err()
                    && zip.by_name("layers/0_color.raw").is_ok()
                {
                    ""
                } else {
                    &prefix
                };
                Self::read_layer_buffers(&mut zip, read_prefix, &mut set_stack)?;
                set_stack.recompute_composite();

                let baked_prefix = read_prefix;
                let set_baked_maps =
                    Self::read_baked_maps(&mut zip, baked_prefix, sm.resolution, baked_fmt);

                let channels = if sm.channels.is_empty() {
                    TextureChannelDef::standard_channels()
                } else {
                    sm.channels.clone()
                };

                if sm.id >= tsm.next_id {
                    tsm.next_id = sm.id + 1;
                }

                tsm.sets.push(TextureSet {
                    id: sm.id,
                    name: sm.name.clone(),
                    udim_tile: sm.udim_tile,
                    resolution: sm.resolution,
                    channels,
                    layer_stack: set_stack,
                    baked_maps: set_baked_maps,
                    visible: sm.visible,
                    solo: sm.solo,
                });
            }

            let prim_stack = tsm.sets[active_idx].layer_stack.clone();
            let prim_baked = tsm.sets[active_idx].baked_maps.clone();
            (prim_stack, prim_baked, Some(tsm))
        } else {
            let w = manifest.width;
            let h = manifest.height;
            let mut layer_stack = LayerStack::new(w, h);
            layer_stack.height_to_normal_scale = manifest.height_to_normal_scale;
            layer_stack.layers.clear();

            for lm in &manifest.layers {
                layer_stack.layers.push(Self::manifest_to_layer(lm, w, h));
            }

            Self::read_layer_buffers(&mut zip, "", &mut layer_stack)?;
            layer_stack.recompute_composite();

            let baked_maps =
                Self::read_baked_maps(&mut zip, "", manifest.baker_options.resolution, baked_fmt);

            let mut tsm = TextureSetManager::new(layer_stack.width);
            tsm.configure_from_mesh(&mesh, layer_stack.width);
            if !tsm.sets.is_empty() {
                tsm.sets[0].layer_stack = layer_stack.clone();
                tsm.sets[0].baked_maps = baked_maps.clone();
            }

            (layer_stack, baked_maps, Some(tsm))
        };

        // 4. Baker Options
        let baker_options = BakerOptions {
            resolution: manifest.baker_options.resolution,
            ao_samples: manifest.baker_options.ao_samples,
            ao_max_distance: manifest.baker_options.ao_max_distance,
            ao_spread: manifest.baker_options.ao_spread,
            curvature_intensity: manifest.baker_options.curvature_intensity,
            invert_normal_y: manifest.baker_options.invert_normal_y,
            bake_ao: manifest.baker_options.bake_ao,
            bake_curvature: manifest.baker_options.bake_curvature,
            bake_normal: manifest.baker_options.bake_normal,
            bake_world_space_normal: manifest.baker_options.bake_world_space_normal,
            bake_position: manifest.baker_options.bake_position,
            bake_thickness: manifest.baker_options.bake_thickness,
            bake_id: manifest.baker_options.bake_id,
            max_frontal_distance: manifest.baker_options.max_frontal_distance,
            max_rear_distance: manifest.baker_options.max_rear_distance,
            match_by_name: manifest.baker_options.match_by_name,
            low_poly_suffix: manifest.baker_options.low_poly_suffix.clone(),
            high_poly_suffix: manifest.baker_options.high_poly_suffix.clone(),
            anti_aliasing: manifest.baker_options.anti_aliasing,
            relative_to_bbox: manifest.baker_options.relative_to_bbox,
            average_normals: manifest.baker_options.average_normals,
            ignore_backfaces: manifest.baker_options.ignore_backfaces,
            use_skew_correction: manifest.baker_options.use_skew_correction,
            skew_invert: manifest.baker_options.skew_invert,
            skew_strength: manifest.baker_options.skew_strength,
            map_format: manifest.baker_options.map_format,
            match_by_shell: manifest.baker_options.match_by_shell,
            skew_map: None,
            udim_tile: manifest.baker_options.udim_tile,
        };

        Ok((
            manifest,
            mesh,
            primary_layer_stack,
            primary_baked_maps,
            baker_options,
            tsm_opt,
        ))
    }

    /// Autosave location directory
    pub fn autosave_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".remoainter").join("autosave.rmp")
    }

    /// Check if a valid autosave file exists
    pub fn check_autosave() -> Option<PathBuf> {
        let path = Self::autosave_path();
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Snapshots directory
    pub fn snapshots_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".remoainter").join("snapshots")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painting::FilterTargetChannel;

    #[test]
    fn test_project_manifest_roundtrip() {
        let manifest = ProjectManifest {
            magic: RMP_MAGIC.to_string(),
            format_version: 1,
            app_version: "0.2.0".to_string(),
            project_name: "TestProject".to_string(),
            created_at: "2026-10-21T00:00:00Z".to_string(),
            modified_at: "2026-10-21T00:00:00Z".to_string(),
            mesh_name: "Sphere".to_string(),
            mesh_file_path: None,
            width: 512,
            height: 512,
            baker_options: BakerOptionsManifest {
                resolution: 512,
                ao_samples: 16,
                ao_max_distance: 1.5,
                ao_spread: 0.8,
                curvature_intensity: 1.2,
                invert_normal_y: false,
                bake_ao: true,
                bake_curvature: true,
                bake_normal: true,
                bake_world_space_normal: true,
                bake_position: true,
                bake_thickness: true,
                bake_id: true,
                max_frontal_distance: 0.05,
                max_rear_distance: 0.05,
                match_by_name: false,
                low_poly_suffix: "_low".to_string(),
                high_poly_suffix: "_high".to_string(),
                anti_aliasing: 1,
                relative_to_bbox: false,
                average_normals: true,
                ignore_backfaces: false,
                use_skew_correction: false,
                skew_invert: false,
                skew_strength: 0.6,
                map_format: BakedMapFormat::F32,
                match_by_shell: false,
                udim_tile: None,
            },
            camera_yaw: 45.0,
            camera_pitch: 15.0,
            camera_dist: 2.5,
            exposure: 0.0,
            light_intensity: 1.0,
            env_rotation: 0.0,
            selected_env_idx: 0,
            height_to_normal_scale: 2.0,
            layers: vec![],
            texture_sets: vec![],
            active_texture_set_index: 0,
        };

        let json = serde_json::to_string(&manifest).expect("Serialize failed");
        let deserialized: ProjectManifest =
            serde_json::from_str(&json).expect("Deserialize failed");
        assert_eq!(deserialized.magic, RMP_MAGIC);
        assert_eq!(deserialized.format_version, 1);
        assert_eq!(deserialized.project_name, "TestProject");
    }

    #[test]
    fn test_project_save_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join("remoainter_test_save_load");
        let _ = fs::create_dir_all(&temp_dir);
        let project_path = temp_dir.join("test_roundtrip.rmp");

        let mesh = Mesh::create_material_sphere(16, 16);
        let mut layer_stack = LayerStack::new(64, 64);
        layer_stack.layers.clear();
        let mut layer = Layer::new_paint(1, "Test Paint", 64, 64);
        layer.color_buffer[0] = [1.0, 0.5, 0.25, 1.0];
        layer_stack.layers.push(layer);
        layer_stack.recompute_composite();

        let baked_maps = BakedMaps::default();
        let baker_options = BakerOptions::default();

        let save_res = ProjectManager::save_project(
            &project_path,
            "RoundtripProject",
            &mesh,
            None,
            &layer_stack,
            &baked_maps,
            &baker_options,
            (45.0, 15.0, 2.5),
            (0.0, 1.0, 0.0, 0),
        );
        assert!(
            save_res.is_ok(),
            "Project save failed: {:?}",
            save_res.err()
        );

        let load_res = ProjectManager::load_project(&project_path);
        assert!(
            load_res.is_ok(),
            "Project load failed: {:?}",
            load_res.err()
        );

        let (manifest, loaded_mesh, loaded_stack, _, _) = load_res.unwrap();
        assert_eq!(manifest.project_name, "RoundtripProject");
        assert_eq!(loaded_stack.layers.len(), 1);
        assert_eq!(loaded_stack.layers[0].name, "Test Paint");
        assert_eq!(
            loaded_stack.layers[0].color_buffer[0],
            [1.0, 0.5, 0.25, 1.0]
        );
        assert_eq!(loaded_mesh.vertices.len(), mesh.vertices.len());

        // Cleanup
        let _ = fs::remove_file(project_path);
        let _ = fs::remove_dir(temp_dir);
    }

    /// Bake output depth: planes saved at 16-bit half round-trip back to f32
    /// within half precision, and land in depth-tagged archive members.
    #[test]
    fn test_project_baked_plane_f16_roundtrip() {
        use crate::baker::engine::BakedMapFormat;

        let temp_dir = std::env::temp_dir().join("remoainter_test_save_load");
        let _ = fs::create_dir_all(&temp_dir);
        let project_path = temp_dir.join("test_roundtrip_f16.rmp");

        let mesh = Mesh::create_material_sphere(16, 16);
        let mut layer_stack = LayerStack::new(64, 64);
        layer_stack.layers.clear();
        layer_stack.recompute_composite();

        // Two known f32 AO samples that stress half precision (banded tail).
        let baked_maps = BakedMaps {
            resolution: 64,
            ambient_occlusion: Some(vec![0.5, 0.002_003]),
            curvature: None,
            normal: None,
            world_space_normal: None,
            position: None,
            thickness: Some(vec![0.75]),
            id: None,
            is_cancelled: false,
        };
        let mut baker_options = BakerOptions::default();
        baker_options.map_format = BakedMapFormat::F16;

        let save_res = ProjectManager::save_project(
            &project_path,
            "F16Roundtrip",
            &mesh,
            None,
            &layer_stack,
            &baked_maps,
            &baker_options,
            (45.0, 15.0, 2.5),
            (0.0, 1.0, 0.0, 0),
        );
        assert!(save_res.is_ok(), "save failed: {:?}", save_res.err());

        // The archive must carry depth-tagged members (legacy `.raw` absent).
        let mut zf = zip::ZipArchive::new(std::fs::File::open(&project_path).unwrap()).unwrap();
        assert!(zf.by_name("baked/ao.f16").is_ok(), "missing ao.f16 member");
        assert!(
            zf.by_name("baked/thickness.f16").is_ok(),
            "missing thickness.f16 member"
        );
        assert!(
            zf.by_name("baked/ao.raw").is_err(),
            "legacy .raw should not exist for F16 projects"
        );
        drop(zf);

        let load_res = ProjectManager::load_project(&project_path);
        assert!(load_res.is_ok(), "load failed: {:?}", load_res.err());
        let (manifest, _, _, loaded_maps, _) = load_res.unwrap();
        assert_eq!(manifest.baker_options.map_format, BakedMapFormat::F16);

        let ao = loaded_maps.ambient_occlusion.expect("ao decoded");
        let half =
            crate::baker::engine::half_bits_to_f32(crate::baker::engine::f32_to_half_bits(0.5));
        assert_eq!(ao[0], half, "0.5 must survive the f16 round-trip exactly");

        let _ = fs::remove_file(project_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn test_adjustment_layer_project_roundtrip() {
        let temp_dir = std::env::temp_dir().join("remoainter_adj_test");
        let _ = fs::create_dir_all(&temp_dir);
        let project_path = temp_dir.join("test_adj.rmp");

        let mesh = Mesh::create_material_sphere(8, 8);
        let mut layer_stack = LayerStack::new(4, 4);
        layer_stack.layers.clear();

        let fill = Layer::new_fill(1, "Base", [0.5, 0.5, 0.5, 1.0], 0.0, 0.5, 4, 4);
        let adj = Layer::new_adjustment(
            2,
            "Levels Adjustment",
            FilterEffect::Levels {
                min_in: 0.1,
                max_in: 0.9,
                gamma: 1.2,
                min_out: 0.0,
                max_out: 1.0,
                channel: FilterTargetChannel::All,
            },
            4,
            4,
        );
        layer_stack.layers.push(fill);
        layer_stack.layers.push(adj);

        let baked_maps = BakedMaps::default();
        let baker_options = BakerOptions::default();

        let save_res = ProjectManager::save_project(
            &project_path,
            "AdjProject",
            &mesh,
            None,
            &layer_stack,
            &baked_maps,
            &baker_options,
            (0.0, 0.0, 2.0),
            (0.0, 1.0, 0.0, 0),
        );
        assert!(save_res.is_ok(), "save failed: {:?}", save_res.err());

        let load_res = ProjectManager::load_project(&project_path);
        assert!(load_res.is_ok(), "load failed: {:?}", load_res.err());
        let (manifest, _, loaded_stack, _, _) = load_res.unwrap();

        assert_eq!(manifest.layers.len(), 2);
        assert_eq!(manifest.layers[1].layer_type, "Adjustment");
        assert_eq!(loaded_stack.layers[1].layer_type, LayerType::Adjustment);
        assert_eq!(loaded_stack.layers[1].effects.len(), 1);

        let _ = fs::remove_file(project_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn test_generator_layer_project_roundtrip() {
        let temp_dir = std::env::temp_dir().join("remoainter_gen_layer_test");
        let _ = fs::create_dir_all(&temp_dir);
        let project_path = temp_dir.join("test_gen_project.rmp");

        let mesh = Mesh::create_plane();
        let mut layer_stack = LayerStack::new(4, 4);
        layer_stack.layers.clear();

        let fill = Layer::new_fill(1, "Base Layer", [0.2, 0.2, 0.2, 1.0], 0.0, 0.8, 4, 4);
        let gen = Layer::new_generator_edge_wear(2, 4, 4);
        layer_stack.layers.push(fill);
        layer_stack.layers.push(gen);

        let baked_maps = BakedMaps::default();
        let baker_options = BakerOptions::default();

        let save_res = ProjectManager::save_project(
            &project_path,
            "GenProject",
            &mesh,
            None,
            &layer_stack,
            &baked_maps,
            &baker_options,
            (0.0, 0.0, 2.0),
            (0.0, 1.0, 0.0, 0),
        );
        assert!(save_res.is_ok(), "save failed: {:?}", save_res.err());

        let load_res = ProjectManager::load_project(&project_path);
        assert!(load_res.is_ok(), "load failed: {:?}", load_res.err());
        let (manifest, _, loaded_stack, _, _) = load_res.unwrap();

        assert_eq!(manifest.layers.len(), 2);
        assert_eq!(manifest.layers[1].layer_type, "Generator");
        assert_eq!(loaded_stack.layers[1].layer_type, LayerType::Generator);
        assert!(loaded_stack.layers[1].generator.is_some());
        let gen_cfg = loaded_stack.layers[1].generator.as_ref().unwrap();
        assert_eq!(gen_cfg.name, "Metal Edge Wear");
        assert_eq!(gen_cfg.wear_level, 0.8);
        assert!(gen_cfg.use_curvature);
        assert!(gen_cfg.use_ao);

        let _ = fs::remove_file(project_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn test_smart_material_hierarchy_project_roundtrip() {
        use crate::painting::material_gen::SmartMaterialPreset;

        let temp_dir = std::env::temp_dir().join("remoainter_smart_mat_roundtrip_test");
        let _ = fs::create_dir_all(&temp_dir);
        let project_path = temp_dir.join("test_smart_mat_project.rmp");

        let mesh = Mesh::create_plane();
        let mut layer_stack = LayerStack::new(4, 4);

        let folder_idx = layer_stack.insert_smart_material(
            SmartMaterialPreset::MachineryHeavyDuty,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let folder_id = layer_stack.layers[folder_idx].id;

        let baked_maps = BakedMaps::default();
        let baker_options = BakerOptions::default();

        let save_res = ProjectManager::save_project(
            &project_path,
            "SmartMatProject",
            &mesh,
            None,
            &layer_stack,
            &baked_maps,
            &baker_options,
            (0.0, 0.0, 2.0),
            (0.0, 1.0, 0.0, 0),
        );
        assert!(save_res.is_ok(), "save failed: {:?}", save_res.err());

        let load_res = ProjectManager::load_project(&project_path);
        assert!(load_res.is_ok(), "load failed: {:?}", load_res.err());
        let (manifest, _, loaded_stack, _, _) = load_res.unwrap();

        assert_eq!(manifest.layers.len(), layer_stack.layers.len());
        assert_eq!(loaded_stack.layers.len(), layer_stack.layers.len());

        let loaded_folder = &loaded_stack.layers[folder_idx];
        assert_eq!(loaded_folder.layer_type, LayerType::Folder);
        assert_eq!(loaded_folder.id, folder_id);

        // Verify child layers maintain their parent_folder_id
        let child_count = loaded_stack
            .layers
            .iter()
            .filter(|l| l.parent_folder_id == Some(folder_id))
            .count();
        assert!(
            child_count >= 3,
            "Must have at least 3 children parented to folder"
        );

        let _ = fs::remove_file(project_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn test_multi_texture_set_project_save_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join("remoainter_test_multi_ts");
        let _ = fs::create_dir_all(&temp_dir);
        let project_path = temp_dir.join("test_multi_ts.rmp");

        let mesh = Mesh::create_material_sphere(16, 16);
        let mut tsm = TextureSetManager::new(64);
        tsm.is_udim_workflow = true;
        tsm.sets.clear();

        // Tile 1001
        let mut set1 = TextureSet::new_udim(1, 1001, 64);
        set1.layer_stack.layers.clear();
        let mut l1 = Layer::new_paint(1, "Tile 1001 Base", 64, 64);
        l1.color_buffer[0] = [1.0, 0.2, 0.3, 1.0];
        set1.layer_stack.layers.push(l1);
        set1.layer_stack.recompute_composite();
        tsm.sets.push(set1);

        // Tile 1002
        let mut set2 = TextureSet::new_udim(2, 1002, 64);
        set2.layer_stack.layers.clear();
        let mut l2 = Layer::new_paint(1, "Tile 1002 Base", 64, 64);
        l2.color_buffer[0] = [0.1, 0.8, 0.4, 1.0];
        set2.layer_stack.layers.push(l2);
        set2.layer_stack.recompute_composite();
        tsm.sets.push(set2);

        tsm.active_index = 1;

        let baked_maps = BakedMaps::default();
        let baker_options = BakerOptions::default();

        let save_res = ProjectManager::save_project_with_texture_sets(
            &project_path,
            "MultiSetProject",
            &mesh,
            None,
            &tsm.active().layer_stack,
            &baked_maps,
            &baker_options,
            (45.0, 15.0, 2.5),
            (0.0, 1.0, 0.0, 0),
            Some(&tsm),
        );
        assert!(save_res.is_ok(), "Save failed: {:?}", save_res.err());

        // Load using load_project_with_texture_sets
        let load_res = ProjectManager::load_project_with_texture_sets(&project_path);
        assert!(load_res.is_ok(), "Load failed: {:?}", load_res.err());

        let (manifest, _, primary_stack, _, _, loaded_tsm) = load_res.unwrap();
        assert_eq!(manifest.project_name, "MultiSetProject");
        assert_eq!(manifest.texture_sets.len(), 2);
        assert_eq!(manifest.active_texture_set_index, 1);

        let tsm_loaded = loaded_tsm.expect("Expected TextureSetManager to be restored");
        assert_eq!(tsm_loaded.sets.len(), 2);
        assert_eq!(tsm_loaded.active_index, 1);
        assert_eq!(tsm_loaded.sets[0].udim_tile, Some(1001));
        assert_eq!(tsm_loaded.sets[1].udim_tile, Some(1002));
        assert_eq!(
            tsm_loaded.sets[0].layer_stack.layers[0].color_buffer[0],
            [1.0, 0.2, 0.3, 1.0]
        );
        assert_eq!(
            tsm_loaded.sets[1].layer_stack.layers[0].color_buffer[0],
            [0.1, 0.8, 0.4, 1.0]
        );

        // Primary stack should match active set (set 1)
        assert_eq!(
            primary_stack.layers[0].color_buffer[0],
            [0.1, 0.8, 0.4, 1.0]
        );

        // Also test legacy load_project backward compatibility
        let legacy_load = ProjectManager::load_project(&project_path);
        assert!(
            legacy_load.is_ok(),
            "Legacy load failed: {:?}",
            legacy_load.err()
        );
        let (_, _, legacy_stack, _, _) = legacy_load.unwrap();
        assert_eq!(legacy_stack.layers[0].color_buffer[0], [0.1, 0.8, 0.4, 1.0]);

        let _ = fs::remove_file(project_path);
        let _ = fs::remove_dir(temp_dir);
    }
}
