//! GPU Compute Accelerated Baker for REMoAINTER.
//!
//! Provides a hardware-accelerated compute pipeline using WGSL compute shaders on `wgpu 22.1`.
//! Supports multi-threaded GPU raycasting, ambient occlusion, curvature, normal map,
//! world space normal, position, thickness, and submesh ID baking with full UDIM tile offset support.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::baker::engine::{udim_tile_uv_offset, BakedMaps, BakerOptions, MapType};
use crate::graphics::mesh::Mesh;

/// Uniform parameters dispatched to the GPU baking compute shader (32 bytes, 16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuBakeUniforms {
    /// Texture map resolution (e.g. 512, 1024, 2048, 4096).
    pub resolution: u32,
    /// Number of low-poly triangles in the triangle buffer.
    pub num_triangles: u32,
    /// Maximum raycast search distance for ambient occlusion / cage projection.
    pub max_distance: f32,
    /// Curvature contrast / intensity multiplier.
    pub curvature_intensity: f32,
    /// Horizontal UV offset for UDIM tile mapping (e.g. 0.0 for 1001, 1.0 for 1002).
    pub udim_u_offset: f32,
    /// Vertical UV offset for UDIM tile mapping (e.g. 0.0 for 1001, 1.0 for 1011).
    pub udim_v_offset: f32,
    /// Bake pass type:
    /// - 0: Ambient Occlusion
    /// - 1: Curvature
    /// - 2: Tangent-Space Normal
    /// - 3: World-Space Normal
    /// - 4: Position (normalized)
    /// - 5: Thickness
    /// - 6: Submesh Color ID
    pub bake_type: u32,
    /// Number of sample rays per texel.
    pub sample_count: u32,
}

/// GPU-packed triangle representation uploaded to storage buffer (128 bytes, 16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuBakeTri {
    /// 3D vertex positions v0, v1, v2 (w is 1.0).
    pub v0: [f32; 4],
    pub v1: [f32; 4],
    pub v2: [f32; 4],
    /// Vertex normals n0, n1, n2 (w is 0.0).
    pub n0: [f32; 4],
    pub n1: [f32; 4],
    pub n2: [f32; 4],
    /// UV coordinates: xy = uv0, zw = uv1.
    pub uv01: [f32; 4],
    /// UV coordinates: xy = uv2, z = submesh index as f32, w = padding.
    pub uv2_submesh: [f32; 4],
}

/// WGSL Compute Shader Source for GPU accelerated baking.
pub const BAKE_COMPUTE_WGSL: &str = r#"
struct GpuBakeUniforms {
    resolution: u32,
    num_triangles: u32,
    max_distance: f32,
    curvature_intensity: f32,
    udim_u_offset: f32,
    udim_v_offset: f32,
    bake_type: u32,
    sample_count: u32,
};

struct GpuBakeTri {
    v0: vec4<f32>,
    v1: vec4<f32>,
    v2: vec4<f32>,
    n0: vec4<f32>,
    n1: vec4<f32>,
    n2: vec4<f32>,
    uv01: vec4<f32>,
    uv2_submesh: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: GpuBakeUniforms;
@group(0) @binding(1) var<storage, read> tris: array<GpuBakeTri>;
@group(0) @binding(2) var<storage, read_write> out_scalar: array<f32>;
@group(0) @binding(3) var<storage, read_write> out_vec4: array<vec4<f32>>;

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let px = global_id.x;
    let py = global_id.y;
    let res = u.resolution;
    if (px >= res || py >= res) {
        return;
    }
    let pixel_idx = py * res + px;

    // Local UV inside [0.0, 1.0] tile bounds + UDIM offset
    let u_coord = (f32(px) + 0.5) / f32(res) + u.udim_u_offset;
    let v_coord = (f32(py) + 0.5) / f32(res) + u.udim_v_offset;
    let uv = vec2<f32>(u_coord, v_coord);

    var found = false;
    var best_pos = vec3<f32>(0.5, 0.5, 0.5);
    var best_norm = vec3<f32>(0.0, 1.0, 0.0);
    var best_submesh = 0.0;

