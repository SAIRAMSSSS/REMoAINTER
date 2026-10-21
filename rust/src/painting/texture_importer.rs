#![allow(dead_code)]
use std::path::{Path, PathBuf};
use crate::painting::layer::Layer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    BaseColor,
    Normal,
    Roughness,
    Metallic,
    AmbientOcclusion,
    Height,
}

#[derive(Debug, Clone, Default)]
pub struct PbrTextureSet {
    pub name: String,
    pub base_color: Option<PathBuf>,
    pub normal: Option<PathBuf>,
    pub roughness: Option<PathBuf>,
    pub metallic: Option<PathBuf>,
    pub ambient_occlusion: Option<PathBuf>,
    pub height: Option<PathBuf>,
    pub is_directx_normal: bool,
}

/// Detects PBR texture channel type and DirectX vs OpenGL normal map convention from filename
pub fn detect_channel_type(filename: &str) -> Option<(ChannelType, bool)> {
    let lower = filename.to_lowercase();
    let stem = Path::new(&lower).file_stem()?.to_str()?;

    // Check normal maps first to avoid false matches on "color" or "norm"
    if stem.contains("nor_dx") || stem.contains("normaldx") || stem.contains("normal_dx") || stem.contains("_n_dx") {
        return Some((ChannelType::Normal, true));
    }
    if stem.contains("nor_gl") || stem.contains("normalgl") || stem.contains("normal_gl")
        || stem.contains("normal") || stem.contains("_norm") || stem.contains("-norm")
        || stem.ends_with("_nor") || stem.ends_with("-nor") || stem.ends_with("_n")
    {
        return Some((ChannelType::Normal, false));
    }

    // Roughness
    if stem.contains("roughness") || stem.contains("_rough") || stem.contains("-rough")
        || stem.contains("_rgh") || stem.contains("-rgh") || stem.ends_with("_r")
    {
        return Some((ChannelType::Roughness, false));
    }

    // Metallic / Metalness
    if stem.contains("metallic") || stem.contains("metalness") || stem.contains("_metal")
        || stem.contains("-metal") || stem.contains("_met") || stem.contains("-met") || stem.ends_with("_m")
    {
        return Some((ChannelType::Metallic, false));
    }

    // Ambient Occlusion
    if stem.contains("ambient_occlusion") || stem.contains("ambientocclusion")
        || stem.contains("_ao") || stem.contains("-ao") || stem.contains("occlusion")
    {
        return Some((ChannelType::AmbientOcclusion, false));
    }

    // Height / Displacement
    if stem.contains("displacement") || stem.contains("_disp") || stem.contains("-disp")
        || stem.contains("height") || stem.ends_with("_h")
    {
        return Some((ChannelType::Height, false));
    }

    // Base Color / Albedo / Diffuse
    if stem.contains("albedo") || stem.contains("basecolor") || stem.contains("base_color")
        || stem.contains("diffuse") || stem.contains("_diff") || stem.contains("-diff")
        || stem.contains("_color") || stem.contains("-color") || stem.contains("_col")
        || stem.contains("-col") || stem.ends_with("_bc") || stem.ends_with("_d")
    {
        return Some((ChannelType::BaseColor, false));
    }

    None
}

