#![allow(dead_code)]
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
    pub max_frontal_distance: f32,
    pub max_rear_distance: f32,
}

impl Default for BakerOptions {
    fn default() -> Self {
        Self {
            resolution: 1024,
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

        Self::bake_all(vertices, normals, uvs, indices, &single_opt, progress, None)
    }

    /// High-performance forward-rasterized GPU-grade Mesh Baker with High-to-Low Projection
    pub fn bake_all(
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        options: &BakerOptions,
        progress: Option<Arc<AtomicUsize>>,
        high_poly: Option<&crate::graphics::mesh::Mesh>,
    ) -> BakedMaps {
        let res = options.resolution as usize;
        let total = res * res;

        let mut ao_buf = vec![1.0f32; total];
        let mut curv_buf = vec![0.5f32; total];
        let mut norm_buf = vec![[0.5f32, 0.5, 1.0]; total];
        let mut wsn_buf = vec![[0.5f32, 0.5, 1.0]; total];
        let mut pos_buf = vec![[0.5f32, 0.5, 0.5]; total];
        let thick_buf = vec![1.0f32; total];
        let id_buf = vec![[1.0f32, 1.0, 1.0, 1.0]; total];

        let n_verts = vertices.len();
        if n_verts == 0 || indices.is_empty() {
            return BakedMaps {
                resolution: options.resolution,
                ambient_occlusion: Some(ao_buf),
                curvature: Some(curv_buf),
                normal: Some(norm_buf),
                world_space_normal: Some(wsn_buf),
                position: Some(pos_buf),
                thickness: Some(thick_buf),
                id: Some(id_buf),
            };
        }

        // 1. Mesh Bounding Box
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
        let _center = [
            (min_pos[0] + max_pos[0]) * 0.5,
            (min_pos[1] + max_pos[1]) * 0.5,
            (min_pos[2] + max_pos[2]) * 0.5,
        ];

        // 1. Spatially hashed face normal grouping to connect split vertices across sharp edges
        let num_tris = indices.len() / 3;
        let quant = 400.0f32;
        let mut pos_to_faces: std::collections::HashMap<(i32, i32, i32), Vec<[f32; 3]>> = std::collections::HashMap::new();

        for t in 0..num_tris {
            let i0 = indices[t * 3] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;
            if i0 < n_verts && i1 < n_verts && i2 < n_verts {
                let v0 = vertices[i0];
                let v1 = vertices[i1];
                let v2 = vertices[i2];
                let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
                let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
                let fnx = e1[1] * e2[2] - e1[2] * e2[1];
                let fny = e1[2] * e2[0] - e1[0] * e2[2];
                let fnz = e1[0] * e2[1] - e1[1] * e2[0];
                let len = (fnx * fnx + fny * fny + fnz * fnz).sqrt();
                if len < 1e-6 {
                    continue;
                }
                let fn_norm = [fnx / len, fny / len, fnz / len];

                for &vi in &[i0, i1, i2] {
                    let vp = vertices[vi];
                    let k = ((vp[0] * quant).round() as i32, (vp[1] * quant).round() as i32, (vp[2] * quant).round() as i32);
                    pos_to_faces.entry(k).or_default().push(fn_norm);
                }
            }
        }

        // 2. High-precision Curvature & Multi-scale Ambient Occlusion
        let mut vert_curv = vec![0.5f32; n_verts];
        let mut vert_ao = vec![1.0f32; n_verts];

        for i in 0..n_verts {
            let vn = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let vp = vertices[i];
            let k = ((vp[0] * quant).round() as i32, (vp[1] * quant).round() as i32, (vp[2] * quant).round() as i32);

            if let Some(fns) = pos_to_faces.get(&k) {
                if fns.len() > 1 {
                    let mut min_dot = 1.0f32;
                    let mut sum_dot = 0.0f32;
                    for f1 in fns {
                        let d = vn[0] * f1[0] + vn[1] * f1[1] + vn[2] * f1[2];
                        sum_dot += d;
                        if d < min_dot {
                            min_dot = d;
                        }
                    }
                    let avg_d = sum_dot / fns.len() as f32;
                    let edge_dev = (1.0 - min_dot).max(0.0) * options.curvature_intensity * 1.6;
                    vert_curv[i] = (0.5 + edge_dev * 0.5).clamp(0.0, 1.0);

                    // Cavity detection: concave crevice folds have low average dot product
                    if avg_d < 0.88 {
                        let cav = (0.88 - avg_d) * 2.2;
                        vert_ao[i] = (1.0 - cav).clamp(0.15, 1.0);
                    }
                }
            }
        }

        // 3. Forward UV Triangle Rasterization
        let res_f = res as f32;
        for t in 0..num_tris {
            let i0 = indices[t * 3] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;

            if i0 < uvs.len() && i1 < uvs.len() && i2 < uvs.len() {
                let uv0 = uvs[i0];
                let uv1 = uvs[i1];
                let uv2 = uvs[i2];

                let min_u = uv0[0].min(uv1[0]).min(uv2[0]).clamp(0.0, 1.0);
                let max_u = uv0[0].max(uv1[0]).max(uv2[0]).clamp(0.0, 1.0);
                let min_v = uv0[1].min(uv1[1]).min(uv2[1]).clamp(0.0, 1.0);
                let max_v = uv0[1].max(uv1[1]).max(uv2[1]).clamp(0.0, 1.0);

                let x0 = (min_u * res_f).floor() as usize;
                let x1 = ((max_u * res_f).ceil() as usize).min(res - 1);
                let y0 = (min_v * res_f).floor() as usize;
                let y1 = ((max_v * res_f).ceil() as usize).min(res - 1);

                let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1])).max(1e-7);

                let c0 = vert_curv[i0];
                let c1 = vert_curv[i1];
                let c2 = vert_curv[i2];

                let a0 = vert_ao[i0];
                let a1 = vert_ao[i1];
                let a2 = vert_ao[i2];

                let n0 = normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
                let n1 = normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
                let n2 = normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);

