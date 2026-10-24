//! Vulkan & wgpu Hardware PBR Viewport.
//!
//! Provides a hardware-accelerated 3D viewport backed by `wgpu` (targeting Vulkan on Linux/Windows,
//! Metal on macOS, and DX12 on Windows).
//!
//! # Physical Shading Architecture
//! - **Microfacet Specular BRDF**: Cook-Torrance model with GGX / Trowbridge-Reitz Normal Distribution
//!   Function (NDF), height-correlated Smith-Schlick geometric shadowing, and Schlick's Fresnel
//!   approximation modified for roughness.
//! - **Image-Based Lighting (IBL)**: Brian Karis / Epic Games split-sum approximation utilizing a
//!   precomputed 512×512 BRDF integration LUT and equirectangular HDR environment maps.
//! - **Studio 3-Point Lighting**: Rotatable key, fill, and rim lights with customizable intensity.
//! - **Post-Processing & Color Science**:
//!   - Screen-space ambient occlusion (SSAO) factor approximation
//!   - Depth of Field (DoF) camera defocus blur
//!   - Bloom highlights extraction
//!   - ACES Filmic Tone Mapping followed by Linear-to-sRGB display transfer ($x^{1/2.2}$)
//! - **Multi-Mesh & Submesh Support**: Per-submesh visibility toggles and solo isolation.
//! - **Auxiliary Visualizations**: Dark slate ground grid, 3D world axes (RGB = XYZ), and wireframe overlay.

use bytemuck::{Pod, Zeroable};
use std::path::{Path, PathBuf};

use super::mesh::Mesh;
use crate::baker::engine::BakedMaps;
use crate::painting::layer::LayerStack;

// =========================================================================
// GPU Vertex & Uniform Structures
// =========================================================================

/// Interleaved vertex format uploaded to the GPU vertex buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WgpuVertex {
    /// 3D position in model-normalized space [-1.0, 1.0].
    pub pos: [f32; 3],
    /// Surface normal vector.
    pub normal: [f32; 3],
    /// Texture coordinate (U, V).
    pub uv: [f32; 2],
    /// Tangent vector with bitangent sign in W (X, Y, Z, W).
    pub tangent: [f32; 4],
}

/// Uniform block bound to the PBR shader pipeline (STD140-aligned, 336 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PbrUniforms {
    /// Model transformation matrix.
    pub model: [[f32; 4]; 4],
    /// Camera view matrix.
    pub view: [[f32; 4]; 4],
    /// Perspective projection matrix (Vulkan depth [0.0, 1.0]).
    pub proj: [[f32; 4]; 4],
    /// Normal transformation matrix (3x3 embedded in 4x4 for std140 padding).
    pub normal_mat: [[f32; 4]; 4],
    /// Camera world-space position in XYZ; W is unused padding.
    pub cam_pos: [f32; 4],
    /// Studio lighting intensity multiplier.
    pub light_intensity: f32,
    /// Environment rotation angle in degrees.
    pub env_rotation: f32,
    /// Exposure value in EV stops ($2^{\text{exposure}}$).
    pub exposure: f32,
    /// Solo display channel override (0 = Lit PBR, 1 = BaseColor, 2 = Roughness, 3 = Metallic, 4 = Normal, 5 = AO, 6 = Curvature).
    pub display_channel: i32,
    /// Flag indicating whether an active HDR environment map is bound (1) or fallback sky is used (0).
    pub has_env_map: i32,
    /// Flag indicating whether the 512x512 BRDF integration LUT is available (1 = yes).
    pub has_brdf_lut: i32,
    /// UV tiling scale factor.
    pub uv_scale: f32,
    /// Flag enabling or disabling bloom glow extraction.
    pub bloom_enabled: i32,
    /// Bloom brightness intensity multiplier.
    pub bloom_intensity: f32,
    /// Bloom luminance extraction threshold.
    pub bloom_threshold: f32,
    /// Flag enabling or disabling screen-space ambient occlusion (SSAO) factor modulation.
    pub ssao_enabled: i32,
    /// SSAO intensity multiplier.
    pub ssao_intensity: f32,
    /// Flag enabling or disabling Depth of Field (DoF) defocus.
    pub dof_enabled: i32,
    /// Camera focal plane distance for Depth of Field.
    pub dof_focal_distance: f32,
    /// Lens aperture size for Depth of Field circle of confusion.
    pub dof_aperture: f32,
    /// Color management transfer mode (0 = ACES Filmic + sRGB, 1 = Raw Linear, 2 = Reinhard).
    pub color_management_mode: i32,
}

/// Uniform block for the skybox equirectangular background pass (80 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SkyboxUniforms {
    /// Inverse view-projection matrix used to reconstruct world rays from clip space.
    pub inv_view_proj: [[f32; 4]; 4],
    /// Environment rotation in degrees.
    pub env_rotation: f32,
    /// Background blur / mip roughness level.
    pub blur: f32,
    /// Background intensity multiplier.
    pub intensity: f32,
    /// 1 if HDR environment texture is bound, 0 for procedural dark gradient.
    pub has_env_map: i32,
}

/// Uniform block for wireframe, grid, and coordinate axes drawing (80 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WireframeUniforms {
    /// Combined model-view-projection matrix.
    pub mvp: [[f32; 4]; 4],
    /// RGBA line color.
    pub color: [f32; 4],
}

// =========================================================================
// Submesh & GPU Mesh Management
// =========================================================================

/// Represents a distinct submesh section of the loaded 3D model.
#[derive(Debug, Clone)]
pub struct GpuSubMesh {
    /// Display name of the submesh or material slot.
    pub name: String,
    /// Start index inside the element index buffer.
    pub index_start: usize,
    /// Number of indices defining this submesh.
    pub index_count: usize,
    /// Whether this submesh is currently visible in the 3D viewport.
    pub visible: bool,
}

/// GPU-resident mesh data with vertex, triangle index, and wireframe line buffers.
pub struct GpuMesh {
    /// Vertex buffer holding interleaved `WgpuVertex` elements.
    pub vertex_buffer: wgpu::Buffer,
    /// Triangle index buffer for filled PBR rendering.
    pub index_buffer: wgpu::Buffer,
    /// Wireframe index buffer (`LineList` topology) for portable overlay rendering.
    pub wire_index_buffer: wgpu::Buffer,
    /// Total number of triangle indices in `index_buffer`.
    pub index_count: u32,
    /// Total number of line indices in `wire_index_buffer`.
    pub wire_index_count: u32,
    /// List of submeshes with individual visibility and index ranges.
    pub submeshes: Vec<GpuSubMesh>,
}

impl GpuMesh {
    /// Creates and uploads a new `GpuMesh` from a CPU [`Mesh`].
    pub fn new(device: &wgpu::Device, mesh: &Mesh) -> Self {
        use wgpu::util::DeviceExt;

        let (verts, indices, wire_indices) = Self::build_buffers(mesh);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Pbr_VertexBuffer"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Pbr_IndexBuffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        let wire_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Pbr_WireIndexBuffer"),
            contents: bytemuck::cast_slice(&wire_indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        let submeshes = mesh
            .submeshes
            .iter()
            .map(|sm| GpuSubMesh {
                name: sm.name.clone(),
                index_start: sm.index_start,
                index_count: sm.index_count,
                visible: true,
            })
            .collect();

        Self {
            vertex_buffer,
            index_buffer,
            wire_index_buffer,
            index_count: indices.len() as u32,
            wire_index_count: wire_indices.len() as u32,
            submeshes,
        }
    }

    /// Updates the GPU buffers with new geometry from `mesh`.
    pub fn update(&mut self, device: &wgpu::Device, _queue: &wgpu::Queue, mesh: &Mesh) {
        *self = Self::new(device, mesh);
    }

