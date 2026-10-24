//! Authentic reference PBR suite starter-asset library integration.
//!
//! The precomputed cache (produced by `scripts/import_material_library.py`) lives at
//! `assets/material_cache/<group>/<ItemName>/` and each item dir carries `meta.json`
//! (label, sp_category, keywords, graph_type, author, icon_kind), `params.json`
//! (curated preset defaults), `thumbnail.png` (curated icon; `icon_kind`
//! tells whether it is a sphere render or a flat image) and — for materials only —
//! `fill_map.png` (a flat, UV-aligned base color map).
//!
//! Golden rule: sphere-render bitmaps (icons) are NEVER used as paint content. Fill
//! layers are materialized from `fill_map.png`, a name-matched legacy
//! `assets/materials` texture set, or plain preset parameter values.

#![allow(dead_code)]

use super::brush::AlphaMaskData;
use super::layer::Layer;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Groups present in the reference cache directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialGroup {
    Materials,
    SmartMaterials,
    Effects,
    Generators,
    Procedurals,
    Alphas,
}

impl MaterialGroup {
    /// Sub-directory name inside `material_cache/`.
    pub fn dir_name(self) -> &'static str {
        match self {
            MaterialGroup::Materials => "materials",
            MaterialGroup::SmartMaterials => "smart_materials",
            MaterialGroup::Effects => "effects",
            MaterialGroup::Generators => "generators",
            MaterialGroup::Procedurals => "procedurals",
            MaterialGroup::Alphas => "alphas",
        }
    }
}

/// All cache groups that feed the shelf (scan order matters for dedupe priority).
pub const ALL_GROUPS: &[MaterialGroup] = &[
    MaterialGroup::Materials,
    MaterialGroup::SmartMaterials,
    MaterialGroup::Effects,
    MaterialGroup::Generators,
    MaterialGroup::Procedurals,
    MaterialGroup::Alphas,
];

/// Whether the SP-provided thumbnail is a sphere render or a flat image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Sphere,
    Flat,
}