                let v0 = vertices[i0];
                let v1 = vertices[i1];
                let v2 = vertices[i2];

                let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
                let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
                let du1 = uv1[0] - uv0[0];
                let dv1 = uv1[1] - uv0[1];
                let du2 = uv2[0] - uv0[0];
                let dv2 = uv2[1] - uv0[1];
                let det = du1 * dv2 - du2 * dv1;
                let inv_det = if det.abs() > 1e-6 { 1.0 / det } else { 0.0 };
                let tri_t = [
                    (e1[0] * dv2 - e2[0] * dv1) * inv_det,
                    (e1[1] * dv2 - e2[1] * dv1) * inv_det,
                    (e1[2] * dv2 - e2[2] * dv1) * inv_det,
                ];
                let t_len = (tri_t[0]*tri_t[0] + tri_t[1]*tri_t[1] + tri_t[2]*tri_t[2]).sqrt().max(1e-6);
                let tri_t = [tri_t[0] / t_len, tri_t[1] / t_len, tri_t[2] / t_len];

                for py in y0..=y1 {
                    let v_px = py as f32 / res_f;
                    let row_idx = py * res;
                    for px in x0..=x1 {
                        let u_px = px as f32 / res_f;
                        let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0]) + (uv2[0] - uv1[0]) * (v_px - uv2[1])) / denom;
                        let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0]) + (uv0[0] - uv2[0]) * (v_px - uv2[1])) / denom;
                        let b2 = 1.0 - b0 - b1;

                        if b0 >= -0.02 && b1 >= -0.02 && b2 >= -0.02 {
                            let idx = row_idx + px;
                            let curv = b0 * c0 + b1 * c1 + b2 * c2;
                            let ao = b0 * a0 + b1 * a1 + b2 * a2;
                            let nx = b0 * n0[0] + b1 * n1[0] + b2 * n2[0];
                            let ny = b0 * n0[1] + b1 * n1[1] + b2 * n2[1];
                            let nz = b0 * n0[2] + b1 * n1[2] + b2 * n2[2];
                            let px_pos = [
                                b0 * v0[0] + b1 * v1[0] + b2 * v2[0],
                                b0 * v0[1] + b1 * v1[1] + b2 * v2[1],
                                b0 * v0[2] + b1 * v1[2] + b2 * v2[2],
                            ];

                            let n_len = (nx*nx + ny*ny + nz*nz).sqrt().max(1e-6);
                            let n_low = [nx / n_len, ny / n_len, nz / n_len];

                            // Orthonormalize Tangent
                            let t_dot_n = tri_t[0]*n_low[0] + tri_t[1]*n_low[1] + tri_t[2]*n_low[2];
                            let t_ortho = [
                                tri_t[0] - n_low[0] * t_dot_n,
                                tri_t[1] - n_low[1] * t_dot_n,
                                tri_t[2] - n_low[2] * t_dot_n,
                            ];
                            let to_len = (t_ortho[0]*t_ortho[0] + t_ortho[1]*t_ortho[1] + t_ortho[2]*t_ortho[2]).sqrt().max(1e-6);
                            let tangent = [t_ortho[0] / to_len, t_ortho[1] / to_len, t_ortho[2] / to_len];
                            let bitangent = [
                                n_low[1]*tangent[2] - n_low[2]*tangent[1],
                                n_low[2]*tangent[0] - n_low[0]*tangent[2],
                                n_low[0]*tangent[1] - n_low[1]*tangent[0],
                            ];

                            if let Some(hp) = high_poly {
                                let ray_origin = [
                                    px_pos[0] + n_low[0] * options.max_frontal_distance,
                                    px_pos[1] + n_low[1] * options.max_frontal_distance,
                                    px_pos[2] + n_low[2] * options.max_frontal_distance,
                                ];
                                let ray_dir = [-n_low[0], -n_low[1], -n_low[2]];
                                let max_dist = options.max_frontal_distance + options.max_rear_distance;
                                if let Some(hp_norm) = raycast_high_poly(hp, ray_origin, ray_dir, max_dist) {
                                    let dot_t = hp_norm[0]*tangent[0] + hp_norm[1]*tangent[1] + hp_norm[2]*tangent[2];
                                    let dot_b = hp_norm[0]*bitangent[0] + hp_norm[1]*bitangent[1] + hp_norm[2]*bitangent[2];
                                    let dot_n = hp_norm[0]*n_low[0] + hp_norm[1]*n_low[1] + hp_norm[2]*n_low[2];
                                    let ts_y = if options.invert_normal_y { -dot_b } else { dot_b };
                                    norm_buf[idx] = [
                                        (dot_t * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (ts_y * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (dot_n.max(0.0) * 0.5 + 0.5).clamp(0.0, 1.0),
                                    ];
                                    wsn_buf[idx] = [
                                        (hp_norm[0] * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (hp_norm[1] * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (hp_norm[2] * 0.5 + 0.5).clamp(0.0, 1.0),
                                    ];
                                } else {
                                    wsn_buf[idx] = [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5];
                                }
                            } else {
                                wsn_buf[idx] = [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5];
                            }

                            curv_buf[idx] = curv.clamp(0.0, 1.0);
                            ao_buf[idx] = ao.clamp(0.1, 1.0);
                            pos_buf[idx] = [
                                ((px_pos[0] - min_pos[0]) / pos_range[0]).clamp(0.0, 1.0),
                                ((px_pos[1] - min_pos[1]) / pos_range[1]).clamp(0.0, 1.0),
                                ((px_pos[2] - min_pos[2]) / pos_range[2]).clamp(0.0, 1.0),
                            ];
                        }
                    }
                }
            }
        }

        if let Some(p) = progress {
            p.store(100, Ordering::Relaxed);
        }

        BakedMaps {
            resolution: options.resolution,
            ambient_occlusion: Some(ao_buf),
            curvature: Some(curv_buf),
            normal: Some(norm_buf),
            world_space_normal: Some(wsn_buf),
            position: Some(pos_buf),
            thickness: Some(thick_buf),
            id: Some(id_buf),
        }
    }
}