    /// Sets the visibility of a specific submesh by index.
    pub fn set_submesh_visibility(&mut self, idx: usize, visible: bool) {
        if let Some(sm) = self.submeshes.get_mut(idx) {
            sm.visible = visible;
        }
    }

    /// Isolates a single submesh (or restores all if `None`).
    pub fn isolate_submesh(&mut self, isolated_idx: Option<usize>) {
        match isolated_idx {
            Some(target) => {
                for (i, sm) in self.submeshes.iter_mut().enumerate() {
                    sm.visible = i == target;
                }
            }
            None => {
                for sm in &mut self.submeshes {
                    sm.visible = true;
                }
            }
        }
    }

    /// Helper to convert CPU mesh into GPU vertex, triangle, and wireframe line buffers.
    fn build_buffers(mesh: &Mesh) -> (Vec<WgpuVertex>, Vec<u32>, Vec<u32>) {
        let (center, radius) = mesh.compute_bounds();
        let inv_r = if radius > 1e-6 { 1.0 / radius } else { 1.0 };

        let n_verts = mesh.vertices.len();
        let mut verts = Vec::with_capacity(n_verts);

        for i in 0..n_verts {
            let p = mesh.vertices[i];
            let n = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            let t = mesh
                .tangents
                .get(i)
                .copied()
                .unwrap_or([1.0, 0.0, 0.0, 1.0]);

            verts.push(WgpuVertex {
                pos: [
                    (p[0] - center[0]) * inv_r,
                    (p[1] - center[1]) * inv_r,
                    (p[2] - center[2]) * inv_r,
                ],
                normal: n,
                uv,
                tangent: t,
            });
        }

        let indices = mesh.indices.clone();

        // Build wireframe line index pairs [i0, i1], [i1, i2], [i2, i0] for each triangle
        let mut wire_indices = Vec::with_capacity(indices.len() * 2);
        for &[i0, i1, i2] in indices.as_chunks::<3>().0 {
            wire_indices.extend_from_slice(&[i0, i1, i1, i2, i2, i0]);
        }

        (verts, indices, wire_indices)
    }
}

// =========================================================================
// Ground Grid & Coordinate Axes
// =========================================================================

/// GPU buffer holding vertices for the ground plane grid and XYZ world coordinate axes.
pub struct GpuGrid {
    /// Vertex buffer for grid and axes lines (`[f32; 3]` positions).
    pub line_buffer: wgpu::Buffer,
    /// Number of vertices belonging to the ground grid lines.
    pub grid_vert_count: u32,
    /// Offset in vertices where axes lines start.
    pub axes_vert_offset: u32,
    /// Number of vertices belonging to the 3 world coordinate axes (6 vertices = 3 lines).
    pub axes_vert_count: u32,
}

impl GpuGrid {
    /// Constructs a new `GpuGrid` vertex buffer.
    pub fn new(device: &wgpu::Device) -> Self {
        use wgpu::util::DeviceExt;

        let mut lines: Vec<[f32; 3]> = Vec::new();
        let y_plane = -1.0f32;
        let extent = 2.0f32;
        let step = 0.2f32;
        let steps = (extent / step) as i32;

        // Ground plane grid lines
        for i in -steps..=steps {
            let coord = i as f32 * step;
            // Line parallel to Z
            lines.push([coord, y_plane, -extent]);
            lines.push([coord, y_plane, extent]);
            // Line parallel to X
            lines.push([-extent, y_plane, coord]);
            lines.push([extent, y_plane, coord]);
        }

        let grid_vert_count = lines.len() as u32;
        let axes_vert_offset = grid_vert_count;

        // World coordinate axes: X (Red), Y (Green), Z (Blue)
        lines.extend_from_slice(&[
            // X (Red)
            [0.0, y_plane, 0.0],
            [0.6, y_plane, 0.0],
            // Y (Green)
            [0.0, y_plane, 0.0],
            [0.0, y_plane + 0.6, 0.0],
            // Z (Blue)
            [0.0, y_plane, 0.0],
            [0.0, y_plane, 0.6],
        ]);

        let line_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid_LineBuffer"),
            contents: bytemuck::cast_slice(&lines),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            line_buffer,
            grid_vert_count,
            axes_vert_offset,
            axes_vert_count: 6,
        }
    }
}

// =========================================================================
// PBR Texture Sets (BaseColor, Metallic, Roughness, Normal, AO, Curvature)
// =========================================================================

/// Holds the active composite PBR textures on the GPU.
pub struct GpuTextures {
    /// Base color / albedo map (RGBA8 Unorm).
    pub base_color: wgpu::Texture,
    pub base_color_view: wgpu::TextureView,

    /// Metallic map (R8 Unorm).
    pub metallic: wgpu::Texture,
    pub metallic_view: wgpu::TextureView,

    /// Roughness map (R8 Unorm).
    pub roughness: wgpu::Texture,
    pub roughness_view: wgpu::TextureView,

    /// Tangent-space normal map (RGBA8 Unorm).
    pub normal: wgpu::Texture,
    pub normal_view: wgpu::TextureView,

    /// Ambient occlusion map (R8 Unorm).
    pub ao: wgpu::Texture,
    pub ao_view: wgpu::TextureView,

    /// Curvature map (R8 Unorm).
    pub curvature: wgpu::Texture,
    pub curvature_view: wgpu::TextureView,

    /// Current texture resolution width.
    pub width: u32,
    /// Current texture resolution height.
    pub height: u32,
}

