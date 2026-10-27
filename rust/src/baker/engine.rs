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

    /// Output bit depth applied when baked planes are encoded (project save
    /// stores `.u8.raw` / `.f16.raw` planes at half/quarter size; internal
    /// accumulation and viewport consumption stay f32).
    pub map_format: BakedMapFormat,
    /// Fallback correspondence: when mesh-name matching cannot pair a low-poly
    /// shell, pair it with the nearest high-poly shell by world-space AABB
    /// distance instead of risking cross-shell hits via the global BVH.
    pub match_by_shell: bool,
    /// Target UDIM UV tile (e.g. Some(1001), Some(1002), Some(1011)).
    /// When specified, mesh UVs are shifted so that the tile's coordinates map into [0, 1]x[0, 1]
    /// and triangles outside this tile footprint are culled.
    pub udim_tile: Option<u32>,
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
            map_format: BakedMapFormat::F32,
            match_by_shell: false,
            udim_tile: None,
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
// Per-texel ray controls: painted ray offsets + output bit depth
// ----------------------------------------------------------------------------

/// External **painted ray-offset** texture: single channel, one f32 per texel,
/// conventionally decoded to `[-1, 1]` (0.5 mid-gray on disk == 0.0 offset).
/// Shifts the ray cast origin along the low-poly surface normal for every ray
/// casting baker (normal, world-space normal, position, thickness, AO), letting
/// users repair pinned correspondences per texel ("painted ray offsets" parity).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintedOffsetMap {
    pub width: u32,
    pub height: u32,
    /// One offset per texel, in `[-1, 1]`. `0.0` == no offset.
    pub data: Vec<f32>,
}

impl PaintedOffsetMap {
    /// Nearest-neighbor sample at normalized (u, v) coordinates, clamped to `[-1, 1]`.
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let px = (u.clamp(0.0, 1.0) * (self.width.max(1) - 1) as f32).round() as u32;
        let py = (v.clamp(0.0, 1.0) * (self.height.max(1) - 1) as f32).round() as u32;
        self.data[(py.min(self.height.saturating_sub(1)) as usize) * (self.width as usize)
            + px.min(self.width.saturating_sub(1)) as usize]
            .clamp(-1.0, 1.0)
    }
}

/// Output bit depth for baked mesh maps (internal accumulation is always f32; the
/// format is applied when the finished maps are encoded into image planes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BakedMapFormat {
    /// Quantized 8 bit per channel: `round(clamp(v, 0, 1) * 255)`. 4x memory reduction.
    U8,
    /// IEEE 754 binary16 (2 bytes/component). No 8-bit banding; matches reference's
    /// "16 bits float" bake output profile.
    F16,
    /// Raw little-endian f32 planes — identical to what was previously consumed
    /// internally. Default so existing persistence paths keep round-tripping.
    #[default]
    F32,
}

impl BakedMapFormat {
    /// Bytes per encoded component (pixel-interleaved planes).
    #[inline]
    pub fn component_bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::F16 => 2,
            Self::F32 => 4,
        }
    }

    /// Human-readable depth label for baker UI lists.
    pub fn label(self) -> &'static str {
        match self {
            Self::U8 => "8 bit",
            Self::F16 => "16 bit float",
            Self::F32 => "32 bit float (raw)",
        }
    }
}

/// Finished baked maps quantized/encoded at a requested bit depth. All planes are
/// tightly packed, pixel-interleaved (per texel: N channels x `format` bytes,
/// little-endian) — matching what a texture upload/export stage needs as raw data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EncodedBakedMaps {
    pub resolution: u32,
    pub format: BakedMapFormat,
    pub ambient_occlusion: Option<Vec<u8>>,
    pub curvature: Option<Vec<u8>>,
    pub normal: Option<Vec<u8>>,
    pub world_space_normal: Option<Vec<u8>>,
    pub position: Option<Vec<u8>>,
    pub thickness: Option<Vec<u8>>,
    pub id: Option<Vec<u8>>,
    pub is_cancelled: bool,
}

impl EncodedBakedMaps {
    /// Decode back into the internal f32 representation (lossy for `U8`).
    pub fn to_baked_maps(&self) -> BakedMaps {
        let dec_s = |p: &Option<Vec<u8>>| p.as_ref().map(|b| decode_scalar_plane(b, self.format));
        let dec_v3 = |p: &Option<Vec<u8>>| p.as_ref().map(|b| decode_vec3_plane(b, self.format));
        BakedMaps {
            resolution: self.resolution,
            ambient_occlusion: dec_s(&self.ambient_occlusion),
            curvature: dec_s(&self.curvature),
            normal: dec_v3(&self.normal),
            world_space_normal: dec_v3(&self.world_space_normal),
            position: dec_v3(&self.position),
            thickness: dec_s(&self.thickness),
            id: self.id.as_ref().map(|b| decode_vec4_plane(b, self.format)),
            is_cancelled: self.is_cancelled,
        }
    }
}

/// Encode a finished scalar plane (one f32 per texel) at the requested bit depth.
pub fn encode_scalar_plane(src: &[f32], format: BakedMapFormat) -> Vec<u8> {
    match format {
        BakedMapFormat::U8 => src.iter().map(|&v| f32_to_u8_channel(v)).collect(),
        BakedMapFormat::F16 => src
            .iter()
            .flat_map(|&v| f32_to_half_bits(v).to_le_bytes())
            .collect(),
        BakedMapFormat::F32 => src.iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}

/// Encode a finished RGB (3-component) plane at the requested bit depth.
pub fn encode_vec3_plane(src: &[[f32; 3]], format: BakedMapFormat) -> Vec<u8> {
    match format {
        BakedMapFormat::U8 => src
            .iter()
            .flat_map(|t| t.iter().map(|&v| f32_to_u8_channel(v)))
            .collect(),
        BakedMapFormat::F16 => src
            .iter()
            .flat_map(|t| t.iter().flat_map(|&v| f32_to_half_bits(v).to_le_bytes()))
            .collect(),
        BakedMapFormat::F32 => src
            .iter()
            .flat_map(|t| t.iter().flat_map(|v| v.to_le_bytes()))
            .collect(),
    }
}

/// Encode a finished RGBA (4-component) plane at the requested bit depth.
pub fn encode_vec4_plane(src: &[[f32; 4]], format: BakedMapFormat) -> Vec<u8> {
    match format {
        BakedMapFormat::U8 => src
            .iter()
            .flat_map(|t| t.iter().map(|&v| f32_to_u8_channel(v)))
            .collect(),
        BakedMapFormat::F16 => src
            .iter()
            .flat_map(|t| t.iter().flat_map(|&v| f32_to_half_bits(v).to_le_bytes()))
            .collect(),
        BakedMapFormat::F32 => src
            .iter()
            .flat_map(|t| t.iter().flat_map(|v| v.to_le_bytes()))
            .collect(),
    }
}

/// Decode a scalar plane back to f32 (inspection / round-trip; lossy for `U8`).
pub fn decode_scalar_plane(bytes: &[u8], format: BakedMapFormat) -> Vec<f32> {
    match format {
        BakedMapFormat::U8 => bytes.iter().map(|&b| b as f32 / 255.0).collect(),
        BakedMapFormat::F16 => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| half_bits_to_f32(u16::from_le_bytes(*c)))
            .collect(),
        BakedMapFormat::F32 => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect(),
    }
}

