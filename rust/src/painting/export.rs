// ─────────────────────────────────────────────────────────────────────────────
// REMoAINTER — Texture Export & Channel Packing Engine (reference PBR suite Parity)
// Presets: Unreal Engine ORM, Unity URP/HDRP MaskMap, glTF 2.0, Generic PBR, EXR
// Bit depths: 8-bit, 16-bit, 32-bit Float
// Features: Declarative channel packing, UV island dilation/padding, format conversion
// ─────────────────────────────────────────────────────────────────────────────

use crate::baker::engine::BakedMaps;
use crate::graphics::mesh::Mesh;
use crate::painting::layer::LayerStack;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetChannel {
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceMap {
    BaseColor,
    Roughness,
    Metallic,
    Normal,
    NormalDirectX, // DirectX format: -Y (Green inverted)
    AmbientOcclusion,
    Curvature,
    Height,
    Thickness,
    Constant(u8), // constant 0..255 value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceComponent {
    R,
    G,
    B,
    A,
    Gray,
    InvertedGray, // 1.0 - Gray (e.g. Smoothness = 1.0 - Roughness)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelMapping {
    pub target: TargetChannel,
    pub source: SourceMap,
    pub component: SourceComponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    Png,
    Tga,
    Exr,
    Jpg,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Png => "png",
            OutputFormat::Tga => "tga",
            OutputFormat::Exr => "exr",
            OutputFormat::Jpg => "jpg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitDepth {
    U8,
    U16,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpace {
    SRgb,
    Linear,
}

/// UV gutter fill / edge-dilation algorithm (reference PBR suite parity).
///
/// Painter's export engine (`exportpresethelpers.cpp`) validates four padding
/// algorithms per export map: `passthrough`, `transparent`, `color` and
/// `diffusion`, each requiring a dilation distance except for `passthrough`.
///
/// The engine default is [`PaddingAlgorithm::Color`], which reproduces the
/// legacy averaged push-pull dilation this exporter has always used, so
/// existing presets and saved projects behave identically to previous
/// versions. The other three modes are opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PaddingAlgorithm {
    /// reference `color`: extend edge colors into the gutter, dilating all
    /// four channels (RGB + alpha). This is the exporter's original and
    /// default behavior (averaged push-pull sweep over `dilation_radius`).
    #[default]
    Color,
    /// reference `transparent`: extend edge RGB into the gutter but keep the
    /// gutter pixels fully transparent (alpha = 0) so the biome-linear
    /// sampler bleed does not alter opacity. Coverage alpha is preserved.
    Transparent,
    /// reference `diffusion`: seed the gutter with the push-pull sweep, then
    /// iteratively diffuse (blur / Jacobi relaxation of the Laplace equation)
    /// the edge values into the remaining unfilled gutter, with the UV
    /// coverage acting as a Dirichlet boundary. Produces the smoothest,
    /// most artifact-free biome mipmap down-chains.
    Diffusion,
    /// reference `passthrough`: leave the gutter completely untouched.
    Passthrough,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTextureOutput {
    pub filename_suffix: String,
    pub format: OutputFormat,
    pub bit_depth: BitDepth,
    pub color_space: ColorSpace,
    pub channels: Vec<ChannelMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub description: String,
    pub outputs: Vec<ExportTextureOutput>,
    /// Optional texture-set name used when resolving naming tokens in
    /// `filename_suffix` templates (`$set_name$`, `$texture_set$`, ...).
    /// Falls back to the mesh name when absent.
    #[serde(default)]
    pub texture_set_name: Option<String>,
    /// UV gutter fill / edge-dilation algorithm (reference parity).
    /// Defaults to [`PaddingAlgorithm::Color`] — the legacy push-pull
    /// dilation — so presets serialized before this field existed keep
    /// their exact previous behavior.
    #[serde(default)]
    pub padding_algorithm: PaddingAlgorithm,
}

impl ExportPreset {
    /// Builder-style override of the gutter padding algorithm (reference's
    /// "padding algorithm" dropdown: passthrough / transparent / color /
    /// diffusion).
    // NOTE: wired by the export dialog UI in a later integration step.
    #[allow(dead_code)]
    pub fn with_padding_algorithm(mut self, algorithm: PaddingAlgorithm) -> Self {
        self.padding_algorithm = algorithm;
        self
    }

    /// In-place override of the gutter padding algorithm.
    // NOTE: wired by the export dialog UI in a later integration step.
    #[allow(dead_code)]
    pub fn set_padding_algorithm(&mut self, algorithm: PaddingAlgorithm) {
        self.padding_algorithm = algorithm;
    }
    /// Unreal Engine 4 / 5 Packed ORM Preset
    pub fn unreal_engine() -> Self {
        Self {
            name: "Unreal Engine 4/5 (Packed ORM)".into(),
            description:
                "BaseColor (sRGB), Normal (DirectX -Y), ORM (R=AO, G=Roughness, B=Metallic)".into(),
            outputs: vec![
                ExportTextureOutput {
                    filename_suffix: "_BaseColor".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::SRgb,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::R,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::G,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::B,
                        },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_OcclusionRoughnessMetallic".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::AmbientOcclusion,
                            component: SourceComponent::Gray,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::Roughness,
                            component: SourceComponent::Gray,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::Metallic,
                            component: SourceComponent::Gray,
                        },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Normal".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::NormalDirectX,
                            component: SourceComponent::R,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::NormalDirectX,
                            component: SourceComponent::G,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::NormalDirectX,
                            component: SourceComponent::B,
                        },
                    ],
                },
            ],
            padding_algorithm: PaddingAlgorithm::Color,
            texture_set_name: None,
        }
    }

    /// Unity Universal Render Pipeline (URP) Preset
    pub fn unity_urp() -> Self {
        Self {
            name: "Unity URP (Metallic/Smoothness)".into(),
            description: "AlbedoTransparency (RGBA), MetallicSmoothness (R=Metallic, A=Smoothness), Normal (OpenGL +Y)".into(),
            outputs: vec![
                ExportTextureOutput {
                    filename_suffix: "_AlbedoTransparency".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::SRgb,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::BaseColor, component: SourceComponent::R },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::BaseColor, component: SourceComponent::G },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::BaseColor, component: SourceComponent::B },
                        ChannelMapping { target: TargetChannel::Alpha, source: SourceMap::BaseColor, component: SourceComponent::A },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_MetallicSmoothness".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::Metallic, component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::Constant(0), component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::Constant(0), component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Alpha, source: SourceMap::Roughness, component: SourceComponent::InvertedGray },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Normal".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::Normal, component: SourceComponent::R },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::Normal, component: SourceComponent::G },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::Normal, component: SourceComponent::B },
                    ],
                },
            ],
            padding_algorithm: PaddingAlgorithm::Color,
            texture_set_name: None,
        }
    }

    /// Unity HDRP (MaskMap) Preset
    pub fn unity_hdrp() -> Self {
        Self {
            name: "Unity HDRP (MaskMap Standard)".into(),
            description: "BaseColor (sRGB), MaskMap (R=Metallic, G=AO, B=Detail Mask, A=Smoothness), Normal (OpenGL)".into(),
            outputs: vec![
                ExportTextureOutput {
                    filename_suffix: "_BaseColor".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::SRgb,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::BaseColor, component: SourceComponent::R },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::BaseColor, component: SourceComponent::G },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::BaseColor, component: SourceComponent::B },
                        ChannelMapping { target: TargetChannel::Alpha, source: SourceMap::BaseColor, component: SourceComponent::A },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_MaskMap".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::Metallic, component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::AmbientOcclusion, component: SourceComponent::Gray },
                        // Unity HDRP MaskMap B = detail mask (NOT thickness/height).
                        // White (1.0) is the neutral value: detail layers are
                        // unmasked when no detail map is authored.
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::Constant(255), component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Alpha, source: SourceMap::Roughness, component: SourceComponent::InvertedGray },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Normal".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::Normal, component: SourceComponent::R },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::Normal, component: SourceComponent::G },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::Normal, component: SourceComponent::B },
                    ],
                },
            ],
            padding_algorithm: PaddingAlgorithm::Color,
            texture_set_name: None,
        }
    }

    /// glTF 2.0 (Khronos PBR Standard)
    pub fn gltf2() -> Self {
        Self {
            name: "glTF 2.0 (Khronos Standard)".into(),
            description: "baseColorTexture (sRGB), metallicRoughnessTexture (R=Occlusion for combined ORM, G=Roughness, B=Metallic per Khronos), normalTexture (OpenGL)".into(),
            outputs: vec![
                ExportTextureOutput {
                    filename_suffix: "_BaseColor".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::SRgb,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::BaseColor, component: SourceComponent::R },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::BaseColor, component: SourceComponent::G },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::BaseColor, component: SourceComponent::B },
                        ChannelMapping { target: TargetChannel::Alpha, source: SourceMap::BaseColor, component: SourceComponent::A },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_MetallicRoughness".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::AmbientOcclusion, component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::Roughness, component: SourceComponent::Gray },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::Metallic, component: SourceComponent::Gray },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Normal".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping { target: TargetChannel::Red, source: SourceMap::Normal, component: SourceComponent::R },
                        ChannelMapping { target: TargetChannel::Green, source: SourceMap::Normal, component: SourceComponent::G },
                        ChannelMapping { target: TargetChannel::Blue, source: SourceMap::Normal, component: SourceComponent::B },
                    ],
                },
            ],
            padding_algorithm: PaddingAlgorithm::Color,
            texture_set_name: None,
        }
    }

    /// Generic Individual PBR Maps (Blender, Cinema4D, Maya)
    pub fn generic_pbr() -> Self {
        Self {
            name: "Generic PBR (Individual Maps)".into(),
            description: "BaseColor, Metallic, Roughness, Normal, Height, AO, Curvature split maps"
                .into(),
            outputs: vec![
                ExportTextureOutput {
                    filename_suffix: "_BaseColor".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::SRgb,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::R,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::G,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::B,
                        },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Metallic".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![ChannelMapping {
                        target: TargetChannel::Red,
                        source: SourceMap::Metallic,
                        component: SourceComponent::Gray,
                    }],
                },
                ExportTextureOutput {
                    filename_suffix: "_Roughness".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![ChannelMapping {
                        target: TargetChannel::Red,
                        source: SourceMap::Roughness,
                        component: SourceComponent::Gray,
                    }],
                },
                ExportTextureOutput {
                    filename_suffix: "_Normal".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U16,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::Normal,
                            component: SourceComponent::R,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::Normal,
                            component: SourceComponent::G,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::Normal,
                            component: SourceComponent::B,
                        },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Height".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U16,
                    color_space: ColorSpace::Linear,
                    channels: vec![ChannelMapping {
                        target: TargetChannel::Red,
                        source: SourceMap::Height,
                        component: SourceComponent::Gray,
                    }],
                },
                ExportTextureOutput {
                    filename_suffix: "_AmbientOcclusion".into(),
                    format: OutputFormat::Png,
                    bit_depth: BitDepth::U8,
                    color_space: ColorSpace::Linear,
                    channels: vec![ChannelMapping {
                        target: TargetChannel::Red,
                        source: SourceMap::AmbientOcclusion,
                        component: SourceComponent::Gray,
                    }],
                },
            ],
            padding_algorithm: PaddingAlgorithm::Color,
            texture_set_name: None,
        }
    }

    /// 32-bit Floating-Point OpenEXR Studio Preset
    pub fn studio_exr() -> Self {
        Self {
            name: "VFX / Film Studio (32-bit OpenEXR)".into(),
            description: "High-dynamic range 32-bit float EXR files for VFX and film production"
                .into(),
            outputs: vec![
                ExportTextureOutput {
                    filename_suffix: "_BaseColor".into(),
                    format: OutputFormat::Exr,
                    bit_depth: BitDepth::F32,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::R,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::G,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::B,
                        },
                        ChannelMapping {
                            target: TargetChannel::Alpha,
                            source: SourceMap::BaseColor,
                            component: SourceComponent::A,
                        },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_Normal".into(),
                    format: OutputFormat::Exr,
                    bit_depth: BitDepth::F32,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::Normal,
                            component: SourceComponent::R,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::Normal,
                            component: SourceComponent::G,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::Normal,
                            component: SourceComponent::B,
                        },
                    ],
                },
                ExportTextureOutput {
                    filename_suffix: "_ORM".into(),
                    format: OutputFormat::Exr,
                    bit_depth: BitDepth::F32,
                    color_space: ColorSpace::Linear,
                    channels: vec![
                        ChannelMapping {
                            target: TargetChannel::Red,
                            source: SourceMap::AmbientOcclusion,
                            component: SourceComponent::Gray,
                        },
                        ChannelMapping {
                            target: TargetChannel::Green,
                            source: SourceMap::Roughness,
                            component: SourceComponent::Gray,
                        },
                        ChannelMapping {
                            target: TargetChannel::Blue,
                            source: SourceMap::Metallic,
                            component: SourceComponent::Gray,
                        },
                    ],
                },
            ],
            padding_algorithm: PaddingAlgorithm::Color,
            texture_set_name: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UV Island Coverage & Dilation / Seam Padding