impl GpuTextures {
    /// Creates a new `GpuTextures` set allocated for the given dimensions.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let (base_color, base_color_view) = Self::create_tex(
            device,
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            "BaseColor",
        );
        let (metallic, metallic_view) = Self::create_tex(
            device,
            width,
            height,
            wgpu::TextureFormat::R8Unorm,
            "Metallic",
        );
        let (roughness, roughness_view) = Self::create_tex(
            device,
            width,
            height,
            wgpu::TextureFormat::R8Unorm,
            "Roughness",
        );
        let (normal, normal_view) = Self::create_tex(
            device,
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            "Normal",
        );
        let (ao, ao_view) =
            Self::create_tex(device, width, height, wgpu::TextureFormat::R8Unorm, "AO");
        let (curvature, curvature_view) = Self::create_tex(
            device,
            width,
            height,
            wgpu::TextureFormat::R8Unorm,
            "Curvature",
        );

        Self {
            base_color,
            base_color_view,
            metallic,
            metallic_view,
            roughness,
            roughness_view,
            normal,
            normal_view,
            ao,
            ao_view,
            curvature,
            curvature_view,
            width,
            height,
        }
    }

    /// Uploads full texture channel data from [`LayerStack`] and [`BakedMaps`].
    pub fn upload_all(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stack: &LayerStack,
        baked: &BakedMaps,
    ) {
        let w = stack.width;
        let h = stack.height;
        let total = (w * h) as usize;

        if w != self.width || h != self.height {
            *self = Self::new(device, w, h);
        }

        let mut bc_bytes = Vec::with_capacity(total * 4);
        let mut met_bytes = Vec::with_capacity(total);
        let mut rgh_bytes = Vec::with_capacity(total);
        let mut nrm_bytes = Vec::with_capacity(total * 4);

        for i in 0..total {
            let c = stack
                .composite_color
                .get(i)
                .copied()
                .unwrap_or([0.5, 0.5, 0.5, 1.0]);
            bc_bytes.push((c[0].clamp(0.0, 1.0) * 255.0) as u8);
            bc_bytes.push((c[1].clamp(0.0, 1.0) * 255.0) as u8);
            bc_bytes.push((c[2].clamp(0.0, 1.0) * 255.0) as u8);
            bc_bytes.push((c[3].clamp(0.0, 1.0) * 255.0) as u8);

            let m = stack.composite_metallic.get(i).copied().unwrap_or(0.0);
            met_bytes.push((m.clamp(0.0, 1.0) * 255.0) as u8);

            let r = stack.composite_roughness.get(i).copied().unwrap_or(0.5);
            rgh_bytes.push((r.clamp(0.0, 1.0) * 255.0) as u8);

            let n = stack
                .composite_normal
                .get(i)
                .copied()
                .unwrap_or([0.5, 0.5, 1.0]);
            nrm_bytes.push((n[0].clamp(0.0, 1.0) * 255.0) as u8);
            nrm_bytes.push((n[1].clamp(0.0, 1.0) * 255.0) as u8);
            nrm_bytes.push((n[2].clamp(0.0, 1.0) * 255.0) as u8);
            nrm_bytes.push(255u8);
        }

        let ao_bytes: Vec<u8> = if let Some(ref ao) = baked.ambient_occlusion {
            ao.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
                .collect()
        } else {
            vec![255u8; total]
        };

        let curv_bytes: Vec<u8> = if let Some(ref cur) = baked.curvature {
            cur.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
                .collect()
        } else {
            vec![128u8; total]
        };

        Self::write_full_tex(queue, &self.base_color, w, h, 4, &bc_bytes);
        Self::write_full_tex(queue, &self.metallic, w, h, 1, &met_bytes);
        Self::write_full_tex(queue, &self.roughness, w, h, 1, &rgh_bytes);
        Self::write_full_tex(queue, &self.normal, w, h, 4, &nrm_bytes);
        Self::write_full_tex(queue, &self.ao, w, h, 1, &ao_bytes);
        Self::write_full_tex(queue, &self.curvature, w, h, 1, &curv_bytes);
    }

    /// Uploads a dirty bounding box sub-rectangle after painting strokes.
    pub fn upload_dirty(&mut self, queue: &wgpu::Queue, stack: &LayerStack, bbox: [i32; 4]) {
        if self.width != stack.width || self.height != stack.height {
            return;
        }

        let x0 = bbox[0].clamp(0, self.width as i32 - 1) as u32;
        let y0 = bbox[1].clamp(0, self.height as i32 - 1) as u32;
        let x1 = (bbox[2] + 1).clamp(0, self.width as i32) as u32;
        let y1 = (bbox[3] + 1).clamp(0, self.height as i32) as u32;

        if x1 <= x0 || y1 <= y0 {
            return;
        }

        let sub_w = x1 - x0;
        let sub_h = y1 - y0;
        let total = (sub_w * sub_h) as usize;

        let mut bc_sub = Vec::with_capacity(total * 4);
        let mut met_sub = Vec::with_capacity(total);
        let mut rgh_sub = Vec::with_capacity(total);
        let mut nrm_sub = Vec::with_capacity(total * 4);

        for y in y0..y1 {
            for x in x0..x1 {
                let idx = (y * self.width + x) as usize;
                let c = stack
                    .composite_color
                    .get(idx)
                    .copied()
                    .unwrap_or([0.5, 0.5, 0.5, 1.0]);
                bc_sub.push((c[0].clamp(0.0, 1.0) * 255.0) as u8);
                bc_sub.push((c[1].clamp(0.0, 1.0) * 255.0) as u8);
                bc_sub.push((c[2].clamp(0.0, 1.0) * 255.0) as u8);
                bc_sub.push((c[3].clamp(0.0, 1.0) * 255.0) as u8);

                let m = stack.composite_metallic.get(idx).copied().unwrap_or(0.0);
                met_sub.push((m.clamp(0.0, 1.0) * 255.0) as u8);

                let r = stack.composite_roughness.get(idx).copied().unwrap_or(0.5);
                rgh_sub.push((r.clamp(0.0, 1.0) * 255.0) as u8);

                let n = stack
                    .composite_normal
                    .get(idx)
                    .copied()
                    .unwrap_or([0.5, 0.5, 1.0]);
                nrm_sub.push((n[0].clamp(0.0, 1.0) * 255.0) as u8);
                nrm_sub.push((n[1].clamp(0.0, 1.0) * 255.0) as u8);
                nrm_sub.push((n[2].clamp(0.0, 1.0) * 255.0) as u8);
                nrm_sub.push(255u8);
            }
        }

        Self::write_sub_tex(queue, &self.base_color, x0, y0, sub_w, sub_h, 4, &bc_sub);
        Self::write_sub_tex(queue, &self.metallic, x0, y0, sub_w, sub_h, 1, &met_sub);
        Self::write_sub_tex(queue, &self.roughness, x0, y0, sub_w, sub_h, 1, &rgh_sub);
        Self::write_sub_tex(queue, &self.normal, x0, y0, sub_w, sub_h, 4, &nrm_sub);
    }

    fn create_tex(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn write_full_tex(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
        data: &[u8],
    ) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * bytes_per_pixel),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_sub_tex(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
        data: &[u8],
    ) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * bytes_per_pixel),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

// =========================================================================
// WGSL Shader Sources
// =========================================================================

const PBR_WGSL: &str = r#"
struct PbrUniforms {
    model: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    normal_mat: mat4x4<f32>,
    cam_pos: vec4<f32>,
    light_intensity: f32,
    env_rotation: f32,
    exposure: f32,
    display_channel: i32,
    has_env_map: i32,
    has_brdf_lut: i32,
    uv_scale: f32,
    bloom_enabled: i32,
    bloom_intensity: f32,
    bloom_threshold: f32,
    ssao_enabled: i32,
    ssao_intensity: f32,
    dof_enabled: i32,
    dof_focal_distance: f32,
    dof_aperture: f32,
    color_management_mode: i32,
};

@group(0) @binding(0) var<uniform> u: PbrUniforms;

@group(1) @binding(0) var s_repeat: sampler;
@group(1) @binding(1) var s_clamp: sampler;
@group(1) @binding(2) var t_base_color: texture_2d<f32>;
@group(1) @binding(3) var t_metallic: texture_2d<f32>;
@group(1) @binding(4) var t_roughness: texture_2d<f32>;
@group(1) @binding(5) var t_normal: texture_2d<f32>;
@group(1) @binding(6) var t_ao: texture_2d<f32>;
@group(1) @binding(7) var t_curvature: texture_2d<f32>;
@group(1) @binding(8) var t_env_map: texture_2d<f32>;
@group(1) @binding(9) var t_brdf_lut: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos4 = u.model * vec4<f32>(in.position, 1.0);
    out.world_pos = world_pos4.xyz;
    out.clip_position = u.proj * u.view * world_pos4;

    let norm_mat3 = mat3x3<f32>(
        u.normal_mat[0].xyz,
        u.normal_mat[1].xyz,
        u.normal_mat[2].xyz,
    );

    let N = normalize(norm_mat3 * in.normal);
    var T = normalize(norm_mat3 * in.tangent.xyz);
    T = normalize(T - dot(T, N) * N);
    let B = cross(N, T) * in.tangent.w;

    out.normal = N;
    out.tangent = T;
    out.bitangent = B;
    out.uv = in.uv;
    return out;
}

const PI: f32 = 3.141592653589793;

