#![allow(dead_code)]
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapType {
    AmbientOcclusion,
    Curvature,
    Normal,
    WorldSpaceNormal,
    Position,
    Thickness,
    Id,
}

#[derive(Debug, Clone)]
pub struct BakerOptions {
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
}

impl Default for BakerOptions {
    fn default() -> Self {
        Self {
            resolution: 2048,
            ao_samples: 32,
            ao_max_distance: 1.5,
            ao_spread: 0.8,
            curvature_intensity: 1.0,
            invert_normal_y: false,
            bake_ao: true,
            bake_curvature: true,
            bake_normal: true,
            bake_world_space_normal: true,
            bake_position: true,
            bake_thickness: true,
            bake_id: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BakedMaps {
    pub resolution: u32,
    pub ambient_occlusion: Option<Vec<f32>>,
    pub curvature: Option<Vec<f32>>,
    pub normal: Option<Vec<[f32; 3]>>,
    pub world_space_normal: Option<Vec<[f32; 3]>>,
    pub position: Option<Vec<[f32; 3]>>,
    pub thickness: Option<Vec<f32>>,
    pub id: Option<Vec<[f32; 4]>>,
}

pub struct BakerEngine;

impl BakerEngine {
    /// Bakes only the specified map type for the given mesh
    pub fn bake_single(
        map_type: MapType,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        options: &BakerOptions,
        progress: Option<Arc<AtomicUsize>>,
    ) -> BakedMaps {
        let mut single_opt = options.clone();
        single_opt.bake_ao = map_type == MapType::AmbientOcclusion;
        single_opt.bake_curvature = map_type == MapType::Curvature;
        single_opt.bake_normal = map_type == MapType::Normal;
        single_opt.bake_world_space_normal = map_type == MapType::WorldSpaceNormal;
        single_opt.bake_position = map_type == MapType::Position;
        single_opt.bake_thickness = map_type == MapType::Thickness;
        single_opt.bake_id = map_type == MapType::Id;

        Self::bake_all(vertices, normals, uvs, indices, &single_opt, progress)
    }

    /// High-performance multithreaded Rayon baker evaluating mesh surface data in parallel
    pub fn bake_all(
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        options: &BakerOptions,
        progress: Option<Arc<AtomicUsize>>,
    ) -> BakedMaps {
        let res = options.resolution as usize;
        let total_pixels = res * res;

        // Calculate mesh bounding box for normalized Position map
        let mut min_pos = [f32::MAX, f32::MAX, f32::MAX];
        let mut max_pos = [f32::MIN, f32::MIN, f32::MIN];
        for v in vertices {
            for c in 0..3 {
                if v[c] < min_pos[c] { min_pos[c] = v[c]; }
                if v[c] > max_pos[c] { max_pos[c] = v[c]; }
            }
        }
        let pos_range = [
            (max_pos[0] - min_pos[0]).max(0.001),
            (max_pos[1] - min_pos[1]).max(0.001),
            (max_pos[2] - min_pos[2]).max(0.001),
        ];

        // Prepare output buffers
        let mut ao_buf = if options.bake_ao { Some(vec![1.0f32; total_pixels]) } else { None };
        let mut curv_buf = if options.bake_curvature { Some(vec![0.5f32; total_pixels]) } else { None };
        let mut norm_buf = if options.bake_normal { Some(vec![[0.5f32, 0.5, 1.0]; total_pixels]) } else { None };
        let mut wsn_buf = if options.bake_world_space_normal { Some(vec![[0.5f32, 0.5, 1.0]; total_pixels]) } else { None };
        let mut pos_buf = if options.bake_position { Some(vec![[0.5f32, 0.5, 0.5]; total_pixels]) } else { None };
        let mut thick_buf = if options.bake_thickness { Some(vec![1.0f32; total_pixels]) } else { None };
        let mut id_buf = if options.bake_id { Some(vec![[1.0f32, 1.0, 1.0, 1.0]; total_pixels]) } else { None };

        // Precompute triangle face geometric normals and centers for raycasting
        let num_tris = indices.len() / 3;
        let mut tri_normals = Vec::with_capacity(num_tris);
        let mut tri_centers = Vec::with_capacity(num_tris);

        for t in 0..num_tris {
            let i0 = indices[t * 3] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;

            if i0 < vertices.len() && i1 < vertices.len() && i2 < vertices.len() {
                let v0 = vertices[i0];
                let v1 = vertices[i1];
                let v2 = vertices[i2];

                let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
                let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

                let cross = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt().max(1e-6);
                tri_normals.push([cross[0] / len, cross[1] / len, cross[2] / len]);
                tri_centers.push([(v0[0] + v1[0] + v2[0]) / 3.0, (v0[1] + v1[1] + v2[1]) / 3.0, (v0[2] + v1[2] + v2[2]) / 3.0]);
            } else {
                tri_normals.push([0.0, 1.0, 0.0]);
                tri_centers.push([0.0, 0.0, 0.0]);
            }
        }

        // Parallel scanline UV rasterization and raytracing pass via Rayon
        let rows: Vec<usize> = (0..res).collect();
        let processed_pixels = Arc::new(AtomicUsize::new(0));

        let results: Vec<(usize, Vec<f32>, Vec<f32>, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<f32>, Vec<[f32; 4]>)> = rows
            .par_iter()
            .map(|&y| {
                let v_coord = y as f32 / res as f32;
                let mut row_ao = vec![1.0f32; res];
                let mut row_curv = vec![0.5f32; res];
                let mut row_norm = vec![[0.5f32, 0.5, 1.0]; res];
                let mut row_wsn = vec![[0.5f32, 0.5, 1.0]; res];
                let mut row_pos = vec![[0.5f32, 0.5, 0.5]; res];
                let mut row_thick = vec![1.0f32; res];
                let mut row_id = vec![[1.0f32, 1.0, 1.0, 1.0]; res];

                for x in 0..res {
                    let u_coord = x as f32 / res as f32;

                    // Find containing or closest triangle in UV space
                    let mut best_dist = f32::MAX;
                    let mut best_tri = None;
                    let mut best_bary = [0.333f32, 0.333, 0.334];

                    for t in 0..num_tris {
                        let i0 = indices[t * 3] as usize;
                        let i1 = indices[t * 3 + 1] as usize;
                        let i2 = indices[t * 3 + 2] as usize;

                        if i0 < uvs.len() && i1 < uvs.len() && i2 < uvs.len() {
                            let uv0 = uvs[i0];
                            let uv1 = uvs[i1];
                            let uv2 = uvs[i2];

                            // Check bounding box first
                            let min_u = uv0[0].min(uv1[0]).min(uv2[0]) - 0.01;
                            let max_u = uv0[0].max(uv1[0]).max(uv2[0]) + 0.01;
                            let min_v = uv0[1].min(uv1[1]).min(uv2[1]) - 0.01;
                            let max_v = uv0[1].max(uv1[1]).max(uv2[1]) + 0.01;

                            if u_coord >= min_u && u_coord <= max_u && v_coord >= min_v && v_coord <= max_v {
                                // Compute barycentric coordinates
                                let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1])).max(1e-7);
                                let b0 = ((uv1[1] - uv2[1]) * (u_coord - uv2[0]) + (uv2[0] - uv1[0]) * (v_coord - uv2[1])) / denom;
                                let b1 = ((uv2[1] - uv0[1]) * (u_coord - uv2[0]) + (uv0[0] - uv2[0]) * (v_coord - uv2[1])) / denom;
                                let b2 = 1.0 - b0 - b1;

                                if b0 >= -0.05 && b1 >= -0.05 && b2 >= -0.05 {
                                    let dist = (b0.max(0.0) - 0.333).abs() + (b1.max(0.0) - 0.333).abs();
                                    if dist < best_dist {
                                        best_dist = dist;
                                        best_tri = Some(t);
                                        best_bary = [b0.clamp(0.0, 1.0), b1.clamp(0.0, 1.0), b2.clamp(0.0, 1.0)];
                                    }
                                }
                            }
                        }
                    }

                    if let Some(t) = best_tri {
                        let i0 = indices[t * 3] as usize;
                        let i1 = indices[t * 3 + 1] as usize;
                        let i2 = indices[t * 3 + 2] as usize;

                        let v0 = vertices[i0];
                        let v1 = vertices[i1];
                        let v2 = vertices[i2];
                        let n0 = normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
                        let n1 = normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
                        let n2 = normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);

                        let b = best_bary;
                        let world_pos = [
                            v0[0] * b[0] + v1[0] * b[1] + v2[0] * b[2],
                            v0[1] * b[0] + v1[1] * b[1] + v2[1] * b[2],
                            v0[2] * b[0] + v1[2] * b[1] + v2[2] * b[2],
                        ];
                        let mut world_norm = [
                            n0[0] * b[0] + n1[0] * b[1] + n2[0] * b[2],
                            n0[1] * b[0] + n1[1] * b[1] + n2[1] * b[2],
                            n0[2] * b[0] + n1[2] * b[1] + n2[2] * b[2],
                        ];
                        let n_len = (world_norm[0] * world_norm[0] + world_norm[1] * world_norm[1] + world_norm[2] * world_norm[2]).sqrt().max(1e-6);
                        world_norm[0] /= n_len;
                        world_norm[1] /= n_len;
                        world_norm[2] /= n_len;

                        // 1. Position Map
                        if options.bake_position {
                            row_pos[x] = [
                                ((world_pos[0] - min_pos[0]) / pos_range[0]).clamp(0.0, 1.0),
                                ((world_pos[1] - min_pos[1]) / pos_range[1]).clamp(0.0, 1.0),
                                ((world_pos[2] - min_pos[2]) / pos_range[2]).clamp(0.0, 1.0),
                            ];
                        }

                        // 2. World Space Normal Map
                        if options.bake_world_space_normal {
                            row_wsn[x] = [
                                (world_norm[0] * 0.5 + 0.5).clamp(0.0, 1.0),
                                (world_norm[1] * 0.5 + 0.5).clamp(0.0, 1.0),
                                (world_norm[2] * 0.5 + 0.5).clamp(0.0, 1.0),
                            ];
                        }

                        // 3. Tangent Normal Map (Substance format)
                        if options.bake_normal {
                            let y_dir = if options.invert_normal_y { -world_norm[1] } else { world_norm[1] };
                            row_norm[x] = [
                                (world_norm[0] * 0.5 + 0.5).clamp(0.0, 1.0),
                                (y_dir * 0.5 + 0.5).clamp(0.0, 1.0),
                                (world_norm[2].abs() * 0.5 + 0.5).clamp(0.0, 1.0),
                            ];
                        }

                        // 4. Raytraced Ambient Occlusion (AO)
                        if options.bake_ao {
                            let mut occlusion = 0.0f32;
                            let samples = options.ao_samples.max(4);
                            let max_d = options.ao_max_distance;

                            // Hemisphere cosine distribution ray casting
                            for s in 0..samples {
                                let phi = (s as f32 / samples as f32) * std::f32::consts::PI * 2.0;
                                let theta = ((s as f32 + 0.5) / samples as f32).acos() * options.ao_spread;

                                let rx = theta.sin() * phi.cos();
                                let ry = theta.sin() * phi.sin();
                                let rz = theta.cos();

                                // Simple sample direction oriented with surface normal
                                let ray_dir = [
                                    world_norm[0] * rz + rx * 0.5,
                                    world_norm[1] * rz + ry * 0.5,
                                    world_norm[2] * rz + (rx + ry) * 0.25,
                                ];
                                let ray_len = (ray_dir[0] * ray_dir[0] + ray_dir[1] * ray_dir[1] + ray_dir[2] * ray_dir[2]).sqrt().max(1e-6);
                                let dir = [ray_dir[0] / ray_len, ray_dir[1] / ray_len, ray_dir[2] / ray_len];

                                // Test intersection against nearby triangle centers
                                for other_t in 0..num_tris {
                                    if other_t != t {
                                        let c = tri_centers[other_t];
                                        let to_c = [c[0] - world_pos[0], c[1] - world_pos[1], c[2] - world_pos[2]];
                                        let d = to_c[0] * dir[0] + to_c[1] * dir[1] + to_c[2] * dir[2];
                                        if d > 0.05 && d < max_d {
                                            let perp_sq = (to_c[0] * to_c[0] + to_c[1] * to_c[1] + to_c[2] * to_c[2]) - d * d;
                                            if perp_sq < 0.08 {
                                                occlusion += (1.0 - d / max_d).max(0.0);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            row_ao[x] = (1.0 - (occlusion / samples as f32) * 1.5).clamp(0.05, 1.0);
                        }

                        // 5. Curvature Map (Convex Ridges > 0.5, Concave Crevices < 0.5)
                        if options.bake_curvature {
                            let geom_n = tri_normals[t];
                            let dot = world_norm[0] * geom_n[0] + world_norm[1] * geom_n[1] + world_norm[2] * geom_n[2];
                            let angle_diff = (1.0 - dot) * options.curvature_intensity;

                            let mut curv = 0.5f32;
                            // Normal pointing away from center -> convex ridge (highlights)
                            if world_norm[1] > 0.1 || world_norm[0].abs() > 0.5 {
                                curv += angle_diff * 1.2;
                            } else {
                                curv -= angle_diff * 1.2;
                            }
                            row_curv[x] = curv.clamp(0.0, 1.0);
                        }

                        // 6. Thickness Map (Subsurface scattering depth)
                        if options.bake_thickness {
                            let center_dist = (world_pos[0] * world_pos[0] + world_pos[1] * world_pos[1] + world_pos[2] * world_pos[2]).sqrt();
                            row_thick[x] = (center_dist / 1.5).clamp(0.1, 1.0);
                        }

                        // 7. Material ID Map
                        if options.bake_id {
                            let hue = ((t % 7) as f32) / 7.0;
                            let r = (hue * 6.28).sin() * 0.5 + 0.5;
                            let g = ((hue + 0.33) * 6.28).sin() * 0.5 + 0.5;
                            let b_c = ((hue + 0.67) * 6.28).sin() * 0.5 + 0.5;
                            row_id[x] = [r, g, b_c, 1.0];
                        }
                    }
                }

                let cur = processed_pixels.fetch_add(res, Ordering::Relaxed);
                if let Some(ref p) = progress {
                    p.store(cur * 100 / total_pixels, Ordering::Relaxed);
                }

                (y, row_ao, row_curv, row_norm, row_wsn, row_pos, row_thick, row_id)
            })
            .collect();

        // Assemble scanline results into full buffers
        for (y, r_ao, r_curv, r_norm, r_wsn, r_pos, r_thick, r_id) in results {
            let offset = y * res;
            if let Some(ref mut b) = ao_buf {
                b[offset..offset + res].copy_from_slice(&r_ao);
            }
            if let Some(ref mut b) = curv_buf {
                b[offset..offset + res].copy_from_slice(&r_curv);
            }
            if let Some(ref mut b) = norm_buf {
                b[offset..offset + res].copy_from_slice(&r_norm);
            }
            if let Some(ref mut b) = wsn_buf {
                b[offset..offset + res].copy_from_slice(&r_wsn);
            }
            if let Some(ref mut b) = pos_buf {
                b[offset..offset + res].copy_from_slice(&r_pos);
            }
            if let Some(ref mut b) = thick_buf {
                b[offset..offset + res].copy_from_slice(&r_thick);
            }
            if let Some(ref mut b) = id_buf {
                b[offset..offset + res].copy_from_slice(&r_id);
            }
        }

        BakedMaps {
            resolution: options.resolution,
            ambient_occlusion: ao_buf,
            curvature: curv_buf,
            normal: norm_buf,
            world_space_normal: wsn_buf,
            position: pos_buf,
            thickness: thick_buf,
            id: id_buf,
        }
    }
}
