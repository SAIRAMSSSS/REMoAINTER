//! Offline path-traced "Visualisation (Render)" mode.
//!
//! CPU (Rayon) progressive RGB path tracer that consumes the paint session's
//! composited layer stack (`LayerStack::composite_*`) plus the `Mesh`
//! triangles and produces a converged, tonemapped film saved as PNG.
//!
//! Honest scope (team decisions):
//! - RGB transport only (no spectral).
//! - Environment is analytic ("studio" hemispherical gradient + ground bounce +
//!   optional sun disc). No HDRI/skybox file parsing in this pass.
//! - No denoiser; convergence is by spp count or user Stop/Pause.
//! - Env-only lighting: the key/fill direct-light rig is out of scope.
//!
//! Conventions mirrored from the rest of the crate:
//! - Texel indexing is the painting-code convention:
//!   `px = round(u * (w - 1))`, `py = round(v * (h - 1))`, `idx = py * w + px`
//!   (see `painting/layer.rs::SampleSource::sample_uv`).
//! - Tangent frame uses `painting::tangent_basis(n)`; composited normals are
//!   tangent-space packed in [0,1] and decoded with `* 2 - 1` exactly like the
//!   GL viewport shader (`viewport_gl.rs`: `texture(...) * 2.0 - 1.0`).
//! - Albedo is degamma'd with `pow(2.2)`, exposure is `pow(2, stops)`, and the
//!   tonemapper is the same ACES filmic fit used in `viewport_gl.rs`, followed
//!   by the viewport's `pow(1 / 2.2)` display gamma. (Documented CPU
//!   convention: gamma matches the GL viewport, not the exact piecewise sRGB
//!   transfer.)
//! - Camera orbit params (yaw/pitch/dist) match `ui/app.rs`
//!   `render_3d_viewport` (45° FOV, camera looking at origin, +Y up).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::graphics::Mesh;

// ---------------------------------------------------------------------------
// Options & presets
// ---------------------------------------------------------------------------

/// reference-style quality presets. `Custom` keeps `RenderOptions::spp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Draft,
    Medium,
    High,
    Extra,
    Custom,
}

impl QualityPreset {
    pub const ALL: [QualityPreset; 5] = [
        QualityPreset::Draft,
        QualityPreset::Medium,
        QualityPreset::High,
        QualityPreset::Extra,
        QualityPreset::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            QualityPreset::Draft => "Draft (64 spp)",
            QualityPreset::Medium => "Medium (256 spp)",
            QualityPreset::High => "High (1024 spp)",
            QualityPreset::Extra => "Extra (2560 spp)",
            QualityPreset::Custom => "Custom",
        }
    }

    pub fn spp(self, custom: u32) -> u32 {
        match self {
            QualityPreset::Draft => 64,
            QualityPreset::Medium => 256,
            QualityPreset::High => 1024,
            QualityPreset::Extra => 2560,
            QualityPreset::Custom => custom,
        }
    }
}

/// Orbit camera matching `ui/app.rs::render_3d_viewport` (captured at
/// "Start Render" so the traced film frames the same view the artist sees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub dist: f32,
    pub fov_deg: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw_deg: 35.0,
            pitch_deg: 18.0,
            dist: 2.2,
            fov_deg: 45.0, // render_3d_viewport uses perspective_rh(45°, ...)
        }
    }
}

/// Analytic environment light. `Studio` is the production look (deterministic,
/// seeded RNG stays valid); `Uniform` is a flat radiance field used by exact
/// analytic unit tests (zero-variance estimates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvKind {
    Studio {
        /// Radiant intensity of the optional sun disc (0 disables it).
        sun_intensity: f32,
        /// Azimuth of the sun direction (degrees, rotated around +Y).
        sun_angle_deg: f32,
    },
    /// Constant radiance `[v; 3]` in every direction.
    /// (Zero-variance helper for exact analytic unit tests.)
    #[allow(dead_code)]
    Uniform(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvConfig {
    pub kind: EnvKind,
    /// Rotation of the whole environment around +Y (degrees), like
    /// `env_rotation` in the interactive viewport.
    pub rotation_deg: f32,
    /// Multiplier applied to all incoming radiance.
    pub intensity: f32,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            kind: EnvKind::Studio {
                sun_intensity: 8.0,
                sun_angle_deg: 40.0,
            },
            rotation_deg: 0.0,
            intensity: 1.0,
        }
    }
}

/// All knobs captured when the user presses "Start Render".
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    pub spp: u32,
    pub width: u32,
    pub height: u32,
    /// Exposure in stops (radiance *= 2^stops), mirroring `u_exposure`.
    pub exposure_stops: f32,
    pub env: EnvConfig,
    pub camera: OrbitCamera,
    /// When rendering completes with a path set here, the tonemapped film is
    /// auto-saved as PNG.
    pub output_path: Option<std::path::PathBuf>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            spp: 256,
            width: 1024,
            height: 1024,
            exposure_stops: 0.0,
            env: EnvConfig::default(),
            camera: OrbitCamera::default(),
            output_path: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

/// One triangle of the traced scene (world space, single precision).
#[derive(Debug, Clone, Copy)]
pub struct RenderTri {
    pub p: [[f32; 3]; 3],
    pub n: [[f32; 3]; 3],
    pub uv: [[f32; 2]; 3],
}

/// Composited PBR maps sampled per-hit at the interpolated UV (nearest texel,
/// painting-code indexing convention).
#[derive(Debug, Clone)]
pub struct MaterialMaps {
    pub color: Vec<[f32; 4]>,
    pub roughness: Vec<f32>,
    pub metallic: Vec<f32>,
    /// Tangent-space normal packed in [0, 1] (composited stack convention).
    pub normal: Vec<[f32; 3]>,
    pub w: u32,
    pub h: u32,
}