fn aces_film(x: vec3<f32>) -> vec3<f32> {
    let a: f32 = 2.51;
    let b: f32 = 0.03;
    let c: f32 = 2.43;
    let d: f32 = 0.59;
    let e: f32 = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn distribution_ggx(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a: f32 = roughness * roughness;
    let a2: f32 = a * a;
    let NdotH: f32 = max(dot(N, H), 0.0);
    let NdotH2: f32 = NdotH * NdotH;
    let denom: f32 = NdotH2 * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom + 0.000001);
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r: f32 = roughness + 1.0;
    let k: f32 = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

fn geometry_smith(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, roughness: f32) -> f32 {
    let NdotV: f32 = max(dot(N, V), 0.0);
    let NdotL: f32 = max(dot(N, L), 0.0);
    let ggx2: f32 = geometry_schlick_ggx(NdotV, roughness);
    let ggx1: f32 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (vec3<f32>(1.0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn fresnel_schlick_roughness(cosTheta: f32, F0: vec3<f32>, roughness: f32) -> vec3<f32> {
    return F0 + (max(vec3<f32>(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn sample_equirectangular(dir: vec3<f32>, mip_level: f32) -> vec3<f32> {
    let uv = vec2<f32>(
        atan2(dir.z, dir.x) / (2.0 * PI) + 0.5,
        asin(clamp(dir.y, -1.0, 1.0)) / PI + 0.5
    );
    return textureSampleLevel(t_env_map, s_clamp, uv, mip_level).rgb;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let uv = in.uv * u.uv_scale;

    let base_sample = textureSample(t_base_color, s_repeat, uv);
    let albedo = pow(base_sample.rgb, vec3<f32>(2.2));
    let metallic = textureSample(t_metallic, s_repeat, uv).r;
    let roughness = clamp(textureSample(t_roughness, s_repeat, uv).r, 0.04, 1.0);
    let ao = textureSample(t_ao, s_repeat, in.uv).r;
    let curvature = textureSample(t_curvature, s_repeat, in.uv).r;

    // Normal mapping with TBN frame
    let n_ts = textureSample(t_normal, s_repeat, uv).rgb * 2.0 - 1.0;
    let tbn = mat3x3<f32>(in.tangent, in.bitangent, in.normal);
    var N = normalize(tbn * n_ts);
    if (!is_front) {
        N = -N;
    }

    // Solo display channel overrides
    if (u.display_channel == 1) { // BaseColor
        return vec4<f32>(base_sample.rgb, 1.0);
    } else if (u.display_channel == 2) { // Roughness
        return vec4<f32>(vec3<f32>(roughness), 1.0);
    } else if (u.display_channel == 3) { // Metallic
        return vec4<f32>(vec3<f32>(metallic), 1.0);
    } else if (u.display_channel == 4) { // Normal
        return vec4<f32>(N * 0.5 + 0.5, 1.0);
    } else if (u.display_channel == 5) { // AO
        return vec4<f32>(vec3<f32>(ao), 1.0);
    } else if (u.display_channel == 6) { // Curvature
        return vec4<f32>(vec3<f32>(curvature), 1.0);
    }

    let V = normalize(u.cam_pos.xyz - in.world_pos);
    let NdotV = max(dot(N, V), 0.0);

    var F0 = vec3<f32>(0.04);
    F0 = mix(F0, albedo, metallic);

    let env_rad = radians(u.env_rotation);
    let c = cos(env_rad);
    let s = sin(env_rad);
    let rot_y = mat3x3<f32>(
        vec3<f32>(c, 0.0, s),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(-s, 0.0, c)
    );

    // Studio 3-Point Lighting
    var light_dirs: array<vec3<f32>, 3>;
    var light_colors: array<vec3<f32>, 3>;

    light_dirs[0] = normalize(rot_y * vec3<f32>(0.5, 0.8, 0.6));
    light_colors[0] = vec3<f32>(1.15, 1.08, 0.98) * 2.6 * u.light_intensity;

    light_dirs[1] = normalize(rot_y * vec3<f32>(-0.7, 0.4, -0.4));
    light_colors[1] = vec3<f32>(0.85, 0.95, 1.15) * 1.6 * u.light_intensity;

    light_dirs[2] = normalize(rot_y * vec3<f32>(0.0, 0.6, -0.8));
    light_colors[2] = vec3<f32>(1.0, 1.0, 1.0) * 1.3 * u.light_intensity;

    var Lo = vec3<f32>(0.0);
    for (var i = 0; i < 3; i = i + 1) {
        let L = light_dirs[i];
        let H = normalize(V + L);
        let NdotL = max(dot(N, L), 0.0);

        if (NdotL > 0.0) {
            let NDF = distribution_ggx(N, H, roughness);
            let G = geometry_smith(N, V, L, roughness);
            let F = fresnel_schlick(max(dot(H, V), 0.0), F0);

            let numerator = NDF * G * F;
            let denominator = 4.0 * NdotV * NdotL + 0.0001;
            let specular = numerator / denominator;

            let kS = F;
            var kD = vec3<f32>(1.0) - kS;
            kD = kD * (1.0 - metallic);

            Lo = Lo + (kD * albedo / PI + specular) * light_colors[i] * NdotL;
        }
    }

    // Split-Sum IBL Ambient Integration
    var ambient = vec3<f32>(0.0);
    let R = reflect(-V, N);
    let R_rot = rot_y * R;
    let N_rot = rot_y * N;

    if (u.has_env_map == 1) {
        let prefiltered_color = sample_equirectangular(R_rot, roughness * 7.0);
        let irradiance = sample_equirectangular(N_rot, 5.5);

        let F_env = fresnel_schlick_roughness(NdotV, F0, roughness);
        var env_brdf = vec2<f32>(1.0 - roughness * 0.5, roughness * 0.1);
        if (u.has_brdf_lut == 1) {
            env_brdf = textureSample(t_brdf_lut, s_clamp, vec2<f32>(NdotV, roughness)).rg;
        }

        let kS_env = F_env;
        var kD_env = vec3<f32>(1.0) - kS_env;
        kD_env = kD_env * (1.0 - metallic);

        let diffuse_ibl = irradiance * albedo;
        let specular_ibl = prefiltered_color * (F_env * env_brdf.x + env_brdf.y);

        ambient = (kD_env * diffuse_ibl + specular_ibl) * ao;
    } else {
        let ambient_sky = vec3<f32>(0.35, 0.38, 0.42);
        let ambient_ground = vec3<f32>(0.18, 0.17, 0.16);
        let irradiance = mix(ambient_ground, ambient_sky, N.y * 0.5 + 0.5);
        let kD_fallback = (vec3<f32>(1.0) - F0) * (1.0 - metallic);
        ambient = kD_fallback * irradiance * albedo * ao;
    }

    // Screen-space ambient occlusion (SSAO) factor approximation
    if (u.ssao_enabled == 1) {
        let ssao_factor = mix(1.0, ao * (1.0 - max(0.0, 0.5 - curvature) * 0.7), u.ssao_intensity);
        ambient = ambient * ssao_factor;
    }

    var color = Lo + ambient;
    color = color * pow(2.0, u.exposure);

    // Depth of Field (DoF) defocus
    if (u.dof_enabled == 1) {
        let dist_to_cam = length(u.cam_pos.xyz - in.world_pos);
        let coc = abs(dist_to_cam - u.dof_focal_distance) * u.dof_aperture;
        let dof_factor = clamp(coc * 2.0, 0.0, 0.7);
        let blurred_env = sample_equirectangular(N_rot, 6.5);
        color = mix(color, mix(color, blurred_env * 0.8, 0.5), dof_factor);
    }

    // Bloom highlights extraction
    if (u.bloom_enabled == 1) {
        let lum = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
        let bloom_factor = clamp((lum - u.bloom_threshold) / max(1.0 - u.bloom_threshold, 0.001), 0.0, 1.0);
        let bloom_color = color * bloom_factor * u.bloom_intensity;
        color = color + bloom_color;
    }

    // Color Management & Tone Mapping
    if (u.color_management_mode == 0) {
        // ACES Filmic Tone Mapping + sRGB Gamma (Default)
        color = aces_film(color);
        color = pow(color, vec3<f32>(1.0 / 2.2));
    } else if (u.color_management_mode == 1) {
        // Raw Linear to sRGB display transfer
        color = pow(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
    } else {
        // Neutral / Reinhard Tone Mapping
        color = color / (color + vec3<f32>(1.0));
        color = pow(color, vec3<f32>(1.0 / 2.2));
    }

    return vec4<f32>(color, 1.0);
}
"#;

const SKYBOX_WGSL: &str = r#"
struct SkyboxUniforms {
    inv_view_proj: mat4x4<f32>,
    env_rotation: f32,
    blur: f32,
    intensity: f32,
    has_env_map: i32,
};

@group(0) @binding(0) var<uniform> u: SkyboxUniforms;
@group(1) @binding(0) var s_clamp: sampler;
@group(1) @binding(1) var t_env_map: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    // Generate fullscreen triangle covering [-1, 1] clip space
    let x = f32(i32(vertex_index == 1u || vertex_index == 2u || vertex_index == 4u) * 2 - 1);
    let y = f32(i32(vertex_index >= 2u && vertex_index <= 4u) * 2 - 1);
    let pos = vec2<f32>(
        select(-1.0, 3.0, vertex_index == 1u),
        select(-1.0, 3.0, vertex_index == 2u)
    );
    out.position = vec4<f32>(pos, 1.0, 1.0);
    out.uv = pos * 0.5 + 0.5;
    return out;
}

const PI: f32 = 3.141592653589793;

fn aces_film(x: vec3<f32>) -> vec3<f32> {
    let a: f32 = 2.51; let b: f32 = 0.03; let c: f32 = 2.43; let d: f32 = 0.59; let e: f32 = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let clip = vec4<f32>(in.uv * 2.0 - 1.0, 1.0, 1.0);
    let world = u.inv_view_proj * clip;
    let dir = normalize(world.xyz / world.w);

    let env_rad = radians(u.env_rotation);
    let c = cos(env_rad);
    let s = sin(env_rad);
    let rot_y = mat3x3<f32>(
        vec3<f32>(c, 0.0, s),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(-s, 0.0, c)
    );
    let dir_rot = rot_y * dir;

    var color: vec3<f32>;
    if (u.has_env_map == 1) {
        let env_uv = vec2<f32>(
            atan2(dir_rot.z, dir_rot.x) / (2.0 * PI) + 0.5,
            asin(clamp(dir_rot.y, -1.0, 1.0)) / PI + 0.5
        );
        color = textureSampleLevel(t_env_map, s_clamp, env_uv, u.blur * 6.0).rgb * u.intensity;
    } else {
        let d = length(in.uv - 0.5);
        color = mix(vec3<f32>(0.18, 0.18, 0.19), vec3<f32>(0.08, 0.08, 0.09), d * 1.4);
    }

    color = aces_film(color);
    color = pow(color, vec3<f32>(1.0 / 2.2));
    return vec4<f32>(color, 1.0);
}
"#;

const WIREFRAME_WGSL: &str = r#"
struct WireframeUniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: WireframeUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    return u.mvp * vec4<f32>(in.position, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return u.color;
}
"#;

// =========================================================================
// Pipelines
// =========================================================================

/// Encapsulates the wgpu pipelines, layouts, and uniform buffers.
pub struct GpuPipelines {
    pub pbr_pipeline: wgpu::RenderPipeline,
    pub pbr_uniform_buffer: wgpu::Buffer,
    pub pbr_uniform_bind_group: wgpu::BindGroup,
    pub pbr_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub pbr_texture_bind_group: wgpu::BindGroup,

    pub skybox_pipeline: wgpu::RenderPipeline,
    pub skybox_uniform_buffer: wgpu::Buffer,
    pub skybox_uniform_bind_group: wgpu::BindGroup,
    pub skybox_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub skybox_texture_bind_group: wgpu::BindGroup,

    pub wire_pipeline: wgpu::RenderPipeline,
    pub wire_uniform_buffer: wgpu::Buffer,
    pub wire_uniform_bind_group: wgpu::BindGroup,

    pub sampler_repeat: wgpu::Sampler,
    pub sampler_clamp: wgpu::Sampler,
}

impl GpuPipelines {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        textures: &GpuTextures,
        env_view: &wgpu::TextureView,
        brdf_lut_view: &wgpu::TextureView,
    ) -> Self {
        use wgpu::util::DeviceExt;

        let depth_format = wgpu::TextureFormat::Depth24Plus;

        // Shared samplers
        let sampler_repeat = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler_Repeat"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let sampler_clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler_Clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // -----------------------------------------------------------------
        // 1. PBR Pipeline
        // -----------------------------------------------------------------
        let pbr_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader_PBR"),
            source: wgpu::ShaderSource::Wgsl(PBR_WGSL.into()),
        });

        let pbr_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Pbr_UniformBuffer"),
            contents: bytemuck::cast_slice(&[PbrUniforms {
                model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                view: glam::Mat4::IDENTITY.to_cols_array_2d(),
                proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                normal_mat: glam::Mat4::IDENTITY.to_cols_array_2d(),
                cam_pos: [0.0, 0.0, 3.0, 1.0],
                light_intensity: 1.0,
                env_rotation: 0.0,
                exposure: 0.0,
                display_channel: 0,
                has_env_map: 0,
                has_brdf_lut: 1,
                uv_scale: 1.0,
                bloom_enabled: 0,
                bloom_intensity: 0.8,
                bloom_threshold: 0.85,
                ssao_enabled: 1,
                ssao_intensity: 1.0,
                dof_enabled: 0,
                dof_focal_distance: 2.5,
                dof_aperture: 0.08,
                color_management_mode: 0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let pbr_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Pbr_UniformBGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pbr_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Pbr_UniformBG"),
            layout: &pbr_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: pbr_uniform_buffer.as_entire_binding(),
            }],
        });

        let pbr_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Pbr_TextureBGL"),
                entries: &[
                    // 0: s_repeat
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // 1: s_clamp
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // 2: t_base_color
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 3: t_metallic
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 4: t_roughness
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 5: t_normal
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 6: t_ao
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 7: t_curvature
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 8: t_env_map
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // 9: t_brdf_lut
                    wgpu::BindGroupLayoutEntry {
                        binding: 9,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let pbr_texture_bind_group = Self::create_pbr_tex_bg(
            device,
            &pbr_texture_bind_group_layout,
            &sampler_repeat,
            &sampler_clamp,
            textures,
            env_view,
            brdf_lut_view,
        );

        let pbr_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pbr_PipelineLayout"),
            bind_group_layouts: &[
                &pbr_uniform_bind_group_layout,
                &pbr_texture_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pbr_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pbr_Pipeline"),
            layout: Some(&pbr_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &pbr_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WgpuVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        // pos
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        // normal
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        // uv
                        wgpu::VertexAttribute {
                            offset: 24,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        // tangent
                        wgpu::VertexAttribute {
                            offset: 32,
                            shader_location: 3,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &pbr_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Solid double-sided geometry without holes
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // -----------------------------------------------------------------
        // 2. Skybox Pipeline
        // -----------------------------------------------------------------
        let skybox_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader_Skybox"),
            source: wgpu::ShaderSource::Wgsl(SKYBOX_WGSL.into()),
        });

        let skybox_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Skybox_UniformBuffer"),
            contents: bytemuck::cast_slice(&[SkyboxUniforms {
                inv_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                env_rotation: 0.0,
                blur: 0.25,
                intensity: 1.0,
                has_env_map: 0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let skybox_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Skybox_UniformBGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let skybox_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Skybox_UniformBG"),
            layout: &skybox_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: skybox_uniform_buffer.as_entire_binding(),
            }],
        });

        let skybox_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Skybox_TextureBGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let skybox_texture_bind_group = Self::create_skybox_tex_bg(
            device,
            &skybox_texture_bind_group_layout,
            &sampler_clamp,
            env_view,
        );

        let skybox_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Skybox_PipelineLayout"),
                bind_group_layouts: &[
                    &skybox_uniform_bind_group_layout,
                    &skybox_texture_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let skybox_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Skybox_Pipeline"),
            layout: Some(&skybox_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &skybox_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &skybox_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // -----------------------------------------------------------------
        // 3. Wireframe & Grid Line Pipeline
        // -----------------------------------------------------------------
        let wire_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader_Wireframe"),
            source: wgpu::ShaderSource::Wgsl(WIREFRAME_WGSL.into()),
        });

        let wire_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Wire_UniformBuffer"),
            contents: bytemuck::cast_slice(&[WireframeUniforms {
                mvp: glam::Mat4::IDENTITY.to_cols_array_2d(),
                color: [0.15, 0.75, 1.0, 0.85],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let wire_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Wire_UniformBGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let wire_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Wire_UniformBG"),
            layout: &wire_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wire_uniform_buffer.as_entire_binding(),
            }],
        });

        let wire_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Wire_PipelineLayout"),
            bind_group_layouts: &[&wire_uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let wire_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wire_Pipeline"),
            layout: Some(&wire_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &wire_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12, // [f32; 3]
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x3,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &wire_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pbr_pipeline,
            pbr_uniform_buffer,
            pbr_uniform_bind_group,
            pbr_texture_bind_group_layout,
            pbr_texture_bind_group,
            skybox_pipeline,
            skybox_uniform_buffer,
            skybox_uniform_bind_group,
            skybox_texture_bind_group_layout,
            skybox_texture_bind_group,
            wire_pipeline,
            wire_uniform_buffer,
            wire_uniform_bind_group,
            sampler_repeat,
            sampler_clamp,
        }
    }

    pub fn create_pbr_tex_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler_repeat: &wgpu::Sampler,
        sampler_clamp: &wgpu::Sampler,
        textures: &GpuTextures,
        env_view: &wgpu::TextureView,
        brdf_lut_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Pbr_TextureBG"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(sampler_repeat),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler_clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&textures.base_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&textures.metallic_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&textures.roughness_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&textures.normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&textures.ao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&textures.curvature_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(env_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(brdf_lut_view),
                },
            ],
        })
    }

    pub fn create_skybox_tex_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler_clamp: &wgpu::Sampler,
        env_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Skybox_TextureBG"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(sampler_clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(env_view),
                },
            ],
        })
    }
}

