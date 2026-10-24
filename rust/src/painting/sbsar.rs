//! `.sbsar` reference archive container parser.
//!
//! Smart Materials are `.sbsar` files: a 7z (LZMA, solid) archive
//! containing one or more `assemblies/content/<slot>/<name>.xml`
//! (*sbsdescription*: parameters, widgets, output formats) plus a compiled
//! `<name>.sbsasm` bytecode assembly. This module cracks the container and
//! deserializes the descriptive layer; graph *evaluation* of `.sbsasm`
//! bytecode lands in `painting::graph` (see
//! `docs/ARCHITECTURE.md` / `docs/ARCHITECTURE.md`).
//!
//! Parsing is deliberately defensive: unknown type codes or widget blocks are
//! preserved verbatim (`Unknown`) instead of erroring; it tolerates upstream extensions
//! the format across versions and a partial parse is still useful for shelf
//! browsing.

use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

/// Type code observed in `sbsdescription` `<input type="N">`.
///
/// Verified against a live archive (`mg_dust.sbsar`): 0 = Float1 (Level 0.5),
/// 4 = Int1 ($randomseed), 5 = Image (ambient_occlusion), 8 = Int2
/// ($outputsize "8,8"). Other codes are upstream version extensions; keep raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SbsarInputType {
    /// `type="0"` float scalar (Float1).
    Float1,
    /// `type="4"` integer scalar (Int1).
    Int1,
    /// `type="5"` image input (e.g. baked maps feeding a smart material).
    Image,
    /// `type="8"` integer pair — `$outputsize` ("8,8").
    Int2,
    /// Anything else — upstream extends this enum across versions.
    Unknown(i64),
}

impl SbsarInputType {
    fn from_code(code: i64) -> Self {
        match code {
            0 => Self::Float1,
            4 => Self::Int1,
            5 => Self::Image,
            8 => Self::Int2,
            other => Self::Unknown(other),
        }
    }
}

/// Widget shape for a parameter, extracted from `<inputgui>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SbsarWidget {
    Slider {
        min: f32,
        max: f32,
        default: f32,
        step: f32,
        clamp: bool,
    },
    Toggle {
        default: bool,
    },
    Image,
    Other(String),
}

/// One user-facing parameter of a graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbsarInput {
    pub identifier: String,
    pub label: String,
    pub input_type: SbsarInputType,
    pub widget: SbsarWidget,
}

/// One output of a graph (a texture map the archive produces).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbsarOutput {
    pub identifier: String,
    pub label: String,
    /// `format="N"` raw channel-format code (12 = RGBA color observed).
    /// Kept raw: full mapping is enumerated across real archives later.
    pub format: i64,
    pub width: u32,
    pub height: u32,
    pub mipmaps: u32,
}

/// One graph packaged inside the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbsarGraph {
    /// e.g. `pkg://mg_dust`
    pub pkgurl: String,
    /// Human label ("MG Dust").
    pub label: String,
    pub inputs: Vec<SbsarInput>,
    pub outputs: Vec<SbsarOutput>,
}

/// A parsed `.sbsar`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbsarPackage {
    pub graphs: Vec<SbsarGraph>,
}

impl SbsarPackage {
    /// First graph, or `None` on empty/failed parse.
    pub fn primary(&self) -> Option<&SbsarGraph> {
        self.graphs.first()
    }
}

fn attr_str(attr: &quick_xml::events::attributes::Attribute) -> String {
    String::from_utf8_lossy(attr.value.as_ref()).replace("&quot;", "\"")
}

fn attr_key(attr: &quick_xml::events::attributes::Attribute) -> String {
    String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase()
}