impl IconKind {
    pub fn parse(value: &str) -> IconKind {
        if value.eq_ignore_ascii_case("sphere") {
            IconKind::Sphere
        } else {
            IconKind::Flat
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct referenceMeta {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub sp_category: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(default)]
    pub graph_type: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub icon_kind: String,
}

/// One scanned item of the reference cache.
#[derive(Debug, Clone)]
pub struct MaterialItem {
    /// Display name from `meta.json.label` (falls back to the dir name).
    pub label: String,
    /// Directory `.../material_cache/<group>/<ItemName>`.
    pub dir: PathBuf,
    pub group: MaterialGroup,
    /// `thumbnail.png` when present, else empty (UI falls back to a color swatch).
    pub thumb_path: PathBuf,
    pub icon_kind: IconKind,
    pub sp_category: String,
    pub keywords: String,
}

impl MaterialItem {
    /// Label + keywords + SP category, lowercased — the shelf search haystack.
    pub fn search_text(&self) -> String {
        format!(
            "{} {} {}",
            self.label.to_lowercase(),
            self.keywords.to_lowercase(),
            self.sp_category.to_lowercase()
        )
    }
}

/// Resolves the material cache directory, if the user has imported one.
pub fn find_material_cache_dir() -> PathBuf {
    crate::paths::find_assets_subdir("material_cache").unwrap_or_default()
}

/// Resolves the app's bundled font directory, if present.
pub fn find_fonts_dir() -> Option<PathBuf> {
    crate::paths::find_assets_subdir("fonts/bundled")
}

fn load_meta(dir: &Path, dir_name: &str) -> (String, String, String, IconKind) {
    match std::fs::read_to_string(dir.join("meta.json")) {
        Ok(raw) => match serde_json::from_str::<referenceMeta>(&raw) {
            Ok(m) => {
                let label = if m.label.trim().is_empty() {
                    dir_name.to_owned()
                } else {
                    m.label
                };
                (
                    label,
                    m.sp_category,
                    m.keywords,
                    IconKind::parse(&m.icon_kind),
                )
            }
            Err(_) => (
                dir_name.to_owned(),
                String::new(),
                String::new(),
                IconKind::Flat,
            ),
        },
        Err(_) => (
            dir_name.to_owned(),
            String::new(),
            String::new(),
            IconKind::Flat,
        ),
    }
}

/// Scans `base/<group>/<ItemName>/` for one group. Pure function of `base` (testable).
pub fn scan_material_group(base: &Path, group: MaterialGroup) -> Vec<MaterialItem> {
    let mut items = Vec::new();
    let group_dir = base.join(group.dir_name());
    let Ok(entries) = std::fs::read_dir(&group_dir) else {
        return items;
    };
    let mut list: Vec<_> = entries.flatten().collect();
    list.sort_by_key(|e| e.file_name());
    for entry in list {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if dir_name.starts_with('.') {
            continue;
        }
        let (label, sp_category, keywords, icon_kind) = load_meta(&p, &dir_name);
        let thumb_path = p.join("thumbnail.png");
        items.push(MaterialItem {
            label,
            dir: p,
            group,
            thumb_path: if thumb_path.exists() {
                thumb_path
            } else {
                PathBuf::new()
            },
            icon_kind,
            sp_category,
            keywords,
        });
    }
    items
}

/// Scans every cache group. Pure function of the base dir (testable).
pub fn scan_material_cache(base: &Path) -> Vec<MaterialItem> {
    let mut items = Vec::new();
    for group in ALL_GROUPS {
        items.extend(scan_material_group(base, *group));
    }
    items
}

// ---------------------------------------------------------------------------
// params.json: curated preset defaults → material channel values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum PresetValue {
    Scalar(f64),
    /// r, g, b (optionally a) normalized color array.
    Color(Vec<f64>),
    Text(String),
    Other,
}

/// Numeric/channel parameter values extracted from an SP preset (the FIRST preset).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaterialPresetParams {
    pub preset_label: Option<String>,
    /// Every numeric input of the first preset (identifier → value).
    pub inputs: Vec<(String, PresetValue)>,
    /// First scalar input whose identifier contains "rough".
    pub roughness: Option<f32>,
    /// First scalar input whose identifier contains "metal".
    pub metallic: Option<f32>,
    /// First color input whose identifier contains "color"/"colour"/"_col".
    pub color: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct referenceParamsFile {
    #[serde(default)]
    presets: Vec<referencePreset>,
}

#[derive(Deserialize)]
struct referencePreset {
    #[serde(default)]
    label: String,
    #[serde(default)]
    inputs: Vec<referenceInput>,
}

#[derive(Deserialize)]
struct referenceInput {
    #[serde(default)]
    identifier: String,
    #[serde(default)]
    value: serde_json::Value,
}

impl From<serde_json::Value> for PresetValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Number(n) => PresetValue::Scalar(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::Array(items) => {
                if items.iter().all(|i| i.is_number()) {
                    PresetValue::Color(
                        items
                            .into_iter()
                            .filter_map(|i| i.as_f64())
                            .collect::<Vec<f64>>(),
                    )
                } else if let Some(s) = items.first().and_then(|i| i.as_str()) {
                    PresetValue::Text(s.to_owned())
                } else {
                    PresetValue::Other
                }
            }
            serde_json::Value::String(s) => PresetValue::Text(s),
            _ => PresetValue::Other,
        }
    }
}

/// Parses curated preset JSON into [`MaterialPresetParams`] (first preset only).
pub fn parse_params_json(raw: &str) -> Result<MaterialPresetParams, String> {
    let file: referenceParamsFile =
        serde_json::from_str(raw).map_err(|e| format!("params.json parse failed: {e}"))?;
    let Some(preset) = file.presets.first() else {
        return Ok(MaterialPresetParams::default());
    };

    let mut out = MaterialPresetParams {
        preset_label: if preset.label.is_empty() {
            None
        } else {
            Some(preset.label.clone())
        },
        ..Default::default()
    };

    for input in &preset.inputs {
        let value = PresetValue::from(input.value.clone());
        let ident_lower = input.identifier.to_lowercase();
        match &value {
            PresetValue::Scalar(_) | PresetValue::Color(_) => {
                out.inputs.push((input.identifier.clone(), value.clone()));
            }
            _ => continue,
        }
        if out.roughness.is_none()
            && matches!(value, PresetValue::Scalar(_))
            && ident_lower.contains("rough")
        {
            out.roughness = scalar_f32(&value);
        }
        if out.metallic.is_none()
            && matches!(value, PresetValue::Scalar(_))
            && ident_lower.contains("metal")
        {
            out.metallic = scalar_f32(&value);
        }
        if out.color.is_none()
            && matches!(value, PresetValue::Color(ref c) if c.len() >= 3)
            && (ident_lower.contains("color")
                || ident_lower.contains("colour")
                || ident_lower.contains("_col"))
        {
            let c = match &value {
                PresetValue::Color(c) => c.clone(),
                _ => unreachable!(),
            };
            out.color = Some([c[0] as f32, c[1] as f32, c[2] as f32]);
        }
    }
    Ok(out)
}