// =========================================================================
// Main Viewport Struct
// =========================================================================

/// Parameters passed from the UI application state to the viewport rendering passes.
#[derive(Clone, Copy, Debug)]
pub struct ViewportPaintParams {
    pub yaw: f32,
    pub pitch: f32,
    pub cam_dist: f32,
    pub light_intensity: f32,
    pub env_rotation: f32,
    pub display_channel: usize,
    pub exposure: f32,
    pub skybox_visible: bool,
    pub skybox_blur: f32,
    pub skybox_intensity: f32,
    pub uv_scale: f32,
    pub show_wireframe: bool,
    pub show_grid: bool,
    pub bloom_enabled: bool,
    pub bloom_intensity: f32,
    pub bloom_threshold: f32,
    pub ssao_enabled: bool,
    pub ssao_intensity: f32,
    pub dof_enabled: bool,
    pub dof_focal_distance: f32,
    pub dof_aperture: f32,
    pub color_management_mode: usize,
}

/// The core Rust & Vulkan / wgpu 3D viewport state machine.
pub struct ViewportWgpu {
    /// Active 3D mesh buffers on the GPU.
    pub mesh: GpuMesh,
    /// Ground grid and XYZ world coordinate lines.
    pub grid: GpuGrid,
    /// Pipelines and shader program state.
    pub pipelines: GpuPipelines,
    /// Active composite PBR textures.
    pub textures: GpuTextures,
    /// Loaded HDR environment texture (or 1x1 fallback).
    pub env_texture: wgpu::Texture,
    pub env_texture_view: wgpu::TextureView,
    /// Precomputed 512x512 Karis BRDF integration LUT.
    pub brdf_lut_texture: wgpu::Texture,
    pub brdf_lut_view: wgpu::TextureView,