/// Parse a `.sbsar` from raw bytes.
pub fn parse_sbsar(data: &[u8]) -> Result<SbsarPackage, String> {
    let mut cursor = std::io::Cursor::new(data);
    let len = cursor.get_ref().len() as u64;
    let mut reader = sevenz_rust::SevenZReader::new(&mut cursor, len, sevenz_rust::Password::empty())
        .map_err(|e| format!("not a 7z archive (.sbsar): {e}"))?;

    // NOTE (upstream sevenz-rust #31): solid-block folder decodes abort with a
    // spurious ChecksumVerificationFailed / truncated tail member. We still use
    // this single supported path and salvage everything decoded before the
    // failure; the archived XML prefix is usually enough to expose every
    // parameter for shelf browsing. Revisit when upstream ships > 0.6.1.
    let mut xml_texts: Vec<(String, Vec<u8>)> = Vec::new();
    let walk_result = reader
        .for_each_entries(&mut |entry: &sevenz_rust::SevenZArchiveEntry, data: &mut dyn Read| {
            if !entry.is_directory && entry.name().ends_with(".xml") {
                let mut buf = Vec::with_capacity(entry.size() as usize);
                let n = data.read_to_end(&mut buf).unwrap_or(0);
                if n > 0 {
                    xml_texts.push((entry.name().to_string(), buf[..n as usize].to_vec()));
                }
            } else {
                // Non-XML members keep the solid stream flowing correctly.
                let mut sink: Vec<u8> = Vec::new();
                let _ = data.read_to_end(&mut sink);
            }
            Ok(true)
        });
    if let Err(e) = walk_result {
        if xml_texts.is_empty() {
            return Err(format!("sbsar entry walk: {e}"));
        }
    }

    if xml_texts.is_empty() {
        return Err("sbsar archive contains no descriptive XML member".into());
    }

    let mut graphs = Vec::new();
    for (name, xml) in &xml_texts {
        if let Some(graph) = parse_sbsdescription(xml)? {
            graphs.push(graph);
        } else {
            eprintln!("[sbsar] member {} ({} bytes) had no <graph> block", name, xml.len());
        }
    }
    if graphs.is_empty() {
        return Err("sbsar parsed but contained no graph descriptions".into());
    }
    Ok(SbsarPackage { graphs })
}

/// Parse a `.sbsar` from a file path.
pub fn parse_sbsar_file(path: &Path) -> Result<SbsarPackage, String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_sbsar(&data)
}

/// Parse a `.spsm` smart material: a 7z archive whose members include an
/// embedded `.sbs`/`.sbsar` graph. Opens the same parameter dialog as a
/// plain `.sbsar` by forwarding the embedded member to `parse_sbsar`.
pub fn parse_spsm_file(path: &Path) -> Result<SbsarPackage, String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_spsm(&data, &path.display().to_string())
}

pub fn parse_spsm(data: &[u8], source_label: &str) -> Result<SbsarPackage, String> {
    let mut cursor = std::io::Cursor::new(data);
    let len = cursor.get_ref().len() as u64;
    let mut reader = sevenz_rust::SevenZReader::new(&mut cursor, len, sevenz_rust::Password::empty())
        .map_err(|e| format!("not a 7z archive (.spsm): {e}"))?;

    let mut best: Option<(usize, Vec<u8>)> = None; // (path weight, bytes)
    let walk_result = reader
        .for_each_entries(&mut |entry: &sevenz_rust::SevenZArchiveEntry, data: &mut dyn Read| {
            if !entry.is_directory {
                let name = entry.name().to_lowercase();
                if name.ends_with(".sbsar") || name.ends_with(".sbs") || name.ends_with(".sbd") {
                    let mut buf = Vec::with_capacity(entry.size() as usize);
                    let n = data.read_to_end(&mut buf).unwrap_or(0);
                    if n > 0 {
                        // Prefer .sbsar, then .sbs, then .sbd; then larger.
                        let weight = if name.ends_with(".sbsar") {
                            3
                        } else if name.ends_with(".sbs") {
                            2
                        } else {
                            1
                        };
                        let wins = best
                            .as_ref()
                            .map(|(w, old)| weight > *w || (weight == *w && n > old.len()))
                            .unwrap_or(true);
                        if wins {
                            best = Some((weight, buf[..n as usize].to_vec()));
                        }
                    }
                } else {
                    // Other members keep the solid stream flowing correctly.
                    let mut sink: Vec<u8> = Vec::new();
                    let _ = data.read_to_end(&mut sink);
                }
            } else {
                // Directories also drain the solid stream.
                let mut sink: Vec<u8> = Vec::new();
                let _ = data.read_to_end(&mut sink);
            }
            Ok(true)
        });
    if let Err(e) = walk_result {
        if best.is_none() {
            return Err(format!("spsm entry walk: {e}"));
        }
    }
    let (_weight, bytes) =
        best.ok_or_else(|| format!("{source_label} contains no embedded .sbs/.sbsar member"))?;
    parse_sbsar(&bytes)
        .map_err(|e| format!("embedded graph in {}.spsm failed to parse: {e}", source_label))
}