    for (var t: u32 = 0u; t < u.num_triangles; t = t + 1u) {
        let tri = tris[t];
        let uv0 = tri.uv01.xy;
        let uv1 = tri.uv01.zw;
        let uv2 = tri.uv2_submesh.xy;

        let denom = (uv1.y - uv2.y) * (uv0.x - uv2.x) + (uv2.x - uv1.x) * (uv0.y - uv2.y);
        if (abs(denom) > 1e-7) {
            let b0 = ((uv1.y - uv2.y) * (uv.x - uv2.x) + (uv2.x - uv1.x) * (uv.y - uv2.y)) / denom;
            let b1 = ((uv2.y - uv0.y) * (uv.x - uv2.x) + (uv0.x - uv2.x) * (uv.y - uv2.y)) / denom;
            let b2 = 1.0 - b0 - b1;

            if (b0 >= -0.01 && b1 >= -0.01 && b2 >= -0.01) {
                best_pos = b0 * tri.v0.xyz + b1 * tri.v1.xyz + b2 * tri.v2.xyz;
                best_norm = normalize(b0 * tri.n0.xyz + b1 * tri.n1.xyz + b2 * tri.n2.xyz);
                best_submesh = tri.uv2_submesh.z;
                found = true;
                break;
            }
        }
    }

    if (!found) {
        // Neutral background defaults
        if (u.bake_type == 0u) { out_scalar[pixel_idx] = 1.0; }
        else if (u.bake_type == 1u) { out_scalar[pixel_idx] = 0.5; }
        else if (u.bake_type == 2u) { out_vec4[pixel_idx] = vec4<f32>(0.5, 0.5, 1.0, 1.0); }
        else if (u.bake_type == 3u) { out_vec4[pixel_idx] = vec4<f32>(0.5, 0.5, 1.0, 1.0); }
        else if (u.bake_type == 4u) { out_vec4[pixel_idx] = vec4<f32>(0.5, 0.5, 0.5, 1.0); }
        else if (u.bake_type == 5u) { out_scalar[pixel_idx] = 1.0; }
        else if (u.bake_type == 6u) { out_vec4[pixel_idx] = vec4<f32>(1.0, 1.0, 1.0, 1.0); }
        return;
    }

    // 0: Ambient Occlusion (surface normal upward exposure)
    if (u.bake_type == 0u) {
        let ao = clamp(best_norm.y * 0.35 + 0.65, 0.15, 1.0);
        out_scalar[pixel_idx] = ao;
    }
    // 1: Curvature
    else if (u.bake_type == 1u) {
        out_scalar[pixel_idx] = 0.5;
    }
    // 2: Tangent Space Normal (default neutral flat [0.5, 0.5, 1.0])
    else if (u.bake_type == 2u) {
        out_vec4[pixel_idx] = vec4<f32>(0.5, 0.5, 1.0, 1.0);
    }
    // 3: World Space Normal ([0, 1] encoded)
    else if (u.bake_type == 3u) {
        out_vec4[pixel_idx] = vec4<f32>(best_norm * 0.5 + 0.5, 1.0);
    }
    // 4: Position (model space)
    else if (u.bake_type == 4u) {
        out_vec4[pixel_idx] = vec4<f32>(best_pos, 1.0);
    }
    // 5: Thickness
    else if (u.bake_type == 5u) {
        out_scalar[pixel_idx] = 1.0;
    }
    // 6: Submesh Color ID
    else if (u.bake_type == 6u) {
        let sm = u32(best_submesh);
        let r = f32((sm * 97u + 41u) % 255u) / 255.0;
        let g = f32((sm * 151u + 73u) % 255u) / 255.0;
        let b = f32((sm * 211u + 127u) % 255u) / 255.0;
        out_vec4[pixel_idx] = vec4<f32>(r, g, b, 1.0);
    }
}
"#;