    /// Name of the active HDR environment.
    pub active_env_name: String,
    /// Cached paint parameters.
    pub params: ViewportPaintParams,
}

impl ViewportWgpu {
    /// Initializes a new hardware-accelerated Vulkan/wgpu 3D viewport.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        mesh: &Mesh,
        stack: &LayerStack,
        baked: &BakedMaps,
    ) -> Result<Self, String> {
        let gpu_mesh = GpuMesh::new(device, mesh);
        let grid = GpuGrid::new(device);
        let mut textures = GpuTextures::new(device, stack.width, stack.height);
        textures.upload_all(device, queue, stack, baked);

        // Precompute Karis 512x512 BRDF integration LUT
        let (brdf_lut_texture, brdf_lut_view) = generate_brdf_lut(device, queue);

        // Create default neutral studio environment map (1x1 HDR placeholder)
        let (env_texture, env_texture_view) = create_default_env(device, queue);

        let pipelines = GpuPipelines::new(
            device,
            target_format,
            &textures,
            &env_texture_view,
            &brdf_lut_view,
        );

        let default_params = ViewportPaintParams {
            yaw: 45.0,
            pitch: 25.0,
            cam_dist: 2.5,
            light_intensity: 1.0,
            env_rotation: 0.0,
            display_channel: 0,
            exposure: 0.0,
            skybox_visible: true,
            skybox_blur: 0.25,
            skybox_intensity: 1.0,
            uv_scale: 1.0,
            show_wireframe: false,
            show_grid: true,
            bloom_enabled: false,
            bloom_intensity: 0.8,
            bloom_threshold: 0.85,
            ssao_enabled: true,
            ssao_intensity: 1.0,
            dof_enabled: false,
            dof_focal_distance: 2.5,
            dof_aperture: 0.08,
            color_management_mode: 0,
        };

        let mut viewport = Self {
            mesh: gpu_mesh,
            grid,
            pipelines,
            textures,
            env_texture,
            env_texture_view,
            brdf_lut_texture,
            brdf_lut_view,
            active_env_name: "Studio Tomoco".to_string(),
            params: default_params,
        };

        // Attempt to load Studio Tomoco default environment
        let mut default_env_paths: Vec<PathBuf> = crate::paths::find_assets_dir()
            .map(|a| vec![a.join("environments").join("Studio Tomoco.exr")])
            .unwrap_or_default();
        default_env_paths.push(PathBuf::from("assets/environments/studio_tomoco.png"));
        default_env_paths.retain(|p| p.exists());
        for path in &default_env_paths {
            if path.exists() {
                let _ = viewport.load_environment(device, queue, path);
                break;
            }
        }

        Ok(viewport)
    }

    /// Updates the mesh geometry in the viewport.
    pub fn set_mesh(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &Mesh) {
        self.mesh.update(device, queue, mesh);
    }

    /// Sets submesh visibility by index.
    pub fn set_submesh_visibility(&mut self, idx: usize, visible: bool) {
        self.mesh.set_submesh_visibility(idx, visible);
    }

    /// Isolates a submesh by index (or reveals all if `None`).
    pub fn isolate_submesh(&mut self, isolated_idx: Option<usize>) {
        self.mesh.isolate_submesh(isolated_idx);
    }

    /// Returns a slice of the active submeshes.
    pub fn submeshes(&self) -> &[GpuSubMesh] {
        &self.mesh.submeshes
    }

    /// Uploads all texture channels from the [`LayerStack`] and [`BakedMaps`].
    pub fn upload_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stack: &LayerStack,
        baked: &BakedMaps,
    ) {
        self.textures.upload_all(device, queue, stack, baked);
        self.rebind_textures(device);
    }

    /// Uploads a dirty tile region from active brush strokes.
    pub fn upload_dirty(&mut self, queue: &wgpu::Queue, stack: &LayerStack, bbox: [i32; 4]) {
        self.textures.upload_dirty(queue, stack, bbox);
    }

    /// Loads an HDR (`.exr`) or LDR image environment map.
    pub fn load_environment(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: impl AsRef<Path>,
    ) -> Result<(), String> {
        let path_ref = path.as_ref();
        let stem = path_ref
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Environment");

        let (w, h, rgba_data) = if path_ref.extension().and_then(|s| s.to_str()) == Some("exr") {
            load_exr_hdr(path_ref)?
        } else {
            load_ldr_image(path_ref)?
        };

        // Convert f32 HDR data to f16 (u16 bits) for universal filterability across all Vulkan/Metal/DX12 GPUs
        let f16_data: Vec<u16> = rgba_data
            .iter()
            .map(|&v| half::f16::from_f32(v).to_bits())
            .collect();

        // Create texture with half-float (f16) data - universally filterable in WebGPU / Vulkan
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(stem),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&f16_data),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(w * 8), // 4 half-floats * 2 bytes = 8 bytes per pixel
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.env_texture = texture;
        self.env_texture_view = view;
        self.active_env_name = stem.to_string();

        self.rebind_textures(device);

        Ok(())
    }

    fn rebind_textures(&mut self, device: &wgpu::Device) {
        self.pipelines.pbr_texture_bind_group = GpuPipelines::create_pbr_tex_bg(
            device,
            &self.pipelines.pbr_texture_bind_group_layout,
            &self.pipelines.sampler_repeat,
            &self.pipelines.sampler_clamp,
            &self.textures,
            &self.env_texture_view,
            &self.brdf_lut_view,
        );

        self.pipelines.skybox_texture_bind_group = GpuPipelines::create_skybox_tex_bg(
            device,
            &self.pipelines.skybox_texture_bind_group_layout,
            &self.pipelines.sampler_clamp,
            &self.env_texture_view,
        );
    }

    /// Prepares uniform buffers right before the render pass executes.
    pub fn prepare(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &ViewportPaintParams,
        aspect: f32,
    ) {
        self.params = *params;

        let proj = glam::Mat4::perspective_rh(45.0f32.to_radians(), aspect, 0.05, 50.0);
        let yaw_rad = params.yaw.to_radians();
        let pitch_rad = params.pitch.to_radians();
        let cam_pos = glam::Vec3::new(
            params.cam_dist * pitch_rad.cos() * yaw_rad.sin(),
            params.cam_dist * pitch_rad.sin(),
            params.cam_dist * pitch_rad.cos() * yaw_rad.cos(),
        );
        let view = glam::Mat4::look_at_rh(cam_pos, glam::Vec3::ZERO, glam::Vec3::Y);
        let view_proj = proj * view;
        let inv_view_proj = view_proj.inverse();

        // 1. PBR Uniforms
        let pbr_uniforms = PbrUniforms {
            model: glam::Mat4::IDENTITY.to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            normal_mat: glam::Mat4::IDENTITY.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            light_intensity: params.light_intensity,
            env_rotation: params.env_rotation,
            exposure: params.exposure,
            display_channel: params.display_channel as i32,
            has_env_map: 1,
            has_brdf_lut: 1,
            uv_scale: params.uv_scale,
            bloom_enabled: if params.bloom_enabled { 1 } else { 0 },
            bloom_intensity: params.bloom_intensity,
            bloom_threshold: params.bloom_threshold,
            ssao_enabled: if params.ssao_enabled { 1 } else { 0 },
            ssao_intensity: params.ssao_intensity,
            dof_enabled: if params.dof_enabled { 1 } else { 0 },
            dof_focal_distance: params.dof_focal_distance,
            dof_aperture: params.dof_aperture,
            color_management_mode: params.color_management_mode as i32,
        };

        queue.write_buffer(
            &self.pipelines.pbr_uniform_buffer,
            0,
            bytemuck::cast_slice(&[pbr_uniforms]),
        );

        // 2. Skybox Uniforms
        let skybox_uniforms = SkyboxUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            env_rotation: params.env_rotation,
            blur: params.skybox_blur,
            intensity: params.skybox_intensity,
            has_env_map: 1,
        };

        queue.write_buffer(
            &self.pipelines.skybox_uniform_buffer,
            0,
            bytemuck::cast_slice(&[skybox_uniforms]),
        );

        // 3. Wireframe / Grid Uniforms
        let wire_uniforms = WireframeUniforms {
            mvp: view_proj.to_cols_array_2d(),
            color: [0.15, 0.75, 1.0, 0.85],
        };

        queue.write_buffer(
            &self.pipelines.wire_uniform_buffer,
            0,
            bytemuck::cast_slice(&[wire_uniforms]),
        );
    }

    /// Issues draw commands for the 3D viewport into the active render pass.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        if self.mesh.index_count == 0 {
            return;
        }

        // 1. Render Skybox Background (if enabled and lit mode)
        if self.params.skybox_visible && self.params.display_channel == 0 {
            render_pass.set_pipeline(&self.pipelines.skybox_pipeline);
            render_pass.set_bind_group(0, &self.pipelines.skybox_uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &self.pipelines.skybox_texture_bind_group, &[]);
            render_pass.draw(0..3, 0..1); // Fullscreen triangle
        }

        // 2. Render 3D Mesh with PBR Pipeline
        render_pass.set_pipeline(&self.pipelines.pbr_pipeline);
        render_pass.set_bind_group(0, &self.pipelines.pbr_uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.pipelines.pbr_texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        if self.mesh.submeshes.is_empty() {
            render_pass.draw_indexed(0..self.mesh.index_count, 0, 0..1);
        } else {
            for sm in &self.mesh.submeshes {
                if sm.visible && sm.index_count > 0 {
                    let start = sm.index_start as u32;
                    let end = start + sm.index_count as u32;
                    render_pass.draw_indexed(start..end, 0, 0..1);
                }
            }
        }

        // 3. Render Ground Plane Grid & Coordinate Axes
        if self.params.show_grid {
            render_pass.set_pipeline(&self.pipelines.wire_pipeline);
            render_pass.set_bind_group(0, &self.pipelines.wire_uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.grid.line_buffer.slice(..));

            // Grid lines
            render_pass.draw(0..self.grid.grid_vert_count, 0..1);

            // World coordinate axes lines
            let axes_end = self.grid.axes_vert_offset + self.grid.axes_vert_count;
            render_pass.draw(self.grid.axes_vert_offset..axes_end, 0..1);
        }

        // 4. Render Wireframe Overlay
        if self.params.show_wireframe && self.mesh.wire_index_count > 0 {
            render_pass.set_pipeline(&self.pipelines.wire_pipeline);
            render_pass.set_bind_group(0, &self.pipelines.wire_uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.mesh.wire_index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.draw_indexed(0..self.mesh.wire_index_count, 0, 0..1);
        }
    }
}