fn scalar_f32(v: &PresetValue) -> Option<f32> {
    match v {
        PresetValue::Scalar(n) => Some((*n) as f32),
        _ => None,
    }
}

fn scalar_f64(v: &PresetValue) -> Option<f64> {
    match v {
        PresetValue::Scalar(n) => Some(*n),
        _ => None,
    }
}

/// Loads and parses `params.json` from an item dir; empty result when absent.
pub fn load_preset_params(dir: &Path) -> MaterialPresetParams {
    std::fs::read_to_string(dir.join("params.json"))
        .ok()
        .and_then(|raw| parse_params_json(&raw).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// SP-style fill-layer instantiation
// ---------------------------------------------------------------------------

/// What paint content a materialized fill layer received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum referenceContentKind {
    /// Flat UV-aligned `fill_map.png` (materials; honest base-color parity).
    FillMapBitmap,
    /// Name-matched legacy `assets/materials` texture set (smart materials).
    LegacyTextureSet,
    /// No bitmap content available: plain flat fill from authentic preset values.
    FlatParams,
}

#[derive(Debug, Clone, PartialEq)]
pub enum referenceApplyError {
    /// The only available bitmap is a sphere-render icon — it must never be
    /// painted as content (SP icon, not a texture).
    ForbiddenSphereBitmap,
    Io(String),
}

/// Neutral SP defaults used when params.json provides no numbers.
pub const DEFAULT_FILL_ROUGHNESS: f32 = 0.5;
pub const DEFAULT_FILL_METALLIC: f32 = 0.0;
pub const DEFAULT_FILL_COLOR: [f32; 4] = [0.7, 0.7, 0.7, 1.0];

/// Materializes an SP-style Fill layer from a material_cache item directory:
/// one new fill layer named after the material label, base color content from
/// `fill_map.png` (materials) or a name-matched legacy `assets/materials` set
/// (smart materials), and roughness/metallic values from the authentic preset.
/// Returns the layer plus which content kind was actually applied.
pub fn build_fill_layer(
    item_dir: &Path,
    id: usize,
    width: u32,
    height: u32,
) -> Result<(Layer, referenceContentKind), referenceApplyError> {
    let dir_name = item_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "SP Material".to_owned());
    let (label, _sp, _kw, icon_kind) = load_meta(item_dir, &dir_name);
    let params = load_preset_params(item_dir);

    let color = params
        .color
        .map(|c| {
            [
                c[0].clamp(0.0, 1.0),
                c[1].clamp(0.0, 1.0),
                c[2].clamp(0.0, 1.0),
                1.0,
            ]
        })
        .unwrap_or(DEFAULT_FILL_COLOR);
    let roughness = params.roughness.unwrap_or(DEFAULT_FILL_ROUGHNESS);
    let metallic = params.metallic.unwrap_or(DEFAULT_FILL_METALLIC);

    let mut layer = Layer::new_fill(
        id,
        &label,
        color,
        metallic.clamp(0.0, 1.0),
        roughness.clamp(0.0, 1.0),
        width,
        height,
    );

    // 1. Materials ship a flat UV-aligned fill map — the honest SP base-color parity.
    let fill_map = item_dir.join("fill_map.png");
    if fill_map.exists() {
        let img = image::open(&fill_map)
            .map_err(|e| referenceApplyError::Io(format!("fill_map.png: {e}")))?
            .to_rgba8();
        bake_bitmap_into_color(&mut layer, &img);
        return Ok((layer, referenceContentKind::FillMapBitmap));
    }

    // 2. Smart materials: name-matched legacy assets/materials texture set.
    if let Some(set_dir) = find_legacy_material_dir(&label) {
        let set = super::texture_importer::PbrTextureSet::from_folder(&set_dir);
        if let Some(ref base_color) = set.base_color {
            if let Ok(img) = image::open(base_color) {
                let rgba = img.to_rgba8();
                if looks_like_sphere_render(&rgba) {
                    // Never paint the sphere-render preview as content.
                } else {
                    let _ = set.load_into_layer(&mut layer, false);
                    // Re-assert authentic preset channel values where the legacy
                    // set itself does not define a roughness/metallic map.
                    let has_rough = set.roughness.is_some();
                    let has_metal = set.metallic.is_some();
                    if !has_rough {
                        layer
                            .roughness_buffer
                            .iter_mut()
                            .for_each(|v| *v = layer.fill_roughness);
                    }
                    if !has_metal {
                        layer
                            .metallic_buffer
                            .iter_mut()
                            .for_each(|v| *v = layer.fill_metallic);
                    }
                    return Ok((layer, referenceContentKind::LegacyTextureSet));
                }
            }
        }
    }

    // 3. If the only bitmaps we had were sphere-render icons, warn via the error
    // channel semantics — but still produce a usable parameter-only fill.
    let _ = icon_kind; // sphere icons are never sources of paint content here

    Ok((layer, referenceContentKind::FlatParams))
}