impl MaterialMaps {
    /// Nearest sample with clamped coordinates — same texel indexing as
    /// `painting/layer.rs::SampleSource::sample_uv`.
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> ([f32; 4], f32, f32, [f32; 3]) {
        let px = (u.clamp(0.0, 1.0) * (self.w as f32 - 1.0)).round().max(0.0) as usize;
        let py = (v.clamp(0.0, 1.0) * (self.h as f32 - 1.0)).round().max(0.0) as usize;
        let idx = (py * self.w as usize + px).min(self.color.len().saturating_sub(1));
        (
            self.color[idx],
            self.roughness[idx.min(self.roughness.len().saturating_sub(1))],
            self.metallic[idx.min(self.metallic.len().saturating_sub(1))],
            self.normal[idx.min(self.normal.len().saturating_sub(1))],
        )
    }
}

/// Flat analytic material used when no composited maps are attached (unit
/// tests, minimal scenes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatMaterial {
    /// Linear-sRGB base color (already degamma'd — tests use exact values).
    pub albedo_linear: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
}

impl Default for FlatMaterial {
    fn default() -> Self {
        Self {
            albedo_linear: [0.5, 0.5, 0.5],
            roughness: 1.0,
            metallic: 0.0,
        }
    }
}

/// Simple top-down BVH over triangle AABBs (leaf size 8) so real meshes
/// (8k+ tris) remain interactive.
#[derive(Debug, Clone)]
struct BvhNode {
    bmin: [f32; 3],
    bmax: [f32; 3],
    left: i32,
    right: i32,
    leaf_tris: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RenderScene {
    pub tris: Vec<RenderTri>,
    pub maps: Option<MaterialMaps>,
    pub flat: FlatMaterial,
    bvh: Vec<BvhNode>,
    bvh_root: i32,
}

const BVH_LEAF: usize = 8;

impl RenderScene {
    /// Analytic scene for unit tests / minimal scenes.
    #[allow(dead_code)] // exercised by the renderer unit tests (lib target)
    pub fn from_tris(tris: Vec<RenderTri>, flat: FlatMaterial) -> Self {
        let mut s = Self {
            tris,
            maps: None,
            flat,
            ..Default::default()
        };
        s.build_bvh();
        s
    }

    /// Build a scene from the paint session: mesh triangles + composited maps.
    pub fn from_pbr(
        mesh: &Mesh,
        color: &[[f32; 4]],
        roughness: &[f32],
        metallic: &[f32],
        normal: &[[f32; 3]],
        w: u32,
        h: u32,
    ) -> Self {
        let mut tris = Vec::with_capacity(mesh.indices.len() / 3);
        for t in 0..mesh.indices.len() / 3 {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
                continue;
            }
            if i0 >= mesh.uvs.len() || i1 >= mesh.uvs.len() || i2 >= mesh.uvs.len() {
                continue;
            }
            let gn = |i: usize| mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            tris.push(RenderTri {
                p: [mesh.vertices[i0], mesh.vertices[i1], mesh.vertices[i2]],
                n: [gn(i0), gn(i1), gn(i2)],
                uv: [mesh.uvs[i0], mesh.uvs[i1], mesh.uvs[i2]],
            });
        }
        let mut s = Self {
            tris,
            maps: Some(MaterialMaps {
                color: color.to_vec(),
                roughness: roughness.to_vec(),
                metallic: metallic.to_vec(),
                normal: normal.to_vec(),
                w,
                h,
            }),
            flat: FlatMaterial::default(),
            ..Default::default()
        };
        s.build_bvh();
        s
    }

    /// Bounds + dominant axis of a triangle-index set used as the BVH split
    /// metric (deterministic).
    fn indices_bounds(tris: &[RenderTri], order: &[u32]) -> ([f32; 3], [f32; 3], usize, f32) {
        let mut bmin = [f32::MAX; 3];
        let mut bmax = [f32::MIN; 3];
        for &i in order {
            let t = &tris[i as usize];
            for p in t.p {
                for a in 0..3 {
                    bmin[a] = bmin[a].min(p[a]);
                    bmax[a] = bmax[a].max(p[a]);
                }
            }
        }
        let mut extent = [0.0f32; 3];
        for a in 0..3 {
            extent[a] = bmax[a] - bmin[a];
        }
        let axis = (0..3)
            .max_by(|a, b| extent[*a].partial_cmp(&extent[*b]).unwrap())
            .unwrap_or(0);
        (bmin, bmax, axis, extent[axis])
    }

    fn build_bvh(&mut self) {
        fn build(tris: &[RenderTri], order: &mut [u32], nodes: &mut Vec<BvhNode>) -> i32 {
            let (bmin, bmax, axis, extent) = RenderScene::indices_bounds(tris, order);
            let node_idx = nodes.len() as i32;
            nodes.push(BvhNode {
                bmin,
                bmax,
                left: -1,
                right: -1,
                leaf_tris: Vec::new(),
            });
            if order.len() <= BVH_LEAF {
                nodes[node_idx as usize].leaf_tris = order.to_vec();
                return node_idx;
            }
            // Midpoint partition (deterministic: stable sort + partition_point).
            let mid = bmin[axis] + extent * 0.5;
            order.sort_by(|&x, &y| {
                let cx = (tris[x as usize].p[0][axis]
                    + tris[x as usize].p[1][axis]
                    + tris[x as usize].p[2][axis])
                    * 0.333_333_34;
                let cy = (tris[y as usize].p[0][axis]
                    + tris[y as usize].p[1][axis]
                    + tris[y as usize].p[2][axis])
                    * 0.333_333_34;
                cx.partial_cmp(&cy).unwrap_or(std::cmp::Ordering::Equal)
            });
            let split = order
                .partition_point(|&i| {
                    (tris[i as usize].p[0][axis]
                        + tris[i as usize].p[1][axis]
                        + tris[i as usize].p[2][axis])
                        * 0.333_333_34
                        < mid
                })
                .clamp(1, order.len() - 1);
            let (lo, hi) = order.split_at_mut(split);
            let li = build(tris, lo, nodes);
            let ri = build(tris, hi, nodes);
            nodes[node_idx as usize].left = li;
            nodes[node_idx as usize].right = ri;
            node_idx
        }
        self.bvh.clear();
        self.bvh_root = -1;
        if self.tris.is_empty() {
            return;
        }
        let mut order: Vec<u32> = (0..self.tris.len() as u32).collect();
        let mut nodes: Vec<BvhNode> = Vec::new();
        let tris = std::mem::take(&mut self.tris);
        self.bvh_root = build(&tris, &mut order, &mut nodes);
        self.tris = tris;
        self.bvh = nodes;
    }