impl PbrTextureSet {
    pub fn from_files(files: &[PathBuf]) -> Self {
        let mut set = Self::default();
        if let Some(first) = files.first() {
            set.name = first.file_stem().and_then(|s| s.to_str()).unwrap_or("Imported Material").to_string();
        }

        for p in files {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if let Some((channel, is_dx)) = detect_channel_type(name) {
                    match channel {
                        ChannelType::BaseColor => set.base_color = Some(p.clone()),
                        ChannelType::Normal => {
                            set.normal = Some(p.clone());
                            set.is_directx_normal = is_dx;
                        }
                        ChannelType::Roughness => set.roughness = Some(p.clone()),
                        ChannelType::Metallic => set.metallic = Some(p.clone()),
                        ChannelType::AmbientOcclusion => set.ambient_occlusion = Some(p.clone()),
                        ChannelType::Height => set.height = Some(p.clone()),
                    }
                }
            }
        }
        set
    }

    pub fn from_folder(dir: &Path) -> Self {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if ext_lower == "png" || ext_lower == "jpg" || ext_lower == "jpeg" || ext_lower == "tga" || ext_lower == "webp" || ext_lower == "exr" {
                            files.push(p);
                        }
                    }
                }
            }
        }
        let mut set = Self::from_files(&files);
        set.name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("Imported Material").to_string();
        set
    }

    /// Loads the texture set directly into the target layer's channel buffers
    pub fn load_into_layer(&self, layer: &mut Layer, force_flip_normal_y: bool) -> Result<(), String> {
        let target_w = layer.width;
        let target_h = layer.height;
        let total = (target_w * target_h) as usize;

        // 1. Base Color (sRGB)
        if let Some(ref path) = self.base_color {
            if let Ok(img) = image::open(path) {
                let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3).to_rgba8();
                for (i, pixel) in resized.pixels().enumerate() {
                    if i < total {
                        layer.color_buffer[i] = [
                            pixel[0] as f32 / 255.0,
                            pixel[1] as f32 / 255.0,
                            pixel[2] as f32 / 255.0,
                            1.0,
                        ];
                    }
                }
            }
        }

        // 2. Metallic (Linear)
        if let Some(ref path) = self.metallic {
            if let Ok(img) = image::open(path) {
                let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3).to_luma8();
                for (i, pixel) in resized.pixels().enumerate() {
                    if i < total {
                        layer.metallic_buffer[i] = pixel[0] as f32 / 255.0;
                    }
                }
            }
        }

        // 3. Roughness (Linear)
        if let Some(ref path) = self.roughness {
            if let Ok(img) = image::open(path) {
                let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3).to_luma8();
                for (i, pixel) in resized.pixels().enumerate() {
                    if i < total {
                        layer.roughness_buffer[i] = pixel[0] as f32 / 255.0;
                    }
                }
            }
        }

        // 4. Normal (Linear with DirectX to OpenGL flipping)
        if let Some(ref path) = self.normal {
            if let Ok(img) = image::open(path) {
                let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3).to_rgb8();
                let flip_g = self.is_directx_normal || force_flip_normal_y;
                for (i, pixel) in resized.pixels().enumerate() {
                    if i < total {
                        let r = pixel[0] as f32 / 255.0;
                        let g_raw = pixel[1] as f32 / 255.0;
                        let g = if flip_g { 1.0 - g_raw } else { g_raw };
                        let b = pixel[2] as f32 / 255.0;
                        layer.normal_buffer[i] = [r, g, b];
                    }
                }
            }
        }

        // 5. Height / Displacement (Linear)
        if let Some(ref path) = self.height {
            if let Ok(img) = image::open(path) {
                let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3).to_luma8();
                for (i, pixel) in resized.pixels().enumerate() {
                    if i < total {
                        layer.height_buffer[i] = (pixel[0] as f32 / 255.0) * 2.0 - 1.0;
                    }
                }
            }
        }

        Ok(())
    }

    /// Loads the ambient occlusion map if available
    pub fn load_ao_map(&self, target_w: u32, target_h: u32) -> Option<Vec<f32>> {
        if let Some(ref path) = self.ambient_occlusion {
            if let Ok(img) = image::open(path) {
                let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3).to_luma8();
                return Some(resized.pixels().map(|p| p[0] as f32 / 255.0).collect());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_channel_type_ambientcg() {
        assert_eq!(detect_channel_type("Metal001_2K_Color.png"), Some((ChannelType::BaseColor, false)));
        assert_eq!(detect_channel_type("Metal001_2K_Roughness.png"), Some((ChannelType::Roughness, false)));
        assert_eq!(detect_channel_type("Metal001_2K_Metalness.png"), Some((ChannelType::Metallic, false)));
        assert_eq!(detect_channel_type("Metal001_2K_NormalGL.png"), Some((ChannelType::Normal, false)));
        assert_eq!(detect_channel_type("Metal001_2K_NormalDX.png"), Some((ChannelType::Normal, true)));
        assert_eq!(detect_channel_type("Metal001_2K_Displacement.png"), Some((ChannelType::Height, false)));
        assert_eq!(detect_channel_type("Metal001_2K_AmbientOcclusion.png"), Some((ChannelType::AmbientOcclusion, false)));
    }

    #[test]
    fn test_detect_channel_type_polyhaven() {
        assert_eq!(detect_channel_type("rock_wall_02_diff_2k.jpg"), Some((ChannelType::BaseColor, false)));
        assert_eq!(detect_channel_type("rock_wall_02_nor_gl_2k.exr"), Some((ChannelType::Normal, false)));
        assert_eq!(detect_channel_type("rock_wall_02_nor_dx_2k.exr"), Some((ChannelType::Normal, true)));
        assert_eq!(detect_channel_type("rock_wall_02_rough_2k.png"), Some((ChannelType::Roughness, false)));
        assert_eq!(detect_channel_type("rock_wall_02_ao_2k.jpg"), Some((ChannelType::AmbientOcclusion, false)));
    }
}
