#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

#[derive(Debug, Clone, PartialEq)]
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
    pub match_by_name: bool,
    pub low_poly_suffix: String,
    pub high_poly_suffix: String,
    pub anti_aliasing: u32, // 1 = None, 2 = 2x2 (4 samples), 4 = 4x4 (16 samples)
    pub relative_to_bbox: bool,
    pub average_normals: bool,
    pub ignore_backfaces: bool,

    // Skew correction ("Use Skew Correction" / USE_SKEW_MAP parity): a 2-channel shear map
    // that warps the high-poly sample position along the tangent/bitangent plane to correct
    // normal-projection stretching on distorted UVs. `skew_map` data holds decoded (u, v)
    // shear offsets in [-1, 1].
    pub use_skew_correction: bool,
    pub skew_invert: bool,
    pub skew_strength: f32,
    pub skew_map: Option<Arc<SkewMap>>,
}

/// External skew texture: RGB image loaded by the App, R/G decoded to [-1, 1] shear offsets.
#[derive(Debug, Clone, PartialEq)]
pub struct SkewMap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<[f32; 2]>,
}

impl SkewMap {
    /// Sample the shear offset at normalized (u, v) texture coordinates.
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> [f32; 2] {
        let px = (u.clamp(0.0, 1.0) * (self.width.max(1) - 1) as f32).round() as u32;
        let py = (v.clamp(0.0, 1.0) * (self.height.max(1) - 1) as f32).round() as u32;
        self.data[((py.min(self.height.saturating_sub(1)) as usize) * (self.width as usize))
            + px.min(self.width.saturating_sub(1)) as usize]
            .map(|o| o.clamp(-1.0, 1.0))
    }
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
            use_skew_correction: false,
            skew_invert: false,
            skew_strength: 0.6,
            skew_map: None,
            max_frontal_distance: 0.05,
            max_rear_distance: 0.05,
            match_by_name: false,
            low_poly_suffix: "_low".to_string(),
            high_poly_suffix: "_high".to_string(),
            anti_aliasing: 1,
            relative_to_bbox: false,
            average_normals: true,
            ignore_backfaces: false,
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
    pub is_cancelled: bool,
}