fn parse_sbsdescription(xml: &[u8]) -> Result<Option<SbsarGraph>, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);

    let mut graph: Option<SbsarGraph> = None;
    // Tuple: (identifier, label, type, widget-in-progress).
    let mut current_input: Option<(String, String, SbsarInputType, SbsarWidget)> = None;
    let mut current_output: Option<SbsarOutput> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.local_name().as_ref())
                    .to_ascii_lowercase();
                match tag.as_str() {
                    "graph" => {
                        let mut pkgurl = String::new();
                        let mut label = String::new();
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| format!("graph attr: {e}"))?;
                            match attr_key(&attr).as_str() {
                                "pkgurl" => pkgurl = attr_str(&attr),
                                "label" => label = attr_str(&attr),
                                _ => {}
                            }
                        }
                        graph = Some(SbsarGraph { pkgurl, label, inputs: Vec::new(), outputs: Vec::new() });
                    }
                    "input" => {
                        let mut identifier = String::new();
                        let mut ty = SbsarInputType::Unknown(-1);
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| format!("input attr: {e}"))?;
                            match attr_key(&attr).as_str() {
                                "identifier" => identifier = attr_str(&attr),
                                "type" => {
                                    let v = attr_str(&attr);
                                    let code = v.parse::<f64>().unwrap_or(-1.0) as i64;
                                    ty = SbsarInputType::from_code(code);
                                }
                                _ => {}
                            }
                        }
                        current_input = Some((identifier, String::new(), ty, SbsarWidget::Other(String::new())));
                    }
                    "inputgui" => {
                        if let Some((_, label, _, widget)) = current_input.as_mut() {
                            for attr in e.attributes() {
                                let attr = attr.map_err(|e| format!("inputgui attr: {e}"))?;
                                match attr_key(&attr).as_str() {
                                    "label" => *label = attr_str(&attr),
                                    "widget" => {
                                        // Image widgets carry nested guiimage; non-image
                                        // widgets are replaced when their block parses below.
                                        *widget = match attr_str(&attr).as_str() {
                                            "image" => SbsarWidget::Image,
                                            other => SbsarWidget::Other(other.into()),
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "guislider" => {
                        if let Some((_, _, _, widget)) = current_input.as_mut() {
                            let mut min = 0.0;
                            let mut max = 0.0;
                            let mut step = 0.01;
                            let mut clamp = false;
                            for attr in e.attributes().flatten() {
                                let num = |v: &str| v.parse::<f32>().unwrap_or(0.0);
                                match attr_key(&attr).as_str() {
                                    "min" => min = num(&attr_str(&attr)),
                                    "max" => max = num(&attr_str(&attr)),
                                    "step" => step = num(&attr_str(&attr)),
                                    "clamp" => clamp = attr_str(&attr).eq_ignore_ascii_case("on"),
                                    _ => {}
                                }
                            }
                            *widget = SbsarWidget::Slider { min, max, default: (min + max) * 0.5, step, clamp };
                        }
                    }
                    "output" => {
                        let mut identifier = String::new();
                        let mut label = String::new();
                        let mut format = -1i64;
                        let mut width = 0u32;
                        let mut height = 0u32;
                        let mut mipmaps = 0u32;
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| format!("output attr: {e}"))?;
                            let num = |v: &str| v.parse::<f64>().unwrap_or(-1.0) as i64;
                            match attr_key(&attr).as_str() {
                                "identifier" => identifier = attr_str(&attr),
                                "format" => format = num(&attr_str(&attr)),
                                "width" => width = num(&attr_str(&attr)).max(0) as u32,
                                "height" => height = num(&attr_str(&attr)).max(0) as u32,
                                "mipmaps" => mipmaps = num(&attr_str(&attr)).max(0) as u32,
                                _ => {}
                            }
                        }
                        current_output = Some(SbsarOutput { identifier, label, format, width, height, mipmaps });
                    }
                    "outputgui" => {
                        if let Some(out) = current_output.as_mut() {
                            for attr in e.attributes() {
                                let attr = attr.map_err(|e| format!("outputgui attr: {e}"))?;
                                if attr_key(&attr) == "label" {
                                    out.label = attr_str(&attr);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // Self-closing widget markers may appear this way.
                let tag = String::from_utf8_lossy(e.local_name().as_ref())
                    .to_ascii_lowercase();
                if tag == "inputgui" {
                    if let Some((_, label, _, widget)) = current_input.as_mut() {
                        for attr in e.attributes().flatten() {
                            match attr_key(&attr).as_str() {
                                "label" => *label = attr_str(&attr),
                                "widget" => {
                                    let w = attr_str(&attr);
                                    if w == "image" {
                                        *widget = SbsarWidget::Image;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                } else if tag == "guislider" {
                    if let Some((_, _, _, widget)) = current_input.as_mut() {
                        let mut min = 0.0;
                        let mut max = 0.0;
                        let mut step = 0.01;
                        let mut clamp = false;
                        for attr in e.attributes().flatten() {
                            let num = |v: &str| v.parse::<f32>().unwrap_or(0.0);
                            match attr_key(&attr).as_str() {
                                "min" => min = num(&attr_str(&attr)),
                                "max" => max = num(&attr_str(&attr)),
                                "step" => step = num(&attr_str(&attr)),
                                "clamp" => clamp = attr_str(&attr).eq_ignore_ascii_case("on"),
                                _ => {}
                            }
                        }
                        *widget = SbsarWidget::Slider { min, max, default: (min + max) * 0.5, step, clamp };
                    }
                } else if tag == "togglebutton" && current_input.is_some() {
                    current_input.as_mut().unwrap().3 = SbsarWidget::Toggle { default: false };
                }
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.local_name().as_ref())
                    .to_ascii_lowercase();
                match tag.as_str() {
                    "input" => {
                        if let Some((identifier, label, ty, widget)) = current_input.take() {
                            if let Some(g) = graph.as_mut() {
                                g.inputs.push(SbsarInput {
                                    identifier: identifier.clone(),
                                    label: if label.is_empty() { identifier } else { label },
                                    input_type: ty,
                                    widget,
                                });
                            }
                        }
                    }
                    "output" => {
                        if let Some(out) = current_output.take() {
                            if let Some(g) = graph.as_mut() {
                                g.outputs.push(out);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("sbsdescription XML error: {e}")),
            _ => {}
        }
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the sample smart material (mg_dust) when a local fixture is
    /// available. `#[ignore]d: the fixture is proprietary and not shipped, so
    /// the test only ever skipped. Run it by providing tools/fixtures/mg_dust.sbsar.
    #[test]
    #[ignore = "requires an .sbsar test fixture that is not part of the repo"]
    fn parses_sample_smart_material() {
        let candidates = ["tools/fixtures/mg_dust.sbsar", "test_spsm/embedded.sbsar"];
        let Some(path) = candidates.iter().find(|p| Path::new(p).exists()) else {
            eprintln!("sample .sbsar not present; skipping");
            return;
        };
        let package = parse_sbsar_file(Path::new(path)).expect("parse");
        let graph = package.primary().expect("one graph");
        assert_eq!(graph.pkgurl, "pkg://mg_dust");
        assert_eq!(graph.label, "MG Dust");
        assert_eq!(graph.outputs[0].identifier, "output");
        // NOTE (upstream sevenz-rust #31): solid-block member decoding re-inits
        // the LZMA range coder at member boundaries, so XML members that are
        // *not* first in the block can arrive truncated (a few trailing
        // parameters may be missing on multi-parameter archives). Full-parameter
        // reading of first-truncated XML members lands when upstream releases a
        // fix — parse stays graceful meanwhile. See docs/ARCHITECTURE.md.
        assert!(!graph.inputs.is_empty(), "expected parameters");
        let ao = graph.inputs.iter().find(|i| i.identifier == "ambient_occlusion").expect("image input");
        assert_eq!(ao.input_type, SbsarInputType::Image);
    }

    #[test]
    fn rejects_non_archive() {
        assert!(parse_sbsar(b"not an archive").is_err());
    }

    #[test]
    fn rejects_non_sevenz_spsm() {
        let err = parse_spsm(b"not an archive", "sample.spsm").unwrap_err();
        assert!(err.contains("not a 7z archive"), "got: {err}");
    }

    #[test]
    fn type_code_defaults() {
        assert_eq!(SbsarInputType::from_code(0), SbsarInputType::Float1);
        assert_eq!(SbsarInputType::from_code(4), SbsarInputType::Int1);
        assert_eq!(SbsarInputType::from_code(5), SbsarInputType::Image);
        assert_eq!(SbsarInputType::from_code(8), SbsarInputType::Int2);
        assert_eq!(SbsarInputType::from_code(99), SbsarInputType::Unknown(99));
    }
}