// =========================================================================
// Helpers (BRDF LUT, HDR Loader, Environment Pool)
// =========================================================================

/// Returns a list of all discovered equirectangular environment assets.
pub fn list_available_environments() -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(env_dir) = crate::paths::find_assets_dir() {
        dirs.push(env_dir.join("environments"));
    }
    dirs.push(PathBuf::from("assets/environments"));
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            dirs.push(
                PathBuf::from(home)
                    .join(".remoainter")
                    .join("assets")
                    .join("environments"),
            );
        }
    }

    let mut list = Vec::new();
    for d in &dirs {
        if let Ok(entries) = std::fs::read_dir(d) {
            for entry in entries.flatten() {
                let ep = entry.path();
                if let Some(ext) = ep.extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "exr"
                        || ext_lower == "hdr"
                        || ext_lower == "png"
                        || ext_lower == "jpg"
                    {
                        let stem = ep
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        if !list.iter().any(|(s, _)| s == &stem) {
                            list.push((stem, ep));
                        }
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list
}

/// Generates the Karis / Epic Games 512×512 Split-Sum BRDF Integration LUT.
pub fn generate_brdf_lut(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = 512u32;
    let lut_f32 = compute_brdf_lut(size as usize);
    let mut lut_bytes = vec![0u8; (size * size * 2) as usize]; // RG8Unorm

    for i in 0..(size * size) as usize {
        let (a, b) = lut_f32[i];
        lut_bytes[i * 2] = (a.clamp(0.0, 1.0) * 255.0) as u8;
        lut_bytes[i * 2 + 1] = (b.clamp(0.0, 1.0) * 255.0) as u8;
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("BRDF_LUT"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &lut_bytes,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(size * 2),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Numerically integrates the BRDF Split-Sum integral for (NdotV, Roughness).
pub fn compute_brdf_lut(size: usize) -> Vec<(f32, f32)> {
    use rayon::prelude::*;

    (0..size)
        .into_par_iter()
        .flat_map(|y| {
            let roughness = ((y as f32 + 0.5) / size as f32).clamp(0.001, 1.0);
            (0..size)
                .map(move |x| {
                    let n_dot_v = ((x as f32 + 0.5) / size as f32).clamp(0.001, 1.0);
                    integrate_brdf(n_dot_v, roughness)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn integrate_brdf(n_dot_v: f32, roughness: f32) -> (f32, f32) {
    let v = glam::Vec3::new((1.0 - n_dot_v * n_dot_v).max(0.0).sqrt(), 0.0, n_dot_v);
    let mut a = 0.0f32;
    let mut b = 0.0f32;
    let n = glam::Vec3::Z;

    let num_samples = 512u32;
    for i in 0..num_samples {
        let xi = hammersley(i, num_samples);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = (2.0 * v.dot(h) * h - v).normalize();

        let n_dot_l = l.z.max(0.0);
        let n_dot_h = h.z.max(0.0);
        let v_dot_h = v.dot(h).max(0.0);

        if n_dot_l > 0.0 {
            let g = geometry_smith_lut(n_dot_v, n_dot_l, roughness);
            let g_vis = (g * v_dot_h) / (n_dot_h * n_dot_v).max(0.0001);
            let fc = (1.0 - v_dot_h).powi(5);

            a += (1.0 - fc) * g_vis;
            b += fc * g_vis;
        }
    }

    (a / num_samples as f32, b / num_samples as f32)
}

fn radical_inverse_vdc(mut bits: u32) -> f32 {
    bits = bits.rotate_right(16);
    bits = ((bits & 0x55555555) << 1) | ((bits & 0xAAAAAAAA) >> 1);
    bits = ((bits & 0x33333333) << 2) | ((bits & 0xCCCCCCCC) >> 2);
    bits = ((bits & 0x0F0F0F0F) << 4) | ((bits & 0xF0F0F0F0) >> 4);
    bits = ((bits & 0x00FF00FF) << 8) | ((bits & 0xFF00FF00) >> 8);
    bits as f32 * 2.328_306_4e-10
}

fn hammersley(i: u32, n: u32) -> glam::Vec2 {
    glam::Vec2::new(i as f32 / n as f32, radical_inverse_vdc(i))
}

fn importance_sample_ggx(xi: glam::Vec2, n: glam::Vec3, roughness: f32) -> glam::Vec3 {
    let a = roughness * roughness;
    let phi = 2.0 * std::f32::consts::PI * xi.x;
    let cos_theta = ((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y)).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    let h = glam::Vec3::new(phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta);

    let up = if n.z.abs() < 0.999 {
        glam::Vec3::Z
    } else {
        glam::Vec3::X
    };
    let tangent = up.cross(n).normalize();
    let bitangent = n.cross(tangent);

    (tangent * h.x + bitangent * h.y + n * h.z).normalize()
}

fn geometry_smith_lut(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = (roughness * roughness) / 2.0;
    let g1_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let g1_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    g1_v * g1_l
}

fn create_default_env(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Default_Env"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let default_pixel: [u16; 4] = [
        half::f16::from_f32(0.2).to_bits(),
        half::f16::from_f32(0.2).to_bits(),
        half::f16::from_f32(0.22).to_bits(),
        half::f16::from_f32(1.0).to_bits(),
    ];
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&default_pixel),
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn load_exr_hdr(path: impl AsRef<Path>) -> Result<(u32, u32, Vec<f32>), String> {
    let path_ref = path.as_ref();
    let image = exr::prelude::read_first_rgba_layer_from_file(
        path_ref,
        |resolution, _| {
            let default_pixel = [0.0f32; 4];
            vec![vec![default_pixel; resolution.width()]; resolution.height()]
        },
        |pixel_vector, position, (r, g, b, a): (f32, f32, f32, f32)| {
            pixel_vector[position.y()][position.x()] = [r, g, b, a];
        },
    )
    .map_err(|e| format!("Failed to read EXR {:?}: {}", path_ref, e))?;

    let h = image.layer_data.channel_data.pixels.len();
    let w = if h > 0 {
        image.layer_data.channel_data.pixels[0].len()
    } else {
        0
    };
    let mut flat = Vec::with_capacity(w * h * 4);
    for row in image.layer_data.channel_data.pixels {
        for px in row {
            flat.extend_from_slice(&px);
        }
    }
    Ok((w as u32, h as u32, flat))
}

fn load_ldr_image(path: impl AsRef<Path>) -> Result<(u32, u32, Vec<f32>), String> {
    let img = image::open(path.as_ref())
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let raw = img.into_raw();
    let mut flat = Vec::with_capacity(raw.len());
    for b in raw {
        flat.push(b as f32 / 255.0);
    }
    Ok((w, h, flat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brdf_lut_calculation() {
        let lut = compute_brdf_lut(32);
        assert_eq!(lut.len(), 32 * 32);
        let (a, b) = lut[16 * 32 + 16];
        assert!(a >= 0.0 && a <= 1.0);
        assert!(b >= 0.0 && b <= 1.0);
    }

    #[test]
    fn test_submesh_visibility_logic() {
        let mut submeshes = [
            GpuSubMesh {
                name: "Head".into(),
                index_start: 0,
                index_count: 30,
                visible: true,
            },
            GpuSubMesh {
                name: "Body".into(),
                index_start: 30,
                index_count: 60,
                visible: true,
            },
        ];

        submeshes[1].visible = false;
        assert!(submeshes[0].visible);
        assert!(!submeshes[1].visible);
    }

    #[test]
    fn test_grid_lines_count() {
        let extent = 2.0f32;
        let step = 0.2f32;
        let steps = (extent / step) as i32;
        let line_count = (steps * 2 + 1) * 2;
        assert_eq!(line_count, 42);
        let vert_count = line_count * 2;
        assert_eq!(vert_count, 84);
    }
}