// ----------------------------------------------------------------------------
// High-Performance Bounding Volume Hierarchy (BVH) for Fast Ray-Mesh Queries
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BoundingBox {
    pub fn empty() -> Self {
        Self {
            min: [f32::MAX, f32::MAX, f32::MAX],
            max: [f32::MIN, f32::MIN, f32::MIN],
        }
    }

    pub fn grow(&mut self, p: [f32; 3]) {
        for i in 0..3 {
            if p[i] < self.min[i] {
                self.min[i] = p[i];
            }
            if p[i] > self.max[i] {
                self.max[i] = p[i];
            }
        }
    }

    pub fn grow_box(&mut self, other: &BoundingBox) {
        self.grow(other.min);
        self.grow(other.max);
    }

    pub fn intersect_ray(&self, origin: &[f32; 3], inv_dir: &[f32; 3], max_t: f32) -> bool {
        let mut tmin = 0.0f32;
        let mut tmax = max_t;
        for i in 0..3 {
            let t1 = (self.min[i] - origin[i]) * inv_dir[i];
            let t2 = (self.max[i] - origin[i]) * inv_dir[i];
            let (tnear, tfar) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            tmin = tmin.max(tnear);
            tmax = tmax.min(tfar);
            if tmin > tmax {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct BvhTriangle {
    pub v0: [f32; 3],
    pub v1: [f32; 3],
    pub v2: [f32; 3],
    pub n0: [f32; 3],
    pub n1: [f32; 3],
    pub n2: [f32; 3],
    pub bbox: BoundingBox,
    pub center: [f32; 3],
}

#[derive(Debug, Clone)]
pub enum BvhNode {
    Leaf {
        bbox: BoundingBox,
        triangles: Vec<BvhTriangle>,
    },
    Internal {
        bbox: BoundingBox,
        left: Box<BvhNode>,
        right: Box<BvhNode>,
    },
}

#[derive(Debug, Clone)]
pub struct HighPolyHit {
    pub distance: f32,
    pub normal: [f32; 3],
    pub position: [f32; 3],
}

impl BvhNode {
    pub fn build(mut tris: Vec<BvhTriangle>) -> Self {
        if tris.is_empty() {
            return BvhNode::Leaf {
                bbox: BoundingBox::empty(),
                triangles: Vec::new(),
            };
        }

        let mut bbox = BoundingBox::empty();
        for t in &tris {
            bbox.grow_box(&t.bbox);
        }

        if tris.len() <= 16 {
            return BvhNode::Leaf {
                bbox,
                triangles: tris,
            };
        }

        let dx = bbox.max[0] - bbox.min[0];
        let dy = bbox.max[1] - bbox.min[1];
        let dz = bbox.max[2] - bbox.min[2];
        let axis = if dx >= dy && dx >= dz {
            0
        } else if dy >= dz {
            1
        } else {
            2
        };

        tris.sort_unstable_by(|a, b| {
            a.center[axis]
                .partial_cmp(&b.center[axis])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mid = tris.len() / 2;
        let right_tris = tris.split_off(mid);
        let left = BvhNode::build(tris);
        let right = BvhNode::build(right_tris);

        BvhNode::Internal {
            bbox,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub fn intersect_ray(
        &self,
        origin: &[f32; 3],
        dir: &[f32; 3],
        inv_dir: &[f32; 3],
        closest_t: &mut f32,
        hit: &mut Option<HighPolyHit>,
        ignore_backfaces: bool,
    ) {
        let bbox = match self {
            BvhNode::Leaf { bbox, .. } => bbox,
            BvhNode::Internal { bbox, .. } => bbox,
        };
        if !bbox.intersect_ray(origin, inv_dir, *closest_t) {
            return;
        }

        match self {
            BvhNode::Leaf { triangles, .. } => {
                for tri in triangles {
                    if let Some(h) =
                        intersect_triangle_moller(origin, dir, tri, *closest_t, ignore_backfaces)
                    {
                        if h.distance < *closest_t {
                            *closest_t = h.distance;
                            *hit = Some(h);
                        }
                    }
                }
            }
            BvhNode::Internal { left, right, .. } => {
                left.intersect_ray(origin, dir, inv_dir, closest_t, hit, ignore_backfaces);
                right.intersect_ray(origin, dir, inv_dir, closest_t, hit, ignore_backfaces);
            }
        }
    }
}

fn intersect_triangle_moller(
    origin: &[f32; 3],
    dir: &[f32; 3],
    tri: &BvhTriangle,
    max_dist: f32,
    ignore_backfaces: bool,
) -> Option<HighPolyHit> {
    let ro = glam::Vec3::from_array(*origin);
    let rd = glam::Vec3::from_array(*dir);
    let v0 = glam::Vec3::from_array(tri.v0);
    let v1 = glam::Vec3::from_array(tri.v1);
    let v2 = glam::Vec3::from_array(tri.v2);

    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = rd.cross(e2);
    let a = e1.dot(h);

    if ignore_backfaces {
        if a <= 1e-7 {
            return None;
        }
    } else if a.abs() < 1e-7 {
        return None;
    }

    let f = 1.0 / a;
    let s = ro - v0;
    let u = f * s.dot(h);
    if u < -1e-4 || u > 1.0 + 1e-4 {
        return None;
    }

    let q = s.cross(e1);
    let v = f * rd.dot(q);
    if v < -1e-4 || (u + v) > 1.0 + 1e-4 {
        return None;
    }

    let dist = f * e2.dot(q);
    if dist > 1e-4 && dist < max_dist {
        let w = 1.0 - u - v;
        let nx = w * tri.n0[0] + u * tri.n1[0] + v * tri.n2[0];
        let ny = w * tri.n0[1] + u * tri.n1[1] + v * tri.n2[1];
        let nz = w * tri.n0[2] + u * tri.n1[2] + v * tri.n2[2];
        let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
        let normal = [nx / len, ny / len, nz / len];
        let position = [
            w * tri.v0[0] + u * tri.v1[0] + v * tri.v2[0],
            w * tri.v0[1] + u * tri.v1[1] + v * tri.v2[1],
            w * tri.v0[2] + u * tri.v1[2] + v * tri.v2[2],
        ];
        Some(HighPolyHit {
            distance: dist,
            normal,
            position,
        })
    } else {
        None
    }
}

// ----------------------------------------------------------------------------
// Baker Engine
// ----------------------------------------------------------------------------

pub struct BakerEngine;

impl BakerEngine {
    /// Bakes only the specified map type for the given mesh
    pub fn bake_single(
        map_type: MapType,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        submeshes: &[crate::graphics::mesh::SubMesh],
        options: &BakerOptions,
        progress: Option<Arc<AtomicUsize>>,
        cancel: Option<Arc<AtomicBool>>,
        high_poly: Option<&crate::graphics::mesh::Mesh>,
    ) -> BakedMaps {
        let mut single_opt = options.clone();
        single_opt.bake_ao = map_type == MapType::AmbientOcclusion;
        single_opt.bake_curvature = map_type == MapType::Curvature;
        single_opt.bake_normal = map_type == MapType::Normal;
        single_opt.bake_world_space_normal = map_type == MapType::WorldSpaceNormal;
        single_opt.bake_position = map_type == MapType::Position;
        single_opt.bake_thickness = map_type == MapType::Thickness;
        single_opt.bake_id = map_type == MapType::Id;

        Self::bake_all(
            vertices,
            normals,
            uvs,
            indices,
            submeshes,
            &single_opt,
            progress,
            cancel,
            high_poly,
        )
    }

    /// High-performance GPU-grade Mesh Baker with High-to-Low Projection,
    /// BVH Acceleration, Match by Mesh Name, Anti-Aliasing, and Cancelation Support.
    pub fn bake_all(
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        submeshes: &[crate::graphics::mesh::SubMesh],
        options: &BakerOptions,
        progress: Option<Arc<AtomicUsize>>,
        cancel: Option<Arc<AtomicBool>>,
        high_poly: Option<&crate::graphics::mesh::Mesh>,
    ) -> BakedMaps {
        let res = options.resolution as usize;
        let total = res * res;

        let mut ao_buf = vec![1.0f32; total];
        let mut curv_buf = vec![0.5f32; total];
        let mut norm_buf = vec![[0.5f32, 0.5, 1.0]; total];
        let mut wsn_buf = vec![[0.5f32, 0.5, 1.0]; total];
        let mut pos_buf = vec![[0.5f32, 0.5, 0.5]; total];
        let mut thick_buf = vec![1.0f32; total];
        let mut id_buf = vec![[1.0f32, 1.0, 1.0, 1.0]; total];

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
                is_cancelled: false,
            };
        }

        // 1. Mesh Bounding Box
        let mut min_pos = [f32::MAX, f32::MAX, f32::MAX];
        let mut max_pos = [f32::MIN, f32::MIN, f32::MIN];
        for v in vertices {
            for c in 0..3 {
                if v[c] < min_pos[c] {
                    min_pos[c] = v[c];
                }
                if v[c] > max_pos[c] {
                    max_pos[c] = v[c];
                }
            }
        }
        let pos_range = [
            (max_pos[0] - min_pos[0]).max(0.001),
            (max_pos[1] - min_pos[1]).max(0.001),
            (max_pos[2] - min_pos[2]).max(0.001),
        ];
        let bbox_diag = (pos_range[0] * pos_range[0]
            + pos_range[1] * pos_range[1]
            + pos_range[2] * pos_range[2])
            .sqrt()
            .max(0.001);

        let frontal_dist = if options.relative_to_bbox {
            options.max_frontal_distance * bbox_diag
        } else {
            options.max_frontal_distance
        };
        let rear_dist = if options.relative_to_bbox {
            options.max_rear_distance * bbox_diag
        } else {
            options.max_rear_distance
        };

        // 2. Build High-Poly BVH Trees (Global and per submesh for Match-by-Name)
        let (hp_global_bvh, hp_submesh_bvhs) = if let Some(hp) = high_poly {
            let num_hp_tris = hp.indices.len() / 3;
            let mut all_hp_tris = Vec::with_capacity(num_hp_tris);

            for t in 0..num_hp_tris {
                let i0 = hp.indices[t * 3] as usize;
                let i1 = hp.indices[t * 3 + 1] as usize;
                let i2 = hp.indices[t * 3 + 2] as usize;
                if i0 < hp.vertices.len() && i1 < hp.vertices.len() && i2 < hp.vertices.len() {
                    let v0 = hp.vertices[i0];
                    let v1 = hp.vertices[i1];
                    let v2 = hp.vertices[i2];
                    let n0 = hp.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
                    let n1 = hp.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
                    let n2 = hp.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);

                    let mut tb = BoundingBox::empty();
                    tb.grow(v0);
                    tb.grow(v1);
                    tb.grow(v2);

                    all_hp_tris.push(BvhTriangle {
                        v0,
                        v1,
                        v2,
                        n0,
                        n1,
                        n2,
                        bbox: tb,
                        center: [
                            (v0[0] + v1[0] + v2[0]) / 3.0,
                            (v0[1] + v1[1] + v2[1]) / 3.0,
                            (v0[2] + v1[2] + v2[2]) / 3.0,
                        ],
                    });
                }
            }

            let global = BvhNode::build(all_hp_tris);

            let mut by_name_bvhs = HashMap::new();
            if options.match_by_name && !hp.submeshes.is_empty() {
                for sm in &hp.submeshes {
                    let base_name = strip_mesh_suffix(&sm.name, &options.high_poly_suffix);
                    let mut sm_tris = Vec::new();
                    let tri_start = sm.index_start / 3;
                    let tri_count = sm.index_count / 3;
                    for t in tri_start..(tri_start + tri_count) {
                        if t * 3 + 2 < hp.indices.len() {
                            let i0 = hp.indices[t * 3] as usize;
                            let i1 = hp.indices[t * 3 + 1] as usize;
                            let i2 = hp.indices[t * 3 + 2] as usize;
                            if i0 < hp.vertices.len()
                                && i1 < hp.vertices.len()
                                && i2 < hp.vertices.len()
                            {
                                let v0 = hp.vertices[i0];
                                let v1 = hp.vertices[i1];
                                let v2 = hp.vertices[i2];
                                let n0 = hp.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
                                let n1 = hp.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
                                let n2 = hp.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
                                let mut tb = BoundingBox::empty();
                                tb.grow(v0);
                                tb.grow(v1);
                                tb.grow(v2);
                                sm_tris.push(BvhTriangle {
                                    v0,
                                    v1,
                                    v2,
                                    n0,
                                    n1,
                                    n2,
                                    bbox: tb,
                                    center: [
                                        (v0[0] + v1[0] + v2[0]) / 3.0,
                                        (v0[1] + v1[1] + v2[1]) / 3.0,
                                        (v0[2] + v1[2] + v2[2]) / 3.0,
                                    ],
                                });
                            }
                        }
                    }
                    if !sm_tris.is_empty() {
                        by_name_bvhs.insert(base_name.to_lowercase(), BvhNode::build(sm_tris));
                    }
                }
            }

            (Some(global), by_name_bvhs)
        } else {
            (None, HashMap::new())
        };

        // 3. Spatially hashed face normal grouping for low-poly fallback curvature & AO
        let num_tris = indices.len() / 3;
        let quant = 400.0f32;
        let mut pos_to_faces: HashMap<(i32, i32, i32), Vec<[f32; 3]>> = HashMap::new();

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
                    let k = (
                        (vp[0] * quant).round() as i32,
                        (vp[1] * quant).round() as i32,
                        (vp[2] * quant).round() as i32,
                    );
                    pos_to_faces.entry(k).or_default().push(fn_norm);
                }
            }
        }

        let mut vert_curv = vec![0.5f32; n_verts];
        let mut vert_ao = vec![1.0f32; n_verts];

        for i in 0..n_verts {
            let vn = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let vp = vertices[i];
            let k = (
                (vp[0] * quant).round() as i32,
                (vp[1] * quant).round() as i32,
                (vp[2] * quant).round() as i32,
            );

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

                    if avg_d < 0.88 {
                        let cav = (0.88 - avg_d) * 2.2;
                        vert_ao[i] = (1.0 - cav).clamp(0.15, 1.0);
                    }
                }
            }
        }

        // 4. Multi-Sample Accumulation Buffer for Anti-Aliased Rasterization
        #[derive(Clone, Default)]
        struct PixelAccum {
            weight: f32,
            ao: f32,
            curv: f32,
            norm: [f32; 3],
            wsn: [f32; 3],
            pos: [f32; 3],
            thick: f32,
            id: [f32; 4],
        }

        let mut accum_buf = vec![PixelAccum::default(); total];

        // Determine subpixel jitter offsets based on anti_aliasing setting
        let subpixel_offsets: &[(f32, f32)] = match options.anti_aliasing {
            2 => &[(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)],
            4 => &[
                (0.125, 0.125),
                (0.375, 0.125),
                (0.625, 0.125),
                (0.875, 0.125),
                (0.125, 0.375),
                (0.375, 0.375),
                (0.625, 0.375),
                (0.875, 0.375),
                (0.125, 0.625),
                (0.375, 0.625),
                (0.625, 0.625),
                (0.875, 0.625),
                (0.125, 0.875),
                (0.375, 0.875),
                (0.625, 0.875),
                (0.875, 0.875),
            ],
            _ => &[(0.5, 0.5)],
        };
        let sample_w = 1.0 / (subpixel_offsets.len() as f32);

        let res_f = res as f32;

        // 5. Forward UV Triangle Rasterization
        for t in 0..num_tris {
            // Check cancellation request every 64 triangles
            if (t & 63) == 0 {
                if let Some(ref c) = cancel {
                    if c.load(Ordering::Relaxed) {
                        return BakedMaps {
                            resolution: options.resolution,
                            ambient_occlusion: Some(ao_buf),
                            curvature: Some(curv_buf),
                            normal: Some(norm_buf),
                            world_space_normal: Some(wsn_buf),
                            position: Some(pos_buf),
                            thickness: Some(thick_buf),
                            id: Some(id_buf),
                            is_cancelled: true,
                        };
                    }
                }
                if let Some(ref p) = progress {
                    p.store((t * 100) / num_tris, Ordering::Relaxed);
                }
            }

            let tri_idx_start = t * 3;
            let i0 = indices[tri_idx_start] as usize;
            let i1 = indices[tri_idx_start + 1] as usize;
            let i2 = indices[tri_idx_start + 2] as usize;

            if i0 >= uvs.len() || i1 >= uvs.len() || i2 >= uvs.len() {
                continue;
            }

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

            let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0])
                + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
                .max(1e-7);

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
            // World-space span of one U/V step on this triangle: needed for both the handedness
            // correction and the skew-map shear offsets (texel-footprint scaling).
            let t_span = (tri_t[0] * tri_t[0] + tri_t[1] * tri_t[1] + tri_t[2] * tri_t[2])
                .sqrt()
                .max(1e-6);
            let bit_span = {
                let bx = (e2[0] * du1 - e1[0] * du2) * inv_det;
                let by = (e2[1] * du1 - e1[1] * du2) * inv_det;
                let bz = (e2[2] * du1 - e1[2] * du2) * inv_det;
                (bx * bx + by * by + bz * bz).sqrt().max(1e-9)
            };
            let handedness = if det < 0.0 { -1.0 } else { 1.0 };
            let t_len = (tri_t[0] * tri_t[0] + tri_t[1] * tri_t[1] + tri_t[2] * tri_t[2])
                .sqrt()
                .max(1e-6);
            let tri_t = [tri_t[0] / t_len, tri_t[1] / t_len, tri_t[2] / t_len];

            // Match-by-Name BVH selection for this triangle
            let target_bvh = if options.match_by_name && !hp_submesh_bvhs.is_empty() {
                let matching_submesh = submeshes.iter().find(|sm| {
                    tri_idx_start >= sm.index_start
                        && tri_idx_start < sm.index_start + sm.index_count
                });
                if let Some(sm) = matching_submesh {
                    let low_base =
                        strip_mesh_suffix(&sm.name, &options.low_poly_suffix).to_lowercase();
                    hp_submesh_bvhs.get(&low_base).or(hp_global_bvh.as_ref())
                } else {
                    hp_global_bvh.as_ref()
                }
            } else {
                hp_global_bvh.as_ref()
            };

            for py in y0..=y1 {
                let row_idx = py * res;
                for px in x0..=x1 {
                    for &(ox, oy) in subpixel_offsets {
                        let u_px = (px as f32 + ox) / res_f;
                        let v_px = (py as f32 + oy) / res_f;

                        let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                            + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                            / denom;
                        let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                            + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                            / denom;
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

                            let n_len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
                            let n_low = [nx / n_len, ny / n_len, nz / n_len];

                            // Orthonormalize Tangent
                            let t_dot_n =
                                tri_t[0] * n_low[0] + tri_t[1] * n_low[1] + tri_t[2] * n_low[2];
                            let t_ortho = [
                                tri_t[0] - n_low[0] * t_dot_n,
                                tri_t[1] - n_low[1] * t_dot_n,
                                tri_t[2] - n_low[2] * t_dot_n,
                            ];
                            let to_len = (t_ortho[0] * t_ortho[0]
                                + t_ortho[1] * t_ortho[1]
                                + t_ortho[2] * t_ortho[2])
                                .sqrt()
                                .max(1e-6);
                            let tangent = [
                                t_ortho[0] / to_len,
                                t_ortho[1] / to_len,
                                t_ortho[2] / to_len,
                            ];
                            // Mirrored-UV parity fix: flip the bitangent on negative UV determinant
                            // (matching Substance's signed handedness) so mirrored islands no longer
                            // bake with an inverted G channel.
                            let bitangent = [
                                (n_low[1] * tangent[2] - n_low[2] * tangent[1]) * handedness,
                                (n_low[2] * tangent[0] - n_low[0] * tangent[2]) * handedness,
                                (n_low[0] * tangent[1] - n_low[1] * tangent[0]) * handedness,
                            ];

                            let mut final_norm = [0.5f32, 0.5, 1.0];
                            let mut final_wsn = [
                                n_low[0] * 0.5 + 0.5,
                                n_low[1] * 0.5 + 0.5,
                                n_low[2] * 0.5 + 0.5,
                            ];
                            let mut final_pos = [
                                ((px_pos[0] - min_pos[0]) / pos_range[0]).clamp(0.0, 1.0),
                                ((px_pos[1] - min_pos[1]) / pos_range[1]).clamp(0.0, 1.0),
                                ((px_pos[2] - min_pos[2]) / pos_range[2]).clamp(0.0, 1.0),
                            ];
                            let mut final_ao = ao.clamp(0.1, 1.0);
                            let final_curv = curv.clamp(0.0, 1.0);
                            let mut final_thick = 1.0f32;
                            let final_id = [1.0f32, 1.0, 1.0, 1.0];

                            // Skew correction: sample the shear map at this texel and offset the
                            // ray origin along the tangent/bitangent plane (texel-footprint
                            // scaled), compensating projected-normal stretching ("External skew
                            // texture from file" / "Invert skew correction" parity).
                            let (skew_o, skew_b) =
                                if options.use_skew_correction && options.bake_normal {
                                    if let Some(ref skew) = options.skew_map {
                                        let o = skew.sample(
                                            px as f32 / res_f.max(1.0),
                                            py as f32 / res_f.max(1.0),
                                        );
                                        let s = if options.skew_invert {
                                            -options.skew_strength
                                        } else {
                                            options.skew_strength
                                        };
                                        (o[0] * s, o[1] * s)
                                    } else {
                                        (0.0, 0.0)
                                    }
                                } else {
                                    (0.0, 0.0)
                                };
                            let skew_world = [
                                tangent[0] * (skew_o * t_span / res_f)
                                    + bitangent[0] * (skew_b * bit_span / res_f),
                                tangent[1] * (skew_o * t_span / res_f)
                                    + bitangent[1] * (skew_b * bit_span / res_f),
                                tangent[2] * (skew_o * t_span / res_f)
                                    + bitangent[2] * (skew_b * bit_span / res_f),
                            ];

                            // High-poly raycasting through BVH
                            if let Some(bvh) = target_bvh {
                                let ray_origin = [
                                    px_pos[0] + n_low[0] * frontal_dist + skew_world[0],
                                    px_pos[1] + n_low[1] * frontal_dist + skew_world[1],
                                    px_pos[2] + n_low[2] * frontal_dist + skew_world[2],
                                ];
                                let ray_dir = [-n_low[0], -n_low[1], -n_low[2]];
                                let max_dist = frontal_dist + rear_dist;

                                let inv_dir = [
                                    if ray_dir[0].abs() > 1e-6 {
                                        1.0 / ray_dir[0]
                                    } else {
                                        1e6
                                    },
                                    if ray_dir[1].abs() > 1e-6 {
                                        1.0 / ray_dir[1]
                                    } else {
                                        1e6
                                    },
                                    if ray_dir[2].abs() > 1e-6 {
                                        1.0 / ray_dir[2]
                                    } else {
                                        1e6
                                    },
                                ];

                                let mut closest_t = max_dist;
                                let mut hit: Option<HighPolyHit> = None;
                                bvh.intersect_ray(
                                    &ray_origin,
                                    &ray_dir,
                                    &inv_dir,
                                    &mut closest_t,
                                    &mut hit,
                                    options.ignore_backfaces,
                                );

                                if let Some(hp_hit) = hit {
                                    let hp_norm = hp_hit.normal;
                                    let dot_t = hp_norm[0] * tangent[0]
                                        + hp_norm[1] * tangent[1]
                                        + hp_norm[2] * tangent[2];
                                    let dot_b = hp_norm[0] * bitangent[0]
                                        + hp_norm[1] * bitangent[1]
                                        + hp_norm[2] * bitangent[2];
                                    let dot_n = hp_norm[0] * n_low[0]
                                        + hp_norm[1] * n_low[1]
                                        + hp_norm[2] * n_low[2];
                                    let ts_y = if options.invert_normal_y {
                                        -dot_b
                                    } else {
                                        dot_b
                                    };
                                    final_norm = [
                                        (dot_t * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (ts_y * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (dot_n.max(0.0) * 0.5 + 0.5).clamp(0.0, 1.0),
                                    ];
                                    final_wsn = [
                                        (hp_norm[0] * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (hp_norm[1] * 0.5 + 0.5).clamp(0.0, 1.0),
                                        (hp_norm[2] * 0.5 + 0.5).clamp(0.0, 1.0),
                                    ];
                                    final_pos = [
                                        ((hp_hit.position[0] - min_pos[0]) / pos_range[0])
                                            .clamp(0.0, 1.0),
                                        ((hp_hit.position[1] - min_pos[1]) / pos_range[1])
                                            .clamp(0.0, 1.0),
                                        ((hp_hit.position[2] - min_pos[2]) / pos_range[2])
                                            .clamp(0.0, 1.0),
                                    ];

                                    // Thickness estimation from High-Poly via reverse raycast
                                    if options.bake_thickness {
                                        let rev_origin = [
                                            hp_hit.position[0] - hp_norm[0] * 0.002,
                                            hp_hit.position[1] - hp_norm[1] * 0.002,
                                            hp_hit.position[2] - hp_norm[2] * 0.002,
                                        ];
                                        let rev_dir = [-hp_norm[0], -hp_norm[1], -hp_norm[2]];
                                        let rev_inv = [
                                            if rev_dir[0].abs() > 1e-6 {
                                                1.0 / rev_dir[0]
                                            } else {
                                                1e6
                                            },
                                            if rev_dir[1].abs() > 1e-6 {
                                                1.0 / rev_dir[1]
                                            } else {
                                                1e6
                                            },
                                            if rev_dir[2].abs() > 1e-6 {
                                                1.0 / rev_dir[2]
                                            } else {
                                                1e6
                                            },
                                        ];
                                        let mut rev_closest = max_dist * 2.0;
                                        let mut rev_hit = None;
                                        bvh.intersect_ray(
                                            &rev_origin,
                                            &rev_dir,
                                            &rev_inv,
                                            &mut rev_closest,
                                            &mut rev_hit,
                                            false,
                                        );
                                        if let Some(rh) = rev_hit {
                                            final_thick =
                                                (rh.distance / (max_dist * 1.5)).clamp(0.05, 1.0);
                                        }
                                    }

                                    // High-Poly AO Hemisphere Sampling
                                    if options.bake_ao && options.ao_samples > 0 {
                                        let mut occluded = 0u32;
                                        let num_samples = options.ao_samples.min(32);
                                        for s in 0..num_samples {
                                            let xi1 =
                                                ((s as f32 + 0.5) / num_samples as f32).fract();
                                            let xi2 = ((s as f32 * 0.6180339887) + 0.1).fract();
                                            let phi = 2.0 * std::f32::consts::PI * xi1;
                                            let cos_theta = (1.0 - xi2).sqrt();
                                            let sin_theta = xi2.sqrt();
                                            let h_dir = [
                                                phi.cos() * sin_theta,
                                                phi.sin() * sin_theta,
                                                cos_theta,
                                            ];
                                            let world_h = [
                                                tangent[0] * h_dir[0]
                                                    + bitangent[0] * h_dir[1]
                                                    + hp_norm[0] * h_dir[2],
                                                tangent[1] * h_dir[0]
                                                    + bitangent[1] * h_dir[1]
                                                    + hp_norm[1] * h_dir[2],
                                                tangent[2] * h_dir[0]
                                                    + bitangent[2] * h_dir[1]
                                                    + hp_norm[2] * h_dir[2],
                                            ];
                                            let ao_origin = [
                                                hp_hit.position[0] + hp_norm[0] * 0.002,
                                                hp_hit.position[1] + hp_norm[1] * 0.002,
                                                hp_hit.position[2] + hp_norm[2] * 0.002,
                                            ];
                                            let ao_inv = [
                                                if world_h[0].abs() > 1e-6 {
                                                    1.0 / world_h[0]
                                                } else {
                                                    1e6
                                                },
                                                if world_h[1].abs() > 1e-6 {
                                                    1.0 / world_h[1]
                                                } else {
                                                    1e6
                                                },
                                                if world_h[2].abs() > 1e-6 {
                                                    1.0 / world_h[2]
                                                } else {
                                                    1e6
                                                },
                                            ];
                                            let mut ao_closest = options.ao_max_distance;
                                            let mut ao_hit = None;
                                            bvh.intersect_ray(
                                                &ao_origin,
                                                &world_h,
                                                &ao_inv,
                                                &mut ao_closest,
                                                &mut ao_hit,
                                                true,
                                            );
                                            if ao_hit.is_some() {
                                                occluded += 1;
                                            }
                                        }
                                        let occ_ratio = occluded as f32 / num_samples as f32;
                                        final_ao =
                                            (1.0 - occ_ratio * options.ao_spread).clamp(0.05, 1.0);
                                    }
                                }
                            }

                            // Accumulate subpixel sample
                            let acc = &mut accum_buf[idx];
                            acc.weight += sample_w;
                            acc.ao += final_ao * sample_w;
                            acc.curv += final_curv * sample_w;
                            acc.thick += final_thick * sample_w;
                            for c in 0..3 {
                                acc.norm[c] += final_norm[c] * sample_w;
                                acc.wsn[c] += final_wsn[c] * sample_w;
                                acc.pos[c] += final_pos[c] * sample_w;
                            }
                            for c in 0..4 {
                                acc.id[c] += final_id[c] * sample_w;
                            }
                        }
                    }
                }
            }
        }

        // 6. Normalize Multi-Sample Accumulations into Output Buffers
        for i in 0..total {
            let acc = &accum_buf[i];
            if acc.weight > 0.001 {
                let inv_w = 1.0 / acc.weight;
                ao_buf[i] = (acc.ao * inv_w).clamp(0.0, 1.0);
                curv_buf[i] = (acc.curv * inv_w).clamp(0.0, 1.0);
                thick_buf[i] = (acc.thick * inv_w).clamp(0.0, 1.0);

                let nx = acc.norm[0] * inv_w;
                let ny = acc.norm[1] * inv_w;
                let nz = acc.norm[2] * inv_w;
                norm_buf[i] = [nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0), nz.clamp(0.0, 1.0)];

                let wx = acc.wsn[0] * inv_w;
                let wy = acc.wsn[1] * inv_w;
                let wz = acc.wsn[2] * inv_w;
                wsn_buf[i] = [wx.clamp(0.0, 1.0), wy.clamp(0.0, 1.0), wz.clamp(0.0, 1.0)];

                pos_buf[i] = [
                    (acc.pos[0] * inv_w).clamp(0.0, 1.0),
                    (acc.pos[1] * inv_w).clamp(0.0, 1.0),
                    (acc.pos[2] * inv_w).clamp(0.0, 1.0),
                ];

                id_buf[i] = [
                    (acc.id[0] * inv_w).clamp(0.0, 1.0),
                    (acc.id[1] * inv_w).clamp(0.0, 1.0),
                    (acc.id[2] * inv_w).clamp(0.0, 1.0),
                    (acc.id[3] * inv_w).clamp(0.0, 1.0),
                ];
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
            is_cancelled: false,
        }
    }
}