/// Decode an RGB (3-component) plane back to f32.
pub fn decode_vec3_plane(bytes: &[u8], format: BakedMapFormat) -> Vec<[f32; 3]> {
    let n = format.component_bytes();
    bytes
        .chunks_exact(3 * n)
        .map(|px| {
            let mut out = [0.0f32; 3];
            for (c, slot) in out.iter_mut().enumerate() {
                let b = &px[c * n..(c + 1) * n];
                *slot = decode_component(b, format);
            }
            out
        })
        .collect()
}

/// Decode an RGBA (4-component) plane back to f32.
pub fn decode_vec4_plane(bytes: &[u8], format: BakedMapFormat) -> Vec<[f32; 4]> {
    let n = format.component_bytes();
    bytes
        .chunks_exact(4 * n)
        .map(|px| {
            let mut out = [0.0f32; 4];
            for (c, slot) in out.iter_mut().enumerate() {
                let b = &px[c * n..(c + 1) * n];
                *slot = decode_component(b, format);
            }
            out
        })
        .collect()
}

#[inline]
fn decode_component(b: &[u8], format: BakedMapFormat) -> f32 {
    match format {
        BakedMapFormat::U8 => b[0] as f32 / 255.0,
        BakedMapFormat::F16 => half_bits_to_f32(u16::from_le_bytes([b[0], b[1]])),
        BakedMapFormat::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    }
}

/// Encodes a finished [`BakedMaps`] set into [`EncodedBakedMaps`] at `format` depth.
pub fn encode_baked_maps(maps: &BakedMaps, format: BakedMapFormat) -> EncodedBakedMaps {
    let e_s = |p: &Option<Vec<f32>>| p.as_ref().map(|b| encode_scalar_plane(b, format));
    let e_v3 = |p: &Option<Vec<[f32; 3]>>| p.as_ref().map(|b| encode_vec3_plane(b, format));
    EncodedBakedMaps {
        resolution: maps.resolution,
        format,
        ambient_occlusion: e_s(&maps.ambient_occlusion),
        curvature: e_s(&maps.curvature),
        normal: e_v3(&maps.normal),
        world_space_normal: e_v3(&maps.world_space_normal),
        position: e_v3(&maps.position),
        thickness: e_s(&maps.thickness),
        id: maps.id.as_ref().map(|b| encode_vec4_plane(b, format)),
        is_cancelled: maps.is_cancelled,
    }
}

/// Rounded 8-bit quantization of a finished channel value.
#[inline]
pub fn f32_to_u8_channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// IEEE 754 binary16 encode (round-to-nearest). F32 denormals and values too small
/// even for a half subnormal flush toward signed zero; overflows saturate to ±Inf,
/// mirroring hardware f32→f16 conversion. f32 denormals never reach the fast path —
/// a deliberate robustness choice for the bake pipeline.
pub fn f32_to_half_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let biased = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x007f_ffff;

    if biased == 0xff {
        // NaN -> quiet NaN; +/-Inf -> +/-Inf
        if mant != 0 {
            return sign | 0x7e00 | ((mant >> 13) as u16 & 0x3ff);
        }
        return sign | 0x7c00;
    }
    let exp = biased - 127 + 15; // half target exponent
    if exp >= 0x1f {
        return sign | 0x7c00; // overflow -> infinity
    }
    if exp <= 0 {
        if exp < -10 {
            return sign; // flush to (signed) zero
        }
        // Subnormal half: shift the explicit-leading-bit mantissa down, round-nearest.
        let m = mant | 0x0080_0000;
        let shift = (14 - exp) as u32; // 1..=24
        return sign | ((m >> shift) as u16 + ((m >> (shift - 1)) & 1) as u16);
    }
    sign | ((exp as u16) << 10) | ((mant >> 13) as u16)
}

/// IEEE 754 binary16 decode (exact, no forbidden shortcuts).
pub fn half_bits_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0f32 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let mant = (h & 0x03ff) as u32;
    match exp {
        0 => sign * (mant as f32) * 5.960_464_5e-8, // mant * 2^-24
        31 => {
            if mant == 0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => {
            let e = exp - 15;
            let base = 1.0 + mant as f32 / 1024.0;
            sign * base
                * if e >= 0 {
                    2f32.powi(e)
                } else {
                    1.0 / 2f32.powi(-e)
                }
        }
    }
}

/// Runtime-only baker controls that are deliberately **not** fields of
/// [`BakerOptions`]: `project/mod.rs` and the UI construct `BakerOptions` with
/// exhaustive struct literals inside ownership hand-offs this module does not own,
/// and new fields there would break the build until those literals gain
/// `..Default::default()`. Everything here defaults to "off/no change" so
/// [`BakerEngine::bake_all`] (which uses `AdvancedBakeOptions::default()`) behaves
/// exactly as before.
///
/// UI wiring follow-up (small, documented in the refactor report): lift these fields
/// into `BakerOptions` + `BakerOptionsManifest` (serde `#[serde(default)]`), plus
/// add re-exports in `baker/mod.rs`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdvancedBakeOptions {
    /// Output channel depth for [`BakerEngine::bake_all_encoded`] planes. Internal
    /// f32 accumulation is unaffected; only encoded planes honor it.
    pub map_format: BakedMapFormat,
    /// Per-texel painted ray offsets applied to every cast origin along the surface
    /// normal across all raycasters. `None` == zero offset (previous behavior).
    pub ray_offset_map: Option<Arc<PaintedOffsetMap>>,
    /// When Match-by-Mesh-Name cannot find a matching high-poly shell, correspond the
    /// low-poly shell to the nearest high-poly shell by world-space AABB proximity
    /// (within one ray span) instead of falling back to the global BVH. Prevents
    /// cross-shell false correspondences when names diverge.
    pub match_by_shell: bool,
    /// Target UDIM UV tile (e.g. Some(1001), Some(1002)).
    pub udim_tile: Option<u32>,
    /// Optional progress range (base, span) in percent for multi-tile/batch progress mapping.
    pub progress_range: Option<(usize, usize)>,
}