// ─────────────────────────────────────────────────────────────────────────────

/// Rasterizes mesh UV triangles into a boolean grid (true = valid low-poly UV island)
pub fn rasterize_uv_coverage(mesh: &Mesh, width: usize, height: usize) -> Vec<bool> {
    let mut mask = vec![false; width * height];
    let num_triangles = mesh.indices.len() / 3;

    for t in 0..num_triangles {
        let i0 = mesh.indices[t * 3] as usize;
        let i1 = mesh.indices[t * 3 + 1] as usize;
        let i2 = mesh.indices[t * 3 + 2] as usize;

        if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
            continue;
        }

        let uv0 = mesh.uvs[i0];
        let uv1 = mesh.uvs[i1];
        let uv2 = mesh.uvs[i2];

        // Convert UV to raster pixel coordinates
        let p0 = [
            uv0[0].fract().abs() * width as f32,
            (1.0 - uv0[1].fract().abs()) * height as f32,
        ];
        let p1 = [
            uv1[0].fract().abs() * width as f32,
            (1.0 - uv1[1].fract().abs()) * height as f32,
        ];
        let p2 = [
            uv2[0].fract().abs() * width as f32,
            (1.0 - uv2[1].fract().abs()) * height as f32,
        ];

        // Triangle bounding box
        let min_x = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as usize;
        let max_x = p0[0].max(p1[0]).max(p2[0]).ceil().min((width - 1) as f32) as usize;
        let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as usize;
        let max_y = p0[1].max(p1[1]).max(p2[1]).ceil().min((height - 1) as f32) as usize;

        let edge_func = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| -> f32 {
            (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
        };

        let area = edge_func(p0, p1, p2);
        if area.abs() < 1e-6 {
            continue;
        }

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = edge_func(p1, p2, p);
                let w1 = edge_func(p2, p0, p);
                let w2 = edge_func(p0, p1, p);

                if (area > 0.0 && w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0)
                    || (area < 0.0 && w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0)
                {
                    mask[y * width + x] = true;
                }
            }
        }
    }

    mask
}