pub fn strip_mesh_suffix(name: &str, suffix: &str) -> String {
    let trimmed = name.trim();
    let s = suffix.trim();
    if !s.is_empty() && trimmed.to_lowercase().ends_with(&s.to_lowercase()) {
        trimmed[..trimmed.len() - s.len()]
            .trim_end_matches('_')
            .to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::mesh::{Mesh, SubMesh};

    #[test]
    fn test_baker_options_defaults_and_suffixes() {
        let opts = BakerOptions::default();
        assert_eq!(opts.resolution, 1024);
        assert_eq!(opts.anti_aliasing, 1);
        assert_eq!(opts.low_poly_suffix, "_low");
        assert_eq!(opts.high_poly_suffix, "_high");
        assert!(!opts.match_by_name);

        assert_eq!(strip_mesh_suffix("Arm_Armor_low", "_low"), "Arm_Armor");
        assert_eq!(strip_mesh_suffix("Head_high", "_high"), "Head");
        assert_eq!(strip_mesh_suffix("SingleMesh", "_low"), "SingleMesh");
    }

    #[test]
    fn test_bvh_construction_and_raycast() {
        let sphere = Mesh::create_material_sphere(16, 16);
        let num_tris = sphere.indices.len() / 3;
        let mut tris = Vec::new();

        for t in 0..num_tris {
            let i0 = sphere.indices[t * 3] as usize;
            let i1 = sphere.indices[t * 3 + 1] as usize;
            let i2 = sphere.indices[t * 3 + 2] as usize;
            let v0 = sphere.vertices[i0];
            let v1 = sphere.vertices[i1];
            let v2 = sphere.vertices[i2];
            let n0 = sphere.normals[i0];
            let n1 = sphere.normals[i1];
            let n2 = sphere.normals[i2];
            let mut bb = BoundingBox::empty();
            bb.grow(v0);
            bb.grow(v1);
            bb.grow(v2);
            tris.push(BvhTriangle {
                v0,
                v1,
                v2,
                n0,
                n1,
                n2,
                bbox: bb,
                center: [
                    (v0[0] + v1[0] + v2[0]) / 3.0,
                    (v0[1] + v1[1] + v2[1]) / 3.0,
                    (v0[2] + v1[2] + v2[2]) / 3.0,
                ],
            });
        }

        let bvh = BvhNode::build(tris);
        let ro = [0.0, 0.0, 3.0];
        let rd = [0.0, 0.0, -1.0];
        let inv_d = [1e6, 1e6, -1.0];
        let mut closest_t = 10.0f32;
        let mut hit = None;

        bvh.intersect_ray(&ro, &rd, &inv_d, &mut closest_t, &mut hit, false);
        assert!(hit.is_some(), "Ray directed at sphere must intersect BVH");
        let h = hit.unwrap();
        assert!(
            (h.position[2] - 0.577).abs() < 0.1,
            "Front surface of normalized sphere should be near z=0.577"
        );
        assert!(
            (h.distance - 2.422).abs() < 0.1,
            "Hit distance should be ~2.42"
        );
        assert!(h.normal[2] > 0.9, "Hit normal should face +Z");
    }

    #[test]
    fn test_high_to_low_bake_with_match_by_name() {
        let mut low = Mesh::create_cube();
        low.submeshes = vec![SubMesh {
            name: "Cube_low".into(),
            index_start: 0,
            index_count: low.indices.len(),
        }];

        let mut high = Mesh::create_material_sphere(16, 16);
        high.submeshes = vec![SubMesh {
            name: "Cube_high".into(),
            index_start: 0,
            index_count: high.indices.len(),
        }];

        let mut opts = BakerOptions::default();
        opts.resolution = 64;
        opts.match_by_name = true;
        opts.anti_aliasing = 2; // 2x2 AA

        let baked = BakerEngine::bake_all(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            None,
            None,
            Some(&high),
        );

        assert!(!baked.is_cancelled);
        assert!(baked.normal.is_some());
        assert!(baked.ambient_occlusion.is_some());
        assert!(baked.curvature.is_some());
        assert!(baked.position.is_some());
        assert!(baked.thickness.is_some());
    }

    #[test]
    fn test_baker_cancellation() {
        let low = Mesh::create_cube();
        let cancel = Arc::new(AtomicBool::new(true)); // Pre-cancelled
        let opts = BakerOptions::default();

        let baked = BakerEngine::bake_all(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            None,
            Some(cancel),
            None,
        );

        assert!(
            baked.is_cancelled,
            "Pre-cancelled baker must set is_cancelled flag"
        );
    }

    #[test]
    fn test_skew_map_sample_decodes_offsets() {
        let skew = SkewMap {
            width: 2,
            height: 2,
            data: vec![[-1.0, 0.5], [1.0, -0.5], [0.25, -1.0], [0.0, 1.0]],
        };
        assert_eq!(
            skew.sample(0.0, 0.0),
            [-1.0, 0.5],
            "nearest sample at top-left"
        );
        assert_eq!(skew.sample(1.0, 0.0), [1.0, -0.5]);
        assert_eq!(skew.sample(0.74, 0.74), [0.0, 1.0]);
        // Out-of-range values are clamped to row/col bounds (nearest sampling)
        assert_eq!(skew.sample(-5.0, 9.0), [0.25, -1.0]);
    }

    #[test]
    fn test_skew_options_default_off() {
        let o = BakerOptions::default();
        assert!(!o.use_skew_correction);
        assert_eq!(o.skew_strength, 0.6);
        assert!(o.skew_map.is_none());
        assert!(!o.skew_invert);
    }
}