    /// Nearest hit: returns `(t, u, v, tri_index)`.
    #[inline]
    fn raycast(&self, org: [f32; 3], dir: [f32; 3]) -> Option<(f32, f32, f32, u32)> {
        if self.bvh_root < 0 {
            return None;
        }
        let inv_dir = [
            1.0 / if_abs_small(dir[0], 1e-12),
            1.0 / if_abs_small(dir[1], 1e-12),
            1.0 / if_abs_small(dir[2], 1e-12),
        ];
        let mut t_best = f32::INFINITY;
        let mut out: Option<(f32, f32, f32, u32)> = None;
        let mut stack = vec![self.bvh_root as usize];
        while let Some(idx) = stack.pop() {
            let n = &self.bvh[idx];
            if !slab_hit(n.bmin, n.bmax, org, inv_dir, t_best) {
                continue;
            }
            if n.leaf_tris.is_empty() {
                if n.left >= 0 {
                    stack.push(n.left as usize);
                }
                if n.right >= 0 {
                    stack.push(n.right as usize);
                }
            } else {
                for &i in &n.leaf_tris {
                    if let Some(hit) = ray_tri(org, dir, &self.tris[i as usize], t_best) {
                        t_best = hit.0;
                        out = Some((hit.0, hit.1, hit.2, i));
                    }
                }
            }
        }
        // Ray-BVH traversal (the loop borrows `dir` via ray_tri below).
        out
    }
}

#[inline]
fn if_abs_small(v: f32, eps: f32) -> f32 {
    if v.abs() < eps {
        if v < 0.0 {
            -eps
        } else {
            eps
        }
    } else {
        v
    }
}

#[inline]
fn slab_hit(bmin: [f32; 3], bmax: [f32; 3], org: [f32; 3], inv_dir: [f32; 3], t_max: f32) -> bool {
    let mut tmin = 0.0f32;
    let mut tmax = t_max;
    for a in 0..3 {
        let mut t0 = (bmin[a] - org[a]) * inv_dir[a];
        let mut t1 = (bmax[a] - org[a]) * inv_dir[a];
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        tmin = tmin.max(t0);
        tmax = tmax.min(t1);
        if tmin > tmax {
            return false;
        }
    }
    true
}

/// Möller–Trumbore. Returns `(t, u, v)` with `t < t_max`, both sides hit.
#[inline]
fn ray_tri(org: [f32; 3], dir: [f32; 3], tri: &RenderTri, t_max: f32) -> Option<(f32, f32, f32)> {
    let e1 = [
        tri.p[1][0] - tri.p[0][0],
        tri.p[1][1] - tri.p[0][1],
        tri.p[1][2] - tri.p[0][2],
    ];
    let e2 = [
        tri.p[2][0] - tri.p[0][0],
        tri.p[2][1] - tri.p[0][1],
        tri.p[2][2] - tri.p[0][2],
    ];
    let pv = cross3(dir, e2);
    let det = dot3(e1, pv);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    let tv = [
        org[0] - tri.p[0][0],
        org[1] - tri.p[0][1],
        org[2] - tri.p[0][2],
    ];
    let u = dot3(tv, pv) * inv_det;
    if u < 0.0 || u > 1.0 {
        return None;
    }
    let qv = cross3(tv, e1);
    let v = dot3(dir, qv) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot3(e2, qv) * inv_det;
    if t > 1e-4 && t < t_max {
        Some((t, u, v))
    } else {
        None
    }
}

