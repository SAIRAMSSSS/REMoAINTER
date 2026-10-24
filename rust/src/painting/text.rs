//! Text / decal rasterization for the Decal tool. reference PBR suite parity: real system
//! TTF/OTF fonts via `ab_glyph` (kerning, arbitrary Unicode, selectable font), with the
//! original embedded bitmap vector font kept as the internal fallback when a chosen font
//! fails to load or none is installed.

#![allow(dead_code)]

use super::brush::AlphaMaskData;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont};

/// 5-dot-wide x 7-dot-tall bitmap font for the printable ASCII subset used in decals/greeble
/// text. Each glyph is 7 u8 rows, bit 4 = leftmost column.
#[rustfmt::skip]
const FONT_5X7: &[(&str, [u8; 7])] = &[
    (" ",    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000]),
    ("!",    [0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100]),
    ("+",    [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000]),
    ("-",    [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000]),
    (".",    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100]),
    (":",    [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000]),
    ("_",    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111]),
    ("/",    [0b00001, 0b00010, 0b00100, 0b00100, 0b01000, 0b10000, 0b10000]),
    ("=",    [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000]),
    ("%",    [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011]),
    ("0",    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ("1",    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ("2",    [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111]),
    ("3",    [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110]),
    ("4",    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ("5",    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ("6",    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ("7",    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ("8",    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ("9",    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    ("A",    [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ("B",    [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ("C",    [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ("D",    [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
    ("E",    [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ("F",    [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ("G",    [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ("H",    [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ("I",    [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ("J",    [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ("K",    [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ("L",    [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ("M",    [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ("N",    [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001]),
    ("O",    [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ("P",    [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ("Q",    [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ("R",    [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ("S",    [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ("T",    [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ("U",    [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ("V",    [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ("W",    [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010]),
    ("X",    [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ("Y",    [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
    ("Z",    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
];

fn glyph(ch: char) -> Option<&'static [u8; 7]> {
    let upper = ch.to_ascii_uppercase();
    FONT_5X7
        .iter()
        .find(|(c, _)| c.starts_with(upper))
        .map(|(_, g)| g)
}

/// Render ASCII text (with `\n` line breaks) into a decal alpha mask (u8 coverage). The aspect
/// is padded 2:1 so it maps 1:1 into the stamp alpha U/V space used by `sample_brush_alpha`.
pub fn rasterize_text(text: &str, pixel_height: u32) -> AlphaMaskData {
    let pixel_height = pixel_height.clamp(16, 2048);
    let lines: Vec<&str> = text.split('\n').collect();
    let num_lines = lines.len().max(1);
    let line_cells = 8u32; // 7 glyph rows + 1 spacing row
    let dots_per_dot = ((pixel_height as f32) / (((num_lines as u32 * line_cells) + 1) as f32))
        .max(0.5)
        .floor()
        .max(1.0) as u32;

    let widest = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(1) as u32;
    let cell_w = dots_per_dot * 9; // 5 glyph + 4 spacing (breathing room inside the pad)
    let text_w = cell_w * widest;
    let width: u32 = (pixel_height * 2).max(text_w + cell_w).max(32);
    let offset_x = ((width as i64 - text_w as i64) / 2).max(0);
    let offset_y =
        ((pixel_height as i64 - (num_lines as u32 * line_cells * dots_per_dot) as i64) / 2).max(1);

    let mut data = vec![0u8; (width * pixel_height) as usize];
    for (li, line) in lines.iter().enumerate() {
        let mut cursor = offset_x;
        for ch in line.chars() {
            if let Some(g) = glyph(ch) {
                for (row, bits) in g.iter().enumerate() {
                    for col in 0..5u32 {
                        if bits & (1 << (4 - col)) != 0 {
                            let x0 = cursor + (col * dots_per_dot) as i64;
                            let y0 = offset_y
                                + ((li as u32 * line_cells + row as u32) * dots_per_dot) as i64;
                            for y in y0..(y0 + dots_per_dot as i64) {
                                if y >= pixel_height as i64 {
                                    continue;
                                }
                                for x in x0..(x0 + dots_per_dot as i64) {
                                    if x >= width as i64 {
                                        continue;
                                    }
                                    let idx = ((y as u32) * width + (x as u32)) as usize;
                                    if idx < data.len() {
                                        data[idx] = 255;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            cursor += cell_w as i64;
            if cursor >= width as i64 {
                break;
            }
        }
    }

    AlphaMaskData {
        width,
        height: pixel_height,
        data,
    }
}

/// Wrap text into multiple lines so long strings fit the decal frame.
pub fn rasterize_text_wrapped(
    text: &str,
    pixel_height: u32,
    max_chars_per_line: usize,
) -> AlphaMaskData {
    let max_chars_per_line = max_chars_per_line.max(2);
    let pixel_height = pixel_height.clamp(32, 2048);
    let lines: Vec<String> = text
        .split('\n')
        .flat_map(|l| {
            l.chars()
                .collect::<Vec<_>>()
                .chunks(max_chars_per_line)
                .map(|c| c.iter().collect::<String>())
                .collect::<Vec<_>>()
        })
        .collect();
    rasterize_text(&lines.join("\n"), pixel_height)
}

// ---------------------------------------------------------------------------
// System font discovery (standard Linux font directories)
// ---------------------------------------------------------------------------

/// Directory roots scanned for system fonts, in XDG-appropriate order.
const FONT_ROOTS: &[&str] = &["/usr/share/fonts", "/usr/local/share/fonts"];
/// Cap on discovered fonts so the dropdown stays usable.
const MAX_SYSTEM_FONTS: usize = 500;
/// Regular sans fonts preferred as the default decal font (exact basenames, lowercase).
const PREFERRED_FONT_BASENAMES: &[&str] = &[
    "dejavusans.ttf",
    "liberationsans-regular.ttf",
    "notosans-regular.ttf",
];
/// Substrings matched against the full path for the second preference tier.
const PREFERRED_FONT_SUBSTRINGS: &[&str] = &["dejavusans", "liberationsans", "notosans"];

fn is_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("ttf") | Some("otf") | Some("ttc") | Some("otc")
    )
}

fn collect_font_files(
    dir: &Path,
    out: &mut Vec<String>,
    depth: u8,
    visited: &mut HashSet<PathBuf>,
) {
    if depth > 8 {
        return;
    }
    let Ok(real) = dir.canonicalize() else {
        return;
    };
    if !visited.insert(real) {
        // symlink loop / already scoured
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files(&path, out, depth + 1, visited);
        } else if is_font_path(&path) {
            if let Some(s) = path.to_str() {
                out.push(s.to_owned());
            }
        }
    }
}

/// App-bundled authentic reference PBR suite font directory (TTF/OTF found via the
/// same candidate-path pattern as `find_materials_dir` / `find_shelf_dir`).
fn fonts_dir_dir() -> Option<PathBuf> {
    crate::painting::material_library::find_fonts_dir()
}

fn font_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            roots.push(PathBuf::from(xdg).join("fonts"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            roots.push(PathBuf::from(home).join(".local/share/fonts"));
        }
    }
    roots.extend(FONT_ROOTS.iter().map(PathBuf::from));
    roots.sort();
    roots.dedup();
    roots
}

/// All discoverable fonts (`*.ttf|otf|ttc|otc`, case-insensitive): the app's own
/// bundled reference PBR suite fonts first (never truncated away by the cap), then
/// standard Linux font directories. Sorted alphabetically within each tier and
/// capped so the dropdown stays usable. Cached for process lifetime.
pub fn system_fonts() -> &'static [String] {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut sp_fonts: Vec<String> = Vec::new();
        if let Some(app) = fonts_dir_dir() {
            if app.is_dir() {
                // Sharing `visited` across tiers is harmless: distinct roots.
                let mut visited = HashSet::new();
                collect_font_files(&app, &mut sp_fonts, 0, &mut visited);
            }
        }
        let mut system: Vec<String> = Vec::new();
        let mut visited = HashSet::new();
        for root in font_roots() {
            if root.is_dir() {
                collect_font_files(&root, &mut system, 0, &mut visited);
            }
        }
        system.sort();
        // The cap still applies in total; app fonts are guaranteed their slots.
        let room = MAX_SYSTEM_FONTS.saturating_sub(sp_fonts.len());
        system.truncate(room);
        let mut fonts = sp_fonts;
        fonts.sort();
        fonts.extend(system);
        fonts
    })
}

/// A sensible default font: DejaVu Sans / Liberation Sans / Noto Sans when present,
/// else the first discovered font. Deterministic for a given installed font set.
pub fn default_font_path() -> Option<String> {
    let fonts = system_fonts();
    // Tier 1: exact preferred basenames (case-insensitive).
    for pref in PREFERRED_FONT_BASENAMES {
        let hit = fonts.iter().find(|f| {
            Path::new(f)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|b| b.to_ascii_lowercase())
                .as_deref()
                == Some(*pref)
        });
        if let Some(p) = hit {
            return Some(p.clone());
        }
    }
    // Tier 2: any font matching a preferred family substring, regular weight favored.
    for pref in PREFERRED_FONT_SUBSTRINGS {
        let hit = fonts
            .iter()
            .find(|f| {
                let l = f.to_ascii_lowercase();
                l.contains(pref) && !l.contains("bold") && !l.contains("italic")
            })
            .or_else(|| fonts.iter().find(|f| f.to_ascii_lowercase().contains(pref)));
        if let Some(p) = hit {
            return Some(p.clone());
        }
    }
    fonts.first().cloned()
}

// ---------------------------------------------------------------------------
// `ab_glyph` rasterization engine (TTF/OTF, kerning, Unicode)
// ---------------------------------------------------------------------------

/// Parse + own a font by path, caching `FontArc`s so a chosen font is parsed once.
pub fn load_font_arc(path: &str) -> Result<FontArc, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, FontArc>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cached) = cache.lock() {
        if let Some(font) = cached.get(path) {
            return Ok(font.clone());
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Font read failed ({path}): {e}"))?;
    let font =
        FontArc::try_from_vec(bytes).map_err(|e| format!("Font parse failed ({path}): {e}"))?;
    if let Ok(mut cached) = cache.lock() {
        cached
            .entry(path.to_owned())
            .or_insert_with(|| font.clone());
    }
    Ok(font)
}

/// Horizontal advance width in px of one laid-out line (kerning included).
fn layout_line_width(font: &FontArc, em_px: f32, line: &str) -> f32 {
    let sf = font.as_scaled(PxScale::from(em_px));
    let mut pen = 0.0f32;
    let mut prev: Option<GlyphId> = None;
    for ch in line.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            pen += sf.kern(p, id);
        }
        pen += sf.h_advance(id);
        prev = Some(id);
    }
    pen
}

/// Rasterize text (with `\n` line breaks) using the provided system font. `None` picks
/// [`default_font_path`]; if even that is unavailable, the builtin bitmap engine is used.
/// Same output contract as [`rasterize_text`]: 2:1-padded alpha coverage mask of the
/// requested height.
pub fn rasterize_text_with_font(
    text: &str,
    pixel_height: u32,
    font: Option<&FontArc>,
) -> AlphaMaskData {
    let pixel_height = pixel_height.clamp(16, 2048);
    let font = font
        .cloned()
        .or_else(|| default_font_path().and_then(|p| load_font_arc(&p).ok()));
    let Some(font) = font else {
        return rasterize_text(text, pixel_height);
    };

    // Line budget: fill the full stamp height across all wrapped lines, em size shrinks
    // with the number of lines (mirrors the bitmap engine's dots-per-dot budget).
    let lines: Vec<&str> = text.split('\n').collect();
    let num_lines = lines.len().max(1) as f32;
    let line_budget = pixel_height as f32 / num_lines;
    let em_px = (line_budget / 1.25).max(4.0);
    let scale = PxScale::from(em_px);
    let sf = font.as_scaled(scale);
    let line_height = sf.height().max(em_px);

    // Pass 1: layout all lines (pen positions, kerning, px bounds) to find global bounds.
    struct Laid {
        outlined: Option<ab_glyph::OutlinedGlyph>,
        baseline: f32,
        x: f32,
    }
    let mut laid: Vec<Laid> = Vec::new();
    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    for (li, line) in lines.iter().enumerate() {
        let baseline = sf.ascent() + line_height * li as f32;
        let mut pen = 0.0f32;
        let mut prev: Option<GlyphId> = None;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                pen += sf.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let b = outlined.px_bounds();
                min.0 = min.0.min(b.min.x);
                min.1 = min.1.min(b.min.y);
                max.0 = max.0.max(b.max.x);
                max.1 = max.1.max(b.max.y);
                laid.push(Laid {
                    outlined: Some(outlined),
                    baseline,
                    x: pen,
                });
            }
            pen += sf.h_advance(id);
            prev = Some(id);
        }
    }

    let text_w = if laid.is_empty() { 1.0 } else { max.0 - min.0 };
    let text_h = if laid.is_empty() { 1.0 } else { max.1 - min.1 };

    // Pass 2: punch coverage into the 2:1-padded, centered alpha mask.
    const MAX_WIDTH: u32 = 8192;
    let margin = em_px.max(8.0) as u32;
    let mut width = ((text_w as u32) + margin * 2).max(pixel_height * 2).max(32);
    width = width.min(MAX_WIDTH);
    let x_offset = ((width as f32 - text_w) / 2.0).max(margin as f32) - min.0;
    let y_offset = ((pixel_height as f32 - text_h) / 2.0).max(4.0) - min.1;

    let mut data = vec![0u8; (width * pixel_height) as usize];
    for g in &laid {
        let Some(outlined) = &g.outlined else {
            continue;
        };
        let b = outlined.px_bounds();
        let bx = b.min.x.floor() as i64 + x_offset as i64;
        let by = b.min.y.floor() as i64 + y_offset as i64;
        outlined.draw(|dx, dy, coverage| {
            let x = bx + dx as i64;
            let y = by + dy as i64;
            if x < 0 || y < 0 || x >= width as i64 || y >= pixel_height as i64 {
                return;
            }
            let idx = (y as u32 * width + x as u32) as usize;
            let v = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
            if v > data[idx] {
                data[idx] = v; // max-coverage merge across overlapping glyphs
            }
        });
    }

    AlphaMaskData {
        width,
        height: pixel_height,
        data,
    }
}

/// Rasterize text with a system font selected by path. `None` or an empty path picks the
/// auto default. Returns `Err(message)` when the font cannot be loaded so the caller can
/// fall back to the builtin bitmap engine without panicking.
pub fn rasterize_text_font(
    text: &str,
    pixel_height: u32,
    font_path: Option<&str>,
) -> Result<AlphaMaskData, String> {
    let pixel_height = pixel_height.clamp(16, 2048);
    let path = match font_path {
        Some(p) if p.trim().is_empty() => default_font_path(),
        Some(p) => Some(p.to_owned()),
        None => default_font_path(),
    };
    match path {
        Some(ref p) => {
            let font = load_font_arc(p)?;
            Ok(rasterize_text_with_font(text, pixel_height, Some(&font)))
        }
        None => Ok(rasterize_text(text, pixel_height)),
    }
}

/// Wrap text into multiple lines so long strings fit the decal frame, rasterizing with a
/// discovered system font (`ab_glyph` engine: kerning + arbitrary Unicode). Returns
/// `Err(message)` when the font could not be loaded so the caller can fall back to the
/// builtin bitmap engine.
pub fn rasterize_text_wrapped_font(
    text: &str,
    pixel_height: u32,
    max_chars_per_line: usize,
    font_path: Option<&str>,
) -> Result<AlphaMaskData, String> {
    let max_chars_per_line = max_chars_per_line.max(2);
    let pixel_height = pixel_height.clamp(32, 2048);
    let lines: Vec<String> = text
        .split('\n')
        .flat_map(|l| {
            l.chars()
                .collect::<Vec<_>>()
                .chunks(max_chars_per_line)
                .map(|c| c.iter().collect::<String>())
                .collect::<Vec<_>>()
        })
        .collect();
    rasterize_text_font(&lines.join("\n"), pixel_height, font_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_nonempty_mask() {
        let a = rasterize_text("REM", 64);
        assert_eq!(a.height, 64);
        assert_eq!(a.data.len(), (a.width * a.height) as usize);
        assert!(a.width >= 128, "padded 2:1 aspect, got {}", a.width);
        assert!(
            a.data.iter().any(|&v| v > 200),
            "text mask should have inked texels"
        );
    }

    #[test]
    fn known_glyph_punches_pixels() {
        // The letter "I" should be roughly centered around x = width/2 with an odd vertical bar
        let a = rasterize_text("I", 128);
        let mut center_col_hits = 0;
        let cx = (a.width / 2) as usize;
        for y in 0..a.height as usize {
            for x in cx.saturating_sub(16)..(cx + 16).min(a.width as usize - 1) {
                if a.data[y * a.width as usize + x] > 200 {
                    center_col_hits += 1;
                }
            }
        }
        assert!(
            center_col_hits > 200,
            "glyph bar missing ({})",
            center_col_hits
        );
    }

    #[test]
    fn wrapped_never_panic_on_long_text() {
        let a = rasterize_text_wrapped("SOMETHING VERY LONG & % PANIC CHECK", 96, 6);
        assert!(!a.data.is_empty());
    }

    // ---- TTF/OTF system-font engine --------------------------------------------------

    #[test]
    fn discovery_finds_fonts_and_default_is_stable() {
        let fonts = system_fonts();
        assert!(
            !fonts.is_empty(),
            "OS font discovery found no TTF/OTF files"
        );
        // The bundled reference PBR suite starter font set must be discovered too.
        let sp_dir = crate::painting::material_library::find_fonts_dir();
        if let Some(app) = sp_dir {
            let sp_hits = fonts
                .iter()
                .filter(|f| f.starts_with(app.to_str().unwrap()))
                .count();
            assert!(sp_hits >= 20, "bundled SP fonts discovered: {sp_hits}");
            // And they must come first (never truncated away by the cap).
            assert!(fonts
                .iter()
                .take(sp_hits)
                .all(|f| f.starts_with(app.to_str().unwrap())));
        }
        let def = default_font_path();
        let def2 = default_font_path();
        assert_eq!(def, def2, "default font selection must be stable");
        let def = def.expect("default font path with fonts installed");
        assert!(
            fonts.iter().any(|f| f == &def),
            "default font must come from the discovered list"
        );
    }

    #[test]
    fn ab_glyph_rasterization_is_nonempty_and_deterministic() {
        // CI runners and minimal containers carry no system fonts; the
        // fixture-dependent half of this suite self-skips there the same way
        // the .sbsar sample test does.
        let Some(path) = default_font_path() else {
            eprintln!("[text] no system fonts — skipping rasterization fixture test");
            return;
        };
        let font = load_font_arc(&path).expect("default font parses");
        let a = rasterize_text_with_font("AB", 64, Some(&font));
        let b = rasterize_text_with_font("AB", 64, Some(&font));
        assert_eq!(a.width, b.width);
        assert_eq!(a.height, b.height);
        assert_eq!(a.data, b.data, "two runs must be byte-identical");
        assert!(
            a.data.iter().any(|&v| v > 200),
            "system-font mask should have inked texels"
        );
    }

    #[test]
    fn invalid_font_path_errors_and_bitmap_fallback_rasterizes() {
        let err = rasterize_text_font("AB", 64, Some("/definitely/missing/dejavu.ttf"));
        assert!(
            err.is_err(),
            "unparseable/missing font must report Err, not panic"
        );
        let fallback = rasterize_text("AB", 64);
        assert!(
            fallback.data.iter().any(|&v| v > 200),
            "bitmap fallback still rasterizes"
        );

        // A real file with garbage content must also fail gracefully.
        let garbage = std::env::temp_dir().join("remoainter_bad_font.ttf");
        std::fs::write(&garbage, b"not a font at all").expect("write garbage font");
        let err = rasterize_text_font("AB", 64, Some(garbage.to_str().unwrap()));
        assert!(
            err.is_err(),
            "garbage font bytes must be rejected ({err:?})"
        );
    }

    #[test]
    fn kerning_is_exercised_by_default_font() {
        let path = default_font_path().expect("default font");
        let font = load_font_arc(&path).expect("default font parses");
        // "AV" must be laid out narrower than side-by-side advances of "A" and "V"
        // (or the raw kern pair must be positive/negative — DejaVu Sans kerns AV).
        let sf = font.as_scaled(PxScale::from(64.0));
        let advance_sum = sf.h_advance(font.glyph_id('A')) + sf.h_advance(font.glyph_id('V'));
        let kern = sf.kern(font.glyph_id('A'), font.glyph_id('V'));
        let w_av = layout_line_width(&font, 64.0, "AV");
        let w_a_v = layout_line_width(&font, 64.0, "A V");
        assert!(
            w_av < advance_sum - f32::EPSILON,
            "AV must kern tighter than raw advances (w={w_av}, sum={advance_sum}, kern={kern})"
        );
        assert!(w_a_v > w_av, "spaced 'A V' must be wider than kerned 'AV'");
    }

    #[test]
    fn unicode_and_multiline_rasterize_with_system_font() {
        let path = default_font_path().expect("default font");
        let font = load_font_arc(&path).expect("default font parses");
        let a = rasterize_text_with_font("Grüße — Ω ✓\nLine 2", 128, Some(&font));
        assert!(
            a.data.iter().any(|&v| v > 200),
            "unicode multiline mask should have ink"
        );
        let w =
            rasterize_text_wrapped_font("SOMETHING VERY LONG & % PANIC CHECK", 96, 6, Some(&path));
        assert!(w.is_ok(), "wrapped font path must not error ({w:?})");
        assert!(!w.unwrap().data.is_empty());
    }
}