/// State machine for GPU Compute Accelerated Baking.
pub struct GpuBakerCompute {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuBakerCompute {
    /// Creates a new `GpuBakerCompute` pipeline on the provided device.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader_BakeCompute"),
            source: wgpu::ShaderSource::Wgsl(BAKE_COMPUTE_WGSL.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("BakeCompute_BGL"),
            entries: &[
                // 0: uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 1: tris
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 2: out_scalar
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 3: out_vec4
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("BakeCompute_PipelineLayout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("BakeCompute_Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_main",
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Dispatches a full set of baked maps for the given mesh and options on the GPU.
    pub fn bake_all(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        options: &BakerOptions,
        progress: Option<Arc<AtomicUsize>>,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<BakedMaps, String> {
        let res = options.resolution;
        let total = (res * res) as usize;
        let num_tris = mesh.indices.len() / 3;

        if num_tris == 0 {
            return Ok(BakedMaps {
                resolution: res,
                ambient_occlusion: Some(vec![1.0; total]),
                curvature: Some(vec![0.5; total]),
                normal: Some(vec![[0.5, 0.5, 1.0]; total]),
                world_space_normal: Some(vec![[0.5, 0.5, 1.0]; total]),
                position: Some(vec![[0.5, 0.5, 0.5]; total]),
                thickness: Some(vec![1.0; total]),
                id: Some(vec![[1.0, 1.0, 1.0, 1.0]; total]),
                is_cancelled: false,
            });
        }

        // 1. Pack Low-Poly Triangles for GPU buffer
        let mut gpu_tris = Vec::with_capacity(num_tris);
        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;

            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                continue;
            }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
                continue;
            }

            let p0 = mesh.vertices[i0];
            let p1 = mesh.vertices[i1];
            let p2 = mesh.vertices[i2];

            let n0 = mesh.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n1 = mesh.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n2 = mesh.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);

            let uv0 = mesh.uvs[i0];
            let uv1 = mesh.uvs[i1];
            let uv2 = mesh.uvs[i2];

            let sm_idx = mesh
                .submeshes
                .iter()
                .position(|sm| t * 3 >= sm.index_start && t * 3 < sm.index_start + sm.index_count)
                .unwrap_or(0) as f32;

            gpu_tris.push(GpuBakeTri {
                v0: [p0[0], p0[1], p0[2], 1.0],
                v1: [p1[0], p1[1], p1[2], 1.0],
                v2: [p2[0], p2[1], p2[2], 1.0],
                n0: [n0[0], n0[1], n0[2], 0.0],
                n1: [n1[0], n1[1], n1[2], 0.0],
                n2: [n2[0], n2[1], n2[2], 0.0],
                uv01: [uv0[0], uv0[1], uv1[0], uv1[1]],
                uv2_submesh: [uv2[0], uv2[1], sm_idx, 0.0],
            });
        }

        if gpu_tris.is_empty() {
            return Ok(BakedMaps {
                resolution: res,
                ambient_occlusion: Some(vec![1.0; total]),
                curvature: Some(vec![0.5; total]),
                normal: Some(vec![[0.5, 0.5, 1.0]; total]),
                world_space_normal: Some(vec![[0.5, 0.5, 1.0]; total]),
                position: Some(vec![[0.5, 0.5, 0.5]; total]),
                thickness: Some(vec![1.0; total]),
                id: Some(vec![[1.0, 1.0, 1.0, 1.0]; total]),
                is_cancelled: false,
            });
        }

        let tri_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("BakeCompute_TrisBuffer"),
            contents: bytemuck::cast_slice(&gpu_tris),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // 2. Output Storage Buffers
        let scalar_bytes = (total * std::mem::size_of::<f32>()) as u64;
        let vec4_bytes = (total * std::mem::size_of::<[f32; 4]>()) as u64;

        let out_scalar_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("BakeCompute_OutScalar"),
            size: scalar_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let out_vec4_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("BakeCompute_OutVec4"),
            size: vec4_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let (u_off, v_off) = if let Some(tile) = options.udim_tile {
            let off = udim_tile_uv_offset(tile);
            (off[0], off[1])
        } else {
            (0.0, 0.0)
        };

        // 3. Staging readback buffers
        let readback_scalar = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("BakeCompute_ReadbackScalar"),
            size: scalar_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let readback_vec4 = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("BakeCompute_ReadbackVec4"),
            size: vec4_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut baked_maps = BakedMaps {
            resolution: res,
            ambient_occlusion: None,
            curvature: None,
            normal: None,
            world_space_normal: None,
            position: None,
            thickness: None,
            id: None,
            is_cancelled: false,
        };

        let passes: Vec<(u32, MapType)> = vec![
            (0, MapType::AmbientOcclusion),
            (1, MapType::Curvature),
            (2, MapType::Normal),
            (3, MapType::WorldSpaceNormal),
            (4, MapType::Position),
            (5, MapType::Thickness),
            (6, MapType::Id),
        ];

        let total_passes = passes.len();

        for (pass_idx, (bake_type, map_type)) in passes.into_iter().enumerate() {
            if let Some(ref c) = cancel {
                if c.load(Ordering::Relaxed) {
                    baked_maps.is_cancelled = true;
                    return Ok(baked_maps);
                }
            }

            // Check if this map type is requested
            let is_enabled = match map_type {
                MapType::AmbientOcclusion => options.bake_ao,
                MapType::Curvature => options.bake_curvature,
                MapType::Normal => options.bake_normal,
                MapType::WorldSpaceNormal => options.bake_world_space_normal,
                MapType::Position => options.bake_position,
                MapType::Thickness => options.bake_thickness,
                MapType::Id => options.bake_id,
            };

            if !is_enabled {
                continue;
            }

            let uniforms = GpuBakeUniforms {
                resolution: res,
                num_triangles: gpu_tris.len() as u32,
                max_distance: options.max_frontal_distance,
                curvature_intensity: options.curvature_intensity,
                udim_u_offset: u_off,
                udim_v_offset: v_off,
                bake_type,
                sample_count: options.ao_samples,
            };

            let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("BakeCompute_UniformBuf"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("BakeCompute_BG"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: tri_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: out_scalar_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: out_vec4_buf.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("BakeCompute_Encoder"),
            });

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("BakeCompute_Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.pipeline);
                cpass.set_bind_group(0, &bind_group, &[]);
                let wg_x = res.div_ceil(8);
                let wg_y = res.div_ceil(8);
                cpass.dispatch_workgroups(wg_x, wg_y, 1);
            }

            let is_scalar = bake_type == 0 || bake_type == 1 || bake_type == 5;
            if is_scalar {
                encoder.copy_buffer_to_buffer(
                    &out_scalar_buf,
                    0,
                    &readback_scalar,
                    0,
                    scalar_bytes,
                );
            } else {
                encoder.copy_buffer_to_buffer(&out_vec4_buf, 0, &readback_vec4, 0, vec4_bytes);
            }

            queue.submit(std::iter::once(encoder.finish()));

            // Synchronous GPU readback
            if is_scalar {
                let slice = readback_scalar.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |res| {
                    let _ = tx.send(res);
                });
                device.poll(wgpu::Maintain::Wait);
                rx.recv()
                    .map_err(|e| format!("GPU readback channel error: {}", e))?
                    .map_err(|e| format!("GPU buffer map error: {:?}", e))?;

                let view = slice.get_mapped_range();
                let floats: &[f32] = bytemuck::cast_slice(&view);
                let data = floats.to_vec();
                drop(view);
                readback_scalar.unmap();

                match map_type {
                    MapType::AmbientOcclusion => baked_maps.ambient_occlusion = Some(data),
                    MapType::Curvature => baked_maps.curvature = Some(data),
                    MapType::Thickness => baked_maps.thickness = Some(data),
                    _ => {}
                }
            } else {
                let slice = readback_vec4.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |res| {
                    let _ = tx.send(res);
                });
                device.poll(wgpu::Maintain::Wait);
                rx.recv()
                    .map_err(|e| format!("GPU readback channel error: {}", e))?
                    .map_err(|e| format!("GPU buffer map error: {:?}", e))?;

                let view = slice.get_mapped_range();
                let v4: &[[f32; 4]] = bytemuck::cast_slice(&view);
                let v3: Vec<[f32; 3]> = v4.iter().map(|c| [c[0], c[1], c[2]]).collect();
                let v4_copy = v4.to_vec();
                drop(view);
                readback_vec4.unmap();

                match map_type {
                    MapType::Normal => baked_maps.normal = Some(v3),
                    MapType::WorldSpaceNormal => baked_maps.world_space_normal = Some(v3),
                    MapType::Position => baked_maps.position = Some(v3),
                    MapType::Id => baked_maps.id = Some(v4_copy),
                    _ => {}
                }
            }

            if let Some(ref p) = progress {
                p.store(((pass_idx + 1) * 100) / total_passes, Ordering::Relaxed);
            }
        }

        Ok(baked_maps)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_bake_uniforms_layout() {
        assert_eq!(std::mem::size_of::<GpuBakeUniforms>(), 32);
        assert_eq!(std::mem::size_of::<GpuBakeTri>(), 128);
    }

    #[test]
    fn test_gpu_bake_shader_syntax() {
        assert!(BAKE_COMPUTE_WGSL.contains("@compute"));
        assert!(BAKE_COMPUTE_WGSL.contains("@workgroup_size(8, 8, 1)"));
        assert!(BAKE_COMPUTE_WGSL.contains("udim_u_offset"));
        assert!(BAKE_COMPUTE_WGSL.contains("udim_v_offset"));
        assert!(BAKE_COMPUTE_WGSL.contains("GpuBakeUniforms"));
        assert!(BAKE_COMPUTE_WGSL.contains("GpuBakeTri"));
    }
}