#[inline]
fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn norm3(a: [f32; 3]) -> [f32; 3] {
    let l = dot3(a, a).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

#[inline]
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

// ---------------------------------------------------------------------------
// Analytic environment
// ---------------------------------------------------------------------------

impl EnvConfig {
    /// Environment radiance for one unit direction.
    pub fn sample(&self, dir: [f32; 3]) -> [f32; 3] {
        // Rotate the direction by env_rotation around +Y (same spirit as the
        // interactive viewport's env rotation).
        let rot = -self.rotation_deg.to_radians();
        let (s, c) = rot.sin_cos();
        let d = [c * dir[0] + s * dir[2], dir[1], -s * dir[0] + c * dir[2]];
        let base = match self.kind {
            EnvKind::Uniform(v) => [v, v, v],
            EnvKind::Studio {
                sun_intensity,
                sun_angle_deg,
            } => {
                let up = d[1].clamp(-1.0, 1.0);
                // Monotonic hemispherical gradient: dim horizon -> bright zenith.
                let zen = [0.95, 0.97, 1.0];
                let horizon = [0.30, 0.32, 0.36];
                let mut sky = if up >= 0.0 {
                    // Steep monotonic gradient keeps mirror-reflection tests
                    // sharply directional (zenith 1.0, dim horizon).
                    lerp3(horizon, zen, up.powi(3))
                } else {
                    // Warm ground-bounce term (dim, slightly brighter downward).
                    let g = (1.0 + 0.25 * (-up)).min(1.25);
                    [0.11 * g, 0.095 * g, 0.08 * g]
                };
                if sun_intensity > 0.0 {
                    let ang = sun_angle_deg.to_radians() + self.rotation_deg.to_radians();
                    let sun_dir = norm3([0.35 * ang.cos(), 0.72, 0.35 * ang.sin()]);
                    // ~2° sun disc, analytic hard cutoff (deterministic).
                    if dot3(d, sun_dir) > 2.0_f32.to_radians().cos() {
                        sky = lerp3(sky, [1.0, 0.96, 0.9], 1.0);
                        sky = [
                            sky[0] + sun_intensity,
                            sky[1] + sun_intensity * 0.96,
                            sky[2] + sun_intensity * 0.9,
                        ];
                    }
                }
                sky
            }
        };
        [
            base[0] * self.intensity,
            base[1] * self.intensity,
            base[2] * self.intensity,
        ]
    }
}

// ---------------------------------------------------------------------------
// Deterministic RNG
// ---------------------------------------------------------------------------

/// SplitMix64 — tiny, fast, dependency-free; fully deterministic.
pub struct Random {
    state: u64,
}

impl Random {
    #[inline]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Stateless seed per (pixel, pass) so film convergence is bit-stable for a
/// given (width, height, spp) regardless of tile scheduling.
#[inline]
fn pixel_seed(x: usize, y: usize, pass: usize) -> u64 {
    let mut seed = (x as u64) ^ (y as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    seed = (seed ^ (pass as u64).wrapping_mul(0xD1B5_4A32_D192_ED03))
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    seed ^= seed >> 31;
    seed
}

// ---------------------------------------------------------------------------
// Film & progress
// ---------------------------------------------------------------------------

/// Shared f32 accumulation buffer (RGB, radiance linear units).
#[derive(Debug)]
pub struct RenderFilm {
    pub width: usize,
    pub height: usize,
    state: Mutex<FilmState>,
}

#[derive(Debug)]
struct FilmState {
    accum: Vec<[f32; 3]>,
    passes: usize,
}

impl RenderFilm {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            state: Mutex::new(FilmState {
                accum: vec![[0.0; 3]; width * height],
                passes: 0,
            }),
        }
    }

    fn add_pass(&self, contrib: Vec<[f32; 3]>) {
        let mut st = self.state.lock().unwrap();
        for (acc, c) in st.accum.iter_mut().zip(contrib) {
            acc[0] += c[0];
            acc[1] += c[1];
            acc[2] += c[2];
        }
        st.passes += 1;
    }

    pub fn passes(&self) -> usize {
        self.state.lock().unwrap().passes
    }

    /// Raw (pre-tonemap) radiance accumulation + completed pass count.
    #[allow(dead_code)] // exercised by the renderer unit tests (lib target)
    pub fn snapshot_raw(&self) -> (Vec<[f32; 3]>, usize) {
        let st = self.state.lock().unwrap();
        (st.accum.clone(), st.passes)
    }

    /// Tonemapped display snapshot (RGB8 bytes, row-major, gamma per the GL
    /// viewport convention) with the pass count used for the average.
    pub fn snapshot_rgba(&self, exposure_stops: f32) -> RgbaSnapshot {
        let st = self.state.lock().unwrap();
        let passes = st.passes.max(1);
        let exposure = 2.0f32.powf(exposure_stops);
        let mut rgb = Vec::with_capacity(st.accum.len() * 3);
        for acc in &st.accum {
            let avg = [
                acc[0] / passes as f32,
                acc[1] / passes as f32,
                acc[2] / passes as f32,
            ];
            let t = aces_tonemap_processed(avg, exposure);
            rgb.push((t[0].clamp(0.0, 1.0) * 255.0) as u8);
            rgb.push((t[1].clamp(0.0, 1.0) * 255.0) as u8);
            rgb.push((t[2].clamp(0.0, 1.0) * 255.0) as u8);
        }
        RgbaSnapshot {
            rgb,
            width: self.width,
            height: self.height,
            passes,
        }
    }
}

/// RGB8 snapshot of the tonemapped film (plus the pass count it averaged).
#[derive(Debug, Clone)]
pub struct RgbaSnapshot {
    pub rgb: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub passes: usize,
}

/// Shared pause/cancel/completion signals (same pattern as the baker engine).
#[derive(Debug, Default)]
pub struct RenderProgress {
    pub passes: AtomicUsize,
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
    pub finished: AtomicBool,
    pub saved: AtomicBool,
}

/// Outcome of a full (possibly interrupted) render session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    Completed,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Renderer core
// ---------------------------------------------------------------------------

const MAX_BOUNCES: usize = 4;

/// Progressive render loop. Runs synchronously; returns the accumulated film.
pub fn render_film_into(
    film: &RenderFilm,
    scene: &RenderScene,
    options: &RenderOptions,
    progress: &RenderProgress,
) -> RenderOutcome {
    let target = options.spp.max(1) as usize;
    let mut outcome = RenderOutcome::Cancelled;
    for pass in 0..target {
        if progress.cancel.load(Ordering::Relaxed) {
            break;
        }
        while progress.pause.load(Ordering::Relaxed) && !progress.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        if progress.cancel.load(Ordering::Relaxed) {
            break;
        }
        let contrib = render_pass(scene, options, pass);
        film.add_pass(contrib);
        progress.passes.fetch_add(1, Ordering::Relaxed);
        outcome = RenderOutcome::Completed;
    }
    if outcome == RenderOutcome::Completed
        && film.passes() >= target
        && !progress.cancel.load(Ordering::Relaxed)
    {
        if let Some(path) = &options.output_path {
            match save_png(film, options.exposure_stops, path) {
                Ok(()) => progress.saved.store(true, Ordering::Relaxed),
                Err(err) => eprintln!("Render auto-save failed: {err}"),
            }
        }
    }
    progress.finished.store(true, Ordering::Relaxed);
    outcome
}

/// One pass ( spp = 1 ) over the whole frame: Rayon over scanlines.
pub fn render_pass(scene: &RenderScene, options: &RenderOptions, pass: usize) -> Vec<[f32; 3]> {
    use rayon::prelude::*;
    let w = options.width as usize;
    let h = options.height as usize;
    (0..h)
        .into_par_iter()
        .with_min_len(8)
        .flat_map_iter(|y| (0..w).map(move |x| trace_pixel(scene, options, x, y, pass)))
        .collect()
}

/// Camera ray generation using the exact same math as
/// `ui/app.rs::render_3d_viewport`: `perspective_rh(fov, aspect)` * `look_at_rh`
/// orbit position around the origin.
pub fn camera_ray(
    camera: &OrbitCamera,
    w: u32,
    h: u32,
    x: usize,
    y: usize,
) -> ([f32; 3], [f32; 3]) {
    let yaw = camera.yaw_deg.to_radians();
    let pitch = camera.pitch_deg.to_radians();
    let cam_pos = glam::Vec3::new(
        camera.dist * pitch.cos() * yaw.sin(),
        camera.dist * pitch.sin(),
        camera.dist * pitch.cos() * yaw.cos(),
    );
    let aspect = w as f32 / h.max(1) as f32;
    let proj = glam::Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, 0.01, 200.0);
    let view = glam::Mat4::look_at_rh(cam_pos, glam::Vec3::ZERO, glam::Vec3::Y);
    let inv = (proj * view).inverse();
    let ndc_x = ((x as f32 + 0.5) / w as f32) * 2.0 - 1.0;
    let ndc_y = 1.0 - ((y as f32 + 0.5) / h as f32) * 2.0;
    let target = inv.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
    let dir = glam::Vec3::normalize(target - cam_pos);
    (cam_pos.to_array(), dir.to_array())
}