/// Case-insensitive exact name match within the legacy `assets/materials` dir.
fn find_legacy_material_dir(label: &str) -> Option<PathBuf> {
    let mat_dir = super::material_gen::find_materials_dir();
    let exact = mat_dir.join(label);
    if exact.is_dir() {
        return Some(exact);
    }
    let lower = label.to_lowercase();
    if let Ok(entries) = std::fs::read_dir(&mat_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') && name.to_lowercase() == lower && entry.path().is_dir() {
                return Some(entry.path());
            }
        }
    }
    None
}

/// Heuristic detector for SP sphere-render previews: a big neutral disc on a
/// plain neutral background (roughly uniform, high-variance center). Used ONLY
/// to reject legacy preview bitmaps as paint content.
fn looks_like_sphere_render(img: &image::RgbaImage) -> bool {
    let (w, h) = img.dimensions();
    if w < 64 || h < 64 {
        return false;
    }
    let at = |x: i64, y: i64| -> image::Rgba<u8> {
        let cx = x.clamp(0, w as i64 - 1) as u32;
        let cy = y.clamp(0, h as i64 - 1) as u32;
        *img.get_pixel(cx, cy)
    };
    let spread_of = |p: image::Rgba<u8>| -> f64 {
        (p.0[0].max(p.0[1]).max(p.0[2]) - p.0[0].min(p.0[1]).min(p.0[2])) as f64 / 255.0
    };
    let m = ((w.min(h)) / 32).clamp(1, 16) as i64;

    let mut border_lum_sum = 0.0f64;
    let mut border_lum_sq = 0.0f64;
    let mut border_n = 0u64;
    let mut border_channel_spread = 0.0f64;
    for p in 0..m {
        for x in 0..w as i64 {
            let px = at(x, p);
            border_lum_sum += luminance(&px);
            border_lum_sq += luminance(&px) * luminance(&px);
            border_n += 1;
            border_channel_spread = border_channel_spread.max(spread_of(px));
            let px2 = at(x, h as i64 - 1 - p);
            border_lum_sum += luminance(&px2);
            border_lum_sq += luminance(&px2) * luminance(&px2);
            border_n += 1;
            border_channel_spread = border_channel_spread.max(spread_of(px2));
        }
        for y in 0..h as i64 {
            let px = at(p, y);
            border_lum_sum += luminance(&px);
            border_lum_sq += luminance(&px) * luminance(&px);
            border_n += 1;
            border_channel_spread = border_channel_spread.max(spread_of(px));
            let px2 = at(w as i64 - 1 - p, y);
            border_lum_sum += luminance(&px2);
            border_lum_sq += luminance(&px2) * luminance(&px2);
            border_n += 1;
            border_channel_spread = border_channel_spread.max(spread_of(px2));
        }
    }

    let mean = border_lum_sum / border_n.max(1) as f64;
    let var = (border_lum_sq / border_n.max(1) as f64) - mean * mean;
    let stddev = var.max(0.0).sqrt();
    let uniform_border = stddev < 0.05 && border_channel_spread < 0.06;

    // Center vs border contrast: sphere renders put the subject away from the frame.
    let mut center_lum_sum = 0.0f64;
    let mut center_n = 0u64;
    let band = m.max(4);
    for dy in -band..=band {
        for dx in -band..=band {
            let px = at(w as i64 / 2 + dx, h as i64 / 2 + dy);
            center_lum_sum += luminance(&px);
            center_n += 1;
        }
    }
    let center_mean = center_lum_sum / center_n.max(1) as f64;

    uniform_border && (center_mean - mean).abs() > 0.12
}