/// Alpha-channel handling during UV gutter dilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GutterAlphaMode {
    /// Dilate all four channels, including alpha (legacy "color" behavior).
    DilateAll,
    /// Dilate RGB only; after dilation the gutter alpha becomes 0 (fully
    /// transparent), while the UV-coverage alpha is preserved untouched
    /// (reference "transparent" padding).
    TransparentGutter,
}

/// Legacy averaged push-pull dilation into gutter regions by `radius` pixels
/// (all four channels — reference "color" padding). Kept as the public
/// entry point for compatibility; equivalent to
/// `dilate_rgba_mode(.., PaddingAlgorithm::Color)`.
pub fn dilate_rgba(
    buffer: &mut [[f32; 4]],
    valid_mask: &[bool],
    width: usize,
    height: usize,
    radius: usize,
) {
    push_pull_dilate(
        buffer,
        valid_mask,
        width,
        height,
        radius,
        GutterAlphaMode::DilateAll,
    );
}

/// Applies one of the four reference padding algorithms to the gutter outside
/// `valid_mask`.
pub fn dilate_rgba_mode(
    buffer: &mut [[f32; 4]],
    valid_mask: &[bool],
    width: usize,
    height: usize,
    radius: usize,
    mode: PaddingAlgorithm,
) {
    if radius == 0 || width == 0 || height == 0 || buffer.len() != width * height {
        return;
    }
    match mode {
        PaddingAlgorithm::Color => push_pull_dilate(
            buffer,
            valid_mask,
            width,
            height,
            radius,
            GutterAlphaMode::DilateAll,
        ),
        PaddingAlgorithm::Transparent => push_pull_dilate(
            buffer,
            valid_mask,
            width,
            height,
            radius,
            GutterAlphaMode::TransparentGutter,
        ),
        PaddingAlgorithm::Diffusion => diffusion_dilate(buffer, valid_mask, width, height, radius),
        PaddingAlgorithm::Passthrough => {}
    }
}

/// Dilates a boolean coverage mask itself (binary 4-neighborhood sweep), used
/// to bound the diffusion relaxation band.
fn dilate_binary_mask(mask: &[bool], width: usize, height: usize, radius: usize) -> Vec<bool> {
    let mut current = mask.to_vec();
    if (radius == 0)
        || (width == 0)
        || (height == 0)
        || (mask.len() != width * height)
        || !mask.iter().any(|&v| v)
    {
        return current;
    }
    for _ in 0..radius {
        let prev = current.clone();
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if current[idx] {
                    continue;
                }
                let expands = (x > 0 && prev[idx - 1])
                    || (x + 1 < width && prev[idx + 1])
                    || (y > 0 && prev[idx - width])
                    || (y + 1 < height && prev[idx + width]);
                if expands {
                    current[idx] = true;
                }
            }
        }
    }
    current
}

/// reference's "diffusion" padding.
///
/// Phase 1 seeds the gutter band with the conventional push-pull sweep up to
/// `radius` pixels; phase 2 runs Jacobi relaxation of the Laplace equation
/// (iterative blur) over the unfilled gutter in a band around the coverage,
/// diffusing the edge values smoothly inward with the UV coverage acting as
/// the Dirichlet boundary. This renders biome mipmaps significantly cleaner
/// than a straight push sweep.
fn diffusion_dilate(
    buffer: &mut [[f32; 4]],
    valid_mask: &[bool],
    width: usize,
    height: usize,
    radius: usize,
) {
    if radius == 0 || width == 0 || height == 0 || buffer.len() != width * height {
        return;
    }

    // Phase 1 — seed the gutter band with edge values.
    push_pull_dilate(
        buffer,
        valid_mask,
        width,
        height,
        radius,
        GutterAlphaMode::DilateAll,
    );

    // Phase 2 — divergent blur diffusion into the remaining unfilled area.
    let band = dilate_binary_mask(valid_mask, width, height, radius + 2);
    let relax: Vec<usize> = (0..width * height)
        .filter(|&i| !valid_mask[i] && band[i])
        .collect();
    if relax.is_empty() {
        return;
    }

    let passes = (radius.saturating_mul(4)).clamp(16, 128);
    for _ in 0..passes {
        // Jacobi: read every neighbor at the previous iteration so boundary
        // conditions diffuse symmetrically.
        let prev: Vec<[f32; 4]> = relax.iter().map(|&i| buffer[i]).collect();
        for &idx in relax.iter() {
            let x = idx % width;
            let y = idx / width;

            let mut sum = [0.0f32; 4];
            let mut neighbor_count = 0.0f32;

            let neighbors = [
                if x > 0 { Some(idx - 1) } else { None },
                if x + 1 < width { Some(idx + 1) } else { None },
                if y > 0 { Some(idx - width) } else { None },
                if y + 1 < height {
                    Some(idx + width)
                } else {
                    None
                },
            ];

            for n_idx in neighbors.into_iter().flatten() {
                // Coverage pixels are fixed Dirichlet values and stay in
                // `buffer`; gutter neighbors were either seeded by the
                // push sweep and are relaxed (read from `prev`), or lie
                // outside the band and never change (also read from
                // `buffer`).
                let val = match relax.binary_search(&n_idx) {
                    Ok(p) => prev[p],
                    Err(_) => buffer[n_idx],
                };
                for ch in 0..4 {
                    sum[ch] += val[ch];
                }
                neighbor_count += 1.0;
            }

            if neighbor_count > 0.0 {
                buffer[idx] = [
                    sum[0] / neighbor_count,
                    sum[1] / neighbor_count,
                    sum[2] / neighbor_count,
                    sum[3] / neighbor_count,
                ];
            }
        }
    }
}