/// Material at a hit: interpolated normal + UV-sampled composited maps (or the
/// analytic flat material when maps are absent).
#[inline]
fn material_at(
    scene: &RenderScene,
    tri_idx: u32,
    u: f32,
    v: f32,
    geo_n: [f32; 3],
) -> ([f32; 3], f32, f32, [f32; 3]) {
    let w = 1.0 - u - v;
    match &scene.maps {
        Some(maps) => {
            let tri = &scene.tris[tri_idx as usize];
            let uv = [
                tri.uv[0][0] * w + tri.uv[1][0] * u + tri.uv[2][0] * v,
                tri.uv[0][1] * w + tri.uv[1][1] * u + tri.uv[2][1] * v,
            ];
            let (albedo_srgb, rough, metal, n_packed) = maps.sample(uv[0], uv[1]);
            // Decode tangent-space normal exactly like viewport_gl.rs:
            // `texture(...).rgb * 2.0 - 1.0`, then TBN with painting's
            // tangent_basis so shading constants match the GL preview.
            let n_ts = [
                n_packed[0] * 2.0 - 1.0,
                n_packed[1] * 2.0 - 1.0,
                n_packed[2] * 2.0 - 1.0,
            ];
            let n_ts_len = dot3(n_ts, n_ts).sqrt();
            let n = if n_ts_len > 1e-4 {
                let (t, b) = crate::painting::tangent_basis(geo_n);
                let w = norm3([
                    t[0] * n_ts[0] + b[0] * n_ts[1] + geo_n[0] * n_ts[2],
                    t[1] * n_ts[0] + b[1] * n_ts[1] + geo_n[1] * n_ts[2],
                    t[2] * n_ts[0] + b[2] * n_ts[1] + geo_n[2] * n_ts[2],
                ]);
                // Flip toward the geometric side to avoid shading back-faces.
                if dot3(w, geo_n) < 0.0 {
                    [-w[0], -w[1], -w[2]]
                } else {
                    w
                }
            } else {
                geo_n
            };
            // Degamma albedo exactly like viewport_gl.rs line 336.
            let albedo = [
                albedo_srgb[0].powf(2.2),
                albedo_srgb[1].powf(2.2),
                albedo_srgb[2].powf(2.2),
            ];
            (albedo, rough, metal, n)
        }
        None => {
            let f = &scene.flat;
            (f.albedo_linear, f.roughness, f.metallic, geo_n)
        }
    }
}