#[inline]
fn luminance(p: &image::Rgba<u8>) -> f64 {
    (0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64) / 255.0
}

/// Nearest-sample bake of a UV-aligned bitmap into the layer's base color buffer.
/// Keeps exact pixel values (no resampling blending) so materials land 1:1.
fn bake_bitmap_into_color(layer: &mut Layer, img: &image::RgbaImage) {
    let (iw, ih) = img.dimensions();
    let (w, h) = (layer.width, layer.height);
    let w_d = w.max(1);
    let h_d = h.max(1);
    for y in 0..h {
        let sy = (y * ih / h_d).min(ih - 1);
        for x in 0..w {
            let sx = (x * iw / w_d).min(iw - 1);
            let p = img.get_pixel(sx, sy);
            let idx = (y * w + x) as usize;
            layer.color_buffer[idx] = [
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
                1.0,
            ];
        }
    }
    // Fill settings mirror the applied content as a fallback (fill-layer semantics).
    let mut acc = [0.0f64; 3];
    let mut n = 0u64;
    let step = ((iw.max(h) as u64).pow(2) / 4096).max(1);
    for (i, p) in img.pixels().enumerate() {
        if i % step as usize == 0 {
            acc[0] += p[0] as f64;
            acc[1] += p[1] as f64;
            acc[2] += p[2] as f64;
            n += 1;
        }
    }
    let avg = [
        acc[0] / (255.0 * n.max(1) as f64),
        acc[1] / (255.0 * n.max(1) as f64),
        acc[2] / (255.0 * n.max(1) as f64),
    ];
    layer.fill_color = [avg[0] as f32, avg[1] as f32, avg[2] as f32, 1.0];
}

// ---------------------------------------------------------------------------
// Alpha masks from shelf items
// ---------------------------------------------------------------------------

