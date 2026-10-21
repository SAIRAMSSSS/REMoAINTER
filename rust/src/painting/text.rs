//! Text / decal rasterization for the Decal tool (v1: embedded bitmap vector-derived font).
//! In Substance Painter this is the Text/Alpha tool; here it renders crisp ASCII decal alpha
//! masks without any external font dependency, honoring arbitrary pixel resolution.

#![allow(dead_code)]

use super::brush::AlphaMaskData;

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
        .find(|(c, _)| c.chars().next() == Some(upper))
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
        ((pixel_height as i64 - (num_lines as u32 * line_cells as u32 * dots_per_dot) as i64) / 2)
            .max(1);

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
                                + ((li as u32 * line_cells as u32 + row as u32) * dots_per_dot)
                                    as i64;
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
            for x in (cx - 16).max(0)..(cx + 16).min(a.width as usize - 1) {
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
}