/// Trace one pixel sample. Env-only lighting, up to `MAX_BOUNCES` surface
/// bounces, exact analytic env on escape.
fn trace_pixel(
    scene: &RenderScene,
    options: &RenderOptions,
    x: usize,
    y: usize,
    pass: usize,
) -> [f32; 3] {
    let mut rng = Random::new(pixel_seed(x, y, pass));
    let (org0, dir0) = camera_ray(&options.camera, options.width, options.height, x, y);
    let mut radiance = [0.0f32; 3];
    let mut throughput = [1.0f32; 3];
    let mut org = org0;
    let mut dir = dir0;

    for _ in 0..MAX_BOUNCES {
        let Some((t, b_u, b_v, tri_idx)) = scene.raycast(org, dir) else {
            radiance[0] += throughput[0] * options.env.sample(dir)[0];
            radiance[1] += throughput[1] * options.env.sample(dir)[1];
            radiance[2] += throughput[2] * options.env.sample(dir)[2];
            return radiance;
        };
        let tri = &scene.tris[tri_idx as usize];
        let hit_p = [
            org[0] + dir[0] * t,
            org[1] + dir[1] * t,
            org[2] + dir[2] * t,
        ];
        let w = (1.0 - b_u - b_v).max(0.0);
        let geo_n = norm3([
            tri.n[0][0] * w + tri.n[1][0] * b_u + tri.n[2][0] * b_v,
            tri.n[0][1] * w + tri.n[1][1] * b_u + tri.n[2][1] * b_v,
            tri.n[0][2] * w + tri.n[1][2] * b_u + tri.n[2][2] * b_v,
        ]);
        // Face the geometric normal against the incoming ray direction.
        let geo_n = if dot3(geo_n, dir) > 0.0 {
            [-geo_n[0], -geo_n[1], -geo_n[2]]
        } else {
            geo_n
        };
        let (albedo, rough, metal, n) = material_at(scene, tri_idx, b_u, b_v, geo_n);

        // BSDF lobe classification:
        // - metallic=1 & roughness~0 -> perfect mirror (exact).
        // - metallic=0 & roughness=1 -> pure Lambert cosine (exact estimator).
        // - otherwise -> GGX specular + Lambert diffuse mixture sampling.
        if metal >= 0.999 && rough <= 0.015 {
            let r = reflect(dir, n);
            throughput = [
                throughput[0] * albedo[0],
                throughput[1] * albedo[1],
                throughput[2] * albedo[2],
            ];
            org = [
                hit_p[0] + n[0] * 1e-3,
                hit_p[1] + n[1] * 1e-3,
                hit_p[2] + n[2] * 1e-3,
            ];
            dir = r;
            continue;
        }
        if metal <= 0.001 && rough >= 0.995 {
            // Cosine-weighted Lambert: radiance estimator = albedo * L(d).
            let d = sample_cosine(n, &mut rng);
            let l = options.env.sample(d);
            radiance[0] += throughput[0] * albedo[0] * l[0];
            radiance[1] += throughput[1] * albedo[1] * l[1];
            radiance[2] += throughput[2] * albedo[2] * l[2];
            return radiance;
        }

        // Mixture importance sampling.
        let a = (rough * rough).max(1e-4);
        let a2 = a * a;
        let v = [-dir[0], -dir[1], -dir[2]];
        let n_dot_v = dot3(n, v).clamp(1e-4, 1.0);
        let f0 = [
            lerp(0.04, albedo[0], metal),
            lerp(0.04, albedo[1], metal),
            lerp(0.04, albedo[2], metal),
        ];
        // Specular sampling probability (F0-energy heuristic; both lobes are
        // always evaluated so the estimator stays unbiased).
        let p_spec = (0.04 + 0.96 * metal).clamp(0.08, 0.95);

        let (dir_out, f_term, pdf_sample) = if rng.next_f32() < p_spec {
            // Sample the GGX NDF for the half vector.
            let h = sample_ggx_half(n, a, &mut rng);
            let v_dot_h = dot3(v, h).clamp(1e-4, 1.0);
            let r = reflect_around(v, h); // mirror `v` about the half vector
            let n_dot_l = dot3(n, r).clamp(0.0, 1.0);
            if n_dot_l <= 0.0 {
                return radiance; // sampled below the horizon; zero throughput
            }
            let d_term = ggx_d(n_dot_h(n, h), a2);
            let vis = smith_vis(n_dot_v, n_dot_l, a2);
            let f = fresnel(v_dot_h, f0);
            let spec = [
                f[0] * d_term * vis,
                f[1] * d_term * vis,
                f[2] * d_term * vis,
            ];
            let pi = std::f32::consts::FRAC_1_PI;
            let diff = [
                albedo[0] * (1.0 - metal) * pi,
                albedo[1] * (1.0 - metal) * pi,
                albedo[2] * (1.0 - metal) * pi,
            ];
            let pdf_spec = d_term * dot3(n, h) / (4.0 * v_dot_h);
            (
                r,
                [spec[0] + diff[0], spec[1] + diff[1], spec[2] + diff[2]],
                p_spec * pdf_spec,
            )
        } else {
            let d = sample_cosine(n, &mut rng);
            let n_dot_l = dot3(n, d).clamp(0.0, 1.0);
            let h = norm3([v[0] + d[0], v[1] + d[1], v[2] + d[2]]);
            let v_dot_h = dot3(v, h).clamp(1e-4, 1.0);
            let d_term = ggx_d(n_dot_h(n, h), a2);
            let vis = smith_vis(n_dot_v, n_dot_l, a2);
            let f = fresnel(v_dot_h, f0);
            let spec = [
                f[0] * d_term * vis,
                f[1] * d_term * vis,
                f[2] * d_term * vis,
            ];
            let pi = std::f32::consts::FRAC_1_PI;
            let diff = [
                albedo[0] * (1.0 - metal) * pi,
                albedo[1] * (1.0 - metal) * pi,
                albedo[2] * (1.0 - metal) * pi,
            ];
            let pdf_spec = d_term * dot3(n, h) / (4.0 * v_dot_h);
            (
                d,
                [spec[0] + diff[0], spec[1] + diff[1], spec[2] + diff[2]],
                p_spec * pdf_spec + (1.0 - p_spec) * n_dot_l * pi,
            )
        };
        let pdf_total = pdf_sample.max(1e-9);
        let n_dot_l = dot3(n, dir_out).clamp(0.0, 1.0);
        throughput = [
            throughput[0] * f_term[0] * n_dot_l / pdf_total,
            throughput[1] * f_term[1] * n_dot_l / pdf_total,
            throughput[2] * f_term[2] * n_dot_l / pdf_total,
        ];
        org = [
            hit_p[0] + n[0] * 1e-3,
            hit_p[1] + n[1] * 1e-3,
            hit_p[2] + n[2] * 1e-3,
        ];
        dir = dir_out;
    }
    radiance
}

// --- BRDF helpers ---------------------------------------------------------

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn n_dot_h(n: [f32; 3], h: [f32; 3]) -> f32 {
    dot3(n, h).clamp(0.0, 1.0)
}

/// GGX normal distribution.
#[inline]
fn ggx_d(n_dot_h: f32, a2: f32) -> f32 {
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (std::f32::consts::PI * d * d)
}

/// Smith GGX visibility: `G1(v) * G1(l) / (4 * NdotV * NdotL)`.
#[inline]
fn smith_vis(n_dot_v: f32, n_dot_l: f32, a2: f32) -> f32 {
    let g1 = |x: f32| 2.0 * x / (x + (a2 + (1.0 - a2) * x * x).sqrt());
    g1(n_dot_v) * g1(n_dot_l) / (4.0 * n_dot_v * n_dot_l)
}

/// Fresnel-Schlick with per-channel F0.
#[inline]
fn fresnel(cos_theta: f32, f0: [f32; 3]) -> [f32; 3] {
    let m = (1.0 - cos_theta).clamp(0.0, 1.0);
    let p = m * m * m * m * m;
    [
        f0[0] + (1.0 - f0[0]) * p,
        f0[1] + (1.0 - f0[1]) * p,
        f0[2] + (1.0 - f0[2]) * p,
    ]
}

#[inline]
fn reflect(dir: [f32; 3], n: [f32; 3]) -> [f32; 3] {
    let d = dot3(dir, n);
    [
        dir[0] - 2.0 * d * n[0],
        dir[1] - 2.0 * d * n[1],
        dir[2] - 2.0 * d * n[2],
    ]
}

/// Mirror one direction about another (`v` about `h`) — used to build the
/// outgoing ray from the sampled half vector.
#[inline]
fn reflect_around(v: [f32; 3], h: [f32; 3]) -> [f32; 3] {
    reflect([-v[0], -v[1], -v[2]], h)
}

fn sample_cosine(n: [f32; 3], rng: &mut Random) -> [f32; 3] {
    let r1 = rng.next_f32();
    let r2 = rng.next_f32();
    let r = r1.sqrt();
    let phi = std::f32::consts::TAU * r2;
    let (t, b) = crate::painting::tangent_basis(n);
    [
        t[0] * r * phi.cos() + b[0] * r * phi.sin() + n[0] * (1.0 - r1).max(0.0).sqrt(),
        t[1] * r * phi.cos() + b[1] * r * phi.sin() + n[1] * (1.0 - r1).max(0.0).sqrt(),
        t[2] * r * phi.cos() + b[2] * r * phi.sin() + n[2] * (1.0 - r1).max(0.0).sqrt(),
    ]
}