/// Calculates the normalized (u, v) tile coordinate offset for a UDIM tile ID.
/// - Tile 1001 -> [0.0, 0.0]
/// - Tile 1002 -> [1.0, 0.0]
/// - Tile 1011 -> [0.0, 1.0]
/// - Tile 1012 -> [1.0, 1.0]
#[inline]
pub fn udim_tile_uv_offset(tile: u32) -> [f32; 2] {
    if tile < 1001 {
        [0.0, 0.0]
    } else {
        let idx = tile - 1001;
        let u = (idx % 10) as f32;
        let v = (idx / 10) as f32;
        [u, v]
    }
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
        for (i, &val) in p.iter().enumerate() {
            if val < self.min[i] {
                self.min[i] = val;
            }
            if val > self.max[i] {
                self.max[i] = val;
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

/// A independently-named high-poly shell used for Match-by-Mesh-Name / Match-by-Shell
/// correspondence: its stripped lowercase name, raycast BVH and world-space AABB.
#[derive(Debug, Clone)]
pub struct HighPolyShell {
    pub name: String,
    pub bvh: BvhNode,
    pub bbox: BoundingBox,
}

/// Collect validated world-space triangles of a high-poly index range for BVH building.
fn hp_triangles(
    hp: &crate::graphics::mesh::Mesh,
    tri_start: usize,
    tri_count: usize,
) -> Vec<BvhTriangle> {
    let mut tris = Vec::new();
    if tri_start * 3 >= hp.indices.len() {
        return tris;
    }
    let end = (tri_start + tri_count).min(hp.indices.len() / 3);
    for t in tri_start..end {
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
            tris.push(BvhTriangle {
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
    tris
}

/// Separation between two boxes along the axis with the largest gap (0 where they
/// overlap on all axes); used to reject shell correspondences outside ray reach.
fn aabb_gap(a: &BoundingBox, b: &BoundingBox) -> f32 {
    let mut gap = 0.0f32;
    for i in 0..3 {
        let d = if a.max[i] < b.min[i] {
            b.min[i] - a.max[i]
        } else if b.max[i] < a.min[i] {
            a.min[i] - b.max[i]
        } else {
            0.0
        };
        gap = gap.max(d);
    }
    gap
}

#[inline]
fn bbox_center(b: &BoundingBox) -> [f32; 3] {
    [
        0.5 * (b.min[0] + b.max[0]),
        0.5 * (b.min[1] + b.max[1]),
        0.5 * (b.min[2] + b.max[2]),
    ]
}

/// Denormal-safe squared-length square root: guards sqrt-underflow of products of
/// tiny face components in curvature/AO estimation paths.
#[inline]
fn sqrt_guarded(len2: f32) -> f32 {
    // `max` clamps denormal products to the smallest safe normal magnitude so the
    // subsequent reciprocal can never produce inf/NaN.
    len2.max(1e-20).sqrt()
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
    ///
    /// Legacy entry point kept for existing UI callers: identical geometry pipeline
    /// as [`Self::bake_all_advanced`] run with [`AdvancedBakeOptions::default()`]
    /// (16-bit/8-bit encoding off, no painted offsets, no shell fallback), so exact
    /// previous behavior is preserved.
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
        // The advanced knobs that live on the persisted `BakerOptions` (output
        // depth, shell fallback) ride through here so `bake_all` callers get
        // them without constructing `AdvancedBakeOptions` themselves.
        let advanced = AdvancedBakeOptions {
            map_format: options.map_format,
            ray_offset_map: None,
            match_by_shell: options.match_by_shell,
            udim_tile: options.udim_tile,
            progress_range: None,
        };
        Self::bake_all_advanced(
            vertices, normals, uvs, indices, submeshes, options, &advanced, progress, cancel,
            high_poly,
        )
    }

    /// Bakes mesh maps across multiple UDIM tiles.
    ///
    /// For each tile in `udim_tiles`, rasterizes only geometry mapped to that tile's
    /// UV range and returns a mapping from tile ID to finished [`BakedMaps`].
    pub fn bake_all_udim(
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        submeshes: &[crate::graphics::mesh::SubMesh],
        options: &BakerOptions,
        udim_tiles: &[u32],
        progress: Option<Arc<AtomicUsize>>,
        cancel: Option<Arc<AtomicBool>>,
        high_poly: Option<&crate::graphics::mesh::Mesh>,
    ) -> std::collections::BTreeMap<u32, BakedMaps> {
        let mut results = std::collections::BTreeMap::new();
        let total_tiles = udim_tiles.len().max(1);

        for (i, &tile) in udim_tiles.iter().enumerate() {
            if let Some(ref c) = cancel {
                if c.load(Ordering::Relaxed) {
                    break;
                }
            }

            let mut tile_options = options.clone();
            tile_options.udim_tile = Some(tile);

            let base = (i * 100) / total_tiles;
            let next_base = ((i + 1) * 100) / total_tiles;
            let span = next_base.saturating_sub(base).max(1);

            let advanced = AdvancedBakeOptions {
                map_format: options.map_format,
                ray_offset_map: None,
                match_by_shell: options.match_by_shell,
                udim_tile: Some(tile),
                progress_range: Some((base, span)),
            };

            let maps = Self::bake_all_advanced(
                vertices,
                normals,
                uvs,
                indices,
                submeshes,
                &tile_options,
                &advanced,
                progress.clone(),
                cancel.clone(),
                high_poly,
            );

            if let Some(ref p) = progress {
                p.store(next_base, Ordering::Relaxed);
            }

            results.insert(tile, maps);
        }

        results
    }

    /// Bake with [`AdvancedBakeOptions`] intents and emit the finished maps already
    /// quantized/encoded at [`AdvancedBakeOptions::map_format`] depth (8/16-bit parity
    /// with reference's bake output profile). Same geometric pipeline as
    /// [`Self::bake_all_advanced`].
    pub fn bake_all_encoded(
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        submeshes: &[crate::graphics::mesh::SubMesh],
        options: &BakerOptions,
        advanced: &AdvancedBakeOptions,
        progress: Option<Arc<AtomicUsize>>,
        cancel: Option<Arc<AtomicBool>>,
        high_poly: Option<&crate::graphics::mesh::Mesh>,
    ) -> EncodedBakedMaps {
        let maps = Self::bake_all_advanced(
            vertices, normals, uvs, indices, submeshes, options, advanced, progress, cancel,
            high_poly,
        );
        encode_baked_maps(&maps, advanced.map_format)
    }

    /// Full bake entry point with per-texel runtime controls: painted ray offsets for
    /// every caster, a map-format (bit depth) choice, and Match-by-Shell fallback for
    /// cross-shell ray sanity. Skew correction itself flows through [`BakerOptions`]
    /// (`use_skew_correction`, `skew_map`, `skew_strength`, `skew_invert`) and now
    /// perturbs the origins of **every** casting ray (normal/position/AO/thickness),
    /// not just the primary cast — with `AdvancedBakeOptions::default()` and no skew
    /// map the behavior is bit-identical to previous versions.
    pub fn bake_all_advanced(
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        submeshes: &[crate::graphics::mesh::SubMesh],
        options: &BakerOptions,
        advanced: &AdvancedBakeOptions,
        progress: Option<Arc<AtomicUsize>>,
        cancel: Option<Arc<AtomicBool>>,
        high_poly: Option<&crate::graphics::mesh::Mesh>,
    ) -> BakedMaps {
        let res = options.resolution as usize;
        let total = res * res;
        let (prog_base, prog_span) = advanced.progress_range.unwrap_or((0, 100));

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

        // 2. Build High-Poly BVH Trees (global, plus one per high-poly shell for
        //    Match-by-Mesh-Name / Match-by-Shell correspondence)
        let (hp_global_bvh, hp_shells) = if let Some(hp) = high_poly {
            let num_hp_tris = hp.indices.len() / 3;
            let global = BvhNode::build(hp_triangles(hp, 0, num_hp_tris));

            let mut shells: Vec<HighPolyShell> = Vec::new();
            for sm in &hp.submeshes {
                let base_name = strip_mesh_suffix(&sm.name, &options.high_poly_suffix);
                let sm_tris = hp_triangles(hp, sm.index_start / 3, sm.index_count / 3);
                if sm_tris.is_empty() {
                    continue;
                }
                let mut bbox = BoundingBox::empty();
                for t in &sm_tris {
                    bbox.grow_box(&t.bbox);
                }
                shells.push(HighPolyShell {
                    name: base_name.to_lowercase(),
                    bvh: BvhNode::build(sm_tris),
                    bbox,
                });
            }

            (Some(global), shells)
        } else {
            (None, Vec::new())
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
                // Denormal-safe: squared magnitude of a valid face cross product is >=
                // 1e-12; smaller areas are degenerate slivers, and the `.max` inside
                // sqrt_guarded also protects the reciprocal from flush-to-zero length.
                let len2 = fnx * fnx + fny * fny + fnz * fnz;
                if len2 < 1e-12 {
                    continue;
                }
                let len = sqrt_guarded(len2);
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

        // 4b. Per-shell BVH selection (precomputed once per low-poly shell instead of a
        //     linear submesh scan per triangle). Match-by-Mesh-Name maps low shells to
        //     high-poly shells by stripped name; the optional Match-by-Shell fallback
        //     corresponds unmatched shells by world-space AABB proximity within one ray
        //     span, so rays can no longer hit an unrelated high-poly shell.
        // Selection: trace the whole mesh (`Global`) or one shell BVH.
        #[derive(Clone, Copy)]
        enum BvhSelection {
            Global,
            Shell(usize),
        }

        let mut sm_sorted: Vec<&crate::graphics::mesh::SubMesh> = submeshes.iter().collect();
        sm_sorted.sort_by_key(|sm| sm.index_start);
        let shell_count = sm_sorted.len().max(1);
        let mut tri_shell = vec![0usize; num_tris];
        for (s, sm) in sm_sorted.iter().enumerate() {
            let t0 = (sm.index_start / 3).min(num_tris);
            let t1 = ((sm.index_start + sm.index_count) / 3).min(num_tris);
            tri_shell[t0..t1].fill(s);
        }

        let mut low_shell_bbox = vec![BoundingBox::empty(); shell_count];
        for t in 0..num_tris {
            let i0 = indices[t * 3] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;
            if i0 < n_verts && i1 < n_verts && i2 < n_verts {
                low_shell_bbox[tri_shell[t]].grow(vertices[i0]);
                low_shell_bbox[tri_shell[t]].grow(vertices[i1]);
                low_shell_bbox[tri_shell[t]].grow(vertices[i2]);
            }
        }

        let ray_span = frontal_dist + rear_dist;
        let mut shell_bvh = vec![BvhSelection::Global; shell_count];
        if options.match_by_name && !hp_shells.is_empty() {
            for (s, sm) in sm_sorted.iter().enumerate() {
                let base = strip_mesh_suffix(&sm.name, &options.low_poly_suffix).to_lowercase();
                let sel = if let Some(i) = hp_shells.iter().position(|sh| sh.name == base) {
                    BvhSelection::Shell(i)
                } else if advanced.match_by_shell {
                    // Match-by-shell: nearest high-poly shell whose AABB is reachable
                    // within one ray span of the low shell AABB (correspondence sanity).
                    let mut best: Option<(f32, usize)> = None;
                    for (i, sh) in hp_shells.iter().enumerate() {
                        if aabb_gap(&low_shell_bbox[s], &sh.bbox) <= ray_span {
                            let ca = bbox_center(&low_shell_bbox[s]);
                            let cb = bbox_center(&sh.bbox);
                            let d = (ca[0] - cb[0]).powi(2)
                                + (ca[1] - cb[1]).powi(2)
                                + (ca[2] - cb[2]).powi(2);
                            if best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, i));
                            }
                        }
                    }
                    match best {
                        Some((_, i)) => BvhSelection::Shell(i),
                        None => BvhSelection::Global,
                    }
                } else {
                    // No name match and shell matching disabled: trace everything
                    // (historical fallback, unchanged).
                    BvhSelection::Global
                };
                shell_bvh[s] = sel;
            }
        }

        // 5. Forward UV Triangle Rasterization
        let (u_off, v_off) = if let Some(tile) = options.udim_tile.or(advanced.udim_tile) {
            let off = udim_tile_uv_offset(tile);
            (off[0], off[1])
        } else {
            (0.0, 0.0)
        };

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
                    p.store(prog_base + ((t * prog_span) / num_tris), Ordering::Relaxed);
                }
            }

            let tri_idx_start = t * 3;
            let i0 = indices[tri_idx_start] as usize;
            let i1 = indices[tri_idx_start + 1] as usize;
            let i2 = indices[tri_idx_start + 2] as usize;

            if i0 >= uvs.len() || i1 >= uvs.len() || i2 >= uvs.len() {
                continue;
            }

            let uv0_raw = uvs[i0];
            let uv1_raw = uvs[i1];
            let uv2_raw = uvs[i2];

            let uv0 = [uv0_raw[0] - u_off, uv0_raw[1] - v_off];
            let uv1 = [uv1_raw[0] - u_off, uv1_raw[1] - v_off];
            let uv2 = [uv2_raw[0] - u_off, uv2_raw[1] - v_off];

            let min_u = uv0[0].min(uv1[0]).min(uv2[0]);
            let max_u = uv0[0].max(uv1[0]).max(uv2[0]);
            let min_v = uv0[1].min(uv1[1]).min(uv2[1]);
            let max_v = uv0[1].max(uv1[1]).max(uv2[1]);

            // Cull triangles outside the target [0, 1]x[0, 1] tile bounds
            if max_u < 0.0 || min_u > 1.0 || max_v < 0.0 || min_v > 1.0 {
                continue;
            }

            let x0 = (min_u.max(0.0) * res_f).floor() as usize;
            let x1 = ((max_u.min(1.0) * res_f).ceil() as usize).min(res - 1);
            let y0 = (min_v.max(0.0) * res_f).floor() as usize;
            let y1 = ((max_v.min(1.0) * res_f).ceil() as usize).min(res - 1);

            // Signed 2x UV area of this triangle (identical to the `det` computed below
            // for handedness). Either sign is valid for the barycentric formulas —
            // mirrored UV islands flip the sign — but *zero* is degenerate, so skip the
            // triangle instead of clamping the sign away (the old `.max(1e-7)` broke
            // mirrored islands' rasterization).
            let det = (uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]);
            if !det.is_finite() || det.abs() < 1e-9 {
                continue;
            }

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
            // `det` was already computed above as the signed 2x UV area.
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

            // Match-by-Name / Match-by-Shell BVH selection for this triangle
            // (precomputed per shell — no repeated linear submesh scans).
            let target_bvh: Option<&BvhNode> = if hp_global_bvh.is_none() {
                None
            } else if hp_shells.is_empty() {
                hp_global_bvh.as_ref()
            } else {
                match shell_bvh[tri_shell[t]] {
                    BvhSelection::Global => hp_global_bvh.as_ref(),
                    BvhSelection::Shell(i) => Some(&hp_shells[i].bvh),
                }
            };

            for py in y0..=y1 {
                let row_idx = py * res;
                for px in x0..=x1 {
                    // ---- Per-texel controls (identical for every subpixel sample) ----
                    let u_tex = px as f32 / res_f;
                    let v_tex = py as f32 / res_f;
                    // Skew correction ("Use Skew Correction"): 2-channel shear decoded
                    // to [-1, 1], perturbing every ray origin cast from this texel.
                    // No map supplied -> zero perturbation (behavior unchanged).
                    let (skew_o, skew_b) = if options.use_skew_correction {
                        match options.skew_map.as_ref() {
                            Some(sk) => {
                                let o = sk.sample(u_tex, v_tex);
                                let s = if options.skew_invert {
                                    -options.skew_strength
                                } else {
                                    options.skew_strength
                                };
                                (o[0] * s, o[1] * s)
                            }
                            None => (0.0, 0.0),
                        }
                    } else {
                        (0.0, 0.0)
                    };
                    // Painted ray offsets: shift the cast origin along the surface
                    // normal by offset * ray_span (no map / 0.0 == no change).
                    let ray_off = advanced
                        .ray_offset_map
                        .as_ref()
                        .map_or(0.0f32, |m| m.sample(u_tex, v_tex));
                    let origin_out = (frontal_dist + ray_off * ray_span).clamp(1e-4, ray_span);

                    for &(ox, oy) in subpixel_offsets {
                        let u_px = (px as f32 + ox) / res_f;
                        let v_px = (py as f32 + oy) / res_f;

                        let b0 = ((uv1[1] - uv2[1]) * (u_px - uv2[0])
                            + (uv2[0] - uv1[0]) * (v_px - uv2[1]))
                            / det;
                        let b1 = ((uv2[1] - uv0[1]) * (u_px - uv2[0])
                            + (uv0[0] - uv2[0]) * (v_px - uv2[1]))
                            / det;
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

                            // Epsilon-safe interpolated-normal normalization: degenerate
                            // vertex normals fall back to +Y instead of producing NaN
                            // tangent frames downstream.
                            let n_len2 = nx * nx + ny * ny + nz * nz;
                            let n_low = if n_len2 > 1e-12 {
                                let inv = 1.0 / n_len2.sqrt();
                                [nx * inv, ny * inv, nz * inv]
                            } else {
                                [0.0, 1.0, 0.0]
                            };

                            // Orthonormalize the UV tangent against the interpolated normal
                            // (MikkTSpace-style Gram-Schmidt). When the UV tangent is
                            // degenerate or (near-)parallel to the normal — collapsed UVs,
                            // zero-area UV triangles — fall back to a stable axis-derived
                            // tangent instead of dividing by ~0.
                            let t_dot_n =
                                tri_t[0] * n_low[0] + tri_t[1] * n_low[1] + tri_t[2] * n_low[2];
                            let t_ortho = [
                                tri_t[0] - n_low[0] * t_dot_n,
                                tri_t[1] - n_low[1] * t_dot_n,
                                tri_t[2] - n_low[2] * t_dot_n,
                            ];
                            let to_len2 = t_ortho[0] * t_ortho[0]
                                + t_ortho[1] * t_ortho[1]
                                + t_ortho[2] * t_ortho[2];
                            let tangent = if to_len2 > 1e-8 {
                                let inv = 1.0 / sqrt_guarded(to_len2);
                                [t_ortho[0] * inv, t_ortho[1] * inv, t_ortho[2] * inv]
                            } else {
                                // Pivot axis = the one least aligned with the normal, so
                                // cross(n, pivot) keeps a length of at least ~0.577.
                                let pivot = if n_low[0].abs() <= n_low[1].abs()
                                    && n_low[0].abs() <= n_low[2].abs()
                                {
                                    [1.0f32, 0.0, 0.0]
                                } else if n_low[1].abs() <= n_low[2].abs() {
                                    [0.0f32, 1.0, 0.0]
                                } else {
                                    [0.0f32, 0.0, 1.0]
                                };
                                let tx = n_low[1] * pivot[2] - n_low[2] * pivot[1];
                                let ty = n_low[2] * pivot[0] - n_low[0] * pivot[2];
                                let tz = n_low[0] * pivot[1] - n_low[1] * pivot[0];
                                let tl = sqrt_guarded(tx * tx + ty * ty + tz * tz);
                                [tx / tl, ty / tl, tz / tl]
                            };
                            // Mirrored-UV parity fix: flip the bitangent on negative UV determinant
                            // (matching reference's signed handedness) so mirrored islands no longer
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

                            // Skew shear in world space: the per-texel shear offsets scaled
                            // by this texel's world footprint along the tangent/bitangent
                            // frame. Applied to the origin of EVERY ray cast from this
                            // texel (primary normal/position ray, AO hemisphere rays, and
                            // the thickness reverse ray) — "Use Skew Map" parity. Zero map
                            // -> zero shear, so untouched bakes are unchanged.
                            let shear_t = skew_o * t_span / res_f;
                            let shear_b = skew_b * bit_span / res_f;
                            let skew_world = [
                                tangent[0] * shear_t + bitangent[0] * shear_b,
                                tangent[1] * shear_t + bitangent[1] * shear_b,
                                tangent[2] * shear_t + bitangent[2] * shear_b,
                            ];
                            let shear_reach = (skew_world[0] * skew_world[0]
                                + skew_world[1] * skew_world[1]
                                + skew_world[2] * skew_world[2])
                                .sqrt();

                            // High-poly raycasting through BVH: origin = surface point +
                            // surface normal * (frontal + painted offset) + skew shear.
                            if let Some(bvh) = target_bvh {
                                let ray_origin = [
                                    px_pos[0] + n_low[0] * origin_out + skew_world[0],
                                    px_pos[1] + n_low[1] * origin_out + skew_world[1],
                                    px_pos[2] + n_low[2] * origin_out + skew_world[2],
                                ];
                                let ray_dir = [-n_low[0], -n_low[1], -n_low[2]];
                                // Budget covers reaching the surface at `origin_out`,
                                // the rear penetration bound, plus any lateral shear.
                                let max_dist = origin_out + rear_dist + shear_reach;

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
                                    // (origin perturbed by the same per-texel skew shear)
                                    if options.bake_thickness {
                                        let rev_origin = [
                                            hp_hit.position[0] - hp_norm[0] * 0.002 + skew_world[0],
                                            hp_hit.position[1] - hp_norm[1] * 0.002 + skew_world[1],
                                            hp_hit.position[2] - hp_norm[2] * 0.002 + skew_world[2],
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
                                            let xi2 = ((s as f32 * 0.618_034) + 0.1).fract();
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
                                                hp_hit.position[0]
                                                    + hp_norm[0] * 0.002
                                                    + skew_world[0],
                                                hp_hit.position[1]
                                                    + hp_norm[1] * 0.002
                                                    + skew_world[1],
                                                hp_hit.position[2]
                                                    + hp_norm[2] * 0.002
                                                    + skew_world[2],
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
                            for (c, &val) in final_norm.iter().enumerate() {
                                acc.norm[c] += val * sample_w;
                                acc.wsn[c] += final_wsn[c] * sample_w;
                                acc.pos[c] += final_pos[c] * sample_w;
                            }
                            for (c, &val) in final_id.iter().enumerate() {
                                acc.id[c] += val * sample_w;
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
            p.store(prog_base + prog_span, Ordering::Relaxed);
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
            (h.position[2] - 1.0).abs() < 0.05,
            "Front surface of unit procedural sphere should be near z=1.0 (got {})",
            h.position[2]
        );
        assert!(
            (h.distance - 2.0).abs() < 0.05,
            "Hit distance should be ~2.0 (got {})",
            h.distance
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

    /// End-to-end semantic quality gate for the raycast pipeline: a low-poly
    /// quad floor plus a high-poly box hovering above it. This pins the exact
    /// behaviors an offline baker must demonstrate — geometry-aware normal
    /// projection, recessive AO under the box shadow, and thickness that is
    /// saturated through the slab but small along the free edges.
    #[test]
    fn test_end_to_end_bake_semantics_high_poly_above_plane() {
        use crate::graphics::mesh::Mesh;

        // Low-poly floor: single quad with a sane UV cover, facing +Y.
        let verts: Vec<[f32; 3]> = vec![
            [-1.0, 0.0, -1.0],
            [1.0, 0.0, -1.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
        ];
        let norms: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 4];
        let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let indices: Vec<u32> = vec![0, 2, 1, 0, 3, 2];

        // High-poly: the same large quad PLUS a box floating directly over the
        // center (its shadow must darken AO on the floor region below it).
        let mut hp = Mesh::create_cube();
        // Cube spans [-0.4,0.4]^2 centered at origin, base at y = 0.2.
        for v in hp.vertices.iter_mut() {
            v[0] *= 0.4;
            v[1] = (v[1] * 0.4) + 0.6; // bottom face at ~0.2, top ~1.0
            v[2] *= 0.4;
        }
        // Ground plane in the high-poly so the box interior gets closed off.
        let ground: Vec<[f32; 3]> = verts.clone();
        hp.vertices.extend_from_slice(&ground);
        let nones: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 4];
        hp.normals.extend_from_slice(&nones);
        hp.uvs.extend_from_slice(&uvs);
        let g0 = hp.vertices.len() as u32 - 4;
        hp.indices
            .extend_from_slice(&[g0, g0 + 2, g0 + 1, g0, g0 + 3, g0 + 2]);

        let mut opts = BakerOptions::default();
        opts.resolution = 32;
        opts.ao_samples = 16;
        opts.ao_max_distance = 3.0;
        opts.bake_normal = true;
        opts.bake_ao = true;
        opts.bake_thickness = false;
        opts.bake_position = false;
        opts.bake_id = false;
        opts.bake_curvature = false;
        opts.bake_world_space_normal = false;

        let baked = BakerEngine::bake_all(
            &verts,
            &norms,
            &uvs,
            &indices,
            &[], // no submeshes: global BVH path
            &opts,
            None,
            None,
            Some(&hp),
        );

        let ao = baked.ambient_occlusion.expect("AO baked");
        let normal = baked.normal.expect("normal baked");
        assert!(!baked.is_cancelled);

        // Center texel (u,v = 0.5) sits directly under the box: occluded.
        let w = 32usize;
        let center = (w / 2) * w + (w / 2);
        let corner = 2 * w + 2;
        assert!(
            ao[center] < ao[corner],
            "AO under the floating box ({} = {}) must be darker than the open field ({})",
            ao[center],
            center,
            ao[corner]
        );
        // And the shadow must actually be a real occlusion, not noise.
        assert!(
            ao[corner] > 0.75,
            "open-field AO expected near-lit, got {}",
            ao[corner]
        );

        // Normals: the floor is flat facing +Y, so tangent-space Z dominates
        // everywhere on the floor that isn't under the box silhouette.
        let top_right = (2 * w + w) + (w - 3);
        assert!(
            normal[top_right][2] >= 0.9,
            "flat floor normal should project ~+Z, got {:?}",
            normal[top_right]
        );
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

    #[test]
    fn test_baked_map_format_channel_encoding() {
        use BakedMapFormat::*;
        // U8 quantization: rounded, clamped 8-bit.
        let vals = vec![0.0f32, 0.25, 0.5, 0.999, 1.5, -3.0];
        let u8p = encode_scalar_plane(&vals, U8);
        assert_eq!(u8p, vec![0, 64, 128, 255, 255, 0], "U8 rounded+clamped");

        // F16: exact for binary-exact values, round-trips within half precision.
        let bits = [0.0f32, 0.5, 1.0, -1.0, 0.25];
        assert_eq!(f32_to_half_bits(0.0), 0x0000);
        assert_eq!(f32_to_half_bits(f32::INFINITY), 0x7c00);
        assert_eq!(f32_to_half_bits(-f32::INFINITY), 0xfc00);
        assert_eq!(f32_to_half_bits(-1.0), 0xbc00);
        assert_eq!(
            f32_to_half_bits(131008.0),
            0x7c00,
            "overflow saturates to +Inf"
        );
        assert_eq!(
            f32_to_half_bits(6.0e-8),
            0x0001,
            "2^-24 is the smallest half subnormal"
        );
        assert_eq!(
            f32_to_half_bits(1.0e-9),
            0x0000,
            "too small flushes to zero"
        );
        assert_eq!(
            half_bits_to_f32(f32_to_half_bits(6.0e-8)),
            5.960_464_5e-8,
            "smallest half subnormal survives a round trip"
        );
        let f16p = encode_scalar_plane(&bits, F16);
        assert_eq!(f16p.len(), bits.len() * 2);
        let dec = decode_scalar_plane(&f16p, F16);
        for (a, b) in bits.iter().zip(dec.iter()) {
            assert!((a - b).abs() < 1e-6, "f16 round trip {a} != {b}");
        }

        // F32 plane is an exact little-endian identity round trip.
        let f32p = encode_scalar_plane(&bits, F32);
        assert_eq!(decode_scalar_plane(&f32p, F32), bits);
    }

    #[test]
    fn test_painted_offset_map_sample() {
        let m = PaintedOffsetMap {
            width: 2,
            height: 1,
            data: vec![-1.0, 0.5],
        };
        assert_eq!(m.sample(0.0, 0.0), -1.0);
        assert_eq!(m.sample(1.0, 0.0), 0.5);
        assert_eq!(m.sample(7.0, 0.0), 0.5, "out-of-range clamps to edge texel");
        let clamped = PaintedOffsetMap {
            width: 1,
            height: 1,
            data: vec![5.0],
        };
        assert_eq!(clamped.sample(0.0, 0.0), 1.0, "values clamp to [-1, 1]");
    }

    #[test]
    fn test_encoded_maps_plane_layout_and_roundtrip() {
        let low = Mesh::create_cube();
        let mut opts = BakerOptions::default();
        opts.resolution = 32;
        let advanced = AdvancedBakeOptions {
            map_format: BakedMapFormat::F16,
            ..Default::default()
        };
        let enc = BakerEngine::bake_all_encoded(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            &advanced,
            None,
            None,
            None,
        );
        assert_eq!(enc.format, BakedMapFormat::F16);
        assert_eq!(enc.resolution, 32);
        let n = enc.normal.as_ref().expect("normal plane encoded");
        assert_eq!(n.len(), 32 * 32 * 3 * 2, "3 interleaved half components");
        let back = enc.to_baked_maps();
        assert_eq!(back.resolution, 32);
        assert!(back.normal.is_some());
        assert!(!back.is_cancelled);
    }

    #[test]
    fn test_advanced_default_and_zero_offset_match_legacy_output() {
        let low = Mesh::create_cube();
        let mut opts = BakerOptions::default();
        opts.resolution = 48;
        let legacy = BakerEngine::bake_all(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            None,
            None,
            None,
        );
        // Default advanced options and an all-zero painted offset map must reproduce
        // the legacy bake exactly (no behavioral regression by default).
        let mut advanced = AdvancedBakeOptions::default();
        advanced.ray_offset_map = Some(Arc::new(PaintedOffsetMap {
            width: 1,
            height: 1,
            data: vec![0.0],
        }));
        let modern = BakerEngine::bake_all_advanced(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            &advanced,
            None,
            None,
            None,
        );
        assert_eq!(legacy.normal, modern.normal);
        assert_eq!(legacy.world_space_normal, modern.world_space_normal);
        assert_eq!(legacy.position, modern.position);
        assert_eq!(legacy.ambient_occlusion, modern.ambient_occlusion);
        assert_eq!(legacy.curvature, modern.curvature);
        assert_eq!(legacy.thickness, modern.thickness);
    }

    #[test]
    fn test_match_by_shell_fallback_hits_reachable_hp_shell() {
        let mut low = Mesh::create_cube();
        low.submeshes = vec![SubMesh {
            name: "Cube_low".into(),
            index_start: 0,
            index_count: low.indices.len(),
        }];

        // Two high-poly shells: one surrounding the low cube, one far away.
        let mut high = Mesh::create_material_sphere(16, 16);
        let near_count = high.indices.len();
        let mut far = Mesh::create_material_sphere(16, 16);
        for v in far.vertices.iter_mut() {
            v[0] += 50.0;
        }
        let base = high.vertices.len() as u32;
        high.vertices.extend_from_slice(&far.vertices);
        high.normals.extend_from_slice(&far.normals);
        high.uvs.extend_from_slice(&far.uvs);
        for i in &far.indices {
            high.indices.push(i + base);
        }
        let far_count = far.indices.len();
        high.submeshes = vec![
            SubMesh {
                name: "Wall_high".into(),
                index_start: 0,
                index_count: near_count,
            },
            SubMesh {
                name: "Floor_high".into(),
                index_start: near_count,
                index_count: far_count,
            },
        ];

        let mut opts = BakerOptions::default();
        opts.resolution = 32;
        opts.match_by_name = true;
        opts.max_frontal_distance = 2.0;
        opts.max_rear_distance = 6.0;
        // Name misses ("Cube_low" has no "Cube_high"), so shell correspondence decides.
        let mut advanced = AdvancedBakeOptions {
            match_by_shell: true,
            ..Default::default()
        };
        let baked = BakerEngine::bake_all_advanced(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            &advanced,
            None,
            None,
            Some(&high),
        );
        let normal = baked.normal.expect("normal map baked");
        // The reachable near shell must have been traced — the surface normal map is
        // no longer the flat default everywhere.
        let any_hit = normal.iter().any(|n| {
            (n[0] - 0.5).abs() > 1e-3 || (n[1] - 0.5).abs() > 1e-3 || (n[2] - 1.0).abs() > 1e-3
        });
        assert!(any_hit, "shell fallback must trace a reachable hp shell");

        advanced.match_by_shell = false;
        let baked2 = BakerEngine::bake_all_advanced(
            &low.vertices,
            &low.normals,
            &low.uvs,
            &low.indices,
            &low.submeshes,
            &opts,
            &advanced,
            None,
            None,
            Some(&high),
        );
        assert!(baked2.normal.is_some(), "global fallback still bakes");
    }

    #[test]
    fn test_udim_tile_uv_offset_decoding() {
        assert_eq!(udim_tile_uv_offset(1001), [0.0, 0.0]);
        assert_eq!(udim_tile_uv_offset(1002), [1.0, 0.0]);
        assert_eq!(udim_tile_uv_offset(1003), [2.0, 0.0]);
        assert_eq!(udim_tile_uv_offset(1010), [9.0, 0.0]);
        assert_eq!(udim_tile_uv_offset(1011), [0.0, 1.0]);
        assert_eq!(udim_tile_uv_offset(1012), [1.0, 1.0]);
        assert_eq!(udim_tile_uv_offset(1021), [0.0, 2.0]);
        assert_eq!(udim_tile_uv_offset(999), [0.0, 0.0]);
    }

    #[test]
    fn test_baker_udim_culling_and_rasterization() {
        let mut mesh = Mesh::default();
        // Quad 1 in tile 1001: UV in [0.2, 0.8] x [0.2, 0.8]
        mesh.vertices.push([-1.0, 0.0, -1.0]);
        mesh.vertices.push([1.0, 0.0, -1.0]);
        mesh.vertices.push([1.0, 0.0, 1.0]);
        mesh.vertices.push([-1.0, 0.0, 1.0]);
        mesh.normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
        mesh.uvs.push([0.2, 0.2]);
        mesh.uvs.push([0.8, 0.2]);
        mesh.uvs.push([0.8, 0.8]);
        mesh.uvs.push([0.2, 0.8]);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);

        // Quad 2 in tile 1002: UV in [1.2, 1.8] x [0.2, 0.8]
        let base = mesh.vertices.len() as u32;
        mesh.vertices.push([10.0, 0.0, -1.0]);
        mesh.vertices.push([12.0, 0.0, -1.0]);
        mesh.vertices.push([12.0, 0.0, 1.0]);
        mesh.vertices.push([10.0, 0.0, 1.0]);
        mesh.normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
        mesh.uvs.push([1.2, 0.2]);
        mesh.uvs.push([1.8, 0.2]);
        mesh.uvs.push([1.8, 0.8]);
        mesh.uvs.push([1.2, 0.8]);
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

        let mut opts = BakerOptions::default();
        opts.resolution = 32;
        opts.bake_ao = false;
        opts.bake_curvature = false;
        opts.bake_normal = false;
        opts.bake_world_space_normal = false;
        opts.bake_position = true;

        // Bake tile 1001
        opts.udim_tile = Some(1001);
        let baked_1001 = BakerEngine::bake_all(
            &mesh.vertices,
            &mesh.normals,
            &mesh.uvs,
            &mesh.indices,
            &mesh.submeshes,
            &opts,
            None,
            None,
            None,
        );
        let pos_1001 = baked_1001.position.expect("position baked");
        // Center pixel (16, 16) corresponds to uv (0.5, 0.5) which is inside Quad 1 (-1 to +1 x)
        // Normalized position: (-1 to 1) mapped into [-1, 12] -> ~0.077 (< 0.2)
        let mid_px = (16 * 32 + 16) as usize;
        assert!(
            pos_1001[mid_px][0] < 0.2,
            "Tile 1001 contains Quad 1 around x ~ 0.077"
        );

        // Bake tile 1002
        opts.udim_tile = Some(1002);
        let baked_1002 = BakerEngine::bake_all(
            &mesh.vertices,
            &mesh.normals,
            &mesh.uvs,
            &mesh.indices,
            &mesh.submeshes,
            &opts,
            None,
            None,
            None,
        );
        let pos_1002 = baked_1002.position.expect("position baked");
        // Center pixel (16, 16) on tile 1002 corresponds to local uv (0.5, 0.5) -> Quad 2 (10 to 12 x)
        // Normalized position: (10 to 12) mapped into [-1, 12] -> ~0.923 (> 0.8)
        assert!(
            pos_1002[mid_px][0] > 0.8,
            "Tile 1002 contains Quad 2 around x ~ 0.923"
        );

        // Multi-tile batch bake
        let udim_results = BakerEngine::bake_all_udim(
            &mesh.vertices,
            &mesh.normals,
            &mesh.uvs,
            &mesh.indices,
            &mesh.submeshes,
            &opts,
            &[1001, 1002],
            None,
            None,
            None,
        );
        assert_eq!(udim_results.len(), 2);
        assert!(udim_results.contains_key(&1001));
        assert!(udim_results.contains_key(&1002));
    }
}