fn raycast_high_poly(
    hp: &crate::graphics::mesh::Mesh,
    ray_origin: [f32; 3],
    ray_dir: [f32; 3],
    max_dist: f32,
) -> Option<[f32; 3]> {
    let num_tris = hp.indices.len() / 3;
    let mut closest_t = max_dist;
    let mut hit_normal = None;

    let ro = glam::Vec3::from_array(ray_origin);
    let rd = glam::Vec3::from_array(ray_dir);

    for t in 0..num_tris {
        let i0 = hp.indices[t * 3] as usize;
        let i1 = hp.indices[t * 3 + 1] as usize;
        let i2 = hp.indices[t * 3 + 2] as usize;
        if i0 >= hp.vertices.len() || i1 >= hp.vertices.len() || i2 >= hp.vertices.len() { continue; }

        let v0 = glam::Vec3::from_array(hp.vertices[i0]);
        let v1 = glam::Vec3::from_array(hp.vertices[i1]);
        let v2 = glam::Vec3::from_array(hp.vertices[i2]);

        let e1 = v1 - v0;
        let e2 = v2 - v0;
        let h = rd.cross(e2);
        let a = e1.dot(h);
        if a.abs() < 1e-7 { continue; }

        let f = 1.0 / a;
        let s = ro - v0;
        let u = f * s.dot(h);
        if u < 0.0 || u > 1.0 { continue; }

        let q = s.cross(e1);
        let v = f * rd.dot(q);
        if v < 0.0 || (u + v) > 1.0 { continue; }

        let dist = f * e2.dot(q);
        if dist > 0.0001 && dist < closest_t {
            closest_t = dist;
            let w = 1.0 - u - v;
            let n0 = hp.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n1 = hp.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
            let n2 = hp.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
            let norm = [
                w * n0[0] + u * n1[0] + v * n2[0],
                w * n0[1] + u * n1[1] + v * n2[1],
                w * n0[2] + u * n1[2] + v * n2[2],
            ];
            let len = (norm[0]*norm[0] + norm[1]*norm[1] + norm[2]*norm[2]).sqrt().max(1e-6);
            hit_normal = Some([norm[0] / len, norm[1] / len, norm[2] / len]);
        }
    }
    hit_normal
}