/// Sample a GGX half-vector by NDF importance sampling:
/// `cos(h) = sqrt((1-u) / (u * (a^2 - 1) + 1))`, azimuth uniform.
fn sample_ggx_half(n: [f32; 3], a: f32, rng: &mut Random) -> [f32; 3] {
    let u = rng.next_f32().clamp(1e-6, 1.0);
    let cos_h = ((1.0 - u) / (u * (a * a - 1.0) + 1.0))
        .sqrt()
        .clamp(0.0, 1.0);
    let sin_h = (1.0 - cos_h * cos_h).sqrt();
    let phi = std::f32::consts::TAU * rng.next_f32();
    let (t, b) = crate::painting::tangent_basis(n);
    let lx = sin_h * phi.cos();
    let ly = sin_h * phi.sin();
    [
        t[0] * lx + b[0] * ly + n[0] * cos_h,
        t[1] * lx + b[1] * ly + n[1] * cos_h,
        t[2] * lx + b[2] * ly + n[2] * cos_h,
    ]
}

// ---------------------------------------------------------------------------
// Tone mapping & output
// ---------------------------------------------------------------------------

/// ACES filmic fit — identical formula to `viewport_gl.rs::aces_film`
/// (exposure is applied by the caller before this, matching the GL pipeline).
#[inline]
pub fn aces_tonemap(x: [f32; 3]) -> [f32; 3] {
    const A: f32 = 2.51;
    const B: f32 = 0.03;
    const C: f32 = 2.43;
    const D: f32 = 0.59;
    const E: f32 = 0.14;
    let f = |c: f32| (c * (A * c + B)) / (c * (C * c + D) + E);
    [
        f(x[0]).clamp(0.0, 1.0),
        f(x[1]).clamp(0.0, 1.0),
        f(x[2]).clamp(0.0, 1.0),
    ]
}

/// Exposure (stops) + ACES + GL-viewport display gamma (pow 1/2.2).
#[inline]
pub fn aces_tonemap_processed(radiance: [f32; 3], exposure_stops: f32) -> [f32; 3] {
    let e = 2.0f32.powf(exposure_stops);
    let x = [radiance[0] * e, radiance[1] * e, radiance[2] * e];
    let t = aces_tonemap(x);
    [
        t[0].powf(1.0 / 2.2),
        t[1].powf(1.0 / 2.2),
        t[2].powf(1.0 / 2.2),
    ]
}