/// Builds a brush alpha mask from an SP cache item's flat thumbnail.
/// Sphere-render icons are rejected: [`referenceApplyError::ForbiddenSphereBitmap`].
pub fn build_alpha_mask(item_dir: &Path) -> Result<AlphaMaskData, referenceApplyError> {
    let dir_name = item_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let (_label, _sp, _kw, icon_kind) = load_meta(item_dir, &dir_name);
    if icon_kind == IconKind::Sphere {
        return Err(referenceApplyError::ForbiddenSphereBitmap);
    }
    let thumb = item_dir.join("thumbnail.png");
    if !thumb.exists() {
        return Err(referenceApplyError::Io("thumbnail.png missing".to_owned()));
    }
    let img =
        image::open(&thumb).map_err(|e| referenceApplyError::Io(format!("thumbnail.png: {e}")))?;
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    Ok(AlphaMaskData {
        width: w,
        height: h,
        data: gray.into_raw(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32, fill: [u8; 4]) {
        let img = image::RgbaImage::from_fn(w, h, |x, _y| {
            if x == 0 {
                // first column deliberately different for pixel assertions
                [10u8, 250, 30, 255].into()
            } else {
                fill.into()
            }
        });
        img.save(path).expect("save test png");
    }

    fn temp_item(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("remoainter_subst_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_material_cache_finds_all_six_groups() {
        let base = find_material_cache_dir();
        let counts = |g: MaterialGroup| scan_material_group(&base, g).len();
        assert!(
            counts(MaterialGroup::Materials) >= 90,
            "materials {}",
            counts(MaterialGroup::Materials)
        );
        assert!(
            counts(MaterialGroup::SmartMaterials) >= 110,
            "smart {}",
            counts(MaterialGroup::SmartMaterials)
        );
        assert!(
            counts(MaterialGroup::Alphas) >= 400,
            "alphas {}",
            counts(MaterialGroup::Alphas)
        );
        assert!(
            counts(MaterialGroup::Effects) >= 50,
            "effects {}",
            counts(MaterialGroup::Effects)
        );
        assert!(
            counts(MaterialGroup::Procedurals) >= 200,
            "procedurals {}",
            counts(MaterialGroup::Procedurals)
        );
        assert!(
            counts(MaterialGroup::Generators) >= 15,
            "generators {}",
            counts(MaterialGroup::Generators)
        );

        // One real item must expose label + flat/sphere icon classification.
        let mats = scan_material_group(&base, MaterialGroup::Materials);
        let clay = mats
            .iter()
            .find(|i| i.label == "Clay Earthenware")
            .expect("clay item");
        assert!(!clay.sp_category.is_empty());
        assert!(clay.icon_kind == IconKind::Sphere);
        assert!(clay.search_text().contains("ceramic"));
    }

    #[test]
    fn scan_group_is_empty_for_missing_dir() {
        let tmp = std::env::temp_dir().join("remoainter_subst_nomiss");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(scan_material_group(&tmp, MaterialGroup::Alphas).is_empty());
    }

    #[test]
    fn clay_params_parse_from_inline_fixture() {
        // The shipped material_cache was removed; parse behavior is covered with
        // an inline fixture so this no longer depends on imported vendor data.
        let raw = r#"{"presets":[{"label":"Clay","inputs":[{"identifier":"clay_color","type":2,"value":[0.633663,0.4,0.2],"uid":1},{"identifier":"clay_roughness","type":0,"value":0.53,"uid":2}]}]}"#;
        let params = parse_params_json(raw).expect("parse");
        let idents: Vec<&str> = params.inputs.iter().map(|(i, _)| i.as_str()).collect();
        assert!(idents.contains(&"clay_color"), "{idents:?}");
        assert!(idents.contains(&"clay_roughness"), "{idents:?}");
        assert_eq!(params.roughness, Some(0.53));
    }

    #[test]
    fn apply_fake_material_creates_fill_layer_with_expected_pixels() {
        let dir = temp_item("fake_mat");
        std::fs::write(
            dir.join("meta.json"),
            r#"{"label":"Test Fake Mat","sp_category":"Test","keywords":"fake","icon_kind":"flat"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("params.json"),
            r#"{"presets":[{"label":"P","inputs":[{"identifier":"preset_rough","type":0,"value":0.35,"uid":1},{"identifier":"preset_metal","type":0,"value":0.2,"uid":2}]}]}"#,
        )
        .unwrap();
        write_png(&dir.join("thumbnail.png"), 4, 4, [230, 230, 230, 255]);
        write_png(&dir.join("fill_map.png"), 8, 8, [0, 0, 255, 255]);

        let (layer, kind) = build_fill_layer(&dir, 7, 32, 32).expect("apply");
        assert_eq!(kind, referenceContentKind::FillMapBitmap);
        assert_eq!(layer.name, "Test Fake Mat");
        assert_eq!(layer.layer_type, crate::painting::layer::LayerType::Fill);
        // Pixel (0,0) comes from fill_map (0,0) — the deliberately distinct column.
        assert_eq!(
            layer.color_buffer[0],
            [10.0 / 255.0, 250.0 / 255.0, 30.0 / 255.0, 1.0]
        );
        // Authentic preset numbers land on the fill layer's channel controls.
        assert!((layer.fill_roughness - 0.35).abs() < 1e-6);
        assert!((layer.fill_metallic - 0.2).abs() < 1e-6);
        // Buffers are consistent with the fill settings.
        assert_eq!(layer.roughness_buffer[5], layer.fill_roughness);
        assert_eq!(layer.metallic_buffer[9], layer.fill_metallic);
    }

    #[test]
    fn apply_fake_material_without_bitmap_is_flat_params_fill() {
        let dir = temp_item("fake_smat");
        std::fs::write(
            dir.join("meta.json"),
            r#"{"label":"Test No Bitmap","icon_kind":"flat"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("params.json"),
            r#"{"presets":[{"label":"x","inputs":[{"identifier":"mat_rough","type":0,"value":0.8,"uid":3}]}]}"#,
        )
        .unwrap();
        write_png(&dir.join("thumbnail.png"), 4, 4, [128, 128, 128, 255]);

        let (layer, kind) = build_fill_layer(&dir, 3, 16, 16).expect("apply");
        assert_eq!(kind, referenceContentKind::FlatParams);
        assert_eq!(layer.name, "Test No Bitmap");
        assert!((layer.fill_roughness - 0.8).abs() < 1e-6);
        assert_eq!(layer.fill_metallic, DEFAULT_FILL_METALLIC);
        // Flat fill: uniform color buffer equal to the preset color/defaults.
        let c = layer.color_buffer[3];
        assert_eq!(c, layer.fill_color);
        assert_ne!([1.0, 0.0, 0.0], [c[0], c[1], c[2]]);
    }

    #[test]
    fn alpha_from_sphere_icon_is_rejected() {
        let dir = temp_item("fake_sphere");
        std::fs::write(
            dir.join("meta.json"),
            r#"{"label":"Sphere Thing","icon_kind":"sphere"}"#,
        )
        .unwrap();
        write_png(&dir.join("thumbnail.png"), 16, 16, [200, 200, 200, 255]);
        let err = build_alpha_mask(&dir).expect_err("sphere must not be a paint source");
        assert_eq!(err, referenceApplyError::ForbiddenSphereBitmap);

        // Missing thumbnail also errors gracefully.
        let dir2 = temp_item("fake_nothumb");
        std::fs::write(
            dir2.join("meta.json"),
            r#"{"label":"No Thumb","icon_kind":"flat"}"#,
        )
        .unwrap();
        assert!(matches!(
            build_alpha_mask(&dir2),
            Err(referenceApplyError::Io(_))
        ));
    }

    #[test]
    fn alpha_from_flat_thumbnail_is_usable() {
        let dir = temp_item("fake_alpha");
        std::fs::write(
            dir.join("meta.json"),
            r#"{"label":"Flat Alpha","icon_kind":"flat"}"#,
        )
        .unwrap();
        write_png(&dir.join("thumbnail.png"), 19, 7, [255, 255, 255, 255]);
        let alpha = build_alpha_mask(&dir).expect("flat alpha ok");
        assert_eq!(alpha.width, 19);
        assert_eq!(alpha.height, 7);
        assert_eq!(alpha.data.len(), (19 * 7) as usize);
        assert!(alpha.data.iter().any(|&v| v > 200));
        assert_ne!(alpha.data[0], 0, "column 0 is the bright test color");
    }

    #[test]
    fn smart_material_uses_legacy_texture_set_when_name_matched() {
        // Only asserts when the legacy assets/materials dir actually ships the item.
        let cache = find_material_cache_dir();
        let mat_dir = super::super::material_gen::find_materials_dir();
        let items = scan_material_group(&cache, MaterialGroup::SmartMaterials);
        let al = items.iter().find(|i| i.label == "Aluminium Anodized Red");
        if let (Some(item), true) = (al, mat_dir.join("Aluminium Anodized Red").is_dir()) {
            let (layer, kind) =
                build_fill_layer(&item.dir, 11, 64, 64).expect("apply smart material");
            assert_eq!(layer.name, "Aluminium Anodized Red");
            assert_eq!(kind, referenceContentKind::LegacyTextureSet);
            // The legacy texture set provides real pixel content (not a flat fill).
            assert!(layer.color_buffer.iter().any(|&c| c != layer.fill_color));
        } else {
            eprintln!("legacy Aluminium Anodized Red dir absent — smart material test skipped");
        }
    }

    #[test]
    fn params_json_malformed_is_an_error_not_panic() {
        assert!(parse_params_json("not json at all").is_err());
        // No presets → defaults.
        let ok = parse_params_json("{}").unwrap();
        assert_eq!(ok, MaterialPresetParams::default());
        assert_eq!(scalar_f32(&PresetValue::Scalar(12.5)), Some(12.5));
        assert_eq!(scalar_f64(&PresetValue::Scalar(12.5)), Some(12.5));
    }
}