/// Averaged push-pull dilation implementation. Iteratively floods gutter
/// pixels with the averaged values of their valid (UV-coverage) neighbors,
/// `radius` pixels at a time — reference's "color" padding algorithm.
/// `alpha_mode` selects whether alpha participates in the dilation or the
/// gutter is made transparent afterwards.
fn push_pull_dilate(
    buffer: &mut [[f32; 4]],
    valid_mask: &[bool],
    width: usize,
    height: usize,
    radius: usize,
    alpha_mode: GutterAlphaMode,
) {
    if radius == 0 || width == 0 || height == 0 || buffer.len() != width * height {
        return;
    }

    let mut current_mask = valid_mask.to_vec();
    let mut frontier = Vec::new();

    // Collect initial frontier: invalid pixels adjacent to at least 1 valid pixel
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if !current_mask[idx] {
                let has_neighbor = (x > 0 && current_mask[idx - 1])
                    || (x + 1 < width && current_mask[idx + 1])
                    || (y > 0 && current_mask[idx - width])
                    || (y + 1 < height && current_mask[idx + width]);
                if has_neighbor {
                    frontier.push(idx);
                }
            }
        }
    }

    for _ in 0..radius {
        if frontier.is_empty() {
            break;
        }

        let mut next_frontier = Vec::new();
        let mut updates = Vec::with_capacity(frontier.len());

        for &idx in &frontier {
            let x = idx % width;
            let y = idx / width;

            let mut sum = [0.0f32; 4];
            let mut count = 0.0f32;

            let neighbors = [
                if x > 0 { Some(idx - 1) } else { None },
                if x + 1 < width { Some(idx + 1) } else { None },
                if y > 0 { Some(idx - width) } else { None },
                if y + 1 < height {
                    Some(idx + width)
                } else {
                    None
                },
            ];

            for n_idx in neighbors.into_iter().flatten() {
                if current_mask[n_idx] {
                    let c = buffer[n_idx];
                    let channels = match alpha_mode {
                        GutterAlphaMode::DilateAll => 4,
                        GutterAlphaMode::TransparentGutter => 3,
                    };
                    for ch in 0..channels {
                        sum[ch] += c[ch];
                    }
                    count += 1.0;
                }
            }

            if count > 0.0 {
                let avg = match alpha_mode {
                    GutterAlphaMode::DilateAll => [
                        sum[0] / count,
                        sum[1] / count,
                        sum[2] / count,
                        sum[3] / count,
                    ],
                    // Alpha stays untouched in "transparent" mode; the gutter
                    // is zeroed after the sweep below.
                    GutterAlphaMode::TransparentGutter => [
                        sum[0] / count,
                        sum[1] / count,
                        sum[2] / count,
                        buffer[idx][3],
                    ],
                };
                updates.push((idx, avg));
            }
        }

        for (idx, color) in updates {
            buffer[idx] = color;
            current_mask[idx] = true;
        }

        for &idx in &frontier {
            let x = idx % width;
            let y = idx / width;

            let neighbors = [
                if x > 0 { Some(idx - 1) } else { None },
                if x + 1 < width { Some(idx + 1) } else { None },
                if y > 0 { Some(idx - width) } else { None },
                if y + 1 < height {
                    Some(idx + width)
                } else {
                    None
                },
            ];

            for n_idx in neighbors.into_iter().flatten() {
                if !current_mask[n_idx] {
                    next_frontier.push(n_idx);
                }
            }
        }

        next_frontier.sort_unstable();
        next_frontier.dedup();
        frontier = next_frontier;
    }

    // reference "transparent" padding: gutter alpha is fully transparent while
    // the coverage keeps its authored alpha untouched.
    if alpha_mode == GutterAlphaMode::TransparentGutter {
        for (idx, pixel) in buffer.iter_mut().enumerate() {
            if !current_mask[idx] {
                pixel[3] = 0.0;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Texture Export Execution
// ─────────────────────────────────────────────────────────────────────────────

/// Does a filename template contain naming tokens? If not, the exporter keeps
/// the legacy `"<mesh><suffix>"` concatenation, which is byte-identical to the
/// pre-token behavior.
fn uses_naming_tokens(template: &str) -> bool {
    template.contains('$')
        || template.contains("{-resolution}")
        || template.contains("{resolution}")
        || template.contains("{lod}")
        || template.contains("{udim}")
        || template.contains("<UDIM>")
        || template.contains("<udim>")
}

/// Resolves reference-Painter-style naming tokens in a filename template
/// (parity with `.spexp` `DataExportMap` file templates and the common Blender
/// / Painter token set):
///
/// - `$mesh$`, `$meshname$` — mesh (texture set fallback) name
/// - `$set_name$`, `$texture_set$`, `$textureset$` — texture set name
/// - `$resolution$`, `{resolution}` — largest texture edge, e.g. `2048`
/// - `$-resolution$`, `{-resolution}` — same, preceded by the `_` separator
/// - `$width$`, `$height$` — exact per-axis resolution
/// - `$lod$`, `{lod}` — LOD index (always `0` for full-resolution exports)
///
/// Spaces in names are normalized to `_`, matching the legacy mesh-name path.
fn resolve_naming_tokens(
    template: &str,
    mesh_name: &str,
    texture_set_name: Option<&str>,
    udim_tile: Option<u32>,
    width: usize,
    height: usize,
    lod: u32,
) -> String {
    let set_name = texture_set_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.replace(' ', "_"))
        .unwrap_or_else(|| mesh_name.to_string());
    let udim_str = udim_tile
        .map(|t| t.to_string())
        .unwrap_or_else(|| "1001".to_string());
    let square_edge = width.max(height).to_string();
    let dash_resolution = format!("_{square_edge}");

    template
        .replace("$mesh$", mesh_name)
        .replace("$meshname$", mesh_name)
        .replace("$set_name$", &set_name)
        .replace("$texture_set$", &set_name)
        .replace("$textureset$", &set_name)
        .replace("$udim$", &udim_str)
        .replace("{udim}", &udim_str)
        .replace("<UDIM>", &udim_str)
        .replace("<udim>", &udim_str)
        .replace("$width$", &width.to_string())
        .replace("$height$", &height.to_string())
        .replace("$resolution$", &square_edge)
        .replace("{resolution}", &square_edge)
        .replace("$-resolution$", &dash_resolution)
        .replace("{-resolution}", &dash_resolution)
        .replace("$lod$", &lod.to_string())
        .replace("{lod}", &lod.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Texture Export Execution
// ─────────────────────────────────────────────────────────────────────────────

pub struct TextureExportContext<'a> {
    pub layer_stack: &'a LayerStack,
    pub baked_maps: &'a BakedMaps,
    pub mesh: &'a Mesh,
    pub output_dir: &'a Path,
    pub preset: &'a ExportPreset,
    pub resolution_override: Option<(u32, u32)>,
    pub padding_radius: usize,
    pub file_format_override: Option<OutputFormat>,
    pub bit_depth_override: Option<BitDepth>,
    pub udim_tile: Option<u32>,
}

pub struct ExportResult {
    pub exported_files: Vec<PathBuf>,
    pub logs: Vec<String>,
}

/// Evaluates a single source component for a pixel index
fn sample_source_component(
    source: SourceMap,
    component: SourceComponent,
    idx: usize,
    layer_stack: &LayerStack,
    baked_maps: &BakedMaps,
) -> f32 {
    let val = match source {
        SourceMap::BaseColor => {
            let c = layer_stack
                .composite_color
                .get(idx)
                .copied()
                .unwrap_or([0.8, 0.8, 0.8, 1.0]);
            match component {
                SourceComponent::R => c[0],
                SourceComponent::G => c[1],
                SourceComponent::B => c[2],
                SourceComponent::A => c[3],
                SourceComponent::Gray => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2],
                SourceComponent::InvertedGray => {
                    1.0 - (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2])
                }
            }
        }
        SourceMap::Roughness => {
            let r = layer_stack
                .composite_roughness
                .get(idx)
                .copied()
                .unwrap_or(0.5);
            match component {
                SourceComponent::InvertedGray => 1.0 - r,
                _ => r,
            }
        }
        SourceMap::Metallic => {
            let m = layer_stack
                .composite_metallic
                .get(idx)
                .copied()
                .unwrap_or(0.0);
            match component {
                SourceComponent::InvertedGray => 1.0 - m,
                _ => m,
            }
        }
        SourceMap::Normal => {
            let n = layer_stack
                .composite_normal
                .get(idx)
                .copied()
                .unwrap_or([0.5, 0.5, 1.0]);
            match component {
                SourceComponent::R => n[0],
                SourceComponent::G => n[1],
                SourceComponent::B => n[2],
                SourceComponent::A => 1.0,
                SourceComponent::Gray => n[2],
                SourceComponent::InvertedGray => 1.0 - n[2],
            }
        }
        SourceMap::NormalDirectX => {
            let n = layer_stack
                .composite_normal
                .get(idx)
                .copied()
                .unwrap_or([0.5, 0.5, 1.0]);
            match component {
                SourceComponent::R => n[0],
                SourceComponent::G => 1.0 - n[1], // Invert Y for DirectX
                SourceComponent::B => n[2],
                SourceComponent::A => 1.0,
                SourceComponent::Gray => n[2],
                SourceComponent::InvertedGray => 1.0 - n[2],
            }
        }
        SourceMap::AmbientOcclusion => {
            let ao = baked_maps
                .ambient_occlusion
                .as_ref()
                .and_then(|b| b.get(idx))
                .copied()
                .unwrap_or(1.0);
            match component {
                SourceComponent::InvertedGray => 1.0 - ao,
                _ => ao,
            }
        }
        SourceMap::Curvature => {
            let c = baked_maps
                .curvature
                .as_ref()
                .and_then(|b| b.get(idx))
                .copied()
                .unwrap_or(0.5);
            match component {
                SourceComponent::InvertedGray => 1.0 - c,
                _ => c,
            }
        }
        SourceMap::Height => {
            let h = layer_stack
                .composite_height
                .get(idx)
                .copied()
                .unwrap_or(0.0);
            match component {
                SourceComponent::InvertedGray => 1.0 - h,
                _ => h,
            }
        }
        SourceMap::Thickness => {
            let t = baked_maps
                .thickness
                .as_ref()
                .and_then(|b| b.get(idx))
                .copied()
                .unwrap_or(0.0);
            match component {
                SourceComponent::InvertedGray => 1.0 - t,
                _ => t,
            }
        }
        SourceMap::Constant(val) => val as f32 / 255.0,
    };
    val.clamp(0.0, 1.0)
}

/// Executes export of all maps defined in the preset
pub fn execute_export(ctx: TextureExportContext) -> Result<ExportResult, String> {
    std::fs::create_dir_all(ctx.output_dir).map_err(|e| {
        format!(
            "Failed to create output directory {}: {}",
            ctx.output_dir.display(),
            e
        )
    })?;

    let src_w = ctx.layer_stack.width as usize;
    let src_h = ctx.layer_stack.height as usize;
    let (dst_w, dst_h) = ctx
        .resolution_override
        .map(|(w, h)| (w as usize, h as usize))
        .unwrap_or((src_w, src_h));

    let mut result = ExportResult {
        exported_files: Vec::new(),
        logs: Vec::new(),
    };

    let mesh_clean_name = ctx.mesh.name.replace(' ', "_");
    let padding_algorithm = ctx.preset.padding_algorithm;

    // Passthrough leaves the gutter untouched; no coverage mask is needed.
    let needs_uv_mask =
        ctx.padding_radius > 0 && padding_algorithm != PaddingAlgorithm::Passthrough;

    // Calculate UV coverage mask for dilation
    let uv_mask = if needs_uv_mask {
        let mask = rasterize_uv_coverage(ctx.mesh, dst_w, dst_h);
        // If mask is completely empty (no valid UVs found), mark all painted pixels as valid
        let has_any_uv = mask.iter().any(|&v| v);
        if has_any_uv {
            mask
        } else {
            vec![true; dst_w * dst_h]
        }
    } else {
        vec![true; dst_w * dst_h]
    };

    for output in &ctx.preset.outputs {
        let format = ctx.file_format_override.unwrap_or(output.format);
        let bit_depth = ctx.bit_depth_override.unwrap_or(output.bit_depth);

        // Naming: tokenized templates ($mesh$, $set_name$, $-resolution$, ...)
        // or the legacy "<mesh><suffix>" concatenation with UDIM disambiguation.
        let file_stem = if uses_naming_tokens(&output.filename_suffix) {
            resolve_naming_tokens(
                &output.filename_suffix,
                &mesh_clean_name,
                ctx.preset.texture_set_name.as_deref(),
                ctx.udim_tile,
                dst_w,
                dst_h,
                0,
            )
        } else {
            let udim_tag = ctx.udim_tile.map(|t| format!("_{}", t)).unwrap_or_default();
            format!("{}{}{}", mesh_clean_name, udim_tag, output.filename_suffix)
        };
        let file_name = format!("{}.{}", file_stem, format.extension());
        let file_path = ctx.output_dir.join(&file_name);

        let num_channels = if output
            .channels
            .iter()
            .any(|c| c.target == TargetChannel::Alpha)
        {
            4
        } else if output
            .channels
            .iter()
            .any(|c| c.target == TargetChannel::Blue)
        {
            3
        } else {
            1
        };

        // Construct 4-channel float buffer
        let mut float_buffer = vec![[0.0f32; 4]; dst_w * dst_h];

        for y in 0..dst_h {
            for x in 0..dst_w {
                let dst_idx = y * dst_w + x;
                // Nearest-neighbor coordinate in source layer stack
                let src_x = (x * src_w) / dst_w;
                let src_y = (y * src_h) / dst_h;
                let src_idx = src_y * src_w + src_x;

                let mut pixel = [0.0f32, 0.0f32, 0.0f32, 1.0f32];

                for mapping in &output.channels {
                    let val = sample_source_component(
                        mapping.source,
                        mapping.component,
                        src_idx,
                        ctx.layer_stack,
                        ctx.baked_maps,
                    );
                    match mapping.target {
                        TargetChannel::Red => pixel[0] = val,
                        TargetChannel::Green => pixel[1] = val,
                        TargetChannel::Blue => pixel[2] = val,
                        TargetChannel::Alpha => pixel[3] = val,
                    }
                }

                // If only 1 channel mapped (grayscale), copy Red to Green and Blue for single-channel formats
                if num_channels == 1 {
                    pixel[1] = pixel[0];
                    pixel[2] = pixel[0];
                }

                float_buffer[dst_idx] = pixel;
            }
        }

        // sRGB / linear transfer-function correctness. The layer-stack
        // composite stores BaseColor in display (gamma 2.2) space — the GL
        // viewport (`viewport_wgpu.rs`) and the CPU raytracer (`renderer.rs`)
        // both degamma sample it with `pow(2.2)`. Linear-space exports must
        // therefore receive the decoded linear values so the on-disk file
        // matches its declared color space. Non-color data (AO, roughness,
        // metallic, normal, height, thickness) is authored linear and is
        // written as-is; alpha is linear even inside sRGB textures.
        if output.color_space == ColorSpace::Linear {
            for mapping in &output.channels {
                if !matches!(mapping.source, SourceMap::BaseColor) {
                    continue;
                }
                let channel_index = match mapping.target {
                    TargetChannel::Red => 0,
                    TargetChannel::Green => 1,
                    TargetChannel::Blue => 2,
                    TargetChannel::Alpha => continue,
                };
                for pixel in float_buffer.iter_mut() {
                    pixel[channel_index] = pixel[channel_index].clamp(0.0, 1.0).powf(1.0 / 2.2);
                }
            }
        }

        // Apply UV Island Edge Dilation / Seam Padding (per-preset
        // reference padding algorithm; dilation happens in the export's
        // working space, after the linearization above). The default route
        // goes through the legacy `dilate_rgba` entry point so the default
        // behavior stays bit-identical to previous versions.
        if ctx.padding_radius > 0 {
            match padding_algorithm {
                PaddingAlgorithm::Color => dilate_rgba(
                    &mut float_buffer,
                    &uv_mask,
                    dst_w,
                    dst_h,
                    ctx.padding_radius,
                ),
                _ => dilate_rgba_mode(
                    &mut float_buffer,
                    &uv_mask,
                    dst_w,
                    dst_h,
                    ctx.padding_radius,
                    padding_algorithm,
                ),
            }
        }

        // Write file depending on format & bit depth
        match format {
            OutputFormat::Png => {
                match bit_depth {
                    BitDepth::U16 => {
                        if num_channels == 1 {
                            let mut img = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::new(
                                dst_w as u32,
                                dst_h as u32,
                            );
                            for (idx, pixel) in img.pixels_mut().enumerate() {
                                let v = (float_buffer[idx][0].clamp(0.0, 1.0) * 65535.0) as u16;
                                *pixel = image::Luma([v]);
                            }
                            img.save(&file_path).map_err(|e| e.to_string())?;
                        } else if num_channels == 3 {
                            let mut img = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::new(
                                dst_w as u32,
                                dst_h as u32,
                            );
                            for (idx, pixel) in img.pixels_mut().enumerate() {
                                let c = float_buffer[idx];
                                *pixel = image::Rgb([
                                    (c[0].clamp(0.0, 1.0) * 65535.0) as u16,
                                    (c[1].clamp(0.0, 1.0) * 65535.0) as u16,
                                    (c[2].clamp(0.0, 1.0) * 65535.0) as u16,
                                ]);
                            }
                            img.save(&file_path).map_err(|e| e.to_string())?;
                        } else {
                            let mut img = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::new(
                                dst_w as u32,
                                dst_h as u32,
                            );
                            for (idx, pixel) in img.pixels_mut().enumerate() {
                                let c = float_buffer[idx];
                                *pixel = image::Rgba([
                                    (c[0].clamp(0.0, 1.0) * 65535.0) as u16,
                                    (c[1].clamp(0.0, 1.0) * 65535.0) as u16,
                                    (c[2].clamp(0.0, 1.0) * 65535.0) as u16,
                                    (c[3].clamp(0.0, 1.0) * 65535.0) as u16,
                                ]);
                            }
                            img.save(&file_path).map_err(|e| e.to_string())?;
                        }
                    }
                    _ => {
                        // U8 or default
                        if num_channels == 1 {
                            let mut img = image::GrayImage::new(dst_w as u32, dst_h as u32);
                            for (idx, pixel) in img.pixels_mut().enumerate() {
                                let v = (float_buffer[idx][0].clamp(0.0, 1.0) * 255.0) as u8;
                                *pixel = image::Luma([v]);
                            }
                            img.save(&file_path).map_err(|e| e.to_string())?;
                        } else if num_channels == 3 {
                            let mut img = image::RgbImage::new(dst_w as u32, dst_h as u32);
                            for (idx, pixel) in img.pixels_mut().enumerate() {
                                let c = float_buffer[idx];
                                *pixel = image::Rgb([
                                    (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                                    (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                                    (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                                ]);
                            }
                            img.save(&file_path).map_err(|e| e.to_string())?;
                        } else {
                            let mut img = image::RgbaImage::new(dst_w as u32, dst_h as u32);
                            for (idx, pixel) in img.pixels_mut().enumerate() {
                                let c = float_buffer[idx];
                                *pixel = image::Rgba([
                                    (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                                    (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                                    (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                                    (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                                ]);
                            }
                            img.save(&file_path).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
            OutputFormat::Tga => {
                if num_channels <= 3 {
                    let mut img = image::RgbImage::new(dst_w as u32, dst_h as u32);
                    for (idx, pixel) in img.pixels_mut().enumerate() {
                        let c = float_buffer[idx];
                        *pixel = image::Rgb([
                            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        ]);
                    }
                    img.save(&file_path).map_err(|e| e.to_string())?;
                } else {
                    let mut img = image::RgbaImage::new(dst_w as u32, dst_h as u32);
                    for (idx, pixel) in img.pixels_mut().enumerate() {
                        let c = float_buffer[idx];
                        *pixel = image::Rgba([
                            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                        ]);
                    }
                    img.save(&file_path).map_err(|e| e.to_string())?;
                }
            }
            OutputFormat::Jpg => {
                let mut img = image::RgbImage::new(dst_w as u32, dst_h as u32);
                for (idx, pixel) in img.pixels_mut().enumerate() {
                    let c = float_buffer[idx];
                    *pixel = image::Rgb([
                        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                    ]);
                }
                img.save(&file_path).map_err(|e| e.to_string())?;
            }
            OutputFormat::Exr => {
                // Write 32-bit floating point OpenEXR using exr crate
                let fpath = file_path
                    .to_str()
                    .ok_or_else(|| "Invalid UTF-8 in file path".to_string())?;
                if num_channels <= 3 {
                    exr::prelude::write_rgb_file(fpath, dst_w, dst_h, |x, y| -> (f32, f32, f32) {
                        let idx = y * dst_w + x;
                        let c = float_buffer[idx];
                        (c[0], c[1], c[2])
                    })
                    .map_err(|e| format!("EXR export error: {:?}", e))?;
                } else {
                    exr::prelude::write_rgba_file(
                        fpath,
                        dst_w,
                        dst_h,
                        |x, y| -> (f32, f32, f32, f32) {
                            let idx = y * dst_w + x;
                            let c = float_buffer[idx];
                            (c[0], c[1], c[2], c[3])
                        },
                    )
                    .map_err(|e| format!("EXR export error: {:?}", e))?;
                }
            }
        }

        result.logs.push(format!(
            "Exported {} ({}×{}, {:?}, {:?}, padding: {:?})",
            file_name, dst_w, dst_h, format, bit_depth, padding_algorithm
        ));
        result.exported_files.push(file_path);
    }

    Ok(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_padding_algorithm_defaults_to_legacy_color() {
        assert_eq!(PaddingAlgorithm::default(), PaddingAlgorithm::Color);
        for preset in [
            ExportPreset::unreal_engine(),
            ExportPreset::unity_urp(),
            ExportPreset::unity_hdrp(),
            ExportPreset::gltf2(),
            ExportPreset::generic_pbr(),
            ExportPreset::studio_exr(),
        ] {
            assert_eq!(preset.padding_algorithm, PaddingAlgorithm::Color);
        }
    }

    #[test]
    fn test_passthrough_leaves_gutter_untouched() {
        let w = 3;
        let h = 3;
        let mut buffer = vec![[0.0f32; 4]; w * h];
        let mut mask = vec![false; w * h];
        let center_idx = w + 1;
        buffer[center_idx] = [1.0, 1.0, 1.0, 1.0];
        mask[center_idx] = true;

        dilate_rgba_mode(&mut buffer, &mask, w, h, 3, PaddingAlgorithm::Passthrough);

        // Gutter must be exactly as it was (alpha untouched as well).
        assert_eq!(buffer[0], [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(buffer[w + 2], [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(buffer[center_idx], [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_transparent_padding_zeroes_gutter_alpha() {
        let w = 3;
        let h = 3;
        let mut buffer = vec![[0.0f32; 4]; w * h];
        let mut mask = vec![false; w * h];
        let center_idx = w + 1;
        buffer[center_idx] = [1.0, 0.0, 0.0, 0.25];
        mask[center_idx] = true;

        dilate_rgba_mode(&mut buffer, &mask, w, h, 1, PaddingAlgorithm::Transparent);

        // Neighbor gutter: RGB extended from the edge, alpha forced to 0.
        let gutter_idx = w + 2;
        assert!(buffer[gutter_idx][0] > 0.0, "gutter RGB must be extended");
        assert_eq!(buffer[gutter_idx][3], 0.0);
        // Coverage alpha is preserved untouched.
        assert_eq!(buffer[center_idx][3], 0.25);
    }

    #[test]
    fn test_diffusion_padding_diffuses_into_gutter() {
        let w = 5;
        let h = 1;
        let mut buffer = vec![[0.0f32; 4]; w * h];
        let mut mask = vec![false; w];
        // Left column red, right column green, gutter in between.
        for y in 0..h {
            mask[y * w] = true;
            mask[y * w + 4] = true;
            buffer[y * w] = [1.0, 0.0, 0.0, 1.0];
            buffer[y * w + 4] = [0.0, 1.0, 0.0, 1.0];
        }

        dilate_rgba_mode(&mut buffer, &mask, w, h, 2, PaddingAlgorithm::Diffusion);

        // The gutter must have received BOTH edge colors (diffusion blends
        // both boundaries smoothly into the unfilled area).
        let mid = 2;
        assert!(
            buffer[mid][0] > 0.01,
            "red must diffuse to mid: {:?}",
            buffer[mid]
        );
        assert!(
            buffer[mid][1] > 0.01,
            "green must diffuse to mid: {:?}",
            buffer[mid]
        );
        // Boundary colors themselves are unchanged (Dirichlet condition).
        assert_eq!(buffer[0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer[4], [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn test_naming_tokens() {
        // No tokens: legacy concatenation still applies at the call site;
        // the resolver itself just substitutes.
        assert_eq!(
            resolve_naming_tokens("_BaseColor", "Cube", None, None, 2048, 1024, 0),
            "_BaseColor"
        );
        assert_eq!(
            resolve_naming_tokens("$mesh$_BaseColor", "Cube", None, None, 2048, 1024, 0),
            "Cube_BaseColor"
        );
        assert_eq!(
            resolve_naming_tokens(
                "$set_name$_$-resolution$",
                "Cube",
                Some("My Material Set"),
                None,
                2048,
                1024,
                0
            ),
            "My_Material_Set__2048"
        );
        assert_eq!(
            resolve_naming_tokens("$mesh${-resolution}", "Cube", None, None, 2048, 2048, 0),
            "Cube_2048"
        );
        assert_eq!(
            resolve_naming_tokens("$mesh$_{lod}", "Cube", None, None, 2048, 2048, 0),
            "Cube_0"
        );
        assert_eq!(
            resolve_naming_tokens("$mesh$_$udim$", "Cube", None, Some(1002), 2048, 2048, 0),
            "Cube_1002"
        );
        assert_eq!(
            resolve_naming_tokens("$mesh$_<UDIM>", "Cube", None, Some(1003), 2048, 2048, 0),
            "Cube_1003"
        );
        assert!(uses_naming_tokens("<UDIM>_BaseColor"));
        assert!(uses_naming_tokens("{udim}_Normal"));
        assert!(uses_naming_tokens("<udim>_Roughness"));
        assert!(!uses_naming_tokens("_BaseColor"));
        assert_eq!(
            resolve_naming_tokens("$width$x$height$", "Cube", None, None, 1024, 512, 0),
            "1024x512"
        );
        // Texture set fallback = mesh name when unset.
        assert_eq!(
            resolve_naming_tokens("$textureset$", "", Option::<&str>::None, None, 512, 512, 0),
            ""
        );
    }

    #[test]
    fn test_hdrp_maskmap_channel_spec() {
        // Unity HDRP MaskMap: R=Metallic, G=AO, B=Detail Mask, A=Smoothness.
        let preset = ExportPreset::unity_hdrp();
        let mask = &preset.outputs[1];
        assert_eq!(mask.filename_suffix, "_MaskMap");
        assert_eq!(mask.channels[0].target, TargetChannel::Red);
        assert_eq!(mask.channels[0].source, SourceMap::Metallic);
        assert_eq!(mask.channels[1].target, TargetChannel::Green);
        assert_eq!(mask.channels[1].source, SourceMap::AmbientOcclusion);
        assert_eq!(mask.channels[2].target, TargetChannel::Blue);
        assert!(
            matches!(mask.channels[2].source, SourceMap::Constant(255)),
            "HDRP MaskMap B must be a neutral detail mask, got {:?}",
            mask.channels[2].source
        );
        assert_eq!(mask.channels[3].target, TargetChannel::Alpha);
        assert_eq!(mask.channels[3].source, SourceMap::Roughness);
        assert_eq!(mask.channels[3].component, SourceComponent::InvertedGray);
    }

    #[test]
    fn test_unreal_orm_channel_spec() {
        // Unreal ORM: R=AO, G=Roughness, B=Metallic, linear space.
        let preset = ExportPreset::unreal_engine();
        let orm = &preset.outputs[1];
        assert_eq!(orm.filename_suffix, "_OcclusionRoughnessMetallic");
        assert_eq!(orm.color_space, ColorSpace::Linear);
        assert_eq!(orm.channels[0].source, SourceMap::AmbientOcclusion);
        assert_eq!(orm.channels[1].source, SourceMap::Roughness);
        assert_eq!(orm.channels[2].source, SourceMap::Metallic);
    }

    #[test]
    fn test_gltf_metallic_roughness_spec() {
        // Khronos glTF 2.0: metallicRoughness G=Roughness, B=Metallic with AO
        // packed in R for the combined occlusion-roughness-metallic texture.
        let preset = ExportPreset::gltf2();
        let orm = &preset.outputs[1];
        assert_eq!(orm.channels[0].source, SourceMap::AmbientOcclusion);
        assert_eq!(orm.channels[1].source, SourceMap::Roughness);
        assert_eq!(orm.channels[2].source, SourceMap::Metallic);
        assert_eq!(preset.outputs[0].color_space, ColorSpace::SRgb);
    }

    #[test]
    fn test_with_padding_algorithm_builder() {
        let preset =
            ExportPreset::unreal_engine().with_padding_algorithm(PaddingAlgorithm::Diffusion);
        assert_eq!(preset.padding_algorithm, PaddingAlgorithm::Diffusion);
    }

    #[test]
    fn test_export_presets_definitions() {
        let ue = ExportPreset::unreal_engine();
        assert_eq!(ue.outputs.len(), 3);
        assert_eq!(ue.outputs[1].filename_suffix, "_OcclusionRoughnessMetallic");

        let unity = ExportPreset::unity_urp();
        assert_eq!(unity.outputs.len(), 3);
        assert_eq!(unity.outputs[1].filename_suffix, "_MetallicSmoothness");

        let gltf = ExportPreset::gltf2();
        assert_eq!(gltf.outputs.len(), 3);

        let generic = ExportPreset::generic_pbr();
        assert_eq!(generic.outputs.len(), 6);
    }

    #[test]
    fn test_dilation_algorithm() {
        let w = 4;
        let h = 4;
        let mut buffer = vec![[0.0f32; 4]; w * h];
        let mut mask = vec![false; w * h];

        // Mark center pixel (1, 1) as valid green
        let center_idx = w + 1;
        buffer[center_idx] = [0.0, 1.0, 0.0, 1.0];
        mask[center_idx] = true;

        // Dilate by 1 pixel
        dilate_rgba(&mut buffer, &mask, w, h, 1);

        // Neighbor (2, 1) must have received color from (1, 1)
        let neighbor_idx = w + 2;
        assert_eq!(buffer[neighbor_idx], [0.0, 1.0, 0.0, 1.0]);

        // Corner (3, 3) was not reached by 1 radius dilation
        let far_idx = 3 * w + 3;
        assert_eq!(buffer[far_idx], [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_sample_source_component() {
        let stack = LayerStack::new(4, 4);
        let baked = BakedMaps::default();

        // Sample roughness from stack
        let r = sample_source_component(
            SourceMap::Roughness,
            SourceComponent::Gray,
            0,
            &stack,
            &baked,
        );
        assert!((0.0..=1.0).contains(&r));

        // Sample smoothness (inverted roughness)
        let smooth = sample_source_component(
            SourceMap::Roughness,
            SourceComponent::InvertedGray,
            0,
            &stack,
            &baked,
        );
        assert!((smooth - (1.0 - r)).abs() < 1e-5);
    }

    #[test]
    fn test_execute_export_unreal() {
        let temp_dir = std::env::temp_dir().join("remoainter_test_export_ue");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let stack = LayerStack::new(16, 16);
        let baked = BakedMaps::default();
        let mesh = Mesh::create_cube();
        let preset = ExportPreset::unreal_engine();

        let ctx = TextureExportContext {
            layer_stack: &stack,
            baked_maps: &baked,
            mesh: &mesh,
            output_dir: &temp_dir,
            preset: &preset,
            resolution_override: Some((32, 32)),
            padding_radius: 2,
            file_format_override: Some(OutputFormat::Png),
            bit_depth_override: Some(BitDepth::U8),
            udim_tile: None,
        };

        let result = execute_export(ctx).expect("Export should succeed");
        assert_eq!(result.exported_files.len(), 3);
        for f in &result.exported_files {
            assert!(f.exists(), "File should exist: {:?}", f);
            let metadata = std::fs::metadata(f).unwrap();
            assert!(metadata.len() > 0, "File should not be empty: {:?}", f);
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