/// Save the tonemapped film as an 8-bit RGB PNG.
pub fn save_png(film: &RenderFilm, exposure_stops: f32, path: &Path) -> Result<(), String> {
    let snap = film.snapshot_rgba(exposure_stops);
    let img = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(
        snap.width as u32,
        snap.height as u32,
        snap.rgb,
    )
    .ok_or_else(|| "Film buffer size mismatch while saving PNG".to_string())?;
    img.save(path).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Background job (same pattern as the baker engine: std::thread + channel-free
// shared atomics; UI polls the film + progress atomics)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RenderJob {
    pub film: Arc<RenderFilm>,
    pub progress: Arc<RenderProgress>,
    pub handle: Option<std::thread::JoinHandle<()>>,
    pub started: std::time::Instant,
}

impl Drop for RenderJob {
    fn drop(&mut self) {
        // Ensure the worker notices cancellation even if the UI forgot (the
        // handle is detached on purpose so mode switches never block the UI).
        self.progress.cancel.store(true, Ordering::Relaxed);
        self.handle.take();
    }
}

impl RenderJob {
    pub fn elapsed_ms(&self) -> u128 {
        self.started.elapsed().as_millis()
    }
}

/// Spawn the render on a background thread owned by the UI session.
pub fn spawn_render(scene: RenderScene, options: RenderOptions) -> RenderJob {
    let film = Arc::new(RenderFilm::new(
        options.width as usize,
        options.height as usize,
    ));
    let progress = Arc::new(RenderProgress::default());
    let (film2, progress2) = (Arc::clone(&film), Arc::clone(&progress));
    let handle = std::thread::Builder::new()
        .name("offline-render".to_string())
        .spawn(move || {
            let _ = render_film_into(&film2, &scene, &options, &progress2);
        })
        .ok();
    RenderJob {
        film,
        progress,
        handle,
        started: std::time::Instant::now(),
    }
}

// ---------------------------------------------------------------------------
// Tests (deterministic, no GUI, each well under a 1 s CPU budget)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Quad in the xz-plane at y = 0, normal +Y, spanning [-half, half]^2.
    fn flat_plane(half: f32, mat: FlatMaterial) -> RenderScene {
        let mk = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| RenderTri {
            p: [a, b, c],
            n: [[0.0, 1.0, 0.0]; 3],
            uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        };
        let tris = vec![
            mk([-half, 0.0, -half], [half, 0.0, -half], [half, 0.0, half]),
            mk([-half, 0.0, -half], [half, 0.0, half], [-half, 0.0, half]),
        ];
        RenderScene::from_tris(tris, mat)
    }

    /// Camera looking straight down (+pitch 90°) so the plane normal points
    /// back at the lens.
    fn top_down_camera(dist: f32) -> OrbitCamera {
        OrbitCamera {
            yaw_deg: 0.0,
            pitch_deg: 90.0,
            dist,
            fov_deg: 45.0,
        }
    }

    fn silent_camera(
        w: u32,
        h: u32,
        cam: OrbitCamera,
        env: EnvConfig,
        mat: FlatMaterial,
    ) -> (RenderScene, RenderOptions) {
        (
            flat_plane(8.0, mat),
            RenderOptions {
                spp: 2,
                width: w,
                height: h,
                exposure_stops: 0.0,
                env,
                camera: cam,
                output_path: None,
            },
        )
    }

    #[test]
    fn white_plane_converges_to_expected_albedo() {
        // 50%-grey uniform env + perfect Lambertian albedo 0.7 (linear):
        // cosine-weighted estimator for uniform radiance has zero variance and
        // integrates to exactly albedo * L = 0.7 * 0.5 = 0.35.
        let (scene, opts) = silent_camera(
            64,
            64,
            top_down_camera(2.2),
            EnvConfig {
                kind: EnvKind::Uniform(0.5),
                rotation_deg: 0.0,
                intensity: 1.0,
            },
            FlatMaterial {
                albedo_linear: [0.7, 0.7, 0.7],
                roughness: 1.0,
                metallic: 0.0,
            },
        );
        let progress = RenderProgress::default();
        let film = RenderFilm::new(opts.width as usize, opts.height as usize);
        let outcome = render_film_into(&film, &scene, &opts, &progress);
        assert_eq!(outcome, RenderOutcome::Completed);
        let (raw, passes) = film.snapshot_raw();
        assert_eq!(passes, 2);
        let mut mean_acc = [0.0f64; 3];
        for px in &raw {
            for (c, m) in mean_acc.iter_mut().enumerate() {
                *m += px[c] as f64;
            }
        }
        let n = raw.len() as f64 * passes as f64;
        for (c, m) in mean_acc.iter().enumerate() {
            let avg = m / n;
            assert!((avg - 0.35).abs() < 0.01, "channel {c} avg {avg} != 0.35");
        }
    }

    #[test]
    fn mirror_reflects_env_directionally() {
        // Chrome plane viewed straight down the normal. The studio env is a
        // monotonic upward gradient, so the center texel (mirror dir = +Y,
        // zenith ≈ 1.0) must out-reflect the edge-midpoint texels (tilted ~24°)
        // which in turn must out-reflect the corners (tilted ~35°).
        let (scene, opts) = silent_camera(
            64,
            64,
            top_down_camera(2.2),
            EnvConfig {
                kind: EnvKind::Studio {
                    sun_intensity: 0.0,
                    sun_angle_deg: 40.0,
                },
                rotation_deg: 0.0,
                intensity: 1.0,
            },
            FlatMaterial {
                albedo_linear: [0.8, 0.8, 0.8],
                roughness: 0.0,
                metallic: 1.0,
            },
        );
        let progress = RenderProgress::default();
        let film = RenderFilm::new(opts.width as usize, opts.height as usize);
        let outcome = render_film_into(&film, &scene, &opts, &progress);
        assert_eq!(outcome, RenderOutcome::Completed);
        let (raw, passes) = film.snapshot_raw();
        let px_avg = |x0: usize, y0: usize, s: usize| -> [f64; 3] {
            let mut acc = [0.0f64; 3];
            let mut count = 0.0f64;
            for y in y0..y0 + s {
                for x in x0..x0 + s {
                    let px = raw[y * 64 + x];
                    for c in 0..3 {
                        acc[c] += px[c] as f64;
                    }
                    count += 1.0;
                }
            }
            [
                acc[0] / count / passes as f64,
                acc[1] / count / passes as f64,
                acc[2] / count / passes as f64,
            ]
        };
        let center = px_avg(28, 28, 8);
        let edge_mid = px_avg(2, 28, 8); // mid-height but left edge
        let corner = px_avg(2, 2, 8);
        let lum = |v: &[f64; 3]| 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
        let (l_c, l_e, l_k) = (lum(&center), lum(&edge_mid), lum(&corner));
        // Directional gradient: center (zenith) > edge (tilted) > corner (more tilted).
        assert!(
            l_c > l_e + 0.03,
            "center {l_c} must reflect the bright zone more than edge {l_e}"
        );
        assert!(
            l_e > l_k + 0.01,
            "edge {l_e} must reflect brighter than corner {l_k}"
        );
    }

    #[test]
    fn determinism_same_seed_same_film() {
        // Mixed-lobe material to exercise every stochastic path (mixture lobe
        // choice, GGX sampling, cosine sampling). Two full runs must be
        // bit-identical.
        let mat = FlatMaterial {
            albedo_linear: [0.62, 0.41, 0.23],
            roughness: 0.35,
            metallic: 0.4,
        };
        let build = || {
            let (scene, opts) = silent_camera(
                64,
                64,
                OrbitCamera {
                    yaw_deg: 30.0,
                    pitch_deg: 55.0,
                    dist: 2.2,
                    fov_deg: 45.0,
                },
                EnvConfig {
                    kind: EnvKind::Studio {
                        sun_intensity: 6.0,
                        sun_angle_deg: 42.0,
                    },
                    rotation_deg: 20.0,
                    intensity: 1.0,
                },
                mat,
            );
            (scene, opts)
        };
        let run = || {
            let (scene, opts) = build();
            let progress = RenderProgress::default();
            let film = RenderFilm::new(opts.width as usize, opts.height as usize);
            let outcome = render_film_into(&film, &scene, &opts, &progress);
            assert_eq!(outcome, RenderOutcome::Completed);
            film.snapshot_raw().0
        };
        let a = run();
        let b = run();
        assert_eq!(a.len(), b.len());
        let bits = |v: [f32; 3]| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
        assert!(
            a.iter().zip(b.iter()).all(|(p, q)| bits(*p) == bits(*q)),
            "two runs of the same configuration must be bit-identical"
        );
    }

    #[test]
    fn aces_tonemap_clamps() {
        // Extreme exposure stays <= 1.0 after tone mapping.
        let blown = aces_tonemap([1000.0, 1000.0, 1000.0]);
        for c in blown {
            assert!(c <= 1.0 && c > 0.9, "must clamp below 1.0: {c}");
        }
        // Documented mid-gray control point: aces_film(0.18) = 0.18*(2.51*0.18
        // + 0.03) / (0.18*(2.43*0.18 + 0.59) + 0.14) = 0.26691...
        let mid = aces_tonemap([0.18, 0.18, 0.18]);
        for c in mid {
            assert!((c - 0.26691).abs() < 0.005, "midgray {c} != ~0.26691");
        }
        // Negative / zero stays clamped to the valid range.
        let zero = aces_tonemap([0.0, 0.0, 0.0]);
        assert!(zero.iter().all(|c| *c >= 0.0));
    }
}
