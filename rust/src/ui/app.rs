use crate::baker::{BakedMaps, BakerEngine, BakerOptions, MapType};
use crate::graphics::viewport_wgpu::{
    list_available_environments, ViewportPaintParams, ViewportWgpu,
};
use crate::graphics::{EnvironmentPreset, Mesh};
use crate::painting::brush::AlphaMaskData;
use crate::painting::texture_importer::PbrTextureSet;
use crate::painting::{
    apply_smart_material_hierarchy, load_real_pbr_material, AnchorPoint, BitDepth, BlendMode,
    Brush, ExportPreset, FilterEffect, FilterTargetChannel, Layer, LayerStack, LayerType,
    MaskBlendMode, MaskEffect, OutputFormat, PaddingAlgorithm, TextureExportContext,
};
use crate::project::AppConfig;
use crate::ui::icons::{draw_icon, IconType};
use crate::ui::theme::*;
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Stroke, Ui};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Paint,
    Eraser,
    Projection,
    PolygonFill,
    Smudge,
    Clone,
    Eyedropper,
    Decal,
}

#[derive(Clone, Debug)]
pub struct ShelfAsset {
    pub name: String,
    pub category: usize, // 1: Smart Mats, 2: Materials, 3: Smart Masks, 4: Alphas, 5: Effects/Generators
    pub path: PathBuf,
    pub thumb_path: PathBuf,
    pub badge: &'static str,
    pub badge_color: Color32,
    pub pbr_color: [f32; 4],
    pub pbr_metallic: f32,
    pub pbr_roughness: f32,
    /// material_cache item dir applied with the SP fill-layer path (materials /
    /// smart materials); `None` for legacy shelf files (plain previews/alphas).
    pub item_dir: Option<PathBuf>,
    /// Icon classification from SP: sphere renders are never paint content.
    pub icon_kind: crate::painting::material_library::IconKind,
    /// Label + SP keywords + SP category, lowercased — the search haystack.
    pub search_text: String,
}

/// An active texture channel in the Texture Set
#[derive(Clone, Debug)]
pub struct TextureChannel {
    pub name: String,
    pub format: String,
    #[allow(dead_code)]
    pub enabled: bool,
}

/// A saved 3D camera bookmark (reference PBR suite standard slots 1..9)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CameraBookmark {
    pub name: String,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

pub struct RemoainterApp {
    pub active_tool: ToolMode,
    pub brush: Brush,
    pub layer_stack: LayerStack,
    pub mesh: Mesh,

    // Baker & Mesh Maps
    pub baker_options: BakerOptions,
    pub baked_maps: BakedMaps,
    pub is_baking: bool,
    pub baking_progress: Arc<AtomicUsize>,
    pub cancel_baking: Arc<AtomicBool>,
    pub baking_receiver: Option<std::sync::mpsc::Receiver<BakedMaps>>,
    pub show_baker_dialog: bool,
    pub baker_selected_map: usize,
    pub baker_subsampling: usize,
    pub baker_use_low_as_high: bool,
    pub baker_frontal_dist: f32,
    pub baker_rear_dist: f32,
    pub baker_skew_enabled: bool,
    pub baker_skew_invert: bool,
    pub baker_skew_strength: f32,
    pub baker_skew_map_path: String,
    pub baker_skew_map: Option<Arc<crate::baker::engine::SkewMap>>,
    #[allow(dead_code)]
    pub baker_curv_method: usize,
    pub baker_high_poly_path: String,
    pub baker_high_poly_mesh: Option<Mesh>,
    pub baker_low_suffix: String,
    pub baker_high_suffix: String,
    pub baker_relative_to_bbox: bool,
    pub baker_average_normals: bool,
    pub baker_ignore_backfaces: bool,

    // Viewport & Lighting
    pub display_channel: usize, // 0: Material, 1: Base Color, 2: Roughness, 3: Metallic, 4: Normal, 5: AO, 6: Curvature
    pub env_preset: usize,
    pub env_rotation: f32,
    pub light_intensity: f32,
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub camera_dist: f32,
    pub symmetry_x: bool,
    pub symmetry_y: bool,
    pub symmetry_z: bool,

    // Viewport Upgrades: Camera Bookmarks, Turntable, Submesh Isolation & Post-Processing
    pub camera_bookmarks: [Option<CameraBookmark>; 10],
    pub turntable_active: bool,
    pub turntable_speed: f32,
    pub isolated_submesh: Option<usize>,
    pub bloom_enabled: bool,
    pub bloom_intensity: f32,
    pub bloom_threshold: f32,
    pub ssao_enabled: bool,
    pub ssao_intensity: f32,
    pub dof_enabled: bool,
    pub dof_focal_distance: f32,
    pub dof_aperture: f32,

    // Brush Engine & Undo/Redo History
    pub history_manager: crate::painting::HistoryManager,
    pub lazy_mouse_tracker: crate::painting::brush::LazyMouseTracker,
    pub is_in_stroke: bool,
    pub paint_target_mask: bool,

    // Shelf & Asset Filter
    pub shelf_category: usize, // 0: All, 1: Smart Mats, 2: Materials, 3: Smart Masks, 4: Alphas, 5: Effects/Generators
    pub search_query: String,
    pub shelf_assets: Vec<ShelfAsset>,
    pub shelf_textures: HashMap<PathBuf, egui::TextureHandle>,
    pub logo_texture: Option<egui::TextureHandle>,

    // Right Strip Active Panel
    pub active_right_panel: usize, // 0: Settings, 1: Texture Sets, 2: Layers, 3: Display, 4: Shaders, 5: History, 6: Log

    // Test Assets
    pub selected_model_idx: usize,
    pub status_message: String,
    pub history: Vec<String>,
    pub logs: Vec<(String, String, String)>, // time, level, text

    pub last_painted_uv: Option<[f32; 2]>,
    pub layer_clipboard: Option<Layer>,

    // Viewport GPU Texture & Multi-View Mode
    pub texture_handle: Option<egui::TextureHandle>,
    pub checker_handle: Option<egui::TextureHandle>,
    pub texture_needs_update: bool,
    pub viewport_view_mode: usize, // 0: 3D View, 1: 2D Texture View, 2: Split 2D | 3D
    // 2D UV View camera (reference PBR suite parity: wheel zoom + middle-drag pan)
    pub uv_zoom: f32,
    pub uv_pan: [f32; 2], // screen-space offset of the texture square around its pane center
    pub show_wireframe: bool,
    pub show_uv_wireframe: bool,
    pub show_grid: bool,

    // Hardware Vulkan / wgpu PBR Viewport & Studio HDR Environment Pipeline
    pub wgpu_device: Option<Arc<wgpu::Device>>,
    pub wgpu_queue: Option<Arc<wgpu::Queue>>,
    pub viewport_wgpu: Option<Arc<Mutex<ViewportWgpu>>>,
    pub available_environments: Vec<(String, PathBuf)>,
    pub selected_env_idx: usize,
    pub exposure: f32,
    pub skybox_visible: bool,
    pub skybox_blur: f32,
    pub skybox_intensity: f32,
    pub uv_scale: f32,

    // Texture Importer & Fallback Dialog
    pub show_texture_importer_dialog: bool,
    pub import_texture_set: PbrTextureSet,
    pub import_flip_normal_y: bool,

    // Texture Channels & Texture Set Manager
    pub texture_channels: Vec<TextureChannel>,
    pub texture_set_manager: crate::painting::TextureSetManager,

    // Export Textures Dialog (File > Export Textures, reference PBR suite Parity)
    pub show_export_dialog: bool,
    pub export_output_dir: PathBuf,
    pub export_preset_idx: usize,
    pub export_resolution: u32,
    pub export_file_format: usize,
    pub export_padding: u32,
    /// UV gutter fill mode: 0 = Color, 1 = Transparent, 2 = Diffusion, 3 = Passthrough
    pub export_padding_algorithm: u32,
    pub export_status: String,
    pub baker_tab_idx: usize,

    // Procedural (Graph) Material dialog — engine-backed parametric materials
    pub show_graph_dialog: bool,
    pub show_sbsar_import_dialog: bool,
    pub imported_sbsar: Option<crate::painting::sbsar::SbsarPackage>,
    pub sbsar_param_values: std::collections::BTreeMap<String, f32>,
    pub sbsar_status: String,
    pub graph_template_idx: usize,
    pub graph_seed: u32,
    pub graph_scale: f32,
    pub graph_contrast: f32,
    pub graph_engine: Option<crate::painting::graph::GraphEngine>,
    pub graph_preview_tex: Option<egui::TextureHandle>,
    pub graph_status: String,

    // New Project Dialog (File > New..., reference PBR suite Parity)
    pub show_new_project_dialog: bool,
    pub new_proj_mesh_path: String,
    pub new_proj_template_idx: usize,
    pub new_proj_resolution: u32,
    pub new_proj_normal_directx: bool,
    pub new_proj_udim: bool,
    pub new_proj_import_cameras: bool,

    // Advanced Tools
    pub poly_fill_mode: usize, // 0: Triangle, 1: UV Island, 2: Mesh
    pub clone_source_uv: Option<[f32; 2]>,
    pub clone_offset_uv: Option<[f32; 2]>, // stroke-relative aligned offset, locked per stroke

    // Text / Decal tool state
    pub decal_alpha: Option<Arc<crate::painting::brush::AlphaMaskData>>,
    pub decal_text: String,
    pub decal_scale: f32,
    pub decal_angle: f32,
    pub decal_font_size: f32,
    /// Discovered system TTF/OTF fonts (lazily populated once from text::system_fonts).
    pub decal_fonts: Vec<String>,
    /// Selected font family path; empty = automatic default (DejaVu/Liberation/Noto preferred).
    pub decal_font_path: String,

    // Project Management (File > Save/Load, Autosave, Snapshots)
    pub project_name: String,
    pub current_project_path: Option<PathBuf>,
    pub project_is_dirty: bool,
    pub last_autosave_time: std::time::Instant,
    pub autosave_interval_secs: u64,
    pub mesh_source_path: Option<String>,
    pub snapshots: Vec<crate::project::SnapshotInfo>,
    pub show_snapshots_dialog: bool,

    // Preferences, Hotkeys & Configuration (Persistent in ~/.remoainter/config.json)
    pub config: AppConfig,
    pub show_preferences_dialog: bool,
    pub show_hotkeys_dialog: bool,
    pub show_shortcuts_dialog: bool,
    pub color_management_mode: usize,

    // Offline Path-Traced Visualisation (Render Mode; CPU Rayon progressive)
    pub show_render_mode: bool,
    pub render_options: crate::render::RenderOptions,
    pub render_preset: crate::render::QualityPreset,
    /// spp used when the preset is `Custom`.
    pub render_custom_spp: u32,
    /// Owned worker job (film + progress atomics + thread handle).
    pub render_state: Option<crate::render::RenderJob>,
    /// Preview texture of the progressive film (separate from the PBR
    /// composite texture so painting stays untouched).
    pub render_film_handle: Option<egui::TextureHandle>,
    /// Pass count shown in the last preview texture (avoid texture churn).
    pub render_last_preview_passes: usize,
    /// Time of last preview upload (~250 ms throttle guard).
    pub render_last_preview: std::time::Instant,
    /// Whether completion was already logged for the current session.
    pub render_logged: bool,
}

fn decal_rasterize(app: &mut RemoainterApp) {
    // Rasterize the decal text with the chosen system font (TTF/OTF via ab_glyph). If the
    // font fails to load/parse, fall back to the builtin bitmap engine — never panic.
    let font_px = app.decal_font_size.clamp(32.0, 2048.0);
    match crate::painting::text::rasterize_text_font(
        &app.decal_text,
        font_px as u32,
        Some(&app.decal_font_path),
    ) {
        Ok(mask) => {
            app.decal_alpha = Some(Arc::new(mask));
            app.status_message = format!("Text decal updated: \"{}\"", app.decal_text);
        }
        Err(err) => {
            app.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "WARN".to_string(),
                format!("Font failed, builtin bitmap fallback used: {err}"),
            ));
            app.decal_alpha = Some(Arc::new(crate::painting::text::rasterize_text(
                &app.decal_text,
                font_px as u32,
            )));
            app.status_message = "Font failed — builtin bitmap font used".to_string();
        }
    }
}

fn infer_pbr(name: &str) -> ([f32; 4], f32, f32) {
    let lower = name.to_lowercase();
    let mut color = [0.70f32, 0.70, 0.72, 1.0];
    let mut metallic = 0.5f32;
    let mut roughness = 0.4f32;

    if lower.contains("gold") {
        color = [1.0, 0.84, 0.25, 1.0];
        metallic = 1.0;
        roughness = 0.20;
    } else if lower.contains("copper") {
        color = [0.95, 0.64, 0.54, 1.0];
        metallic = 1.0;
        roughness = 0.25;
    } else if lower.contains("bronze") || lower.contains("brass") {
        color = [0.85, 0.65, 0.35, 1.0];
        metallic = 0.95;
        roughness = 0.30;
    } else if lower.contains("chrome") {
        color = [0.95, 0.95, 0.98, 1.0];
        metallic = 1.0;
        roughness = 0.08;
    } else if lower.contains("steel")
        || lower.contains("iron")
        || lower.contains("aluminium")
        || lower.contains("aluminum")
        || lower.contains("metal")
    {
        color = [0.75, 0.76, 0.78, 1.0];
        metallic = 0.95;
        roughness = 0.32;
    } else if lower.contains("cobalt") {
        color = [0.28, 0.45, 0.78, 1.0];
        metallic = 0.85;
        roughness = 0.28;
    } else if lower.contains("rubber") {
        color = [0.15, 0.15, 0.16, 1.0];
        metallic = 0.0;
        roughness = 0.75;
    } else if lower.contains("plastic") {
        color = [0.85, 0.18, 0.18, 1.0];
        metallic = 0.0;
        roughness = 0.28;
    } else if lower.contains("wood") {
        color = [0.55, 0.35, 0.20, 1.0];
        metallic = 0.0;
        roughness = 0.45;
    } else if lower.contains("concrete") || lower.contains("asphalt") || lower.contains("stone") {
        color = [0.52, 0.52, 0.52, 1.0];
        metallic = 0.0;
        roughness = 0.82;
    } else if lower.contains("fabric") || lower.contains("leather") {
        color = [0.45, 0.38, 0.32, 1.0];
        metallic = 0.0;
        roughness = 0.65;
    } else if lower.contains("carbon") {
        color = [0.12, 0.12, 0.14, 1.0];
        metallic = 0.0;
        roughness = 0.22;
    }

    if lower.contains("green") {
        color = [0.22, 0.42, 0.26, 1.0];
    } else if lower.contains("red") {
        color = [0.72, 0.18, 0.15, 1.0];
    } else if lower.contains("blue") {
        color = [0.18, 0.36, 0.65, 1.0];
    } else if lower.contains("yellow") {
        color = [0.82, 0.68, 0.16, 1.0];
    } else if lower.contains("orange") {
        color = [0.85, 0.42, 0.12, 1.0];
    } else if lower.contains("black") || lower.contains("dark") {
        color = [0.14, 0.14, 0.15, 1.0];
    } else if lower.contains("white") {
        color = [0.90, 0.90, 0.92, 1.0];
    }

    if lower.contains("paint") {
        metallic = if lower.contains("metallic") {
            0.85
        } else {
            0.04
        };
        roughness = 0.30;
    }

    (color, metallic, roughness)
}

fn find_shelf_dir() -> PathBuf {
    crate::paths::find_assets_subdir("shelf").unwrap_or_default()
}

/// Old shelf sources (legacy previews + alphas), kept with dedupe against the
/// cache; group dir + shelf category for legacy scan.
fn legacy_scan_category(
    shelf_dir: &Path,
    cat_id: usize,
    sub_folder: &str,
    badge: &'static str,
    badge_color: Color32,
    out: &mut Vec<ShelfAsset>,
    seen: &mut std::collections::HashSet<(usize, String)>,
) {
    let folder = shelf_dir.join(sub_folder);
    let mut stack: Vec<PathBuf> = vec![folder.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut list: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        list.sort_by_key(|e| e.file_name());
        for entry in list {
            let p = entry.path();
            if p.is_dir() {
                // Include flat BMP alpha packs shipped directly on the shelf.
                stack.push(p);
                continue;
            }
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            let ext = ext.to_lowercase();
            if ext != "png"
                && ext != "webp"
                && ext != "sbsar"
                && ext != "spmsk"
                && ext != "spsm"
                && ext != "sbs"
            {
                continue;
            }
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let name = stem.clone();
            let key = (cat_id, name.to_lowercase());
            if !seen.insert(key) {
                continue; // cache asset with the same name already owns this slot
            }
            let (pbr_color, pbr_metallic, pbr_roughness) = infer_pbr(&stem);
            out.push(ShelfAsset {
                name,
                category: cat_id,
                path: p.clone(),
                thumb_path: p,
                badge,
                badge_color,
                pbr_color,
                pbr_metallic,
                pbr_roughness,
                item_dir: None,
                icon_kind: crate::painting::material_library::IconKind::Flat,
                search_text: stem.to_lowercase(),
            });
        }
    }
}

fn scan_shelf_assets() -> Vec<ShelfAsset> {
    let shelf_dir = find_shelf_dir();
    let cache_base = crate::painting::material_library::find_material_cache_dir();
    let cache_items = crate::painting::material_library::scan_material_cache(&cache_base);

    let mut assets: Vec<ShelfAsset> = Vec::new();

    // 1. Reference cache assets (the primary source of truth).
    for item in &cache_items {
        let (cat_id, badge, badge_color) = match item.group {
            crate::painting::material_library::MaterialGroup::SmartMaterials => {
                (1, "SMAT", SMART_MAT_ORANGE)
            }
            crate::painting::material_library::MaterialGroup::Materials => (2, "MAT", ACTIVE_BLUE),
            crate::painting::material_library::MaterialGroup::Effects
            | crate::painting::material_library::MaterialGroup::Generators => {
                (5, "FX", Color32::from_rgb(200, 90, 40))
            }
            crate::painting::material_library::MaterialGroup::Procedurals
            | crate::painting::material_library::MaterialGroup::Alphas => {
                (4, "ALPHA", Color32::from_rgb(46, 180, 110))
            }
        };
        let (pbr_color, pbr_metallic, pbr_roughness) = if cat_id == 2 || cat_id == 1 {
            let params = crate::painting::material_library::load_preset_params(&item.dir);
            (
                params
                    .color
                    .map(|c| [c[0], c[1], c[2], 1.0])
                    .unwrap_or(infer_pbr(&item.label).0),
                params.metallic.unwrap_or(0.0),
                params.roughness.unwrap_or(0.5),
            )
        } else {
            infer_pbr(&item.label)
        };
        assets.push(ShelfAsset {
            name: item.label.clone(),
            category: cat_id,
            path: item.dir.clone(),
            thumb_path: item.thumb_path.clone(),
            badge,
            badge_color,
            pbr_color,
            pbr_metallic,
            pbr_roughness,
            item_dir: Some(item.dir.clone()),
            icon_kind: item.icon_kind,
            search_text: item.search_text(),
        });
    }

    let mut seen: std::collections::HashSet<(usize, String)> = assets
        .iter()
        .map(|a| (a.category, a.name.to_lowercase()))
        .collect();

    // 2. Scan core bundled PBR materials from assets/materials/<name>
    if let Some(materials_dir) = crate::paths::find_assets_subdir("materials") {
        if let Ok(entries) = std::fs::read_dir(&materials_dir) {
            let mut list: Vec<_> = entries.flatten().collect();
            list.sort_by_key(|e| e.file_name());
            for entry in list {
                let p = entry.path();
                if !p.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let key = (2, name.to_lowercase());
                if !seen.insert(key) {
                    continue;
                }
                let (pbr_color, pbr_metallic, pbr_roughness) = infer_pbr(&name);
                let thumb_path = p.join("base_color.png");
                assets.push(ShelfAsset {
                    name: name.clone(),
                    category: 2, // Materials
                    path: p.clone(),
                    thumb_path: if thumb_path.exists() {
                        thumb_path
                    } else {
                        p.clone()
                    },
                    badge: "MAT",
                    badge_color: ACTIVE_BLUE,
                    pbr_color,
                    pbr_metallic,
                    pbr_roughness,
                    item_dir: Some(p),
                    icon_kind: crate::painting::material_library::IconKind::Flat,
                    search_text: format!("{} pbr material", name.to_lowercase()),
                });
            }
        }
    }

    // 3. Bundled Starter Smart Materials (multi-layer PBR hierarchy with wear)
    let starter_smart_mats = [
        ("Steel Painted Worn", [0.18, 0.42, 0.22, 1.0], 0.1, 0.45),
        ("Copper Patina", [0.22, 0.65, 0.55, 1.0], 0.2, 0.70),
        ("Rusted Iron", [0.45, 0.22, 0.12, 1.0], 0.15, 0.85),
        ("Gold Scratched", [1.0, 0.78, 0.28, 1.0], 1.0, 0.35),
        ("Bronze Antique", [0.65, 0.45, 0.25, 1.0], 0.85, 0.40),
        ("Aluminium Scratched", [0.85, 0.86, 0.88, 1.0], 0.95, 0.30),
    ];
    for (sm_name, col, met, rgh) in starter_smart_mats {
        let key = (1, sm_name.to_lowercase());
        if seen.insert(key) {
            assets.push(ShelfAsset {
                name: sm_name.to_string(),
                category: 1, // Smart Materials
                path: PathBuf::from(sm_name),
                thumb_path: PathBuf::new(),
                badge: "SMAT",
                badge_color: SMART_MAT_ORANGE,
                pbr_color: col,
                pbr_metallic: met,
                pbr_roughness: rgh,
                item_dir: None,
                icon_kind: crate::painting::material_library::IconKind::Flat,
                search_text: format!("{} smart material worn wear", sm_name.to_lowercase()),
            });
        }
    }

    // 4. Bundled Starter Smart Masks (procedural curvature & AO generators)
    let starter_smart_masks = [
        "Metal Edge Wear",
        "Dirt Crevices",
        "Cavity Dust",
        "Surface Scratches",
    ];
    for mask_name in starter_smart_masks {
        let key = (3, mask_name.to_lowercase());
        if seen.insert(key) {
            assets.push(ShelfAsset {
                name: mask_name.to_string(),
                category: 3, // Smart Masks
                path: PathBuf::from(mask_name),
                thumb_path: PathBuf::new(),
                badge: "MSK",
                badge_color: Color32::from_rgb(160, 60, 220),
                pbr_color: [1.0, 1.0, 1.0, 1.0],
                pbr_metallic: 0.0,
                pbr_roughness: 0.5,
                item_dir: None,
                icon_kind: crate::painting::material_library::IconKind::Flat,
                search_text: format!("{} smart mask generator", mask_name.to_lowercase()),
            });
        }
    }

    // 5. Bundled Starter Alphas (procedural brush tips)
    let starter_alphas = [
        "Soft Round",
        "Hard Square",
        "Noise Splatter",
        "Scratch Streak",
    ];
    for alpha_name in starter_alphas {
        let key = (4, alpha_name.to_lowercase());
        if seen.insert(key) {
            assets.push(ShelfAsset {
                name: alpha_name.to_string(),
                category: 4, // Alphas
                path: PathBuf::from(alpha_name),
                thumb_path: PathBuf::new(),
                badge: "ALPHA",
                badge_color: Color32::from_rgb(46, 180, 110),
                pbr_color: [1.0, 1.0, 1.0, 1.0],
                pbr_metallic: 0.0,
                pbr_roughness: 0.5,
                item_dir: None,
                icon_kind: crate::painting::material_library::IconKind::Flat,
                search_text: format!("{} brush alpha procedural", alpha_name.to_lowercase()),
            });
        }
    }

    // Legacy shelf sources: kept for anything the cache does not already cover.
    // `assets/materials` is deliberately OUT — those SP sphere-render previews are
    // forbidden as content and must never be interpreted as flat maps.
    legacy_scan_category(
        &shelf_dir,
        1,
        "smart_materials",
        "SM",
        SMART_MAT_ORANGE,
        &mut assets,
        &mut seen,
    );
    legacy_scan_category(
        &shelf_dir,
        3,
        "smart_masks",
        "MSK",
        Color32::from_rgb(160, 60, 220),
        &mut assets,
        &mut seen,
    );
    legacy_scan_category(
        &shelf_dir,
        4,
        "alphas",
        "A",
        Color32::from_rgb(46, 180, 110),
        &mut assets,
        &mut seen,
    );

    // Materials now come exclusively from material_cache/materials (fill maps).

    assets
}

impl Default for RemoainterApp {
    fn default() -> Self {
        let config = AppConfig::load();
        let autosave_interval = config.autosave_interval_secs;
        let default_export = PathBuf::from(&config.default_export_dir);
        let color_mgmt = config.color_management_mode;

        let mut app = Self {
            active_tool: ToolMode::Paint,
            brush: Brush::default(),
            layer_stack: LayerStack::new(1024, 1024),
            mesh: Mesh::create_material_sphere(64, 64),
            baker_options: BakerOptions::default(),
            baked_maps: BakedMaps::default(),
            is_baking: false,
            baking_progress: Arc::new(AtomicUsize::new(0)),
            cancel_baking: Arc::new(AtomicBool::new(false)),
            baking_receiver: None,
            show_baker_dialog: false,
            baker_selected_map: 99, // Default to Common settings (High-Poly parameters & Output)
            baker_subsampling: 0,
            baker_use_low_as_high: true,
            baker_frontal_dist: 0.08,
            baker_skew_enabled: false,
            baker_skew_invert: false,
            baker_skew_strength: 0.6,
            baker_skew_map_path: String::new(),
            baker_skew_map: None,
            baker_rear_dist: 0.08,
            baker_curv_method: 0,
            baker_high_poly_path: String::new(),
            baker_high_poly_mesh: None,
            baker_low_suffix: "_low".into(),
            baker_high_suffix: "_high".into(),
            baker_relative_to_bbox: false,
            baker_average_normals: true,
            baker_ignore_backfaces: false,

            display_channel: 0,
            uv_zoom: 1.0,
            uv_pan: [0.0, 0.0],
            env_preset: 1, // Warm & Cool Studio
            env_rotation: 0.0,
            light_intensity: 1.0,
            camera_yaw: 35.0,
            camera_pitch: 18.0,
            camera_dist: 2.2,
            symmetry_x: false,
            symmetry_y: false,
            symmetry_z: false,

            camera_bookmarks: Default::default(),
            turntable_active: false,
            turntable_speed: 45.0,
            isolated_submesh: None,
            bloom_enabled: false,
            bloom_intensity: 0.8,
            bloom_threshold: 0.85,
            ssao_enabled: true,
            ssao_intensity: 1.0,
            dof_enabled: false,
            dof_focal_distance: 2.2,
            dof_aperture: 0.08,
            history_manager: crate::painting::HistoryManager::new(64),
            lazy_mouse_tracker: crate::painting::brush::LazyMouseTracker::default(),
            is_in_stroke: false,

            shelf_category: 0,
            search_query: String::new(),
            shelf_assets: scan_shelf_assets(),
            shelf_textures: HashMap::new(),
            logo_texture: None,
            active_right_panel: 2, // Layers panel open by default

            selected_model_idx: 0, // Default to Material Calibration Sphere (Industry standard)
            status_message: "Ready".to_string(),
            history: vec![
                "01  New Project (Material Calibration Sphere)".into(),
                "02  Load Mesh (Calibration Sphere)".into(),
                "03  Apply Smart Material (Steel Painted Scraped Green)".into(),
            ],
            logs: vec![
                (
                    "09:00:01".into(),
                    "INFO".into(),
                    "REMoAINTER v2.0.0 (High-Performance Rust Core) initialized.".into(),
                ),
                (
                    "09:00:02".into(),
                    "INFO".into(),
                    "reference PBR suite 1-to-1 Architecture & UI Active.".into(),
                ),
                (
                    "09:00:03".into(),
                    "INFO".into(),
                    "Loaded 991+ authentic reference PBR suite starter assets (materials, smart mats, effects, generators, procedurals & alphas).".into(),
                ),
                (
                    "09:00:04".into(),
                    "INFO".into(),
                    "Cook-Torrance / Blinn-Phong GGX Studio Lighting active.".into(),
                ),
            ],
            last_painted_uv: None,
            paint_target_mask: false,
            layer_clipboard: None,
            texture_handle: None,
            texture_needs_update: true,
            checker_handle: None,
            viewport_view_mode: 2, // 3D | 2D Split View as default on startup (reference PBR suite parity)
            show_wireframe: false,
            show_uv_wireframe: true,
            show_grid: true,

            wgpu_device: None,
            wgpu_queue: None,
            viewport_wgpu: None,
            available_environments: list_available_environments(),
            selected_env_idx: 0,
            exposure: 0.0,
            skybox_visible: true,
            skybox_blur: 0.25,
            skybox_intensity: 1.0,
            uv_scale: 1.0,

            show_texture_importer_dialog: false,
            import_texture_set: PbrTextureSet::default(),
            import_flip_normal_y: false,

            texture_channels: vec![
                TextureChannel {
                    name: "Base color".into(),
                    format: "sRGB8".into(),
                    enabled: true,
                },
                TextureChannel {
                    name: "Metallic".into(),
                    format: "L8".into(),
                    enabled: true,
                },
                TextureChannel {
                    name: "Roughness".into(),
                    format: "L8".into(),
                    enabled: true,
                },
                TextureChannel {
                    name: "Normal".into(),
                    format: "RGB16F".into(),
                    enabled: true,
                },
                TextureChannel {
                    name: "Height".into(),
                    format: "L16F".into(),
                    enabled: true,
                },
            ],
            texture_set_manager: crate::painting::TextureSetManager::new(2048),

            show_export_dialog: false,
            export_output_dir: default_export,
            export_preset_idx: 0,
            export_resolution: 2048,
            export_file_format: 0,
            export_padding: 16,
            export_padding_algorithm: 0,

            show_graph_dialog: false,
            show_sbsar_import_dialog: false,
            imported_sbsar: None,
            sbsar_param_values: std::collections::BTreeMap::new(),
            sbsar_status: String::new(),
            graph_template_idx: 0,
            graph_seed: 7,
            graph_scale: 8.0,
            graph_contrast: 0.5,
            graph_engine: None,
            graph_preview_tex: None,
            graph_status: String::new(),
            export_status: String::new(),
            baker_tab_idx: 0,

            show_new_project_dialog: false,
            new_proj_mesh_path: String::new(),
            new_proj_template_idx: 0,
            new_proj_resolution: 2048,
            new_proj_normal_directx: false,
            new_proj_udim: false,
            new_proj_import_cameras: true,
            poly_fill_mode: 1, // UV Island by default
            clone_source_uv: None,
            clone_offset_uv: None,
            decal_alpha: None,
            decal_text: "REM".to_string(),
            decal_scale: 1.0,
            decal_angle: 0.0,
            decal_font_size: 64.0,
            decal_fonts: Vec::new(),
            decal_font_path: String::new(),

            project_name: "Untitled Project".to_string(),
            current_project_path: None,
            project_is_dirty: false,
            last_autosave_time: std::time::Instant::now(),
            autosave_interval_secs: autosave_interval,
            mesh_source_path: None,
            snapshots: Vec::new(),
            show_snapshots_dialog: false,

            config,
            show_preferences_dialog: false,
            show_hotkeys_dialog: false,
            show_shortcuts_dialog: false,
            color_management_mode: color_mgmt,

            show_render_mode: false,
            render_options: crate::render::RenderOptions::default(),
            render_preset: crate::render::QualityPreset::Medium,
            render_custom_spp: 512,
            render_state: None,
            render_film_handle: None,
            render_last_preview_passes: 0,
            render_last_preview: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(1))
                .unwrap_or_else(std::time::Instant::now),
            render_logged: false,
        };

        // Material Calibration Sphere is active by default
        app.mesh = Mesh::create_material_sphere(64, 64);
        app.selected_model_idx = 0;
        app.status_message = "Loaded Calibration Material Sphere".to_string();

        // High-speed GPU-grade mesh map baking on startup (<15ms forward rasterizer)
        if !app.mesh.indices.is_empty() {
            app.baked_maps = BakerEngine::bake_all(
                &app.mesh.vertices,
                &app.mesh.normals,
                &app.mesh.uvs,
                &app.mesh.indices,
                &app.mesh.submeshes,
                &app.baker_options,
                None,
                None,
                None,
            );
        } else {
            let total = (1024 * 1024) as usize;
            app.baked_maps = BakedMaps {
                resolution: 1024,
                ambient_occlusion: Some(vec![1.0; total]),
                curvature: Some(vec![0.5; total]),
                normal: Some(vec![[0.5, 0.5, 1.0]; total]),
                world_space_normal: Some(vec![[0.5, 1.0, 0.5]; total]),
                position: Some(vec![[0.5, 0.5, 0.5]; total]),
                thickness: Some(vec![0.5; total]),
                id: Some(vec![[0.8, 0.8, 0.8, 1.0]; total]),
                is_cancelled: false,
            };
        }

        // Load clean base PBR material and user paint detail layer on startup (reference standard)
        let w = app.layer_stack.width;
        let h = app.layer_stack.height;
        let base_mat = load_real_pbr_material(
            1,
            "Steel Painted",
            [0.75, 0.76, 0.78, 1.0],
            0.95,
            0.32,
            w,
            h,
        );
        let paint_layer = Layer::new_paint(2, "Paint Layer 1", w, h);
        app.layer_stack.layers.push(base_mat);
        app.layer_stack.layers.push(paint_layer);
        app.layer_stack.active_layer_index = 1;

        app.recompute_composite();
        app
    }
}

impl RemoainterApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let mut app = Self::default();
        if let Some(render_state) = &cc.wgpu_render_state {
            app.wgpu_device = Some(render_state.device.clone());
            app.wgpu_queue = Some(render_state.queue.clone());
            match ViewportWgpu::new(
                &render_state.device,
                &render_state.queue,
                render_state.target_format,
                &app.mesh,
                &app.layer_stack,
                &app.baked_maps,
            ) {
                Ok(vp) => {
                    app.viewport_wgpu = Some(Arc::new(Mutex::new(vp)));
                }
                Err(e) => {
                    eprintln!(
                        "[ERROR] Failed to initialize Vulkan/wgpu PBR Viewport: {}",
                        e
                    );
                }
            }
        }
        app
    }

    pub fn get_env_preset(&self) -> EnvironmentPreset {
        match self.env_preset {
            0 => EnvironmentPreset::StudioTomoco,
            1 => EnvironmentPreset::GlazedPatio,
            2 => EnvironmentPreset::Panorama,
            _ => EnvironmentPreset::WarmCoolStudio,
        }
    }

    fn log_line(&mut self, level: &str, text: &str) {
        self.logs.push((
            chrono::Local::now().format("%H:%M:%S").to_string(),
            level.to_string(),
            text.to_string(),
        ));
    }

    /// Unified shelf click behavior for every asset category:
    /// 1 Smart Mats / 2 Materials — cache-backed SP fill-layer instantiation
    ///   (legacy shelf files keep their old paths);
    /// 3 Smart Masks — legacy smart-mask apply on the active layer;
    /// 4 Alphas / procedurals — brush custom alpha from a flat (non-sphere) icon;
    /// 5 Effects/Generators — SP generator applied as a mask on the active layer.
    pub fn handle_shelf_click(&mut self, asset: ShelfAsset) {
        use crate::painting::material_library as sl;
        let name = asset.name.clone();
        match asset.category {
            // Cache-backed materials & smart materials: SP-style fill-layer instantiation.
            1 | 2 => {
                if let Some(dir) = asset.item_dir.clone() {
                    let next_id = self
                        .layer_stack
                        .layers
                        .iter()
                        .map(|l| l.id)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    match sl::build_fill_layer(
                        &dir,
                        next_id,
                        self.layer_stack.width,
                        self.layer_stack.height,
                    ) {
                        Ok((layer, kind)) => {
                            if kind == sl::ReferenceContentKind::FlatParams
                                && asset.icon_kind == sl::IconKind::Sphere
                            {
                                self.log_line(
                                    "WARN",
                                    &format!(
                                        "'{}': sphere-render SP icon was NOT used as paint content; applied parameter-only fill",
                                        name
                                    ),
                                );
                            }
                            self.brush.color = layer.fill_color;
                            self.brush.metallic = layer.fill_metallic;
                            self.brush.roughness = layer.fill_roughness;
                            let layer_name = layer.name.clone();
                            self.layer_stack.layers.push(layer);
                            self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                            self.recompute_composite();
                            let note = match kind {
                                sl::ReferenceContentKind::FlatParams => "parameters-only fill",
                                _ => "preview-derived content",
                            };
                            self.status_message =
                                format!("Applied SP material: {} ({})", layer_name, note);
                            self.history.push(format!(
                                "{:02}  Apply SP Material ({}) [{}]",
                                self.history.len() + 1,
                                layer_name,
                                note
                            ));
                        }
                        Err(sl::ReferenceApplyError::ForbiddenSphereBitmap) => {
                            self.status_message =
                                format!("Refused sphere-render icon as paint content: {}", name);
                            {
                                let m = self.status_message.clone();
                                self.log_line("WARN", &m);
                            }
                        }
                        Err(sl::ReferenceApplyError::Io(e)) => {
                            self.status_message =
                                format!("Failed to apply SP material: {}: {}", name, e);
                            {
                                let m = self.status_message.clone();
                                self.log_line("ERROR", &m);
                            }
                        }
                    }
                } else {
                    // Legacy shelf previews (no cache entry) keep their old paths.
                    self.brush.color = asset.pbr_color;
                    self.brush.metallic = asset.pbr_metallic;
                    self.brush.roughness = asset.pbr_roughness;
                    if asset.category == 1 {
                        apply_smart_material_hierarchy(
                            &mut self.layer_stack,
                            &name,
                            asset.pbr_color,
                            asset.pbr_metallic,
                            asset.pbr_roughness,
                            self.baked_maps.curvature.as_deref(),
                            self.baked_maps.ambient_occlusion.as_deref(),
                        );
                        self.recompute_composite();
                        self.status_message = format!("Applied Smart Material: {}", name);
                        self.history.push(format!(
                            "{:02}  Apply Smart Material ({})",
                            self.history.len() + 1,
                            name
                        ));
                    } else {
                        let id = self.layer_stack.layers.len() + 1;
                        let fill = load_real_pbr_material(
                            id,
                            &name,
                            asset.pbr_color,
                            asset.pbr_metallic,
                            asset.pbr_roughness,
                            self.layer_stack.width,
                            self.layer_stack.height,
                        );
                        self.layer_stack.layers.push(fill);
                        self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                        self.recompute_composite();
                        self.status_message = format!("Applied Real PBR Material: {}", name);
                        self.history.push(format!(
                            "{:02}  Apply Real PBR Material ({})",
                            self.history.len() + 1,
                            name
                        ));
                    }
                }
            }
            // Legacy shell smart masks (.spmsk / .webp previews).
            3 => {
                if let Some(active_layer) = self
                    .layer_stack
                    .layers
                    .get_mut(self.layer_stack.active_layer_index)
                {
                    let path = asset.path.clone();
                    active_layer.apply_smart_mask_file(
                        &path,
                        self.baked_maps.curvature.as_deref(),
                        self.baked_maps.ambient_occlusion.as_deref(),
                    );
                    self.status_message = format!(
                        "Applied Smart Mask ({}) to Layer: {}",
                        name, active_layer.name
                    );
                    self.history.push(format!(
                        "{:02}  Apply Smart Mask ({})",
                        self.history.len() + 1,
                        name
                    ));
                }
                self.recompute_composite();
            }
            // Alphas / procedurals: brush custom alpha from a flat SP thumbnail.
            4 => {
                let mut applied = false;
                if let Some(dir) = asset.item_dir.clone() {
                    match sl::build_alpha_mask(&dir) {
                        Ok(alpha) => {
                            self.brush.custom_alpha = Some(Arc::new(alpha));
                            applied = true;
                        }
                        Err(sl::ReferenceApplyError::ForbiddenSphereBitmap) => {
                            self.status_message =
                                format!("'{}': sphere-render icon cannot be used as alpha", name);
                            {
                                let m = self.status_message.clone();
                                self.log_line("WARN", &m);
                            }
                        }
                        Err(sl::ReferenceApplyError::Io(e)) => {
                            self.status_message = format!("Failed to load alpha '{}': {}", name, e);
                            {
                                let m = self.status_message.clone();
                                self.log_line("WARN", &m);
                            }
                        }
                    }
                } else if let Ok(img) = image::open(&asset.thumb_path) {
                    let gray = img.to_luma8();
                    let (w, h) = gray.dimensions();
                    self.brush.custom_alpha = Some(Arc::new(AlphaMaskData {
                        width: w,
                        height: h,
                        data: gray.into_raw(),
                    }));
                    applied = true;
                }
                if applied {
                    self.brush.alpha_name = name.clone();
                    self.brush.alpha_shape = crate::painting::brush::AlphaShape::Custom;
                    self.status_message = format!("Selected Alpha: {}", name);
                    self.history.push(format!(
                        "{:02}  Select Alpha ({})",
                        self.history.len() + 1,
                        name
                    ));
                }
            }
            // Effects / generators: SP generator behavior applied as a layer mask.
            5 => {
                if let Some(active_layer) = self
                    .layer_stack
                    .layers
                    .get_mut(self.layer_stack.active_layer_index)
                {
                    let (w, h) = (self.layer_stack.width, self.layer_stack.height);
                    let mask_name = asset.name.clone();
                    active_layer.mask = Some(crate::painting::material_gen::generate_smart_mask(
                        &mask_name,
                        self.baked_maps.curvature.as_deref(),
                        self.baked_maps.ambient_occlusion.as_deref(),
                        w,
                        h,
                    ));
                    self.status_message =
                        format!("Applied SP Effect/Generator: {} (as layer generator)", name);
                    self.history.push(format!(
                        "{:02}  Apply SP Generator ({})",
                        self.history.len() + 1,
                        name
                    ));
                }
                self.recompute_composite();
            }
            _ => {}
        }
    }

    pub fn recompute_composite(&mut self) {
        let preset = self.get_env_preset();
        let rot = self.env_rotation.to_radians();
        let intensity = self.light_intensity;
        self.layer_stack.composite(
            self.baked_maps.curvature.as_deref(),
            self.baked_maps.ambient_occlusion.as_deref(),
            self.baked_maps.world_space_normal.as_deref(),
            self.baked_maps.position.as_deref(),
            self.baked_maps.thickness.as_deref(),
            self.baked_maps.id.as_deref(),
            preset,
            rot,
            intensity,
        );
        self.texture_needs_update = true;

        if let Some(ref vp_mutex) = self.viewport_wgpu {
            if let (Some(ref device), Some(ref queue)) = (&self.wgpu_device, &self.wgpu_queue) {
                if let Ok(mut vp) = vp_mutex.lock() {
                    vp.upload_textures(device, queue, &self.layer_stack, &self.baked_maps);
                }
            }
        }
    }

    pub fn recompute_dirty(&mut self, bbox: [i32; 4]) {
        let preset = self.get_env_preset();
        let rot = self.env_rotation.to_radians();
        let intensity = self.light_intensity;
        self.layer_stack.composite_dirty(
            bbox,
            self.baked_maps.ambient_occlusion.as_deref(),
            preset,
            rot,
            intensity,
        );
        self.texture_needs_update = true;

        if let Some(ref vp_mutex) = self.viewport_wgpu {
            if let Some(ref queue) = self.wgpu_queue {
                if let Ok(mut vp) = vp_mutex.lock() {
                    vp.upload_dirty(queue, &self.layer_stack, bbox);
                }
            }
        }
    }

    pub fn perform_undo(&mut self) {
        if let Some(desc) = self.history_manager.undo(&mut self.layer_stack) {
            self.recompute_composite();
            let msg = format!("Undo: {}", desc);
            self.status_message = msg.clone();
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                msg,
            ));
        } else {
            self.status_message = "Nothing to undo".into();
        }
    }

    pub fn perform_redo(&mut self) {
        if let Some(desc) = self.history_manager.redo(&mut self.layer_stack) {
            self.recompute_composite();
            let msg = format!("Redo: {}", desc);
            self.status_message = msg.clone();
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                msg,
            ));
        } else {
            self.status_message = "Nothing to redo".into();
        }
    }

    pub fn channel_name(&self, channel: usize) -> &'static str {
        match channel {
            0 => "Material (M)",
            1 => "Base Color (C)",
            2 => "Roughness",
            3 => "Metallic",
            4 => "Normal",
            5 => "Ambient Occlusion (B)",
            6 => "Curvature",
            7 => "Position",
            8 => "Thickness",
            _ => "Material",
        }
    }
    pub fn bake_mesh_maps(&mut self) {
        if self.is_baking {
            return;
        }
        self.is_baking = true;
        self.cancel_baking.store(false, Ordering::Relaxed);
        self.baking_progress.store(0, Ordering::Relaxed);

        let progress = Arc::clone(&self.baking_progress);
        let cancel = Arc::clone(&self.cancel_baking);

        // Sync baker options from UI state
        self.baker_options.max_frontal_distance = self.baker_frontal_dist;
        self.baker_options.max_rear_distance = self.baker_rear_dist;
        self.baker_options.low_poly_suffix = self.baker_low_suffix.clone();
        self.baker_options.high_poly_suffix = self.baker_high_suffix.clone();
        self.baker_options.relative_to_bbox = self.baker_relative_to_bbox;
        self.baker_options.average_normals = self.baker_average_normals;
        self.baker_options.ignore_backfaces = self.baker_ignore_backfaces;
        self.baker_options.use_skew_correction =
            self.baker_skew_enabled && self.baker_skew_map.is_some();
        self.baker_options.skew_invert = self.baker_skew_invert;
        self.baker_options.skew_strength = self.baker_skew_strength;
        self.baker_options.skew_map = self.baker_skew_map.clone();
        self.baker_options.anti_aliasing = match self.baker_subsampling {
            1 => 2,
            2 => 4,
            _ => 1,
        };

        let verts = self.mesh.vertices.clone();
        let norms = self.mesh.normals.clone();
        let uvs = self.mesh.uvs.clone();
        let inds = self.mesh.indices.clone();
        let subms = self.mesh.submeshes.clone();
        let opts = self.baker_options.clone();
        let hp_mesh = if !self.baker_use_low_as_high {
            self.baker_high_poly_mesh.clone()
        } else {
            None
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.baking_receiver = Some(rx);

        std::thread::spawn(move || {
            let result = BakerEngine::bake_all(
                &verts,
                &norms,
                &uvs,
                &inds,
                &subms,
                &opts,
                Some(progress),
                Some(cancel),
                hp_mesh.as_ref(),
            );
            let _ = tx.send(result);
        });

        self.status_message = format!(
            "Baking mesh maps ({}x{})...",
            self.baker_options.resolution, self.baker_options.resolution
        );
        self.logs.push((
            chrono::Local::now().format("%H:%M:%S").to_string(),
            "INFO".into(),
            format!(
                "Started background bake (res={}, AA={}x) for {}",
                self.baker_options.resolution, self.baker_options.anti_aliasing, self.mesh.name
            ),
        ));
    }

    pub fn bake_single_map(&mut self, map_type: MapType) {
        self.is_baking = true;
        self.cancel_baking.store(false, Ordering::Relaxed);
        self.baking_progress.store(0, Ordering::Relaxed);

        let progress = Arc::clone(&self.baking_progress);
        let cancel = Arc::clone(&self.cancel_baking);

        self.baker_options.max_frontal_distance = self.baker_frontal_dist;
        self.baker_options.max_rear_distance = self.baker_rear_dist;
        self.baker_options.low_poly_suffix = self.baker_low_suffix.clone();
        self.baker_options.high_poly_suffix = self.baker_high_suffix.clone();
        self.baker_options.anti_aliasing = match self.baker_subsampling {
            1 => 2,
            2 => 4,
            _ => 1,
        };

        let hp_mesh = if !self.baker_use_low_as_high {
            self.baker_high_poly_mesh.as_ref()
        } else {
            None
        };

        let single = BakerEngine::bake_single(
            map_type,
            &self.mesh.vertices,
            &self.mesh.normals,
            &self.mesh.uvs,
            &self.mesh.indices,
            &self.mesh.submeshes,
            &self.baker_options,
            Some(progress),
            Some(cancel),
            hp_mesh,
        );

        if !single.is_cancelled {
            match map_type {
                MapType::AmbientOcclusion => {
                    self.baked_maps.ambient_occlusion = single.ambient_occlusion
                }
                MapType::Curvature => self.baked_maps.curvature = single.curvature,
                MapType::Normal => self.baked_maps.normal = single.normal,
                MapType::WorldSpaceNormal => {
                    self.baked_maps.world_space_normal = single.world_space_normal
                }
                MapType::Position => self.baked_maps.position = single.position,
                MapType::Thickness => self.baked_maps.thickness = single.thickness,
                MapType::Id => self.baked_maps.id = single.id,
            }
            self.status_message = format!(
                "Baked {:?} map at {}x{}",
                map_type, self.baker_options.resolution, self.baker_options.resolution
            );
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                format!(
                    "Baked {:?} map at {}x{}",
                    map_type, self.baker_options.resolution, self.baker_options.resolution
                ),
            ));
            self.recompute_composite();
        } else {
            self.status_message = format!("Cancelled baking {:?}", map_type);
        }
        self.is_baking = false;
    }

    pub fn load_model(&mut self, idx: usize) {
        self.selected_model_idx = idx;
        match idx {
            0 => {
                self.mesh = Mesh::create_material_sphere(64, 64);
                self.status_message = "Loaded Material Sphere".to_string();
            }
            1 => {
                let paths: Vec<PathBuf> = [
                    "test_models/lantern.obj",
                    "../assets/test_models/lantern.obj",
                ]
                .iter()
                .filter_map(|sub| crate::paths::find_assets_subdir(sub))
                .collect();
                for p in &paths {
                    if let Ok(m) = Mesh::load_obj(p) {
                        self.mesh = m;
                        self.status_message = "Loaded Test Asset: Antique Lantern".to_string();
                        break;
                    }
                }
            }
            2 => {
                let paths: Vec<PathBuf> =
                    ["test_models/radio.obj", "../assets/test_models/radio.obj"]
                        .iter()
                        .filter_map(|sub| crate::paths::find_assets_subdir(sub))
                        .collect();
                for p in &paths {
                    if let Ok(m) = Mesh::load_obj(p) {
                        self.mesh = m;
                        self.status_message = "Loaded Test Asset: Retro Radio".to_string();
                        break;
                    }
                }
            }
            3 => {
                self.mesh = Mesh::create_cube();
                self.status_message = "Loaded Primitive: Cube".to_string();
            }
            4 => {
                self.mesh = Mesh::create_plane();
                self.status_message = "Loaded Primitive: Plane".to_string();
            }
            _ => {}
        }
        if !self.mesh.indices.is_empty() {
            self.baked_maps = BakerEngine::bake_all(
                &self.mesh.vertices,
                &self.mesh.normals,
                &self.mesh.uvs,
                &self.mesh.indices,
                &self.mesh.submeshes,
                &self.baker_options,
                None,
                None,
                None,
            );
            if self.layer_stack.layers.is_empty() {
                let w = self.layer_stack.width;
                let h = self.layer_stack.height;
                let base_mat = load_real_pbr_material(
                    1,
                    "Steel Painted",
                    [0.75, 0.76, 0.78, 1.0],
                    0.95,
                    0.32,
                    w,
                    h,
                );
                let paint_layer = Layer::new_paint(2, "Paint Layer 1", w, h);
                self.layer_stack.layers.push(base_mat);
                self.layer_stack.layers.push(paint_layer);
                self.layer_stack.active_layer_index = 1;
            }
        }
        if let Some(ref vp_mutex) = self.viewport_wgpu {
            if let (Some(ref device), Some(ref queue)) = (&self.wgpu_device, &self.wgpu_queue) {
                if let Ok(mut vp) = vp_mutex.lock() {
                    vp.set_mesh(device, queue, &self.mesh);
                }
            }
        }
        self.recompute_composite();
        self.status_message = format!("Loaded {} with baked mesh maps", self.mesh.name);
    }

    pub fn import_mesh_file(&mut self, path: &std::path::Path) {
        match Mesh::load_file(path) {
            Ok(loaded_mesh) => {
                self.mesh = loaded_mesh;
                self.mesh_source_path = Some(path.display().to_string());
                self.selected_model_idx = 99; // custom imported mesh
                let time_str = chrono::Local::now().format("%H:%M:%S").to_string();
                self.status_message = format!("Imported 3D Model: {}", path.display());
                self.history.push(format!(
                    "{:02}  Import Mesh ({})",
                    self.history.len() + 1,
                    self.mesh.name
                ));
                self.logs.push((
                    time_str.clone(),
                    "INFO".into(),
                    format!(
                        "Successfully loaded mesh {} ({} vertices, {} triangles)",
                        self.mesh.name,
                        self.mesh.vertices.len(),
                        self.mesh.indices.len() / 3
                    ),
                ));

                // Re-bake all mesh maps for the newly loaded mesh
                self.baked_maps = BakerEngine::bake_all(
                    &self.mesh.vertices,
                    &self.mesh.normals,
                    &self.mesh.uvs,
                    &self.mesh.indices,
                    &self.mesh.submeshes,
                    &self.baker_options,
                    None,
                    None,
                    None,
                );

                if let Some(ref vp_mutex) = self.viewport_wgpu {
                    if let (Some(ref device), Some(ref queue)) =
                        (&self.wgpu_device, &self.wgpu_queue)
                    {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.set_mesh(device, queue, &self.mesh);
                        }
                    }
                }

                if self.layer_stack.layers.is_empty() {
                    let w = self.layer_stack.width;
                    let h = self.layer_stack.height;
                    let base_mat = load_real_pbr_material(
                        1,
                        "Steel Painted",
                        [0.75, 0.76, 0.78, 1.0],
                        0.95,
                        0.32,
                        w,
                        h,
                    );
                    let paint_layer = Layer::new_paint(2, "Paint Layer 1", w, h);
                    self.layer_stack.layers.push(base_mat);
                    self.layer_stack.layers.push(paint_layer);
                    self.layer_stack.active_layer_index = 1;
                }

                self.texture_set_manager
                    .configure_from_mesh(&self.mesh, self.layer_stack.width);
                self.recompute_composite();
                self.status_message = format!(
                    "Loaded {} ({} vertices, {} triangles) with baked mesh maps",
                    self.mesh.name,
                    self.mesh.vertices.len(),
                    self.mesh.indices.len() / 3
                );
            }
            Err(e) => {
                let err_msg = format!("Failed to import 3D model {}: {}", path.display(), e);
                self.status_message = err_msg.clone();
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "ERROR".into(),
                    err_msg,
                ));
            }
        }
    }

    /// Saves the active project to current path or prompts for path
    pub fn save_project(&mut self) {
        if let Some(ref path) = self.current_project_path.clone() {
            let res = crate::project::ProjectManager::save_project(
                path,
                &self.project_name,
                &self.mesh,
                self.mesh_source_path.as_deref(),
                &self.layer_stack,
                &self.baked_maps,
                &self.baker_options,
                (self.camera_yaw, self.camera_pitch, self.camera_dist),
                (
                    self.exposure,
                    self.light_intensity,
                    self.env_rotation,
                    self.selected_env_idx,
                ),
            );
            match res {
                Ok(_) => {
                    self.project_is_dirty = false;
                    self.config.add_recent(path.display().to_string());
                    self.status_message = format!("Saved project to {}", path.display());
                    self.history
                        .push(format!("{:02}  Save Project", self.history.len() + 1));
                    self.logs.push((
                        chrono::Local::now().format("%H:%M:%S").to_string(),
                        "INFO".into(),
                        format!("Saved project file: {}", path.display()),
                    ));
                }
                Err(e) => {
                    self.status_message = format!("Failed to save project: {}", e);
                    self.logs.push((
                        chrono::Local::now().format("%H:%M:%S").to_string(),
                        "ERROR".into(),
                        format!("Project save error: {}", e),
                    ));
                }
            }
        } else {
            self.save_project_as();
        }
    }

    /// Prompts user for file location and saves project
    pub fn save_project_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save REMoAINTER Project")
            .add_filter("REMoAINTER Project (*.rmp)", &["rmp"])
            .set_file_name(&format!("{}.rmp", self.project_name))
            .save_file()
        {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                self.project_name = stem.to_string();
            }
            self.current_project_path = Some(path);
            self.save_project();
        }
    }

    /// Opens a `.rmp` project file from path
    pub fn open_project_file(&mut self, path: &std::path::Path) {
        match crate::project::ProjectManager::load_project(path) {
            Ok((manifest, mesh, layer_stack, baked_maps, baker_options)) => {
                self.project_name = manifest.project_name;
                self.current_project_path = Some(path.to_path_buf());
                self.mesh = mesh;
                self.mesh_source_path = manifest.mesh_file_path;
                self.layer_stack = layer_stack;
                self.baked_maps = baked_maps;
                self.baker_options = baker_options;
                self.camera_yaw = manifest.camera_yaw;
                self.camera_pitch = manifest.camera_pitch;
                self.camera_dist = manifest.camera_dist;
                self.exposure = manifest.exposure;
                self.light_intensity = manifest.light_intensity;
                self.env_rotation = manifest.env_rotation;
                self.selected_env_idx = manifest.selected_env_idx;
                self.project_is_dirty = false;
                self.texture_needs_update = true;

                // Update GPU mesh in Viewport
                if let Some(ref vp_mutex) = self.viewport_wgpu {
                    if let (Some(ref device), Some(ref queue)) =
                        (&self.wgpu_device, &self.wgpu_queue)
                    {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.set_mesh(device, queue, &self.mesh);
                        }
                    }
                }

                self.recompute_composite();
                self.config.add_recent(path.display().to_string());
                self.status_message = format!("Opened project: {}", path.display());
                self.history.push(format!(
                    "{:02}  Open Project ({})",
                    self.history.len() + 1,
                    self.project_name
                ));
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "INFO".into(),
                    format!(
                        "Loaded project archive: {} ({} layers)",
                        path.display(),
                        self.layer_stack.layers.len()
                    ),
                ));
            }
            Err(e) => {
                self.status_message = format!("Failed to open project: {}", e);
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "ERROR".into(),
                    format!("Project open error: {}", e),
                ));
            }
        }
    }

    /// Opens a `.rmp` project file from disk via file dialog
    pub fn open_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open REMoAINTER Project")
            .add_filter("REMoAINTER Project (*.rmp)", &["rmp"])
            .pick_file()
        {
            self.open_project_file(&path);
        }
    }

    /// Cuts active layer to layer clipboard
    pub fn cut_active_layer(&mut self) {
        let idx = self.layer_stack.active_layer_index;
        if idx < self.layer_stack.layers.len() {
            let l = self.layer_stack.layers.remove(idx);
            self.status_message = format!("Cut Layer: {}", l.name);
            self.layer_clipboard = Some(l);
            if self.layer_stack.active_layer_index >= self.layer_stack.layers.len()
                && !self.layer_stack.layers.is_empty()
            {
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
            }
            self.recompute_composite();
        }
    }

    /// Copies active layer to layer clipboard
    pub fn copy_active_layer(&mut self) {
        let idx = self.layer_stack.active_layer_index;
        if let Some(l) = self.layer_stack.layers.get(idx) {
            self.layer_clipboard = Some(l.clone());
            self.status_message = format!("Copied Layer: {}", l.name);
        }
    }

    /// Pastes layer from layer clipboard above active layer
    pub fn paste_layer(&mut self) {
        if let Some(l) = self.layer_clipboard.clone() {
            let mut new_l = l.clone();
            new_l.id = self.layer_stack.layers.len() + 1;
            new_l.name = format!("{} (Paste)", l.name);
            let insert_at =
                (self.layer_stack.active_layer_index + 1).min(self.layer_stack.layers.len());
            self.layer_stack.layers.insert(insert_at, new_l);
            self.layer_stack.active_layer_index = insert_at;
            self.recompute_composite();
            self.status_message = format!("Pasted Layer: {}", l.name);
        }
    }

    /// Resets workspace dock panels, viewport view mode, and tool to factory defaults
    pub fn reset_layout(&mut self) {
        self.active_right_panel = 2; // Layers
        self.viewport_view_mode = 2; // 3D | 2D Split View
        self.camera_yaw = 35.0;
        self.camera_pitch = 18.0;
        self.camera_dist = 2.2;
        self.active_tool = ToolMode::Paint;
        self.show_wireframe = false;
        self.show_uv_wireframe = true;
        self.turntable_active = false;
        self.isolated_submesh = None;
        if let Some(ref vp_mutex) = self.viewport_wgpu {
            if let Ok(mut vp) = vp_mutex.lock() {
                vp.isolate_submesh(None);
            }
        }
        self.status_message = "Workspace layout reset to default".into();
    }

    /// Automatically saves the project in the background
    pub fn trigger_autosave(&mut self) {
        let autosave_path = crate::project::ProjectManager::autosave_path();
        if let Err(e) = crate::project::ProjectManager::save_project(
            &autosave_path,
            &format!("{} (Autosave)", self.project_name),
            &self.mesh,
            self.mesh_source_path.as_deref(),
            &self.layer_stack,
            &self.baked_maps,
            &self.baker_options,
            (self.camera_yaw, self.camera_pitch, self.camera_dist),
            (
                self.exposure,
                self.light_intensity,
                self.env_rotation,
                self.selected_env_idx,
            ),
        ) {
            eprintln!("[AUTOSAVE ERROR] {}", e);
        } else {
            self.last_autosave_time = std::time::Instant::now();
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                format!("Autosaved project to {}", autosave_path.display()),
            ));
        }
    }

    /// Creates a project snapshot
    pub fn take_snapshot(&mut self, name: &str) {
        let snap_dir = crate::project::ProjectManager::snapshots_dir();
        let _ = std::fs::create_dir_all(&snap_dir);
        let id = self.snapshots.len() + 1;
        let snap_path = snap_dir.join(format!("snapshot_{}_{}.rmp", id, name.replace(' ', "_")));

        if let Ok(_) = crate::project::ProjectManager::save_project(
            &snap_path,
            name,
            &self.mesh,
            self.mesh_source_path.as_deref(),
            &self.layer_stack,
            &self.baked_maps,
            &self.baker_options,
            (self.camera_yaw, self.camera_pitch, self.camera_dist),
            (
                self.exposure,
                self.light_intensity,
                self.env_rotation,
                self.selected_env_idx,
            ),
        ) {
            let snap = crate::project::SnapshotInfo {
                id,
                name: name.to_string(),
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                path: snap_path,
            };
            self.snapshots.push(snap);
            self.status_message = format!("Captured snapshot: {}", name);
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                format!("Created project snapshot: {}", name),
            ));
        }
    }

    /// Restores a project snapshot
    pub fn restore_snapshot(&mut self, idx: usize) {
        if let Some(snap) = self.snapshots.get(idx).cloned() {
            if let Ok((_, mesh, layer_stack, baked_maps, baker_options)) =
                crate::project::ProjectManager::load_project(&snap.path)
            {
                self.mesh = mesh;
                self.layer_stack = layer_stack;
                self.baked_maps = baked_maps;
                self.baker_options = baker_options;
                self.texture_needs_update = true;

                if let Some(ref vp_mutex) = self.viewport_wgpu {
                    if let (Some(ref device), Some(ref queue)) =
                        (&self.wgpu_device, &self.wgpu_queue)
                    {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.set_mesh(device, queue, &self.mesh);
                        }
                    }
                }
                self.recompute_composite();
                self.status_message = format!("Restored snapshot: {}", snap.name);
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "INFO".into(),
                    format!("Restored state from snapshot: {}", snap.name),
                ));
            }
        }
    }

    /// Reloads the active mesh from disk preserving all layers and channels
    pub fn reload_mesh(&mut self) {
        if let Some(ref path_str) = self.mesh_source_path.clone() {
            let path = PathBuf::from(path_str);
            if path.exists() {
                match Mesh::load_obj(&path) {
                    Ok(reloaded_mesh) => {
                        self.mesh = reloaded_mesh;
                        self.baked_maps = BakerEngine::bake_all(
                            &self.mesh.vertices,
                            &self.mesh.normals,
                            &self.mesh.uvs,
                            &self.mesh.indices,
                            &self.mesh.submeshes,
                            &self.baker_options,
                            None,
                            None,
                            None,
                        );
                        if let Some(ref vp_mutex) = self.viewport_wgpu {
                            if let (Some(ref device), Some(ref queue)) =
                                (&self.wgpu_device, &self.wgpu_queue)
                            {
                                if let Ok(mut vp) = vp_mutex.lock() {
                                    vp.set_mesh(device, queue, &self.mesh);
                                }
                            }
                        }
                        self.recompute_composite();
                        self.status_message =
                            format!("Reloaded mesh: {} (paint preserved)", path.display());
                        self.history.push(format!(
                            "{:02}  Reload Mesh ({})",
                            self.history.len() + 1,
                            self.mesh.name
                        ));
                        self.logs.push((
                            chrono::Local::now().format("%H:%M:%S").to_string(),
                            "INFO".into(),
                            format!(
                                "Reloaded 3D mesh from disk: {}. Retained {} paint layers.",
                                path.display(),
                                self.layer_stack.layers.len()
                            ),
                        ));
                    }
                    Err(e) => {
                        self.status_message = format!("Failed to reload mesh: {}", e);
                    }
                }
            } else {
                self.status_message = format!("Mesh source file not found: {}", path_str);
            }
        } else {
            // Re-initialize standard primitive model
            self.load_model(self.selected_model_idx);
            self.status_message = format!("Reloaded base model: {}", self.mesh.name);
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                format!("Reloaded standard primitive model: {}", self.mesh.name),
            ));
        }
    }

    /// Switches the active Texture Set (Material Group / UDIM Tile)
    pub fn switch_texture_set(&mut self, new_idx: usize) {
        if new_idx >= self.texture_set_manager.sets.len()
            || new_idx == self.texture_set_manager.active_index
        {
            return;
        }

        // 1. Sync current layer stack and baked maps into outgoing set
        let cur_idx = self.texture_set_manager.active_index;
        if cur_idx < self.texture_set_manager.sets.len() {
            self.texture_set_manager.sets[cur_idx].layer_stack = self.layer_stack.clone();
            self.texture_set_manager.sets[cur_idx].baked_maps = self.baked_maps.clone();
        }

        // 2. Activate incoming set and load its layers & baked maps
        self.texture_set_manager.active_index = new_idx;
        let incoming = &self.texture_set_manager.sets[new_idx];
        self.layer_stack = incoming.layer_stack.clone();
        self.baked_maps = incoming.baked_maps.clone();
        self.texture_needs_update = true;

        let time_str = chrono::Local::now().format("%H:%M:%S").to_string();
        let log_msg = format!(
            "Switched active Texture Set to: {} (ID: {})",
            incoming.name, incoming.id
        );
        self.status_message = log_msg.clone();
        self.logs.push((time_str, "INFO".into(), log_msg));
    }

    /// Full reference PBR suite-grade texture export (Unreal ORM, Unity URP/HDRP, glTF, Generic PBR, OpenEXR)
    pub fn export_textures(&mut self) {
        let mut preset = match self.export_preset_idx {
            0 => ExportPreset::unreal_engine(),
            1 => ExportPreset::unity_urp(),
            2 => ExportPreset::unity_hdrp(),
            3 => ExportPreset::gltf2(),
            4 => ExportPreset::generic_pbr(),
            _ => ExportPreset::studio_exr(),
        };
        preset.set_padding_algorithm(match self.export_padding_algorithm {
            1 => PaddingAlgorithm::Transparent,
            2 => PaddingAlgorithm::Diffusion,
            3 => PaddingAlgorithm::Passthrough,
            _ => PaddingAlgorithm::Color,
        });

        let (format_override, depth_override) = match self.export_file_format {
            1 => (Some(OutputFormat::Png), Some(BitDepth::U8)),
            2 => (Some(OutputFormat::Png), Some(BitDepth::U16)),
            3 => (Some(OutputFormat::Tga), Some(BitDepth::U8)),
            4 => (Some(OutputFormat::Exr), Some(BitDepth::F32)),
            5 => (Some(OutputFormat::Jpg), Some(BitDepth::U8)),
            _ => (None, None), // Use preset defaults
        };

        let res_override =
            if self.export_resolution > 0 && self.export_resolution != self.layer_stack.width {
                Some((self.export_resolution, self.export_resolution))
            } else {
                None
            };

        let ctx = TextureExportContext {
            layer_stack: &self.layer_stack,
            baked_maps: &self.baked_maps,
            mesh: &self.mesh,
            output_dir: &self.export_output_dir,
            preset: &preset,
            resolution_override: res_override,
            padding_radius: self.export_padding as usize,
            file_format_override: format_override,
            bit_depth_override: depth_override,
        };

        match crate::painting::export::execute_export(ctx) {
            Ok(result) => {
                let count = result.exported_files.len();
                self.export_status = format!(
                    "Exported {} maps to {}",
                    count,
                    self.export_output_dir.display()
                );
                self.status_message = format!("Exported {} textures successfully!", count);
                let time_str = chrono::Local::now().format("%H:%M:%S").to_string();
                for log in result.logs {
                    self.logs.push((time_str.clone(), "INFO".into(), log));
                }
                self.logs.push((
                    time_str,
                    "SUCCESS".into(),
                    format!(
                        "Export complete: {} textures with {}px seam dilation",
                        count, self.export_padding
                    ),
                ));
            }
            Err(e) => {
                self.export_status = format!("Export failed: {}", e);
                self.status_message = format!("Export error: {}", e);
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "ERROR".into(),
                    format!("Texture export failed: {}", e),
                ));
            }
        }
    }

    // ── Procedural (Graph) Material — engine-backed parametric materials ──

    pub const GRAPH_TEMPLATES: [&str; 6] = [
        "Clouds / FBM Noise",
        "Grain",
        "Blotches (Soft Spots)",
        "Marble",
        "Brick",
        "Crystal (Voronoi)",
    ];

    fn build_graph_template(&self, kind: usize) -> crate::painting::graph::Graph {
        use crate::painting::graph::{Graph, GraphNodeBody};
        let seed = self.graph_seed;
        let scale = self.graph_scale.max(1.0);
        let contrast = self.graph_contrast;
        let mut graph = Graph::new();
        let out = graph
            .add_node(GraphNodeBody::output())
            .expect("output node");
        let link = |graph: &mut Graph, from: crate::painting::graph::GraphNodeId| {
            let _ = graph.connect(from, out, 0);
        };
        match kind {
            0 => {
                let n = graph
                    .add_node(GraphNodeBody::FbmNoise {
                        seed,
                        octaves: 6,
                        scale,
                        gain: 0.55,
                        contrast,
                        color_a: [0.16, 0.28, 0.56, 1.0],
                        color_b: [0.66, 0.78, 0.92, 1.0],
                    })
                    .expect("fbm");
                link(&mut graph, n);
            }
            1 => {
                let n = graph
                    .add_node(GraphNodeBody::WhiteNoise {
                        seed,
                        cells: (scale * 2.0).max(2.0) as u32,
                        color_a: [0.12, 0.12, 0.13, 1.0],
                        color_b: [0.92, 0.92, 0.90, 1.0],
                    })
                    .expect("grain");
                link(&mut graph, n);
            }
            2 => {
                // Blotches: jittered spots, then soften with gaussian blur.
                let n = graph
                    .add_node(GraphNodeBody::BnWSpots {
                        seed,
                        cells: scale.max(2.0) as u32,
                        radius: 0.9,
                        falloff: 1.8,
                        invert: false,
                    })
                    .expect("spots");
                let blur = graph
                    .add_node(GraphNodeBody::GaussianBlur {
                        radius: (scale * 0.25).clamp(1.0, 6.0),
                    })
                    .expect("blur");
                let _ = graph.connect(n, blur, 0);
                let _ = graph.connect(blur, out, 0);
            }
            3 => {
                let n = graph
                    .add_node(GraphNodeBody::Marble {
                        seed,
                        veins: scale * 0.5 + 2.0,
                        turbulence: 2.0 + contrast * 6.0,
                        octaves: 5,
                        color_a: [0.06, 0.06, 0.07, 1.0],
                        color_b: [0.93, 0.93, 0.90, 1.0],
                    })
                    .expect("marble");
                link(&mut graph, n);
            }
            4 => {
                let n = graph
                    .add_node(GraphNodeBody::Brick {
                        seed,
                        cols: scale as u32,
                        rows: (scale / 2.0).ceil() as u32,
                        mortar: 0.06,
                        bevel: 0.10,
                        brick_color: [0.47, 0.19, 0.11, 1.0],
                        mortar_color: [0.26, 0.24, 0.22, 1.0],
                    })
                    .expect("brick");
                link(&mut graph, n);
            }
            _ => {
                let n = graph
                    .add_node(GraphNodeBody::Crystal {
                        seed,
                        cells: scale.max(2.0) as u32,
                        edge_width: 0.16,
                        edge_darkness: 0.9,
                        color_a: [0.38, 0.52, 0.62, 1.0],
                        color_b: [0.10, 0.14, 0.18, 1.0],
                    })
                    .expect("crystal");
                link(&mut graph, n);
            }
        }
        let _ = graph.set_output(out);
        graph
    }

    /// (Re)builds the graph engine from the current template + parameter state.
    fn graph_reset_engine(&mut self) {
        let ctx = crate::painting::graph::GraphContext::new(128, 128, self.graph_seed);
        let graph = self.build_graph_template(self.graph_template_idx);
        self.graph_engine = Some(crate::painting::graph::GraphEngine::new(graph, ctx));
        self.graph_preview_tex = None;
    }

    /// Applies the graph output to a brand-new paint layer's base color.
    fn graph_apply_to_document(&mut self) {
        if self.graph_engine.is_none() {
            self.graph_reset_engine();
        }
        let (w, h) = (self.layer_stack.width, self.layer_stack.height);
        let Some(engine) = self.graph_engine.as_mut() else {
            return;
        };
        engine.context.width = w;
        engine.context.height = h;
        match engine.render() {
            Ok(buf) => {
                if buf.len() != (w * h) as usize {
                    self.status_message =
                        "Graph buffer size mismatch — resolution change mid-render".into();
                    return;
                }
                let id = self.layer_stack.layers.len() + 1;
                let name = format!(
                    "Graph: {}",
                    Self::GRAPH_TEMPLATES
                        .get(self.graph_template_idx)
                        .copied()
                        .unwrap_or("Material")
                );
                let mut layer = crate::painting::Layer::new_paint(id, name.clone(), w, h);
                layer.color_buffer.copy_from_slice(&buf);
                self.layer_stack.layers.push(layer);
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.recompute_composite();
                self.texture_needs_update = true;
                self.status_message = "Procedural material applied to a new paint layer".into();
                self.history.push(format!(
                    "{:02}  Apply Graph Material ({})",
                    self.history.len() + 1,
                    name
                ));
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "INFO".into(),
                    format!("Applied procedural '{}' at {}x{}", name, w, h),
                ));
            }
            Err(e) => {
                self.status_message = format!("Graph render failed: {e}");
            }
        }
        self.graph_preview_tex = None;
    }

    /// Parses a `.spsm` smart material (7z with an embedded `.sbs`/`.sbsar`
    /// graph) and opens the same parameter import dialog.
    pub fn import_spsm_file(&mut self, path: &Path) {
        match crate::painting::sbsar::parse_spsm_file(path) {
            Ok(package) => self.open_sbsar_package(package, path),
            Err(e) => self.status_message = format!(".spsm import failed: {e}"),
        }
    }

    /// Parses a `.sbsar` file and opens the parameter import dialog.
    pub fn import_sbsar_file(&mut self, path: &Path) {
        match std::fs::read(path) {
            Ok(bytes) => match crate::painting::sbsar::parse_sbsar(&bytes) {
                Ok(package) => self.open_sbsar_package(package, path),
                Err(e) => {
                    self.status_message = format!(".sbsar import failed: {e}");
                }
            },
            Err(e) => self.status_message = format!("Could not read '{}': {e}", path.display()),
        }
    }

    /// Routes a parsed archive package into the parameter import dialog,
    /// seeding parameter values with widget defaults.
    pub fn open_sbsar_package(
        &mut self,
        package: crate::painting::sbsar::SbsarPackage,
        path: &Path,
    ) {
        let graph_count = package.graphs.len();
        if let Some(graph) = package.primary() {
            // Seed parameter values with widget defaults.
            let mut values = std::collections::BTreeMap::new();
            for input in &graph.inputs {
                let default = match &input.widget {
                    crate::painting::sbsar::SbsarWidget::Slider { default, .. } => *default,
                    crate::painting::sbsar::SbsarWidget::Toggle { default } => {
                        if *default {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                };
                values.insert(input.identifier.clone(), default);
            }
            self.sbsar_param_values = values;
            self.imported_sbsar = Some(package);
            self.sbsar_status = String::new();
            self.show_sbsar_import_dialog = true;
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                format!("Imported '{}': {} graphs", path.display(), graph_count),
            ));
        } else {
            self.status_message =
                format!("Imported '{}': archive contains no graphs", path.display());
        }
    }

    pub fn ui_sbsar_import_dialog(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("🧩 reference Archive (.sbsar) Import")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                let Some(package) = self.imported_sbsar.as_ref() else {
                    ui.label("No archive imported.");
                    return;
                };
                // Graph metadata.
                for (gi, graph) in package.graphs.iter().enumerate() {
                    ui.heading(&graph.label);
                    ui.label(
                        egui::RichText::new(format!("{} · {} input(s) / {} output(s)",
                            graph.pkgurl, graph.inputs.len(), graph.outputs.len()))
                            .weak().small(),
                    );
                    ui.horizontal(|ui| {
                        for out in &graph.outputs {
                            ui.label(format!("▸ {} ({}x{})", out.identifier, out.width, out.height));
                        }
                    });
                    if gi + 1 < package.graphs.len() {
                        ui.separator();
                    }
                }

                ui.add_space(4.0);
                ui.separator();

                // Editable parameter surface (scalar params only, honest about limits).
                let Some(graph) = package.primary() else { return };
                let mut any_changed = false;
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        egui::Grid::new("sbsar_params_grid").num_columns(2)
                            .spacing([10.0, 6.0]).show(ui, |ui| {
                            for input in &graph.inputs {
                                if input.input_type == crate::painting::sbsar::SbsarInputType::Image {
                                    continue;
                                }
                                let current = self
                                    .sbsar_param_values
                                    .entry(input.identifier.clone())
                                    .or_insert(0.0);
                                match &input.widget {
                                    crate::painting::sbsar::SbsarWidget::Slider {
                                        min, max, step, ..
                                    } => {
                                        ui.label(&input.label);
                                        if ui
                                            .add(
                                                egui::Slider::new(current, *min..=*max)
                                                    .step_by(((*step).min(0.001)) as f64),
                                            )
                                            .changed()
                                        {
                                            any_changed = true;
                                        }
                                        ui.end_row();
                                    }
                                    crate::painting::sbsar::SbsarWidget::Toggle {
                                        default: _,
                                    } => {
                                        ui.label(&input.label);
                                        let mut on = *current > 0.5;
                                        if ui.checkbox(&mut on, "").changed() {
                                            *current = if on { 1.0 } else { 0.0 };
                                            any_changed = true;
                                        }
                                        ui.end_row();
                                    }
                                    _ => {
                                        ui.label(&input.label);
                                        ui.label(
                                            egui::RichText::new("procedural pending").weak(),
                                        );
                                        ui.end_row();
                                    }
                                }
                            }
                        });
                    });

                if any_changed {
                    self.sbsar_status =
                        "Parameters bound to the archive session.".to_string();
                }

                ui.add_space(6.0);
                if !self.sbsar_status.is_empty() {
                    ui.colored_label(SUN_FLOWER, &self.sbsar_status);
                }
                ui.label(
                    egui::RichText::new(
                        "Rendered .sbsar output (SBAM bytecode VM) is being built —                          parameters are imported and editable now; CPU/GPU graph                          evaluation binds automatically when the VM lands.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.show_sbsar_import_dialog = false;
                    }
                });
            });
        if !open {
            self.show_sbsar_import_dialog = false;
        }
    }

    pub fn ui_graph_dialog(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("⚗ Procedural (Graph) Material — REMoAINTER")
            .open(&mut open)
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                if self.graph_engine.is_none() {
                    self.graph_reset_engine();
                }

                // Template selector
                let templates = Self::GRAPH_TEMPLATES;
                egui::ComboBox::from_id_salt("graph_template_combo")
                    .selected_text(templates.get(self.graph_template_idx).copied().unwrap_or(""))
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in templates.iter().enumerate() {
                            if ui.selectable_label(i == self.graph_template_idx, *name).clicked() {
                                self.graph_template_idx = i;
                                self.graph_reset_engine();
                            }
                        }
                    });

                // Parameters (each change rebuilds + re-renders)
                let mut changed = false;
                ui.add_space(4.0);
                egui::Grid::new("graph_params").num_columns(2).show(ui, |ui| {
                    ui.label("Seed:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.graph_seed)
                                .range(1..=9_999)
                                .speed(1.0),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Scale:");
                    if ui
                        .add(
                            egui::Slider::new(&mut self.graph_scale, 1.0..=64.0)
                                .integer()
                                .show_value(true),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Contrast:");
                    if ui
                        .add(egui::Slider::new(&mut self.graph_contrast, 0.0..=1.0))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });

                if changed {
                    self.graph_reset_engine();
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                // Live preview (re-rendered when invalidated; engine memoizes).
                let want_preview = self.graph_preview_tex.is_none();
                if want_preview || changed {
                    if let Some(engine) = self.graph_engine.as_mut() {
                        engine.context.width = 128;
                        engine.context.height = 128;
                        match engine.render() {
                            Ok(buf) => {
                                let side = 128usize;
                                let mut rgba = Vec::with_capacity(side * side * 4);
                                for px in buf.iter() {
                                    rgba.push((px[0].clamp(0.0, 1.0) * 255.0) as u8);
                                    rgba.push((px[1].clamp(0.0, 1.0) * 255.0) as u8);
                                    rgba.push((px[2].clamp(0.0, 1.0) * 255.0) as u8);
                                    rgba.push((px[3].clamp(0.0, 1.0) * 255.0) as u8);
                                }
                                let img = egui::ColorImage::from_rgba_unmultiplied(
                                    [side, side],
                                    &rgba,
                                );
                                let existing = self.graph_preview_tex.take();
                                if let Some(mut h) = existing {
                                    h.set(img, egui::TextureOptions::default());
                                    self.graph_preview_tex = Some(h);
                                } else {
                                    self.graph_preview_tex = Some(ctx.load_texture(
                                        "graph_material_preview",
                                        img,
                                        egui::TextureOptions::default(),
                                    ));
                                }
                            }
                            Err(e) => {
                                self.graph_status = format!("Preview error: {e}");
                            }
                        }
                    }
                }
                if let Some(tex) = self.graph_preview_tex.as_ref() {
                    let side = 160.0;
                    ui.vertical_centered(|ui| {
                        ui.image(egui::load::SizedTexture::new(
                            tex.id(),
                            egui::vec2(side, side),
                        ));
                    });
                }
                if !self.graph_status.is_empty() {
                    ui.colored_label(SUN_FLOWER, egui::RichText::new(&self.graph_status));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(egui::RichText::new("⚡ Apply to New Paint Layer").strong())
                        )
                        .clicked()
                    {
                        self.graph_apply_to_document();
                    }
                    if ui.button("Close").clicked() {
                        self.show_graph_dialog = false;
                        ui.close_menu();
                    }
                });
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(
                        "Preview renders at 128²; Apply renders at the current texture-set resolution.",
                    ).weak().small()
                );
            });
        if !open {
            self.show_graph_dialog = false;
        }
    }
}

impl eframe::App for RemoainterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx);

        // Handle background asynchronous baking
        if self.is_baking {
            ctx.request_repaint();
            if let Some(ref rx) = self.baking_receiver {
                if let Ok(baked) = rx.try_recv() {
                    self.is_baking = false;
                    self.baking_receiver = None;
                    if baked.is_cancelled {
                        self.status_message = "Mesh map baking cancelled by user".to_string();
                        self.logs.push((
                            chrono::Local::now().format("%H:%M:%S").to_string(),
                            "WARN".into(),
                            "Mesh map baking cancelled by user".into(),
                        ));
                    } else {
                        self.baked_maps = baked;
                        self.status_message = format!(
                            "Successfully baked mesh maps at {}×{}",
                            self.baker_options.resolution, self.baker_options.resolution
                        );
                        self.logs.push((
                            chrono::Local::now().format("%H:%M:%S").to_string(),
                            "INFO".into(),
                            format!("Completed mesh map bake (AO, Curvature, Normals, Position, Thickness) for {}", self.mesh.name),
                        ));
                        self.recompute_composite();
                    }
                }
            }
        }

        // Automatic background autosave
        if self.project_is_dirty
            && self.last_autosave_time.elapsed().as_secs() >= self.autosave_interval_secs
        {
            self.trigger_autosave();
        }

        // Turntable continuous camera orbit animation
        if self.turntable_active {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            self.camera_yaw = (self.camera_yaw + self.turntable_speed * dt).rem_euclid(360.0);
            ctx.request_repaint();
        }

        // Global hotkey processing (reference PBR suite standard)
        let num_keys = [
            (egui::Key::Num1, 1),
            (egui::Key::Num2, 2),
            (egui::Key::Num3, 3),
            (egui::Key::Num4, 4),
            (egui::Key::Num5, 5),
            (egui::Key::Num6, 6),
            (egui::Key::Num7, 7),
            (egui::Key::Num8, 8),
            (egui::Key::Num9, 9),
        ];

        let mut bookmark_action = false;
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && !i.modifiers.alt) {
            for &(key, slot) in &num_keys {
                if ctx.input(|i| i.key_pressed(key)) {
                    self.camera_bookmarks[slot] = Some(CameraBookmark {
                        name: format!("Bookmark {}", slot),
                        yaw: self.camera_yaw,
                        pitch: self.camera_pitch,
                        dist: self.camera_dist,
                    });
                    self.status_message = format!("Saved Camera Bookmark {} (Ctrl+{})", slot, slot);
                    bookmark_action = true;
                    break;
                }
            }
        } else if ctx.input(|i| i.modifiers.alt && !i.modifiers.command && !i.modifiers.shift) {
            for &(key, slot) in &num_keys {
                if ctx.input(|i| i.key_pressed(key)) {
                    if let Some(ref bm) = self.camera_bookmarks[slot] {
                        self.camera_yaw = bm.yaw;
                        self.camera_pitch = bm.pitch;
                        self.camera_dist = bm.dist;
                        self.status_message =
                            format!("Restored Camera Bookmark {} (Alt+{})", slot, slot);
                    } else {
                        self.status_message = format!("Camera Bookmark {} is empty", slot);
                    }
                    bookmark_action = true;
                    break;
                }
            }
        }

        if bookmark_action {
            // Handled bookmark action
        } else if ctx
            .input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z))
        {
            self.perform_undo();
        } else if ctx.input(|i| {
            (i.modifiers.command && i.key_pressed(egui::Key::Y))
                || (i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z))
        }) {
            self.perform_redo();
        } else if ctx
            .input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S))
        {
            self.save_project();
        } else if ctx
            .input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S))
        {
            self.save_project_as();
        } else if ctx
            .input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::O))
        {
            self.open_project();
        } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::R)) {
            self.reload_mesh();
        } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::N)) {
            self.show_new_project_dialog = true;
        } else if ctx
            .input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::B))
        {
            self.show_baker_dialog = true;
        } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::G)) {
            let id = self.layer_stack.layers.len() + 1;
            let folder = Layer::new_folder(
                id,
                format!("Folder {}", id),
                self.layer_stack.width,
                self.layer_stack.height,
            );
            let active_idx = self.layer_stack.active_layer_index;
            if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                active.parent_folder_id = Some(id);
            }
            self.layer_stack.layers.insert(active_idx + 1, folder);
            self.layer_stack.active_layer_index = active_idx + 1;
            self.recompute_composite();
            self.status_message = format!("Created Group Folder {}", id);
        } else if ctx
            .input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::E))
        {
            self.show_export_dialog = true;
        } else if ctx.input(|i| {
            i.modifiers.command
                && !i.modifiers.shift
                && !i.modifiers.alt
                && i.key_pressed(egui::Key::X)
        }) {
            self.cut_active_layer();
        } else if ctx.input(|i| {
            i.modifiers.command
                && !i.modifiers.shift
                && !i.modifiers.alt
                && i.key_pressed(egui::Key::C)
        }) {
            self.copy_active_layer();
        } else if ctx.input(|i| {
            i.modifiers.command
                && !i.modifiers.shift
                && !i.modifiers.alt
                && i.key_pressed(egui::Key::V)
        }) {
            self.paste_layer();
        } else if ctx.input(|i| i.key_pressed(egui::Key::Y))
            && !ctx.input(|i| i.modifiers.command || i.modifiers.shift || i.modifiers.alt)
        {
            self.turntable_active = !self.turntable_active;
            self.status_message = format!(
                "Turntable: {}",
                if self.turntable_active {
                    "ON (Y)"
                } else {
                    "OFF (Y)"
                }
            );
        } else if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.viewport_view_mode = 2; // 3D|2D Split
            self.status_message = "Viewport: 3D | 2D Split View (F1)".into();
        } else if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.viewport_view_mode = 0; // 3D Only
            self.status_message = "Viewport: 3D View Only (F2)".into();
        } else if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
            self.viewport_view_mode = 1; // 2D UV Only
            self.status_message = "Viewport: 2D UV Texture Map View (F3)".into();
        } else if ctx.input(|i| i.key_pressed(egui::Key::F4)) {
            self.show_wireframe = !self.show_wireframe;
            self.show_uv_wireframe = self.show_wireframe;
            self.status_message = format!(
                "Wireframe Overlay: {}",
                if self.show_wireframe {
                    "ON (F4)"
                } else {
                    "OFF"
                }
            );
        } else if ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.display_channel = 0;
            self.texture_needs_update = true;
            self.status_message = "Channel: Material (Lit PBR) [M]".to_string();
        } else if ctx.input(|i| i.key_pressed(egui::Key::C)) {
            self.display_channel = match self.display_channel {
                0 => 1,
                1 => 2,
                2 => 3,
                3 => 4,
                _ => 0,
            };
            self.texture_needs_update = true;
            self.status_message = format!(
                "Solo Channel: {} [C]",
                self.channel_name(self.display_channel)
            );
        } else if ctx.input(|i| i.key_pressed(egui::Key::B)) && !ctx.input(|i| i.modifiers.shift) {
            self.display_channel = match self.display_channel {
                5 => 6,
                6 => 7,
                7 => 8,
                _ => 5,
            };
            self.texture_needs_update = true;
            self.status_message =
                format!("Mesh Map: {} [B]", self.channel_name(self.display_channel));
        } else if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.camera_yaw = 35.0;
            self.camera_pitch = 18.0;
            self.camera_dist = 1.8;
            self.status_message = "Framed 3D Mesh [F]".to_string();
        } else {
            let is_typing = ctx.memory(|m| m.focused().is_some()) && ctx.wants_keyboard_input();
            if !is_typing {
                if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
                    self.active_tool = ToolMode::Paint;
                    self.status_message = "Tool: Paint Brush (1)".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
                    self.active_tool = ToolMode::Eraser;
                    self.status_message = "Tool: Eraser (2)".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
                    self.active_tool = ToolMode::Projection;
                    self.status_message = "Tool: Projection (3)".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num4)) {
                    self.active_tool = ToolMode::PolygonFill;
                    self.status_message = format!(
                        "Tool: Polygon Fill (4) [{}]",
                        match self.poly_fill_mode {
                            0 => "Triangle",
                            1 => "UV Island",
                            _ => "Mesh",
                        }
                    );
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num5)) {
                    self.active_tool = ToolMode::Smudge;
                    self.status_message = "Tool: Smudge (5)".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num6)) {
                    self.active_tool = ToolMode::Clone;
                    self.status_message = "Tool: Clone Stamp (6) [Alt+Click to set source]".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num7)) {
                    self.active_tool = ToolMode::Decal;
                    self.status_message =
                        "Tool: Text / Decal (7) [stamp decal alpha with click]".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::I)) {
                    self.active_tool = ToolMode::Eyedropper;
                    self.status_message = "Tool: Color Eyedropper (I)".into();
                }
            }
        }

        // Handle Drag & Drop of Texture Folders / Image Files onto Viewport / App
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() {
            let mut paths = Vec::new();
            for df in dropped_files {
                if let Some(p) = df.path {
                    paths.push(p);
                }
            }
            if !paths.is_empty() {
                // Single dropped material archive (.sbsar / .spsm / .sbs)
                // routes to the smart-material parameter dialog first.
                if paths.len() == 1 && paths[0].is_file() {
                    let p = &paths[0];
                    let ext = p
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_lowercase())
                        .unwrap_or_default();
                    if ext == "sbsar" || ext == "spsm" || ext == "sbs" {
                        if ext == "sbsar" {
                            self.import_sbsar_file(p);
                        } else {
                            self.import_spsm_file(p);
                        }
                        return;
                    }
                }
                if paths.len() == 1 && paths[0].is_dir() {
                    let set = PbrTextureSet::from_folder(&paths[0]);
                    self.import_texture_set = set.clone();
                    self.import_flip_normal_y = set.is_directx_normal;
                    if let Some(active) = self
                        .layer_stack
                        .layers
                        .get_mut(self.layer_stack.active_layer_index)
                    {
                        if let Ok(_) = set.load_into_layer(active, set.is_directx_normal) {
                            if let Some(ao) =
                                set.load_ao_map(self.layer_stack.width, self.layer_stack.height)
                            {
                                self.baked_maps.ambient_occlusion = Some(ao);
                            }
                            self.recompute_composite();
                            self.status_message =
                                format!("Imported Texture Set via Drag & Drop: {}", set.name);
                            self.history.push(format!(
                                "{:02}  Import Texture Set ({})",
                                self.history.len() + 1,
                                set.name
                            ));
                        }
                    }
                } else {
                    let set = PbrTextureSet::from_files(&paths);
                    self.import_texture_set = set.clone();
                    self.import_flip_normal_y = set.is_directx_normal;
                    if let Some(active) = self
                        .layer_stack
                        .layers
                        .get_mut(self.layer_stack.active_layer_index)
                    {
                        if let Ok(_) = set.load_into_layer(active, set.is_directx_normal) {
                            if let Some(ao) =
                                set.load_ao_map(self.layer_stack.width, self.layer_stack.height)
                            {
                                self.baked_maps.ambient_occlusion = Some(ao);
                            }
                            self.recompute_composite();
                            self.status_message =
                                format!("Imported Texture Maps via Drag & Drop: {}", set.name);
                            self.history.push(format!(
                                "{:02}  Import Texture Maps ({})",
                                self.history.len() + 1,
                                set.name
                            ));
                        }
                    }
                }
            }
        }

        // If Offline Render mode is active, tear down cleanly when leaving
        // (painting keeps running; the worker is cancelled via RenderJob::Drop).
        if !self.show_render_mode && self.render_state.is_some() {
            self.stop_render_session();
        }

        // Offline Path-Traced Render Studio (full-screen mode, mirrors the
        // Baking Studio pattern)
        if self.show_render_mode {
            self.ui_render_studio_mode(ctx);
            return;
        }

        // If Baker mode is active, render full docked Baking Studio Mode (reference PBR suite 1:1)
        if self.show_baker_dialog {
            self.ui_baker_studio_mode(ctx);
            if self.show_texture_importer_dialog {
                self.ui_texture_importer_modal(ctx);
            }
            if self.show_export_dialog {
                self.ui_export_modal(ctx);
            }
            if self.show_graph_dialog {
                self.ui_graph_dialog(ctx);
            }
            if self.show_sbsar_import_dialog {
                self.ui_sbsar_import_dialog(ctx);
            }
            if self.show_preferences_dialog {
                self.ui_preferences_modal(ctx);
            }
            if self.show_hotkeys_dialog {
                self.ui_hotkey_editor_modal(ctx);
            }
            if self.show_shortcuts_dialog {
                self.ui_shortcuts_help_modal(ctx);
            }
            return;
        }

        // 1. TOP MENU BAR (with logo and Allegorithmic styling)
        egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
            if self.logo_texture.is_none() {
                let logo_bytes = include_bytes!("../../../assets/logo_icon.png");
                if let Ok(img) = image::load_from_memory(logo_bytes) {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        &rgba.into_raw(),
                    );
                    self.logo_texture = Some(ctx.load_texture(
                        "remoainter_logo",
                        color_img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }

            ui.horizontal(|ui| {
                // Application Logo / Brand
                if let Some(ref logo) = self.logo_texture {
                    ui.image(egui::load::SizedTexture::new(logo.id(), vec2(20.0, 20.0)));
                } else {
                    ui.colored_label(SMART_MAT_ORANGE, "RM");
                }
                ui.label(
                    egui::RichText::new("REMoAINTER")
                        .strong()
                        .color(Color32::WHITE),
                );

                ui.separator();
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New Project... (Ctrl+N)").clicked() {
                            self.show_new_project_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("Open Project... (Ctrl+O)").clicked() {
                            self.open_project();
                            ui.close_menu();
                        }
                        ui.menu_button("Open Recent", |ui| {
                            if self.config.recent_projects.is_empty() {
                                ui.colored_label(TEXT_DIMMED, "No recent projects");
                            } else {
                                let mut to_open = None;
                                for path_str in &self.config.recent_projects {
                                    let filename = std::path::Path::new(path_str)
                                        .file_name()
                                        .and_then(|f| f.to_str())
                                        .unwrap_or(path_str.as_str());
                                    if ui.button(filename).on_hover_text(path_str).clicked() {
                                        to_open = Some(path_str.clone());
                                        ui.close_menu();
                                    }
                                }
                                ui.separator();
                                if ui.button("Clear Recent Projects").clicked() {
                                    self.config.clear_recent();
                                    ui.close_menu();
                                }
                                if let Some(path_str) = to_open {
                                    self.open_project_file(std::path::Path::new(&path_str));
                                }
                            }
                        });
                        if ui.button("Save Project (Ctrl+S)").clicked() {
                            self.save_project();
                            ui.close_menu();
                        }
                        if ui.button("Save Project As... (Ctrl+Shift+S)").clicked() {
                            self.save_project_as();
                            ui.close_menu();
                        }
                        if ui.button("Snapshots & Autosave Recovery...").clicked() {
                            self.show_snapshots_dialog = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("📦 Import 3D Mesh (OBJ/FBX/glTF)...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Import 3D Mesh — REMoAINTER")
                                .add_filter(
                                    "3D Models (*.obj, *.fbx, *.gltf, *.glb)",
                                    &["obj", "OBJ", "fbx", "FBX", "gltf", "GLTF", "glb", "GLB"],
                                )
                                .add_filter("All Files", &["*"])
                                .pick_file()
                            {
                                self.import_mesh_file(&path);
                            }
                            ui.close_menu();
                        }
                        if ui.button("🔄 Reload Current Mesh (Ctrl+R)").clicked() {
                            self.reload_mesh();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Bake Mesh Maps... (Ctrl+Shift+B)").clicked() {
                            self.show_baker_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("📤 Export Textures... (Ctrl+Shift+E)").clicked() {
                            self.show_export_dialog = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("📁 Import Texture Set (Folder)...").clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.import_texture_set = PbrTextureSet::from_folder(&folder);
                                self.import_flip_normal_y =
                                    self.import_texture_set.is_directx_normal;
                                self.show_texture_importer_dialog = true;
                            }
                            ui.close_menu();
                        }
                        if ui.button("📄 Import Texture Maps (Files)...").clicked() {
                            if let Some(files) = rfd::FileDialog::new().pick_files() {
                                self.import_texture_set = PbrTextureSet::from_files(&files);
                                self.import_flip_normal_y =
                                    self.import_texture_set.is_directx_normal;
                                self.show_texture_importer_dialog = true;
                            }
                            ui.close_menu();
                        }
                        if ui.button("⚙ Texture Set Importer Dialog...").clicked() {
                            self.show_texture_importer_dialog = true;
                            ui.close_menu();
                        }
                        if ui
                            .button("🧩 Import Material Archive (.sbsar / .spsm)...")
                            .clicked()
                        {
                            let picked = rfd::FileDialog::new()
                                .add_filter("Material Archive", &["sbsar", "spsm", "sbs", "spp"])
                                .pick_file();
                            if let Some(path) = picked {
                                match path
                                    .extension()
                                    .and_then(|s| s.to_str())
                                    .map(|s| s.to_lowercase())
                                    .as_deref()
                                {
                                    Some("spsm") | Some("sbs") => self.import_spsm_file(&path),
                                    _ => self.import_sbsar_file(&path),
                                }
                            }
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit (Alt+F4)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        let undo_label =
                            if let Some(last_cmd) = self.history_manager.undo_stack.last() {
                                format!("Undo {} (Ctrl+Z)", last_cmd.description)
                            } else {
                                "Undo (Ctrl+Z)".into()
                            };
                        if ui
                            .add_enabled(
                                self.history_manager.can_undo(),
                                egui::Button::new(undo_label),
                            )
                            .clicked()
                        {
                            self.perform_undo();
                            ui.close_menu();
                        }

                        let redo_label =
                            if let Some(next_cmd) = self.history_manager.redo_stack.last() {
                                format!("Redo {} (Ctrl+Y)", next_cmd.description)
                            } else {
                                "Redo (Ctrl+Y)".into()
                            };
                        if ui
                            .add_enabled(
                                self.history_manager.can_redo(),
                                egui::Button::new(redo_label),
                            )
                            .clicked()
                        {
                            self.perform_redo();
                            ui.close_menu();
                        }

                        ui.separator();
                        if ui.button("Cut Layer (Ctrl+X)").clicked() {
                            self.cut_active_layer();
                            ui.close_menu();
                        }
                        if ui.button("Copy Layer (Ctrl+C)").clicked() {
                            self.copy_active_layer();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                self.layer_clipboard.is_some(),
                                egui::Button::new("Paste Layer (Ctrl+V)"),
                            )
                            .clicked()
                        {
                            self.paste_layer();
                            ui.close_menu();
                        }

                        ui.separator();
                        if ui.button("Procedural (Graph) Material...").clicked() {
                            self.show_graph_dialog = true;
                            self.graph_reset_engine();
                            ui.close_menu();
                        }
                        if ui.button("Preferences...").clicked() {
                            self.show_preferences_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("Hotkey Editor...").clicked() {
                            self.show_hotkeys_dialog = true;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Mode", |ui| {
                        if ui.button("🎨 Painting (Edition)").clicked() {
                            self.show_baker_dialog = false;
                            self.status_message = "Mode: Painting".into();
                            ui.close_menu();
                        }
                        if ui.button("⚡ Bake Mesh Maps (Studio)").clicked() {
                            self.show_baker_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("📷 Visualisation (Render)").clicked() {
                            self.show_render_mode = true;
                            self.show_baker_dialog = false;
                            self.render_film_handle = None;
                            self.render_last_preview_passes = 0;
                            self.status_message =
                                "Visualisation Mode (Offline Path-Traced Render)".into();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Geometry", |ui| {
                        if ui.button("reference Material Sphere").clicked() {
                            self.load_model(0);
                            ui.close_menu();
                        }
                        if ui.button("Test Asset: Antique Lantern").clicked() {
                            self.load_model(1);
                            ui.close_menu();
                        }
                        if ui.button("Test Asset: Retro Radio").clicked() {
                            self.load_model(2);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("🔄 Reload Current Mesh (Ctrl+R)").clicked() {
                            self.reload_mesh();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Viewport", |ui| {
                        if ui.button("2D | 3D Split View [F1]").clicked() {
                            self.viewport_view_mode = 2;
                            self.status_message = "Viewport: 3D | 2D Split View (F1)".into();
                            ui.close_menu();
                        }
                        if ui.button("3D View Only [F2]").clicked() {
                            self.viewport_view_mode = 0;
                            self.status_message = "Viewport: 3D View Only (F2)".into();
                            ui.close_menu();
                        }
                        if ui.button("2D UV Texture Map Only [F3]").clicked() {
                            self.viewport_view_mode = 1;
                            self.status_message = "Viewport: 2D UV Texture Map View (F3)".into();
                            ui.close_menu();
                        }
                        let wire_label = if self.show_wireframe {
                            "Disable Wireframe Overlay [F4]"
                        } else {
                            "Enable Wireframe Overlay [F4]"
                        };
                        if ui.button(wire_label).clicked() {
                            self.show_wireframe = !self.show_wireframe;
                            self.show_uv_wireframe = self.show_wireframe;
                            ui.close_menu();
                        }
                        ui.separator();
                        let mut ch_changed = false;
                        if ui.button("Material (Lit) [M]").clicked() {
                            self.display_channel = 0;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Base Color (Solo) [C]").clicked() {
                            self.display_channel = 1;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Roughness (Solo)").clicked() {
                            self.display_channel = 2;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Metallic (Solo)").clicked() {
                            self.display_channel = 3;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Normal (Solo)").clicked() {
                            self.display_channel = 4;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Ambient Occlusion [B]").clicked() {
                            self.display_channel = 5;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Curvature").clicked() {
                            self.display_channel = 6;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Position").clicked() {
                            self.display_channel = 7;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ui.button("Thickness").clicked() {
                            self.display_channel = 8;
                            ch_changed = true;
                            ui.close_menu();
                        }
                        if ch_changed {
                            self.texture_needs_update = true;
                            self.status_message =
                                format!("Channel: {}", self.channel_name(self.display_channel));
                        }
                    });

                    ui.menu_button("Window", |ui| {
                        if ui.button("Texture Set Settings").clicked() {
                            self.active_right_panel = 0;
                            ui.close_menu();
                        }
                        if ui.button("Texture Set List").clicked() {
                            self.active_right_panel = 1;
                            ui.close_menu();
                        }
                        if ui.button("Layers").clicked() {
                            self.active_right_panel = 2;
                            ui.close_menu();
                        }
                        if ui.button("Display Settings").clicked() {
                            self.active_right_panel = 3;
                            ui.close_menu();
                        }
                        if ui.button("Shader Settings").clicked() {
                            self.active_right_panel = 4;
                            ui.close_menu();
                        }
                        if ui.button("History").clicked() {
                            self.active_right_panel = 5;
                            ui.close_menu();
                        }
                        if ui.button("Log Console").clicked() {
                            self.active_right_panel = 6;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Reset Layout").clicked() {
                            self.reset_layout();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Help", |ui| {
                        if ui.button("Documentation / Shortcuts...").clicked() {
                            self.show_shortcuts_dialog = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("About REMoAINTER...").clicked() {
                            self.status_message = "REMoAINTER v2.0.0 (Rust Core)".into();
                            ui.close_menu();
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&self.status_message).color(TEXT_DIMMED));
                    ui.colored_label(SMART_MAT_ORANGE, "●");
                });
            });
        });

        // 2. CONTEXT CONTROL BAR (36px below menu bar)
        egui::TopBottomPanel::top("control_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Mode Switcher Pills
                let baking_mode = self.show_baker_dialog;
                if ui.selectable_label(!baking_mode, "🎨 Painting").clicked() {
                    self.show_baker_dialog = false;
                }
                if ui
                    .selectable_label(baking_mode, "⚡ Bake Mesh Maps")
                    .clicked()
                {
                    self.show_baker_dialog = true;
                }
                ui.separator();

                ui.label("Size");
                ui.add(egui::Slider::new(&mut self.brush.size, 1.0..=128.0).suffix(" px"));

                ui.label("Flow");
                let mut flow_pct = self.brush.flow * 100.0;
                if ui
                    .add(egui::Slider::new(&mut flow_pct, 1.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.brush.flow = flow_pct / 100.0;
                }

                ui.label("Opacity");
                let mut op_pct = self.brush.opacity * 100.0;
                if ui
                    .add(egui::Slider::new(&mut op_pct, 1.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.brush.opacity = op_pct / 100.0;
                }

                ui.label("Spacing");
                let mut sp_pct = self.brush.spacing * 100.0;
                if ui
                    .add(egui::Slider::new(&mut sp_pct, 1.0..=100.0).suffix("%"))
                    .changed()
                {
                    self.brush.spacing = sp_pct / 100.0;
                }

                ui.checkbox(&mut self.brush.lazy_mouse, "Lazy");
                if self.brush.lazy_mouse {
                    ui.add(
                        egui::Slider::new(&mut self.lazy_mouse_tracker.radius, 2.0..=60.0)
                            .suffix(" px")
                            .text("Radius"),
                    );
                }
                ui.checkbox(&mut self.symmetry_x, "Sym X");
                ui.checkbox(&mut self.symmetry_y, "Sym Y");
                ui.checkbox(&mut self.symmetry_z, "Sym Z");

                ui.menu_button("Dynamics ⚙", |ui| {
                    ui.label(RichText::new("Pressure Dynamics").strong());
                    ui.checkbox(&mut self.brush.pressure_affects_size, "Pressure -> Size");
                    ui.checkbox(&mut self.brush.pressure_affects_flow, "Pressure -> Flow");
                    ui.horizontal(|ui| {
                        ui.label("Curve:");
                        let mut curve_idx = match self.brush.pressure_curve {
                            crate::painting::brush::PressureCurve::Linear => 0,
                            crate::painting::brush::PressureCurve::EaseIn => 1,
                            crate::painting::brush::PressureCurve::EaseOut => 2,
                            crate::painting::brush::PressureCurve::Smooth => 3,
                        };
                        egui::ComboBox::from_id_salt("dyn_curve")
                            .selected_text(match curve_idx {
                                0 => "Linear",
                                1 => "Ease In",
                                2 => "Ease Out",
                                _ => "Smooth (S-Curve)",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut curve_idx, 0, "Linear");
                                ui.selectable_value(&mut curve_idx, 1, "Ease In");
                                ui.selectable_value(&mut curve_idx, 2, "Ease Out");
                                ui.selectable_value(&mut curve_idx, 3, "Smooth (S-Curve)");
                            });
                        self.brush.pressure_curve = match curve_idx {
                            0 => crate::painting::brush::PressureCurve::Linear,
                            1 => crate::painting::brush::PressureCurve::EaseIn,
                            2 => crate::painting::brush::PressureCurve::EaseOut,
                            _ => crate::painting::brush::PressureCurve::Smooth,
                        };
                    });
                    ui.separator();
                    ui.label(RichText::new("Jitter Dynamics").strong());
                    let mut sj = self.brush.size_jitter * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut sj, 0.0..=100.0).text("Size Jitter %"))
                        .changed()
                    {
                        self.brush.size_jitter = sj / 100.0;
                    }
                    let mut fj = self.brush.flow_jitter * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut fj, 0.0..=100.0).text("Flow Jitter %"))
                        .changed()
                    {
                        self.brush.flow_jitter = fj / 100.0;
                    }
                    let mut aj = self.brush.angle_jitter * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut aj, 0.0..=100.0).text("Angle Jitter %"))
                        .changed()
                    {
                        self.brush.angle_jitter = aj / 100.0;
                    }
                    ui.separator();
                    ui.label(RichText::new("Channels to Paint").strong());
                    ui.checkbox(&mut self.brush.paint_color, "Color (BaseColor)");
                    ui.checkbox(&mut self.brush.paint_metallic, "Metallic");
                    ui.checkbox(&mut self.brush.paint_roughness, "Roughness");
                    ui.checkbox(&mut self.brush.paint_normal, "Normal");
                    ui.checkbox(&mut self.brush.paint_height, "Height");

                    ui.separator();
                    // Paint-through modifier (PaintThroughModifier parity from the PE strings)
                    ui.checkbox(&mut self.brush.paint_through, "Paint Through (3D)")
                        .on_hover_text(
                            "Paint on back-facing / occluded surfaces without rejecting geometry",
                        );
                });

                ui.separator();
                // Tool-specific dynamics
                if self.active_tool == ToolMode::Smudge {
                    ui.label(
                        RichText::new("SMUDGE (EngineActionSmudge)")
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.checkbox(&mut self.brush.smudge_sample_merged, "Sample Merged");
                    ui.checkbox(&mut self.brush.smudge_finger_painting, "Finger Painting");
                }
                if self.active_tool == ToolMode::Projection {
                    ui.label(
                        RichText::new("PROJECTION (screenspace)")
                            .strong()
                            .color(Color32::WHITE),
                    );
                    let mut d = (self.brush.projection_depth * 100.0).round();
                    if ui
                        .add(egui::Slider::new(&mut d, 2.0..=200.0).text("Depth Window"))
                        .changed()
                    {
                        self.brush.projection_depth = d / 100.0;
                    }
                }
                if self.active_tool == ToolMode::Clone {
                    ui.label(RichText::new("CLONE STAMP").strong().color(Color32::WHITE));
                    if self.clone_source_uv.is_some() {
                        ui.colored_label(Color32::from_rgb(100, 240, 150), "● Source Set");
                    } else {
                        ui.colored_label(SMART_MAT_ORANGE, "Alt+Click: Source");
                    }
                    let mut rot = self.brush.clone_rotation;
                    if ui
                        .add(egui::Slider::new(&mut rot, -180.0..=180.0).text("Pattern Rotate °"))
                        .changed()
                    {
                        self.brush.clone_rotation = rot;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Pattern Scale");
                        ui.add(
                            egui::Slider::new(&mut self.brush.clone_scale, 0.05..=4.0)
                                .logarithmic(true),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.brush.clone_mirror_x, "Mirror U");
                        ui.checkbox(&mut self.brush.clone_mirror_y, "Mirror V");
                    });
                }
                if self.active_tool == ToolMode::Decal {
                    ui.label(RichText::new("TEXT / DECAL").strong().color(Color32::WHITE));
                    let mut text_changed = false;
                    ui.add(
                        egui::TextEdit::singleline(&mut self.decal_text)
                            .hint_text("Decal text…")
                            .desired_width(150.0),
                    )
                    .changed();
                    // Font family dropdown — discovered system TTF/OTF fonts; empty path
                    // selects the automatic default (DejaVu/Liberation/Noto preferred).
                    if self.decal_fonts.is_empty() {
                        self.decal_fonts = crate::painting::text::system_fonts().to_vec();
                        self.decal_font_path =
                            crate::painting::text::default_font_path().unwrap_or_default();
                    }
                    let font_name = |path: &str| -> String {
                        std::path::Path::new(path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(path)
                            .to_owned()
                    };
                    let mut font_changed = false;
                    ui.horizontal(|ui| {
                        ui.label("Font Family");
                        let entries: Vec<(String, String)> = std::iter::once((
                            String::new(),
                            font_name(
                                &crate::painting::text::default_font_path().unwrap_or_default(),
                            ),
                        ))
                        .chain(
                            self.decal_fonts
                                .iter()
                                .map(|p| (p.clone(), font_name(p)))
                                .collect::<Vec<_>>(),
                        )
                        .collect();
                        let shown = font_name(&self.decal_font_path);
                        let mut selected = self.decal_font_path.clone();
                        egui::ComboBox::from_id_salt("decal_font_family")
                            .selected_text(shown)
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                for (path, label) in &entries {
                                    let label = if path.is_empty() {
                                        format!("(Auto: {label})")
                                    } else {
                                        label.clone()
                                    };
                                    if ui
                                        .selectable_value(&mut selected, path.clone(), label)
                                        .changed()
                                    {
                                        font_changed = true;
                                    }
                                }
                            });
                        if font_changed {
                            self.decal_font_path = selected;
                        }
                    });
                    if font_changed {
                        // keep the decal preview coherent with the newly chosen family
                        text_changed = true;
                    }
                    if ui.button("Rasterize Text → Alpha").clicked() {
                        text_changed = true;
                    }
                    if text_changed {
                        decal_rasterize(self);
                    }
                    let _ = text_changed;
                    if ui.button("Load Image Alpha…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Image", &["png", "jpg", "jpeg", "tga", "bmp"])
                            .pick_file()
                        {
                            match image::open(&path) {
                                Ok(img) => {
                                    let rgba = img.to_rgba8();
                                    let (w, h) = (rgba.width(), rgba.height());
                                    let a = rgba.into_raw().chunks_exact(4).map(|p| p[3]).collect();
                                    self.decal_alpha = Some(std::sync::Arc::new(
                                        crate::painting::brush::AlphaMaskData {
                                            width: w,
                                            height: h,
                                            data: a,
                                        },
                                    ));
                                    self.status_message =
                                        format!("Decal alpha loaded: {}", path.display());
                                }
                                Err(e) => self.logs.push((
                                    chrono::Local::now().format("%H:%M:%S").to_string(),
                                    "ERROR".into(),
                                    format!("Decal load failed: {}", e),
                                )),
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label("Decal Scale");
                        ui.add(
                            egui::Slider::new(&mut self.decal_scale, 0.05..=8.0).logarithmic(true),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Decal Rotate °");
                        ui.add(egui::Slider::new(&mut self.decal_angle, -180.0..=180.0));
                    });
                    if self.decal_alpha.is_none() {
                        ui.colored_label(SMART_MAT_ORANGE, "No alpha — strokes paint plain brush");
                    }
                }

                ui.separator();
                // 3D/2D View Mode Switchers (reference PBR suite F1/F2/F3/F4)
                if ui
                    .selectable_label(self.viewport_view_mode == 2, "2D | 3D (F1)")
                    .on_hover_text("Split 2D UV Texture Map View & 3D Viewport (reference default)")
                    .clicked()
                {
                    self.viewport_view_mode = 2;
                }
                if ui
                    .selectable_label(self.viewport_view_mode == 0, "3D (F2)")
                    .on_hover_text("3D Viewport Only")
                    .clicked()
                {
                    self.viewport_view_mode = 0;
                }
                if ui
                    .selectable_label(self.viewport_view_mode == 1, "2D (F3)")
                    .on_hover_text("2D UV Texture Map View Only")
                    .clicked()
                {
                    self.viewport_view_mode = 1;
                }
                if ui
                    .selectable_label(self.show_wireframe, "Wire (F4)")
                    .on_hover_text("Toggle Wireframe Overlay (F4)")
                    .clicked()
                {
                    self.show_wireframe = !self.show_wireframe;
                    self.show_uv_wireframe = self.show_wireframe;
                }

                if self.active_tool == ToolMode::PolygonFill {
                    ui.separator();
                    ui.label(egui::RichText::new("Fill:").strong().color(Color32::WHITE));
                    if ui
                        .selectable_label(self.poly_fill_mode == 0, "△ Tri")
                        .clicked()
                    {
                        self.poly_fill_mode = 0;
                    }
                    if ui
                        .selectable_label(self.poly_fill_mode == 1, "⬡ Island")
                        .clicked()
                    {
                        self.poly_fill_mode = 1;
                    }
                    if ui
                        .selectable_label(self.poly_fill_mode == 2, "⬛ Mesh")
                        .clicked()
                    {
                        self.poly_fill_mode = 2;
                    }
                }

                if self.active_tool == ToolMode::Clone {
                    ui.separator();
                    if self.clone_source_uv.is_some() {
                        ui.colored_label(Color32::from_rgb(100, 240, 150), "● Source Set");
                    } else {
                        ui.colored_label(SMART_MAT_ORANGE, "Alt+Click: Source");
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let prev_channel = self.display_channel;
                    egui::ComboBox::from_id_salt("vp_channel_combo")
                        .selected_text(self.channel_name(self.display_channel))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.display_channel, 0, "Material (M)");
                            ui.selectable_value(&mut self.display_channel, 1, "Base Color (C)");
                            ui.selectable_value(&mut self.display_channel, 2, "Roughness");
                            ui.selectable_value(&mut self.display_channel, 3, "Metallic");
                            ui.selectable_value(&mut self.display_channel, 4, "Normal");
                            ui.separator();
                            ui.selectable_value(
                                &mut self.display_channel,
                                5,
                                "Ambient Occlusion (B)",
                            );
                            ui.selectable_value(&mut self.display_channel, 6, "Curvature");
                            ui.selectable_value(&mut self.display_channel, 7, "Position");
                            ui.selectable_value(&mut self.display_channel, 8, "Thickness");
                        });
                    if self.display_channel != prev_channel {
                        self.texture_needs_update = true;
                        self.status_message =
                            format!("Channel: {}", self.channel_name(self.display_channel));
                    }
                });
            });
        });

        // 3. BOTTOM STATUS BAR
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.paint_target_mask {
                    ui.colored_label(
                        Color32::from_rgb(100, 230, 255),
                        "❚ MASK EDITING MODE (Grayscale)",
                    );
                } else {
                    ui.colored_label(Color32::from_rgb(58, 217, 115), "● Ready");
                }
                ui.label(format!(
                    "| Tool: {:?} | Mesh: {} | Stack: {} layers | {}",
                    self.active_tool,
                    self.mesh.name,
                    self.layer_stack.layers.len(),
                    self.status_message
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Disk Cache: 42% | REMoAINTER v2.0.0 (Rust Core)");
                });
            });
        });

        // 4. FAR-LEFT TOOLBAR (38px strip with custom razor-sharp vector icons)
        egui::SidePanel::left("left_tools")
            .exact_width(38.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    let tools = [
                        (ToolMode::Paint, IconType::Brush, "Paint Brush (1)"),
                        (ToolMode::Eraser, IconType::Eraser, "Eraser (2)"),
                        (ToolMode::Projection, IconType::Projection, "Projection (3)"),
                        (
                            ToolMode::PolygonFill,
                            IconType::PolygonFill,
                            "Polygon Fill (4)",
                        ),
                        (ToolMode::Smudge, IconType::Smudge, "Smudge (5)"),
                        (ToolMode::Clone, IconType::CloneStamp, "Clone Stamp (6)"),
                        (ToolMode::Decal, IconType::Decal, "Text / Decal (7)"),
                        (
                            ToolMode::Eyedropper,
                            IconType::Eyedropper,
                            "Color Eyedropper (I)",
                        ),
                    ];

                    for (mode, icon_type, tip) in tools {
                        let active = self.active_tool == mode;
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
                        if ui.is_rect_visible(rect) {
                            let painter = ui.painter();
                            if active {
                                painter.rect_filled(rect, 4.0_f32, ACTIVE_ROW_FILL);
                                painter.rect_stroke(
                                    rect,
                                    4.0_f32,
                                    Stroke::new(1.2_f32, ACTIVE_BLUE),
                                );
                            } else if response.hovered() {
                                painter.rect_filled(rect, 4.0_f32, Color32::from_rgb(45, 45, 48));
                            }
                            let color = if active {
                                Color32::WHITE
                            } else if response.hovered() {
                                Color32::from_rgb(220, 220, 220)
                            } else {
                                Color32::from_rgb(170, 170, 175)
                            };
                            draw_icon(painter, rect, icon_type, color);
                        }
                        if response.on_hover_text(tip).clicked() {
                            self.active_tool = mode;
                        }
                        ui.add_space(3.0);
                    }
                });
            });

        // 5. ASSET SHELF (`ASSETS`) (280px resizable)
        egui::SidePanel::left("asset_shelf")
            .default_width(280.0)
            .width_range(220.0..=480.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("ASSETS").size(14.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{} items", self.shelf_assets.len()))
                                .size(11.0)
                                .color(TEXT_DIMMED),
                        );
                    });
                });

                // Category filter chips
                ui.horizontal_wrapped(|ui| {
                    let cats = [
                        (0, "All"),
                        (1, "Smart Mats"),
                        (2, "Materials"),
                        (3, "Smart Masks"),
                        (4, "Alphas"),
                        (5, "Effects/Generators"),
                    ];
                    for (i, c) in cats {
                        let active = self.shelf_category == i;
                        let btn = egui::Button::new(c).fill(if active {
                            ACTIVE_BLUE
                        } else {
                            Color32::from_rgb(45, 45, 48)
                        });
                        if ui.add(btn).clicked() {
                            self.shelf_category = i;
                        }
                    }
                });

                ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search reference assets (label, category, keywords)..."),
                );
                ui.separator();

                let active_cat = self.shelf_category;
                let query = self.search_query.trim().to_lowercase();

                let matching_indices: Vec<usize> = self
                    .shelf_assets
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, asset)| {
                        if active_cat != 0 && asset.category != active_cat {
                            return None;
                        }
                        if !query.is_empty() && !asset.search_text.contains(&query) {
                            return None;
                        }
                        Some(idx)
                    })
                    .collect();

                let mut clicked_asset: Option<ShelfAsset> = None;

                // Materials & Alphas Grid
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for &idx in &matching_indices {
                            let asset = &self.shelf_assets[idx];
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(76.0, 96.0), egui::Sense::click());
                            let painter = ui.painter();

                            if ui.is_rect_visible(rect) {
                                // Load thumbnail into GPU texture handle if needed
                                if !self.shelf_textures.contains_key(&asset.thumb_path) {
                                    if let Ok(img) = image::open(&asset.thumb_path) {
                                        let rgba = img.to_rgba8();
                                        let (w, h) = rgba.dimensions();
                                        let color_img = egui::ColorImage::from_rgba_unmultiplied(
                                            [w as usize, h as usize],
                                            &rgba.into_raw(),
                                        );
                                        let handle = ctx.load_texture(
                                            format!("thumb_{}", asset.name),
                                            color_img,
                                            egui::TextureOptions::LINEAR,
                                        );
                                        self.shelf_textures
                                            .insert(asset.thumb_path.clone(), handle);
                                    }
                                }

                                painter.rect_filled(rect, 4.0_f32, Color32::from_rgb(26, 26, 26));

                                let preview_rect = Rect::from_min_size(
                                    pos2(rect.left() + 4.0, rect.top() + 4.0),
                                    vec2(68.0, 68.0),
                                );

                                if let Some(tex) = self.shelf_textures.get(&asset.thumb_path) {
                                    painter.image(
                                        tex.id(),
                                        preview_rect,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                } else {
                                    let center = preview_rect.center();
                                    let col = Color32::from_rgba_premultiplied(
                                        (asset.pbr_color[0] * 255.0) as u8,
                                        (asset.pbr_color[1] * 255.0) as u8,
                                        (asset.pbr_color[2] * 255.0) as u8,
                                        255,
                                    );
                                    painter.circle_filled(center, 28.0, col);
                                }

                                // Badge
                                let badge_rect = Rect::from_min_size(
                                    pos2(preview_rect.right() - 20.0, preview_rect.bottom() - 13.0),
                                    vec2(18.0, 12.0),
                                );
                                painter.rect_filled(badge_rect, 2.0_f32, asset.badge_color);
                                painter.text(
                                    badge_rect.center(),
                                    Align2::CENTER_CENTER,
                                    asset.badge,
                                    egui::FontId::proportional(8.5),
                                    Color32::WHITE,
                                );

                                // Label
                                let text_rect = Rect::from_min_size(
                                    pos2(rect.left() + 2.0, rect.top() + 74.0),
                                    vec2(72.0, 18.0),
                                );
                                let mut display_name = asset.name.clone();
                                if display_name.len() > 11 {
                                    display_name.truncate(9);
                                    display_name.push_str("..");
                                }
                                painter.text(
                                    text_rect.center(),
                                    Align2::CENTER_CENTER,
                                    display_name,
                                    egui::FontId::proportional(10.0),
                                    TEXT_PRIMARY,
                                );

                                if response.hovered() {
                                    painter.rect_stroke(
                                        rect,
                                        4.0_f32,
                                        Stroke::new(1.5_f32, ACTIVE_BLUE),
                                    );
                                }
                            }

                            if response.on_hover_text(&asset.name).clicked() {
                                clicked_asset = Some(asset.clone());
                            }
                        }
                    });
                });

                if let Some(asset) = clicked_asset {
                    self.handle_shelf_click(asset);
                }
            });

        // 6. FAR-RIGHT DOCK STRIP (34px narrow strip with vector icons)
        egui::SidePanel::right("right_strip")
            .exact_width(34.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    let strip = [
                        (0, IconType::Settings, "Texture Set Settings"),
                        (1, IconType::TextureSetList, "Texture Set List"),
                        (2, IconType::Layers, "Layers (F4)"),
                        (3, IconType::Display, "Display Settings"),
                        (4, IconType::Shader, "Shader Settings"),
                        (5, IconType::History, "History"),
                        (6, IconType::Log, "Log Console"),
                    ];

                    for (idx, icon_type, tip) in strip {
                        let active = self.active_right_panel == idx;
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
                        if ui.is_rect_visible(rect) {
                            let painter = ui.painter();
                            if active {
                                painter.rect_filled(rect, 3.0_f32, ACTIVE_ROW_FILL);
                                painter.rect_stroke(
                                    rect,
                                    3.0_f32,
                                    Stroke::new(1.2_f32, ACTIVE_BLUE),
                                );
                            } else if response.hovered() {
                                painter.rect_filled(rect, 3.0_f32, Color32::from_rgb(45, 45, 48));
                            }
                            let color = if active {
                                Color32::WHITE
                            } else if response.hovered() {
                                Color32::from_rgb(220, 220, 220)
                            } else {
                                Color32::from_rgb(170, 170, 175)
                            };
                            draw_icon(painter, rect, icon_type, color);
                        }
                        if response.on_hover_text(tip).clicked() {
                            self.active_right_panel = idx;
                        }
                        ui.add_space(3.0);
                    }
                });
            });

        // 7. RIGHT DOCK PANELS
        egui::SidePanel::right("right_panel")
            .default_width(320.0)
            .width_range(260.0..=480.0)
            .show(ctx, |ui| match self.active_right_panel {
                0 => self.ui_texture_set_settings(ui),
                1 => self.ui_texture_set_list(ui),
                2 => self.ui_layers_panel(ui),
                3 => self.ui_display_settings(ui),
                4 => self.ui_shader_settings(ui),
                5 => self.ui_history_panel(ui),
                6 => self.ui_log_panel(ui),
                _ => {}
            });

        // 8. CENTRAL 3D VIEWPORT & PAINT CANVAS
        egui::CentralPanel::default().show(ctx, |ui| {
            // Update GPU texture if dirty or not loaded
            if self.texture_needs_update || self.texture_handle.is_none() {
                let w = self.layer_stack.width as usize;
                let h = self.layer_stack.height as usize;
                let pixels: Vec<Color32> = match self.display_channel {
                    1 => self
                        .layer_stack
                        .composite_color
                        .iter()
                        .map(|c| {
                            Color32::from_rgb(
                                (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                                (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                                (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                            )
                        })
                        .collect(),
                    2 => self
                        .layer_stack
                        .composite_roughness
                        .iter()
                        .map(|r| {
                            let v = (r.clamp(0.0, 1.0) * 255.0) as u8;
                            Color32::from_rgb(v, v, v)
                        })
                        .collect(),
                    3 => self
                        .layer_stack
                        .composite_metallic
                        .iter()
                        .map(|m| {
                            let v = (m.clamp(0.0, 1.0) * 255.0) as u8;
                            Color32::from_rgb(v, v, v)
                        })
                        .collect(),
                    4 => self
                        .layer_stack
                        .composite_normal
                        .iter()
                        .map(|n| {
                            Color32::from_rgb(
                                (n[0].clamp(0.0, 1.0) * 255.0) as u8,
                                (n[1].clamp(0.0, 1.0) * 255.0) as u8,
                                (n[2].clamp(0.0, 1.0) * 255.0) as u8,
                            )
                        })
                        .collect(),
                    5 => {
                        if let Some(ref ao) = self.baked_maps.ambient_occlusion {
                            ao.iter()
                                .map(|v| {
                                    let b = (v.clamp(0.0, 1.0) * 255.0) as u8;
                                    Color32::from_rgb(b, b, b)
                                })
                                .collect()
                        } else {
                            vec![Color32::WHITE; w * h]
                        }
                    }
                    6 => {
                        if let Some(ref cur) = self.baked_maps.curvature {
                            cur.iter()
                                .map(|v| {
                                    let b = (v.clamp(0.0, 1.0) * 255.0) as u8;
                                    Color32::from_rgb(b, b, b)
                                })
                                .collect()
                        } else {
                            vec![Color32::from_gray(128); w * h]
                        }
                    }
                    7 => {
                        if let Some(ref pos) = self.baked_maps.position {
                            pos.iter()
                                .map(|p| {
                                    Color32::from_rgb(
                                        (p[0].clamp(0.0, 1.0) * 255.0) as u8,
                                        (p[1].clamp(0.0, 1.0) * 255.0) as u8,
                                        (p[2].clamp(0.0, 1.0) * 255.0) as u8,
                                    )
                                })
                                .collect()
                        } else {
                            vec![Color32::from_gray(128); w * h]
                        }
                    }
                    8 => {
                        if let Some(ref thk) = self.baked_maps.thickness {
                            thk.iter()
                                .map(|v| {
                                    let b = (v.clamp(0.0, 1.0) * 255.0) as u8;
                                    Color32::from_rgb(b, b, b)
                                })
                                .collect()
                        } else {
                            vec![Color32::WHITE; w * h]
                        }
                    }
                    _ => self
                        .layer_stack
                        .composite_lit
                        .iter()
                        .map(|c| {
                            Color32::from_rgb(
                                (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                                (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                                (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                            )
                        })
                        .collect(),
                };
                let img = egui::ColorImage {
                    size: [w, h],
                    pixels,
                };
                if let Some(ref mut handle) = self.texture_handle {
                    handle.set(img, egui::TextureOptions::LINEAR_REPEAT);
                } else {
                    self.texture_handle = Some(ctx.load_texture(
                        "pbr_composite",
                        img,
                        egui::TextureOptions::LINEAR_REPEAT,
                    ));
                }
                self.texture_needs_update = false;
            }

            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter();

            // Background Studio Gradient
            painter.rect_filled(rect, 0.0_f32, Color32::from_rgb(26, 26, 28));

            // Top Floating Overlay Bar
            let overlay_rect = Rect::from_min_size(
                pos2(rect.left() + 16.0, rect.top() + 12.0),
                vec2(580.0, 32.0),
            );
            painter.rect_filled(overlay_rect, 4.0_f32, Color32::from_black_alpha(200));
            painter.text(
                pos2(overlay_rect.left() + 10.0, overlay_rect.center().y),
                Align2::LEFT_CENTER,
                format!("{} | Persp 50mm | Lit", self.mesh.name),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );

            // View mode switchers (Wire: F4, F1: 3D|2D, F2: 3D, F3: 2D)
            let vm_wire = Rect::from_min_size(
                pos2(overlay_rect.right() - 280.0, overlay_rect.top() + 4.0),
                vec2(66.0, 24.0),
            );
            let vm_3d = Rect::from_min_size(
                pos2(overlay_rect.right() - 210.0, overlay_rect.top() + 4.0),
                vec2(62.0, 24.0),
            );
            let vm_2d = Rect::from_min_size(
                pos2(overlay_rect.right() - 142.0, overlay_rect.top() + 4.0),
                vec2(62.0, 24.0),
            );
            let vm_split = Rect::from_min_size(
                pos2(overlay_rect.right() - 74.0, overlay_rect.top() + 4.0),
                vec2(70.0, 24.0),
            );

            painter.rect_filled(
                vm_wire,
                3.0_f32,
                if self.show_wireframe {
                    ACTIVE_ROW_FILL
                } else {
                    PANEL_BG
                },
            );
            painter.text(
                vm_wire.center(),
                Align2::CENTER_CENTER,
                "Wire (F4)",
                egui::FontId::proportional(10.5),
                if self.show_wireframe {
                    Color32::WHITE
                } else {
                    TEXT_DIMMED
                },
            );

            painter.rect_filled(
                vm_3d,
                3.0_f32,
                if self.viewport_view_mode == 0 {
                    ACTIVE_ROW_FILL
                } else {
                    PANEL_BG
                },
            );
            painter.text(
                vm_3d.center(),
                Align2::CENTER_CENTER,
                "3D (F2)",
                egui::FontId::proportional(10.5),
                Color32::WHITE,
            );

            painter.rect_filled(
                vm_2d,
                3.0_f32,
                if self.viewport_view_mode == 1 {
                    ACTIVE_ROW_FILL
                } else {
                    PANEL_BG
                },
            );
            painter.text(
                vm_2d.center(),
                Align2::CENTER_CENTER,
                "2D (F3)",
                egui::FontId::proportional(10.5),
                Color32::WHITE,
            );

            painter.rect_filled(
                vm_split,
                3.0_f32,
                if self.viewport_view_mode == 2 {
                    ACTIVE_ROW_FILL
                } else {
                    PANEL_BG
                },
            );
            painter.text(
                vm_split.center(),
                Align2::CENTER_CENTER,
                "2D|3D (F1)",
                egui::FontId::proportional(10.5),
                Color32::WHITE,
            );

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if vm_wire.contains(pos) {
                        self.show_wireframe = !self.show_wireframe;
                        self.show_uv_wireframe = !self.show_uv_wireframe;
                    } else if vm_3d.contains(pos) {
                        self.viewport_view_mode = 0;
                    } else if vm_2d.contains(pos) {
                        self.viewport_view_mode = 1;
                    } else if vm_split.contains(pos) {
                        self.viewport_view_mode = 2;
                    }
                }
            }

            if ctx.input(|i| i.key_pressed(egui::Key::F4)) {
                self.show_wireframe = !self.show_wireframe;
                self.show_uv_wireframe = !self.show_uv_wireframe;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
                self.viewport_view_mode = 2;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                self.viewport_view_mode = 0;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
                self.viewport_view_mode = 1;
            }

            // Layout based on view mode
            match self.viewport_view_mode {
                0 => {
                    self.render_3d_viewport(painter, rect, &response, ctx);
                }
                1 => {
                    self.render_2d_canvas(painter, rect, &response, ctx);
                }
                _ => {
                    // reference PBR suite parity: 2D UV view on the LEFT, 3D viewport on the RIGHT
                    let half_w = rect.width() * 0.5;
                    let rect_2d = Rect::from_min_size(rect.min, vec2(half_w, rect.height()));
                    let rect_3d = Rect::from_min_size(
                        pos2(rect.min.x + half_w, rect.min.y),
                        vec2(half_w, rect.height()),
                    );

                    painter.rect_filled(rect_2d, 0.0_f32, Color32::from_rgb(38, 38, 40));
                    painter.line_segment(
                        [
                            pos2(rect.min.x + half_w, rect.min.y),
                            pos2(rect.min.x + half_w, rect.max.y),
                        ],
                        Stroke::new(1.0_f32, BORDER_COLOR),
                    );
                    self.render_3d_viewport(painter, rect_3d, &response, ctx);
                    self.render_2d_canvas(painter, rect_2d, &response, ctx);
                }
            }
        });

        // Texture Importer Modal
        if self.show_texture_importer_dialog {
            self.ui_texture_importer_modal(ctx);
        }

        // Export Textures Modal (File > Export Textures, Ctrl+Shift+E)
        if self.show_export_dialog {
            self.ui_export_modal(ctx);
        }

        // New Project Modal (File > New..., Ctrl+N)
        if self.show_new_project_dialog {
            self.ui_new_project_modal(ctx);
        }

        // Project Snapshots & Autosave Recovery Modal
        if self.show_snapshots_dialog {
            self.ui_snapshots_modal(ctx);
        }

        // Application Preferences Modal (Edit > Preferences...)
        if self.show_preferences_dialog {
            self.ui_preferences_modal(ctx);
        }

        // Hotkey Editor Modal (Edit > Hotkey Editor...)
        if self.show_hotkeys_dialog {
            self.ui_hotkey_editor_modal(ctx);
        }

        // Documentation & Shortcuts Help Modal (Help > Documentation / Shortcuts...)
        if self.show_shortcuts_dialog {
            self.ui_shortcuts_help_modal(ctx);
        }
    }
}

// Right panel implementations
impl RemoainterApp {
    #[inline]
    fn uv_to_2d_screen(&self, uv: [f32; 2], canvas_rect: Rect) -> egui::Pos2 {
        pos2(
            canvas_rect.left() + uv[0] * canvas_rect.width(),
            canvas_rect.top() + uv[1] * canvas_rect.height(),
        )
    }

    fn render_2d_canvas(
        &mut self,
        painter: &egui::Painter,
        canvas_bounds: Rect,
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        // reference PBR suite parity: a full-pane dark 2D UV viewport with the texture in a
        // fit-to-pane window, checkerboard under transparent areas, wheel zoom and
        // middle-drag pan around the pointer.
        painter.rect_filled(canvas_bounds, 0.0_f32, Color32::from_rgb(38, 38, 40));

        let base_size = (canvas_bounds.width().min(canvas_bounds.height()) - 48.0).max(64.0);
        let base_center = canvas_bounds.center();

        // Wheel zoom around the pointer (reference 2D view behavior)
        if canvas_bounds.contains(
            ctx.pointer_latest_pos()
                .unwrap_or(egui::pos2(f32::INFINITY, f32::INFINITY)),
        ) {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.05_f32 {
                let old = self.uv_zoom;
                let new = (self.uv_zoom * (1.0 + scroll * 0.0015)).clamp(0.1_f32, 16.0_f32);
                if (new - old).abs() > f32::EPSILON {
                    // Keep the texture point under the cursor pinned while zooming.
                    let ptr = ctx
                        .pointer_latest_pos()
                        .unwrap_or_else(|| canvas_bounds.center());
                    let rel = egui::vec2(
                        ptr.x - base_center.x - self.uv_pan[0],
                        ptr.y - base_center.y - self.uv_pan[1],
                    ) / old;
                    self.uv_pan = [
                        ptr.x - base_center.x - rel.x * new,
                        ptr.y - base_center.y - rel.y * new,
                    ];
                    self.uv_zoom = new;
                }
            }
        }

        // Middle-mouse-drag pan
        if response.dragged() && ctx.input(|i| i.pointer.middle_down()) {
            let d = response.drag_delta();
            self.uv_pan[0] += d.x;
            self.uv_pan[1] += d.y;
        }

        let size = base_size * self.uv_zoom;
        let canvas_rect = Rect::from_center_size(
            base_center + egui::vec2(self.uv_pan[0], self.uv_pan[1]),
            vec2(size, size),
        );

        // Checkerboard behind the texture (reference transparency indicator), 16px tile
        let checker_id = self
            .checker_handle
            .get_or_insert_with(|| {
                let mut px = Vec::with_capacity(4);
                for y in 0..2 {
                    for x in 0..2 {
                        px.push(if (x ^ y) & 1 == 0 {
                            Color32::from_rgb(70, 70, 70)
                        } else {
                            Color32::from_rgb(46, 46, 46)
                        });
                    }
                }
                ctx.load_texture(
                    "2d_checker",
                    egui::ColorImage {
                        size: [2, 2],
                        pixels: px,
                    },
                    egui::TextureOptions::LINEAR_REPEAT,
                )
            })
            .id();
        painter.image(
            checker_id,
            canvas_rect,
            Rect::from_min_max(
                pos2(0.0, 0.0),
                pos2(canvas_rect.width() / 16.0, canvas_rect.height() / 16.0),
            ),
            Color32::WHITE,
        );

        // Draw live GPU texture map
        if let Some(ref handle) = self.texture_handle {
            painter.image(
                handle.id(),
                canvas_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // Texture window border
        painter.rect_stroke(canvas_rect, 2.0_f32, Stroke::new(1.0_f32, BORDER_COLOR));

        // Draw UV Mesh Wireframe Overlay (reference PBR suite 2D UV Layout)
        // reference renders UV island outlines & seams (boundary edges), not the dense
        // triangle soup — boundary edges (counted once) are exactly that.
        if self.show_uv_wireframe {
            let mut edge_counts: std::collections::HashMap<(i64, i64, i64, i64), u32> =
                std::collections::HashMap::new();
            let num_tris = self.mesh.indices.len() / 3;
            for t in 0..num_tris {
                let i0 = self.mesh.indices[t * 3] as usize;
                let i1 = self.mesh.indices[t * 3 + 1] as usize;
                let i2 = self.mesh.indices[t * 3 + 2] as usize;
                if i0 < self.mesh.uvs.len() && i1 < self.mesh.uvs.len() && i2 < self.mesh.uvs.len()
                {
                    for (a, b) in [(i0, i1), (i1, i2), (i2, i0)] {
                        let ua = self.mesh.uvs[a];
                        let ub = self.mesh.uvs[b];
                        let ka = (
                            (ua[0] * 8192.0).round() as i64,
                            (ua[1] * 8192.0).round() as i64,
                        );
                        let kb = (
                            (ub[0] * 8192.0).round() as i64,
                            (ub[1] * 8192.0).round() as i64,
                        );
                        let pair = if (ka, kb) < (kb, ka) {
                            (ka.0, ka.1, kb.0, kb.1)
                        } else {
                            (kb.0, kb.1, ka.0, ka.1)
                        };
                        *edge_counts.entry(pair).or_insert(0) += 1;
                    }
                }
            }
            let outline_color = Color32::from_rgba_unmultiplied(65, 175, 245, 190);
            let outline_stroke = Stroke::new(1.4_f32, outline_color);
            for ((a0, a1, b0, b1), count) in edge_counts {
                if count != 1 {
                    continue; // interior (shared) edge — not an island outline
                }
                let pa =
                    self.uv_to_2d_screen([a0 as f32 / 8192.0, a1 as f32 / 8192.0], canvas_rect);
                let pb =
                    self.uv_to_2d_screen([b0 as f32 / 8192.0, b1 as f32 / 8192.0], canvas_rect);
                painter.line_segment([pa, pb], outline_stroke);
            }
        }

        // Texture border & UV grid indicator
        painter.text(
            pos2(canvas_rect.left() + 8.0, canvas_rect.top() + 8.0),
            Align2::LEFT_TOP,
            format!(
                "UV Layout ({}x{}) | Wire: {} | Zoom {:.0}%",
                self.layer_stack.width,
                self.layer_stack.height,
                if self.show_uv_wireframe {
                    "ON (F4)"
                } else {
                    "OFF"
                },
                self.uv_zoom * 100.0
            ),
            egui::FontId::monospace(10.5),
            TEXT_DIMMED,
        );
        // Pan/zoom hint (reference-style toolbar info)
        if canvas_bounds.width() > 320.0 {
            painter.text(
                pos2(canvas_bounds.right() - 8.0, canvas_bounds.top() + 8.0),
                Align2::RIGHT_TOP,
                "Wheel: Zoom | MMB: Pan",
                egui::FontId::monospace(10.0),
                TEXT_DIMMED,
            );
        }

        // Brush cursor preview
        if let Some(pos) = ctx.pointer_hover_pos() {
            if canvas_rect.contains(pos) {
                let brush_screen_r = (self.brush.size / self.layer_stack.width as f32) * size * 0.5;
                painter.circle_stroke(
                    pos,
                    brush_screen_r.max(2.0),
                    Stroke::new(1.2_f32, ACTIVE_BLUE),
                );
            }
        }

        // Paint handling inside 2D Canvas
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if canvas_rect.contains(pos)
                    && !ctx.input(|i| i.modifiers.alt)
                    && !ctx.input(|i| i.pointer.middle_down())
                {
                    if !self.is_in_stroke {
                        self.history_manager.begin_stroke();
                        self.is_in_stroke = true;
                    }

                    let paint_pos = if self.brush.lazy_mouse {
                        let smoothed = self.lazy_mouse_tracker.update([pos.x, pos.y]);
                        if self.lazy_mouse_tracker.is_dragging {
                            painter.line_segment(
                                [pos, pos2(smoothed[0], smoothed[1])],
                                Stroke::new(1.2_f32, Color32::from_rgb(255, 120, 40)),
                            );
                            painter.circle_stroke(
                                pos2(smoothed[0], smoothed[1]),
                                2.5,
                                Stroke::new(1.0_f32, Color32::WHITE),
                            );
                        }
                        pos2(smoothed[0], smoothed[1])
                    } else {
                        pos
                    };

                    let u =
                        ((paint_pos.x - canvas_rect.left()) / canvas_rect.width()).clamp(0.0, 1.0);
                    let v =
                        ((paint_pos.y - canvas_rect.top()) / canvas_rect.height()).clamp(0.0, 1.0);
                    let uv = [u, v];

                    let active_idx = self.layer_stack.active_layer_index;

                    let bbox = if self.active_tool == ToolMode::Eyedropper {
                        let px = (uv[0] * (self.layer_stack.width - 1) as f32) as usize;
                        let py = (uv[1] * (self.layer_stack.height - 1) as f32) as usize;
                        let idx = py * self.layer_stack.width as usize + px;
                        if idx < self.layer_stack.composite_color.len() {
                            self.brush.color = self.layer_stack.composite_color[idx];
                            self.brush.metallic = self.layer_stack.composite_metallic[idx];
                            self.brush.roughness = self.layer_stack.composite_roughness[idx];
                            self.status_message = format!(
                                "Sampled: Color [{:.2}, {:.2}, {:.2}], Met: {:.2}, Rough: {:.2}",
                                self.brush.color[0],
                                self.brush.color[1],
                                self.brush.color[2],
                                self.brush.metallic,
                                self.brush.roughness
                            );
                        }
                        [0, 0, 0, 0]
                    } else if self.active_tool == ToolMode::PolygonFill {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                            if self.poly_fill_mode == 2 {
                                active_layer
                                    .flood_fill(&self.brush, Some(&mut self.history_manager));
                            } else {
                                let num_tris = self.mesh.indices.len() / 3;
                                let mut hit_t = None;
                                for t in 0..num_tris {
                                    let i0 = self.mesh.indices[t * 3] as usize;
                                    let i1 = self.mesh.indices[t * 3 + 1] as usize;
                                    let i2 = self.mesh.indices[t * 3 + 2] as usize;
                                    if i0 < self.mesh.uvs.len()
                                        && i1 < self.mesh.uvs.len()
                                        && i2 < self.mesh.uvs.len()
                                    {
                                        let uv0 = self.mesh.uvs[i0];
                                        let uv1 = self.mesh.uvs[i1];
                                        let uv2 = self.mesh.uvs[i2];
                                        let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0])
                                            + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1]))
                                            .max(1e-7);
                                        let b0 = ((uv1[1] - uv2[1]) * (uv[0] - uv2[0])
                                            + (uv2[0] - uv1[0]) * (uv[1] - uv2[1]))
                                            / denom;
                                        let b1 = ((uv2[1] - uv0[1]) * (uv[0] - uv2[0])
                                            + (uv0[0] - uv2[0]) * (uv[1] - uv2[1]))
                                            / denom;
                                        let b2 = 1.0 - b0 - b1;
                                        if b0 >= 0.0 && b1 >= 0.0 && b2 >= 0.0 {
                                            hit_t = Some(t);
                                            break;
                                        }
                                    }
                                }
                                if let Some(t) = hit_t {
                                    if self.poly_fill_mode == 0 {
                                        let i0 = self.mesh.indices[t * 3] as usize;
                                        let i1 = self.mesh.indices[t * 3 + 1] as usize;
                                        let i2 = self.mesh.indices[t * 3 + 2] as usize;
                                        active_layer.fill_triangle(
                                            self.mesh.uvs[i0],
                                            self.mesh.uvs[i1],
                                            self.mesh.uvs[i2],
                                            &self.brush,
                                            Some(&mut self.history_manager),
                                        );
                                    } else {
                                        let island = self.mesh.find_uv_island(t);
                                        for it in island {
                                            let i0 = self.mesh.indices[it * 3] as usize;
                                            let i1 = self.mesh.indices[it * 3 + 1] as usize;
                                            let i2 = self.mesh.indices[it * 3 + 2] as usize;
                                            if i0 < self.mesh.uvs.len()
                                                && i1 < self.mesh.uvs.len()
                                                && i2 < self.mesh.uvs.len()
                                            {
                                                active_layer.fill_triangle(
                                                    self.mesh.uvs[i0],
                                                    self.mesh.uvs[i1],
                                                    self.mesh.uvs[i2],
                                                    &self.brush,
                                                    Some(&mut self.history_manager),
                                                );
                                            }
                                        }
                                    }
                                } else {
                                    active_layer
                                        .flood_fill(&self.brush, Some(&mut self.history_manager));
                                }
                            }
                        }
                        self.recompute_composite();
                        [
                            0,
                            0,
                            self.layer_stack.width as i32,
                            self.layer_stack.height as i32,
                        ]
                    } else if self.active_tool == ToolMode::Smudge {
                        // EngineActionSmudge parity: displacement along the stroke path
                        let delta = match self.last_painted_uv {
                            Some(last) => [uv[0] - last[0], uv[1] - last[1]],
                            None => [0.0, 0.0],
                        };
                        self.layer_stack.smudge_stamp_at(
                            uv,
                            delta,
                            &self.brush,
                            Some(&mut self.history_manager),
                        )
                    } else if self.active_tool == ToolMode::Clone {
                        if ctx.input(|i| i.modifiers.alt) {
                            self.clone_source_uv = Some(uv);
                            self.clone_offset_uv = None;
                            self.status_message =
                                format!("Clone Source Set: [{:.2}, {:.2}]", uv[0], uv[1]);
                            [0, 0, 0, 0]
                        } else if let Some(source) = self.clone_source_uv {
                            // Aligned offset locked at the first stamp of each stroke
                            if self.last_painted_uv.is_none() {
                                self.clone_offset_uv = Some([uv[0] - source[0], uv[1] - source[1]]);
                            }
                            let offset = self
                                .clone_offset_uv
                                .unwrap_or([uv[0] - source[0], uv[1] - source[1]]);
                            let src_uv = [uv[0] - offset[0], uv[1] - offset[1]];
                            self.layer_stack.clone_stamp_at(
                                uv,
                                src_uv,
                                &self.brush,
                                self.paint_target_mask,
                                Some(&mut self.history_manager),
                            )
                        } else {
                            self.status_message =
                                "Hold Alt and click to set the clone source".into();
                            [0, 0, 0, 0]
                        }
                    } else {
                        let is_eraser = self.active_tool == ToolMode::Eraser;
                        let on_mask = self.paint_target_mask;
                        // Decal tool: paint with the decal alpha + transform as one-shot custom alpha
                        let brush_eff = if self.active_tool == ToolMode::Decal {
                            let mut b = self.brush.clone();
                            b.custom_alpha = self.decal_alpha.clone();
                            b.angle = self.decal_angle;
                            b.size = (self.brush.size * self.decal_scale.max(0.05)).max(2.0);
                            b
                        } else {
                            self.brush.clone()
                        };
                        if let Some(last_uv) = self.last_painted_uv {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx)
                            {
                                let mut b = if is_eraser {
                                    active_layer.erase_line(
                                        last_uv,
                                        uv,
                                        &brush_eff,
                                        on_mask,
                                        Some(&mut self.history_manager),
                                    )
                                } else {
                                    active_layer.paint_line(
                                        last_uv,
                                        uv,
                                        &brush_eff,
                                        on_mask,
                                        Some(&mut self.history_manager),
                                    )
                                };
                                if self.symmetry_x {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_line(
                                            [1.0 - last_uv[0], last_uv[1]],
                                            [1.0 - uv[0], uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    } else {
                                        active_layer.paint_line(
                                            [1.0 - last_uv[0], last_uv[1]],
                                            [1.0 - uv[0], uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    };
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_y {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_line(
                                            [last_uv[0], 1.0 - last_uv[1]],
                                            [uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    } else {
                                        active_layer.paint_line(
                                            [last_uv[0], 1.0 - last_uv[1]],
                                            [uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    };
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_z {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_line(
                                            [1.0 - last_uv[0], 1.0 - last_uv[1]],
                                            [1.0 - uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    } else {
                                        active_layer.paint_line(
                                            [1.0 - last_uv[0], 1.0 - last_uv[1]],
                                            [1.0 - uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    };
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                                b
                            } else {
                                [0, 0, 0, 0]
                            }
                        } else {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx)
                            {
                                let mut b = if is_eraser {
                                    active_layer.erase_stamp(
                                        uv,
                                        &brush_eff,
                                        on_mask,
                                        Some(&mut self.history_manager),
                                    )
                                } else {
                                    active_layer.paint_stamp(
                                        uv,
                                        &brush_eff,
                                        on_mask,
                                        Some(&mut self.history_manager),
                                    )
                                };
                                if self.symmetry_x {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_stamp(
                                            [1.0 - uv[0], uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    } else {
                                        active_layer.paint_stamp(
                                            [1.0 - uv[0], uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    };
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_y {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_stamp(
                                            [uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    } else {
                                        active_layer.paint_stamp(
                                            [uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    };
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_z {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_stamp(
                                            [1.0 - uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    } else {
                                        active_layer.paint_stamp(
                                            [1.0 - uv[0], 1.0 - uv[1]],
                                            &brush_eff,
                                            on_mask,
                                            Some(&mut self.history_manager),
                                        )
                                    };
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                                b
                            } else {
                                [0, 0, 0, 0]
                            }
                        }
                    };
                    self.last_painted_uv = Some(uv);
                    if bbox[2] > bbox[0] && bbox[3] > bbox[1] {
                        self.recompute_dirty(bbox);
                    }
                }
            }
        } else {
            self.last_painted_uv = None;
            if self.is_in_stroke {
                let tool_name = match self.active_tool {
                    ToolMode::Paint => "Paint Stroke",
                    ToolMode::Eraser => "Erase",
                    ToolMode::PolygonFill => "Polygon Fill",
                    ToolMode::Smudge => "Smudge",
                    ToolMode::Clone => "Clone Stamp",
                    ToolMode::Projection => "Projection Paint",
                    ToolMode::Decal => "Decal Stamp",
                    _ => "Brush Tool",
                };
                self.history_manager.commit_stroke(tool_name);
                self.history
                    .push(format!("{:02}  {}", self.history.len() + 1, tool_name));
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "INFO".into(),
                    format!("Committed stroke: {}", tool_name),
                ));
                self.is_in_stroke = false;
                self.clone_offset_uv = None;
                self.lazy_mouse_tracker.reset();
            }
        }
    }

    fn render_3d_viewport(
        &mut self,
        painter: &egui::Painter,
        view_bounds: Rect,
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        let center = view_bounds.center();
        let yaw_rad = self.camera_yaw.to_radians();
        let pitch_rad = self.camera_pitch.to_radians();
        let _view_scale = view_bounds.height().min(view_bounds.width()) * 1.35;

        // Smooth camera zoom via mouse scroll wheel
        if view_bounds.contains(ctx.pointer_latest_pos().unwrap_or_default()) {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.05 {
                self.camera_dist = (self.camera_dist - scroll * 0.003).clamp(0.5, 6.0);
            }
        }

        // 1. Rust & Vulkan / wgpu Hardware PBR 3D Render Callback (24-bit Depth Test + Per-Fragment GGX + ACES)
        if let Some(ref vp_mutex) = self.viewport_wgpu {
            let aspect = view_bounds.width() / view_bounds.height().max(1.0);
            let params = ViewportPaintParams {
                yaw: self.camera_yaw,
                pitch: self.camera_pitch,
                cam_dist: self.camera_dist,
                light_intensity: self.light_intensity,
                env_rotation: self.env_rotation,
                display_channel: self.display_channel,
                exposure: self.exposure,
                skybox_visible: self.skybox_visible,
                skybox_blur: self.skybox_blur,
                skybox_intensity: self.skybox_intensity,
                uv_scale: self.uv_scale,
                show_wireframe: self.show_wireframe,
                show_grid: self.show_grid,
                bloom_enabled: self.bloom_enabled,
                bloom_intensity: self.bloom_intensity,
                bloom_threshold: self.bloom_threshold,
                ssao_enabled: self.ssao_enabled,
                ssao_intensity: self.ssao_intensity,
                dof_enabled: self.dof_enabled,
                dof_focal_distance: self.dof_focal_distance,
                dof_aperture: self.dof_aperture,
                color_management_mode: self.color_management_mode,
            };

            struct ViewportWgpuCallback {
                viewport: Arc<Mutex<ViewportWgpu>>,
                params: ViewportPaintParams,
                aspect: f32,
            }

            impl egui_wgpu::CallbackTrait for ViewportWgpuCallback {
                fn prepare(
                    &self,
                    device: &wgpu::Device,
                    queue: &wgpu::Queue,
                    _screen_descriptor: &egui_wgpu::ScreenDescriptor,
                    _egui_encoder: &mut wgpu::CommandEncoder,
                    _callback_resources: &mut egui_wgpu::CallbackResources,
                ) -> Vec<wgpu::CommandBuffer> {
                    if let Ok(mut vp) = self.viewport.lock() {
                        vp.prepare(device, queue, &self.params, self.aspect);
                    }
                    Vec::new()
                }

                fn paint(
                    &self,
                    _info: egui::PaintCallbackInfo,
                    render_pass: &mut wgpu::RenderPass<'static>,
                    _callback_resources: &egui_wgpu::CallbackResources,
                ) {
                    if let Ok(vp) = self.viewport.lock() {
                        vp.paint(render_pass);
                    }
                }
            }

            let callback = ViewportWgpuCallback {
                viewport: vp_mutex.clone(),
                params,
                aspect,
            };

            painter.add(egui_wgpu::Callback::new_paint_callback(
                view_bounds,
                callback,
            ));
        }

        // Raycasting & 3D Surface Painting
        let (mesh_center, mesh_radius) = self.mesh.compute_bounds();
        let aspect = view_bounds.width() / view_bounds.height().max(1.0);
        let proj = glam::Mat4::perspective_rh(45.0f32.to_radians(), aspect, 0.05, 50.0);
        let cam_pos = glam::Vec3::new(
            self.camera_dist * pitch_rad.cos() * yaw_rad.sin(),
            self.camera_dist * pitch_rad.sin(),
            self.camera_dist * pitch_rad.cos() * yaw_rad.cos(),
        );
        let view = glam::Mat4::look_at_rh(cam_pos, glam::Vec3::ZERO, glam::Vec3::Y);
        let inv_view_proj = (proj * view).inverse();

        // Alt+Click (without dragging) sets the clone stamp source in the 3D view
        if response.clicked()
            && ctx.input(|i| i.modifiers.alt)
            && self.active_tool == ToolMode::Clone
            && view_bounds.contains(
                response
                    .interact_pointer_pos()
                    .unwrap_or(egui::pos2(f32::INFINITY, f32::INFINITY)),
            )
        {
            if let Some(pos) = response.interact_pointer_pos() {
                let ndc_x = (pos.x - view_bounds.left()) / view_bounds.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - view_bounds.top()) / view_bounds.height() * 2.0;
                let p_far = inv_view_proj.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
                let ray_dir = (p_far - cam_pos).normalize();
                if let Some(hit) =
                    self.mesh
                        .raycast_normalized(cam_pos, ray_dir, mesh_center, mesh_radius)
                {
                    self.clone_source_uv = Some(hit.uv);
                    self.clone_offset_uv = None;
                    self.status_message = format!(
                        "Clone Source Set (3D): [{:.2}, {:.2}]",
                        hit.uv[0], hit.uv[1]
                    );
                }
            }
        }

        // Camera Orbit (Alt+LMB/MMB) / Zoom (Alt+RMB) / Environment Rotate (Shift+RMB) & 3D Surface Painting
        // (drag contained to this viewport pane so it cannot leak into the 2D pane in split layout)
        if response.dragged()
            && view_bounds.contains(
                response
                    .interact_pointer_pos()
                    .unwrap_or(egui::pos2(f32::INFINITY, f32::INFINITY)),
            )
        {
            let delta = response.drag_delta();
            if ctx
                .input(|i| i.modifiers.alt && i.pointer.button_down(egui::PointerButton::Secondary))
            {
                self.camera_dist = (self.camera_dist + delta.y * 0.01).clamp(0.5, 6.0);
            } else if ctx.input(|i| {
                i.modifiers.shift && i.pointer.button_down(egui::PointerButton::Secondary)
            }) {
                self.env_rotation = (self.env_rotation + delta.x * 0.8).rem_euclid(360.0);
            } else if ctx.input(|i| i.modifiers.alt) || ctx.input(|i| i.pointer.middle_down()) {
                self.camera_yaw += delta.x * 0.4;
                self.camera_pitch = (self.camera_pitch + delta.y * 0.4).clamp(-85.0, 85.0);
            } else if let Some(pos) = response.interact_pointer_pos() {
                let ndc_x = (pos.x - view_bounds.left()) / view_bounds.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - view_bounds.top()) / view_bounds.height() * 2.0;
                let p_far = inv_view_proj.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
                let ray_dir = (p_far - cam_pos).normalize();

                if let Some(hit) =
                    self.mesh
                        .raycast_normalized(cam_pos, ray_dir, mesh_center, mesh_radius)
                {
                    if !self.is_in_stroke {
                        self.history_manager.begin_stroke();
                        self.is_in_stroke = true;
                    }

                    let uv = hit.uv;
                    let radius_3d = (self.brush.size / 200.0) * mesh_radius;
                    let active_idx = self.layer_stack.active_layer_index;

                    let bbox = if self.active_tool == ToolMode::Eyedropper {
                        let px = (uv[0] * (self.layer_stack.width - 1) as f32) as usize;
                        let py = (uv[1] * (self.layer_stack.height - 1) as f32) as usize;
                        let idx = py * self.layer_stack.width as usize + px;
                        if idx < self.layer_stack.composite_color.len() {
                            self.brush.color = self.layer_stack.composite_color[idx];
                            self.brush.metallic = self.layer_stack.composite_metallic[idx];
                            self.brush.roughness = self.layer_stack.composite_roughness[idx];
                            self.status_message = format!(
                                "Sampled: Color [{:.2}, {:.2}, {:.2}], Met: {:.2}, Rough: {:.2}",
                                self.brush.color[0],
                                self.brush.color[1],
                                self.brush.color[2],
                                self.brush.metallic,
                                self.brush.roughness
                            );
                        }
                        [0, 0, 0, 0]
                    } else if self.active_tool == ToolMode::PolygonFill {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                            match self.poly_fill_mode {
                                0 => {
                                    // Triangle Fill
                                    let i0 = self.mesh.indices[hit.tri_idx * 3] as usize;
                                    let i1 = self.mesh.indices[hit.tri_idx * 3 + 1] as usize;
                                    let i2 = self.mesh.indices[hit.tri_idx * 3 + 2] as usize;
                                    if i0 < self.mesh.uvs.len()
                                        && i1 < self.mesh.uvs.len()
                                        && i2 < self.mesh.uvs.len()
                                    {
                                        active_layer.fill_triangle(
                                            self.mesh.uvs[i0],
                                            self.mesh.uvs[i1],
                                            self.mesh.uvs[i2],
                                            &self.brush,
                                            Some(&mut self.history_manager),
                                        );
                                    }
                                }
                                1 => {
                                    // UV Island Fill
                                    let island = self.mesh.find_uv_island(hit.tri_idx);
                                    for t in island {
                                        let i0 = self.mesh.indices[t * 3] as usize;
                                        let i1 = self.mesh.indices[t * 3 + 1] as usize;
                                        let i2 = self.mesh.indices[t * 3 + 2] as usize;
                                        if i0 < self.mesh.uvs.len()
                                            && i1 < self.mesh.uvs.len()
                                            && i2 < self.mesh.uvs.len()
                                        {
                                            active_layer.fill_triangle(
                                                self.mesh.uvs[i0],
                                                self.mesh.uvs[i1],
                                                self.mesh.uvs[i2],
                                                &self.brush,
                                                Some(&mut self.history_manager),
                                            );
                                        }
                                    }
                                }
                                _ => {
                                    // Mesh / Chunk Fill
                                    active_layer
                                        .flood_fill(&self.brush, Some(&mut self.history_manager));
                                }
                            }
                        }
                        self.recompute_composite();
                        [
                            0,
                            0,
                            self.layer_stack.width as i32,
                            self.layer_stack.height as i32,
                        ]
                    } else if self.active_tool == ToolMode::Smudge {
                        // EngineActionSmudge parity: displacement along the stroke path in 3D
                        let delta = match self.last_painted_uv {
                            Some(last) => [uv[0] - last[0], uv[1] - last[1]],
                            None => [0.0, 0.0],
                        };
                        self.layer_stack.smudge_3d_at(
                            &self.mesh,
                            hit.pos,
                            hit.normal,
                            radius_3d,
                            delta,
                            &self.brush,
                            Some(&mut self.history_manager),
                        )
                    } else if self.active_tool == ToolMode::Clone {
                        if let Some(source) = self.clone_source_uv {
                            // Aligned offset locked at the first stamp of the stroke
                            if self.clone_offset_uv.is_none() {
                                self.clone_offset_uv = Some([uv[0] - source[0], uv[1] - source[1]]);
                            }
                            let off = self.clone_offset_uv.unwrap_or([0.0, 0.0]);
                            let src_uv = [uv[0] - off[0], uv[1] - off[1]];
                            self.layer_stack.clone_stamp_3d_at(
                                &self.mesh,
                                hit.pos,
                                hit.normal,
                                src_uv,
                                radius_3d,
                                &self.brush,
                                self.paint_target_mask,
                                Some(&mut self.history_manager),
                            )
                        } else {
                            self.status_message = "Hold Alt and Click (no drag) on the surface to set the clone source".into();
                            [0, 0, 0, 0]
                        }
                    } else if self.active_tool == ToolMode::Projection {
                        // Screen-space projection painting (uprojmat/udepthscale parity): the stamp
                        // alpha is evaluated by projecting every touched surface point back into
                        // screen space and intersecting with the brush disc + depth window.
                        let view_proj = inv_view_proj.inverse();
                        if let Some(pos) = response.interact_pointer_pos() {
                            let vb_left = view_bounds.left();
                            let vb_top = view_bounds.top();
                            let vb_w = view_bounds.width();
                            let vb_h = view_bounds.height();
                            let cur_x = pos.x;
                            let cur_y = pos.y;
                            let radius_px = (self.brush.size * 0.5).max(2.0);
                            let hardness = self.brush.hardness.clamp(0.0, 0.999);
                            let cam_pos_c = cam_pos;
                            let ray_dir_c = ray_dir;
                            let d_ref = (glam::Vec3::from(hit.pos) - cam_pos).dot(ray_dir);
                            let depth_win = (self.brush.projection_depth.max(0.001)) * mesh_radius;
                            let screen_alpha = move |p3: [f32; 3]| -> f32 {
                                // Depth window along the view ray
                                let depth = (glam::Vec3::from(p3) - cam_pos_c).dot(ray_dir_c);
                                if (depth - d_ref).abs() > depth_win {
                                    return 0.0;
                                }
                                let ndc = view_proj.project_point3(glam::Vec3::from(p3));
                                let sx = (ndc.x * 0.5 + 0.5) * vb_w;
                                let sy = (1.0 - (ndc.y * 0.5 + 0.5)) * vb_h;
                                let dx = sx - (cur_x - vb_left);
                                let dy = sy - (cur_y - vb_top);
                                let dist = (dx * dx + dy * dy).sqrt();
                                if dist > radius_px {
                                    return 0.0;
                                }
                                let nd = dist / radius_px;
                                if nd <= hardness {
                                    1.0
                                } else {
                                    (1.0 - (nd - hardness) / (1.0 - hardness).max(0.001))
                                        .clamp(0.0, 1.0)
                                }
                            };
                            let on_mask = self.paint_target_mask;
                            let mut b = self.layer_stack.paint_3d_projection_at(
                                &self.mesh,
                                &screen_alpha,
                                &self.brush,
                                false,
                                on_mask,
                                Some(&mut self.history_manager),
                            );
                            if self.symmetry_x {
                                let sym_alpha =
                                    |p3: [f32; 3]| -> f32 { screen_alpha([-p3[0], p3[1], p3[2]]) };
                                let b_sym = self.layer_stack.paint_3d_projection_at(
                                    &self.mesh,
                                    &sym_alpha,
                                    &self.brush,
                                    false,
                                    on_mask,
                                    Some(&mut self.history_manager),
                                );
                                b[0] = b[0].min(b_sym[0]);
                                b[1] = b[1].min(b_sym[1]);
                                b[2] = b[2].max(b_sym[2]);
                                b[3] = b[3].max(b_sym[3]);
                            }
                            b
                        } else {
                            [0, 0, 0, 0]
                        }
                    } else if self.active_tool == ToolMode::Decal {
                        // Text/Decal stamping: one projected stamp of the decal alpha/transform
                        let mut decal_brush = self.brush.clone();
                        decal_brush.custom_alpha = self.decal_alpha.clone();
                        decal_brush.angle = self.decal_angle;
                        decal_brush.size = (self.brush.size * self.decal_scale.max(0.05)).max(2.0);
                        let decal_radius_3d = (decal_brush.size / 200.0) * mesh_radius;
                        let mut b = if let Some(active_layer) =
                            self.layer_stack.layers.get_mut(active_idx)
                        {
                            active_layer.paint_3d_surface(
                                &self.mesh,
                                hit.pos,
                                hit.normal,
                                decal_radius_3d,
                                &decal_brush,
                                false,
                                self.paint_target_mask,
                                Some(&mut self.history_manager),
                            )
                        } else {
                            [0, 0, 0, 0]
                        };
                        if self.symmetry_x {
                            let sym_pos = [-hit.pos[0], hit.pos[1], hit.pos[2]];
                            let sym_norm = [-hit.normal[0], hit.normal[1], hit.normal[2]];
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx)
                            {
                                let b_sym = active_layer.paint_3d_surface(
                                    &self.mesh,
                                    sym_pos,
                                    sym_norm,
                                    decal_radius_3d,
                                    &decal_brush,
                                    false,
                                    self.paint_target_mask,
                                    Some(&mut self.history_manager),
                                );
                                if b_sym[2] >= b_sym[0] && b_sym[3] >= b_sym[1] {
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                            }
                        }
                        b
                    } else {
                        // Paint (1), Eraser (2), Projection (3)
                        let is_eraser = self.active_tool == ToolMode::Eraser;
                        let on_mask = self.paint_target_mask;
                        let mut b = if let Some(active_layer) =
                            self.layer_stack.layers.get_mut(active_idx)
                        {
                            active_layer.paint_3d_surface(
                                &self.mesh,
                                hit.pos,
                                hit.normal,
                                radius_3d,
                                &self.brush,
                                is_eraser,
                                on_mask,
                                Some(&mut self.history_manager),
                            )
                        } else {
                            [0, 0, 0, 0]
                        };
                        if self.symmetry_x {
                            let sym_pos = [-hit.pos[0], hit.pos[1], hit.pos[2]];
                            let sym_norm = [-hit.normal[0], hit.normal[1], hit.normal[2]];
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx)
                            {
                                let b_sym = active_layer.paint_3d_surface(
                                    &self.mesh,
                                    sym_pos,
                                    sym_norm,
                                    radius_3d,
                                    &self.brush,
                                    is_eraser,
                                    on_mask,
                                    Some(&mut self.history_manager),
                                );
                                if b_sym[2] >= b_sym[0] && b_sym[3] >= b_sym[1] {
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                            }
                        }
                        if self.symmetry_y {
                            let sym_pos = [hit.pos[0], -hit.pos[1], hit.pos[2]];
                            let sym_norm = [hit.normal[0], -hit.normal[1], hit.normal[2]];
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx)
                            {
                                let b_sym = active_layer.paint_3d_surface(
                                    &self.mesh,
                                    sym_pos,
                                    sym_norm,
                                    radius_3d,
                                    &self.brush,
                                    is_eraser,
                                    on_mask,
                                    Some(&mut self.history_manager),
                                );
                                if b_sym[2] >= b_sym[0] && b_sym[3] >= b_sym[1] {
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                            }
                        }
                        if self.symmetry_z {
                            let sym_pos = [hit.pos[0], hit.pos[1], -hit.pos[2]];
                            let sym_norm = [hit.normal[0], hit.normal[1], -hit.normal[2]];
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx)
                            {
                                let b_sym = active_layer.paint_3d_surface(
                                    &self.mesh,
                                    sym_pos,
                                    sym_norm,
                                    radius_3d,
                                    &self.brush,
                                    is_eraser,
                                    on_mask,
                                    Some(&mut self.history_manager),
                                );
                                if b_sym[2] >= b_sym[0] && b_sym[3] >= b_sym[1] {
                                    b[0] = b[0].min(b_sym[0]);
                                    b[1] = b[1].min(b_sym[1]);
                                    b[2] = b[2].max(b_sym[2]);
                                    b[3] = b[3].max(b_sym[3]);
                                }
                            }
                        }
                        b
                    };
                    self.last_painted_uv = Some(uv);
                    if bbox[2] > bbox[0] && bbox[3] > bbox[1] {
                        self.recompute_dirty(bbox);
                    }
                }
            }
        } else {
            self.last_painted_uv = None;
            if self.is_in_stroke {
                let tool_name = match self.active_tool {
                    ToolMode::Paint => "Paint 3D Surface",
                    ToolMode::Eraser => "Erase 3D Surface",
                    ToolMode::PolygonFill => "Polygon Fill",
                    ToolMode::Smudge => "Smudge 3D Surface",
                    ToolMode::Clone => "Clone Stamp 3D",
                    ToolMode::Projection => "Projection Paint 3D",
                    ToolMode::Decal => "Decal Stamp 3D",
                    _ => "Brush Tool",
                };
                self.history_manager.commit_stroke(tool_name);
                self.history
                    .push(format!("{:02}  {}", self.history.len() + 1, tool_name));
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    "INFO".into(),
                    format!("Committed stroke: {}", tool_name),
                ));
                self.is_in_stroke = false;
                self.clone_offset_uv = None;
                self.lazy_mouse_tracker.reset();
            }
        }

        // Brush cursor on 3D surface
        if let Some(pos) = ctx.pointer_hover_pos() {
            if view_bounds.contains(pos) {
                let ndc_x = (pos.x - view_bounds.left()) / view_bounds.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - view_bounds.top()) / view_bounds.height() * 2.0;
                let p_far = inv_view_proj.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
                let ray_dir = (p_far - cam_pos).normalize();
                if self
                    .mesh
                    .raycast_normalized(cam_pos, ray_dir, mesh_center, mesh_radius)
                    .is_some()
                {
                    painter.circle_stroke(
                        pos,
                        (self.brush.size * 0.5).max(3.0),
                        Stroke::new(1.2_f32, ACTIVE_BLUE),
                    );
                }
            }
        }

        // Viewport navigation indicator
        painter.text(pos2(center.x, view_bounds.bottom() - 18.0), Align2::CENTER_CENTER, "Alt + LMB / MMB: Orbit | Alt + RMB / Scroll: Zoom | Shift + RMB: Rotate Light | Click & Drag: Paint 3D", egui::FontId::proportional(11.5), TEXT_DIMMED);
    }

    fn ui_texture_set_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("TEXTURE SET LIST");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(egui::RichText::new("+ Add").strong().color(Color32::WHITE))
                    .on_hover_text("Add a new Texture Set (Material Group)")
                    .clicked()
                {
                    let new_id = self.texture_set_manager.sets.len() + 1;
                    let name = format!("TextureSet_{}", new_id);
                    let res = self.layer_stack.width;
                    let new_idx = self.texture_set_manager.add_set(name, res);
                    self.status_message = format!("Added new Texture Set: TextureSet_{}", new_id);
                    self.switch_texture_set(new_idx);
                }
            });
        });
        ui.separator();

        if self.texture_set_manager.is_udim_workflow {
            ui.horizontal(|ui| {
                ui.colored_label(ACTIVE_BLUE, "🗂 UDIM Multi-Tile Workflow Active");
            });
            ui.add_space(4.0);
        }

        // Texture Sets List
        let active_set_idx = self.texture_set_manager.active_index;
        let num_sets = self.texture_set_manager.sets.len();
        let mut switch_to: Option<usize> = None;
        let mut to_duplicate: Option<usize> = None;
        let mut to_delete: Option<usize> = None;

        for (idx, set) in self.texture_set_manager.sets.iter_mut().enumerate() {
            let is_active = active_set_idx == idx;
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 30.0), egui::Sense::click());

            if is_active {
                ui.painter().rect_filled(rect, 3.0_f32, ACTIVE_ROW_FILL);
                ui.painter()
                    .rect_stroke(rect, 3.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
            } else if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 3.0_f32, Color32::from_rgb(38, 38, 44));
            }

            // Eye visibility dot/icon
            let eye_rect =
                Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(14.0, 14.0));
            let eye_resp = ui.interact(
                eye_rect,
                ui.id().with(("ts_eye", idx)),
                egui::Sense::click(),
            );
            if eye_resp.clicked() {
                set.visible = !set.visible;
            }
            ui.painter().circle_filled(
                eye_rect.center(),
                4.5,
                if set.visible {
                    ACTIVE_BLUE
                } else {
                    Color32::from_rgb(70, 70, 75)
                },
            );

            // Name + UDIM badge
            let display_name = if let Some(udim) = set.udim_tile {
                format!("{} [{}]", set.name, udim)
            } else {
                set.name.clone()
            };
            ui.painter().text(
                pos2(rect.left() + 26.0, rect.center().y),
                Align2::LEFT_CENTER,
                &display_name,
                egui::FontId::proportional(12.5),
                Color32::WHITE,
            );

            // Resolution badge
            let res_text = format!("{}×{}", set.resolution, set.resolution);
            ui.painter().text(
                pos2(rect.right() - 8.0, rect.center().y),
                Align2::RIGHT_CENTER,
                &res_text,
                egui::FontId::proportional(10.0),
                if is_active { ACTIVE_BLUE } else { TEXT_DIMMED },
            );

            if resp.clicked() && !is_active {
                switch_to = Some(idx);
            }

            resp.context_menu(|ui| {
                if ui.button("Select This Texture Set").clicked() {
                    switch_to = Some(idx);
                    ui.close_menu();
                }
                if ui.button("Duplicate Texture Set").clicked() {
                    to_duplicate = Some(idx);
                    ui.close_menu();
                }
                if num_sets > 1 {
                    if ui.button("Delete Texture Set").clicked() {
                        to_delete = Some(idx);
                        ui.close_menu();
                    }
                }
            });
        }

        if let Some(target) = switch_to {
            self.switch_texture_set(target);
        }
        if let Some(dup_target) = to_duplicate {
            if let Some(new_idx) = self.texture_set_manager.duplicate_set(dup_target) {
                self.switch_texture_set(new_idx);
                self.status_message = "Duplicated texture set".into();
            }
        }
        if let Some(del_target) = to_delete {
            self.texture_set_manager.remove_set(del_target);
            self.status_message = "Removed texture set".into();
        }

        ui.add_space(10.0);
        ui.separator();

        // ── Preview Primitives ──────────────────────────────────────────
        ui.strong("PREVIEW PRIMITIVE MESHES");
        ui.add_space(2.0);
        let models = [
            ("Material Calibration Sphere", 0usize),
            ("Cube (Primitive)", 3),
            ("Plane (Primitive)", 4),
            ("Antique Lantern", 1),
            ("Retro Radio", 2),
        ];
        for (name, idx) in models {
            let active = self.selected_model_idx == idx;
            if ui.selectable_label(active, name).clicked() {
                self.load_model(idx);
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.strong("IMPORT");
        ui.add_space(2.0);
        let import_btn = egui::Button::new(
            egui::RichText::new("📂 Import Mesh (OBJ/FBX/glTF)...").color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(40, 55, 75))
        .min_size(vec2(ui.available_width(), 26.0));
        if ui
            .add(import_btn)
            .on_hover_text("Open a 3D mesh file with materials or UDIMs")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Import Mesh — REMoAINTER")
                .add_filter(
                    "3D Models (*.obj, *.gltf, *.glb)",
                    &["obj", "OBJ", "gltf", "glb"],
                )
                .add_filter("All Files", &["*"])
                .pick_file()
            {
                self.import_mesh_file(&path);
            }
        }
    }

    fn ui_texture_set_settings(&mut self, ui: &mut Ui) {
        ui.heading("TEXTURE SET SETTINGS");
        ui.separator();

        let active_idx = self.texture_set_manager.active_index;
        if active_idx >= self.texture_set_manager.sets.len() {
            ui.label("No active texture set.");
            return;
        }

        // Texture Set Name & Resolution
        let mut cur_res = self.texture_set_manager.sets[active_idx].resolution;
        let old_res = cur_res;
        let mut set_name = self.texture_set_manager.sets[active_idx].name.clone();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Name:").strong());
            if ui.text_edit_singleline(&mut set_name).changed() {
                self.texture_set_manager.sets[active_idx].name = set_name;
            }
        });

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Size:").strong());
            egui::ComboBox::from_id_salt("ts_size_combo")
                .selected_text(format!("{}×{}", cur_res, cur_res))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut cur_res, 512, "512×512");
                    ui.selectable_value(&mut cur_res, 1024, "1024×1024");
                    ui.selectable_value(&mut cur_res, 2048, "2048×2048");
                    ui.selectable_value(&mut cur_res, 4096, "4096×4096");
                });
        });

        if cur_res != old_res {
            self.texture_set_manager.sets[active_idx].resize(cur_res);
            self.layer_stack.resize(cur_res, cur_res);
            self.texture_needs_update = true;
            self.logs.push((
                chrono::Local::now().format("%H:%M:%S").to_string(),
                "INFO".into(),
                format!("Resized Texture Set to {}×{}", cur_res, cur_res),
            ));
        }

        ui.add_space(6.0);
        ui.separator();

        // ── CHANNELS Section ──────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.strong("CHANNELS");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // "+" add channel button
                ui.menu_button(
                    egui::RichText::new("+").strong().color(Color32::WHITE),
                    |ui| {
                        ui.set_min_width(160.0);
                        ui.label(egui::RichText::new("Add Channel").strong());
                        ui.separator();
                        let all_possible = [
                            ("Base color", "sRGB8"),
                            ("Metallic", "L8"),
                            ("Roughness", "L8"),
                            ("Normal", "RGB16F"),
                            ("Height", "L16F"),
                            ("Emissive", "sRGB8"),
                            ("Opacity", "L8"),
                            ("Ambient Occlusion", "L8"),
                            ("Specular", "L8"),
                            ("Glossiness", "L8"),
                            ("Displacement", "L16F"),
                            ("User0", "L8"),
                            ("User1", "L8"),
                        ];
                        for (ch_name, fmt) in all_possible {
                            let already = self
                                .texture_channels
                                .iter()
                                .any(|c: &TextureChannel| c.name == ch_name);
                            let label = if already {
                                egui::RichText::new(format!("{} (active)", ch_name))
                                    .color(TEXT_DIMMED)
                            } else {
                                egui::RichText::new(ch_name).color(Color32::WHITE)
                            };
                            if !already && ui.button(label).clicked() {
                                self.texture_channels.push(TextureChannel {
                                    name: ch_name.to_string(),
                                    format: fmt.to_string(),
                                    enabled: true,
                                });
                                ui.close_menu();
                            }
                        }
                    },
                );
            });
        });

        // Channel rows — each with format label and × delete
        let mut to_remove: Option<usize> = None;
        let required = ["Base color", "Metallic", "Roughness", "Normal"];
        for (ci, ch) in self.texture_channels.iter_mut().enumerate() {
            let is_required = required.contains(&ch.name.as_str());
            ui.horizontal(|ui| {
                // Colored channel dot
                let dot_color = match ch.name.as_str() {
                    "Base color" => Color32::from_rgb(228, 110, 60),
                    "Metallic" => Color32::from_rgb(150, 200, 255),
                    "Roughness" => Color32::from_rgb(80, 200, 130),
                    "Normal" => Color32::from_rgb(140, 120, 255),
                    "Height" => Color32::from_rgb(255, 210, 80),
                    "Emissive" => Color32::from_rgb(255, 100, 50),
                    "Opacity" => Color32::from_rgb(200, 200, 200),
                    _ => TEXT_DIMMED,
                };
                let (dot_rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot_rect.center(), 4.5, dot_color);
                ui.label(egui::RichText::new(&ch.name).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !is_required {
                        if ui
                            .small_button(
                                egui::RichText::new("×").color(Color32::from_rgb(255, 80, 80)),
                            )
                            .clicked()
                        {
                            to_remove = Some(ci);
                        }
                    }
                    ui.label(
                        egui::RichText::new(&ch.format)
                            .color(TEXT_DIMMED)
                            .size(10.5),
                    );
                });
            });
        }
        if let Some(ci) = to_remove {
            self.texture_channels.remove(ci);
        }

        ui.add_space(8.0);
        ui.separator();

        // ── NORMAL MAP SETTINGS ──────────────────────────────────────────
        ui.collapsing("NORMAL MAP SETTINGS", |ui| {
            ui.horizontal(|ui| {
                ui.label("Normal mixing:");
                egui::ComboBox::from_id_salt("normal_mix_mode")
                    .selected_text("Combine")
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, "Combine");
                        let _ = ui.selectable_label(false, "Replace");
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Height to Normal:");
                egui::ComboBox::from_id_salt("height_to_normal_method")
                    .selected_text("Smooth Sobel")
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, "Smooth Sobel");
                        let _ = ui.selectable_label(false, "Sharp Sobel");
                        let _ = ui.selectable_label(false, "Prewitt");
                    });
            });
            ui.checkbox(
                &mut self.baker_options.invert_normal_y,
                "Invert Green Channel (DirectX)",
            );
        });

        // ── UV UNWRAP SETTINGS ───────────────────────────────────────────
        ui.collapsing("UV UNWRAP SETTINGS", |ui| {
            ui.horizontal(|ui| {
                ui.label("UV padding:");
                egui::ComboBox::from_id_salt("uv_padding")
                    .selected_text("16px")
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(false, "2px");
                        let _ = ui.selectable_label(false, "4px");
                        let _ = ui.selectable_label(false, "8px");
                        let _ = ui.selectable_label(true, "16px");
                        let _ = ui.selectable_label(false, "32px");
                    });
            });
        });

        // ── MESH MAPS BAKING ─────────────────────────────────────────────
        ui.add_space(4.0);
        ui.separator();
        ui.strong("MESH MAPS");
        ui.add_space(4.0);

        // Show baked map status badges
        let map_statuses = [
            ("Normal", self.baked_maps.normal.is_some()),
            (
                "World Space Nrm",
                self.baked_maps.world_space_normal.is_some(),
            ),
            ("AO", self.baked_maps.ambient_occlusion.is_some()),
            ("Curvature", self.baked_maps.curvature.is_some()),
            ("Position", self.baked_maps.position.is_some()),
            ("Thickness", self.baked_maps.thickness.is_some()),
            ("ID Map", self.baked_maps.id.is_some()),
        ];
        egui::Grid::new("mesh_map_grid")
            .num_columns(2)
            .spacing([4.0, 2.0])
            .show(ui, |ui| {
                for (name, baked) in map_statuses {
                    let (color, text) = if baked {
                        (Color32::from_rgb(100, 240, 150), "BAKED")
                    } else {
                        (TEXT_DIMMED, "—")
                    };
                    ui.label(name);
                    ui.colored_label(color, text);
                    ui.end_row();
                }
            });
        ui.add_space(4.0);
        let bake_btn = egui::Button::new(
            egui::RichText::new("Bake Mesh Maps...")
                .color(Color32::WHITE)
                .strong(),
        )
        .fill(Color32::from_rgb(40, 90, 160))
        .min_size(vec2(ui.available_width(), 24.0));
        if ui.add(bake_btn).clicked() {
            self.show_baker_dialog = true;
        }
    }

    fn ui_layers_panel(&mut self, ui: &mut Ui) {
        ui.heading("LAYERS");

        // Action Toolbar
        ui.horizontal(|ui| {
            if ui.small_button("+ Paint").clicked() {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_paint(
                    id,
                    format!("Layer {}", id),
                    self.layer_stack.width,
                    self.layer_stack.height,
                ));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.paint_target_mask = false;
                self.recompute_composite();
                self.status_message = "Added Paint Layer".into();
            }
            if ui.small_button("+ Fill").clicked() {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_fill(
                    id,
                    format!("Fill {}", id),
                    self.brush.color,
                    self.brush.metallic,
                    self.brush.roughness,
                    self.layer_stack.width,
                    self.layer_stack.height,
                ));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.paint_target_mask = false;
                self.recompute_composite();
                self.status_message = "Added Fill Layer".into();
            }
            if ui
                .small_button("+ Folder")
                .on_hover_text("Add Folder Group (Ctrl+G)")
                .clicked()
            {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_folder(
                    id,
                    format!("Folder {}", id),
                    self.layer_stack.width,
                    self.layer_stack.height,
                ));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.paint_target_mask = false;
                self.recompute_composite();
                self.status_message = "Added Folder".into();
            }
            if ui
                .small_button("+ Black Mask")
                .on_hover_text("Add Black Mask with Sub-Stack")
                .clicked()
            {
                if let Some(active) = self
                    .layer_stack
                    .layers
                    .get_mut(self.layer_stack.active_layer_index)
                {
                    active.add_black_mask();
                    self.status_message = "Added Black Mask".into();
                }
                self.recompute_composite();
            }
            if ui
                .small_button("+ White Mask")
                .on_hover_text("Add White Mask")
                .clicked()
            {
                if let Some(active) = self
                    .layer_stack
                    .layers
                    .get_mut(self.layer_stack.active_layer_index)
                {
                    active.add_white_mask();
                    self.status_message = "Added White Mask".into();
                }
                self.recompute_composite();
            }
            ui.menu_button("+ Filter", |ui| {
                let active_idx = self.layer_stack.active_layer_index;
                if ui.button("Levels").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Levels {
                            min_in: 0.0,
                            max_in: 1.0,
                            gamma: 1.0,
                            min_out: 0.0,
                            max_out: 1.0,
                            channel: FilterTargetChannel::All,
                        });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("Gaussian Blur").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Blur {
                            radius: 3.0,
                            channel: FilterTargetChannel::All,
                        });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("Sharpen").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Sharpen {
                            strength: 1.5,
                            channel: FilterTargetChannel::All,
                        });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("HSL Adjustment").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::HSL {
                            hue_shift: 0.0,
                            sat_mult: 1.0,
                            light_offset: 0.0,
                        });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("Invert").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Invert {
                            channel: FilterTargetChannel::All,
                        });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
            });
            if ui
                .small_button("+ Anchor")
                .on_hover_text("Add Anchor Point")
                .clicked()
            {
                if let Some(active) = self
                    .layer_stack
                    .layers
                    .get_mut(self.layer_stack.active_layer_index)
                {
                    active.anchor_point = Some(AnchorPoint::new(
                        1,
                        format!("Anchor_{}", active.name),
                        active.id,
                        crate::painting::AnchorChannel::All,
                    ));
                    self.status_message = format!("Created Anchor Point for {}", active.name);
                }
            }
            if ui
                .small_button("📁 Import...")
                .on_hover_text("Import PBR Texture Set (Folder or Files with Auto-Matching)")
                .clicked()
            {
                self.show_texture_importer_dialog = true;
            }
        });

        ui.separator();

        enum LayerListAction {
            Select(usize, bool), // (idx, is_mask)
            ToggleVisibility(usize),
            ToggleFolder(usize),
            AddBlackMask(usize),
            AddWhiteMask(usize),
            AddBitmapMask(usize, PathBuf),
            InvertMask(usize),
            ClearMask(usize),
            RemoveMask(usize),
            AddGenerator(usize, String),
            AddFillEffect(usize),
            AddLevels(usize),
            AddFilter(usize, FilterEffect),
            AddAnchor(usize),
            Duplicate(usize),
            ClearLayer(usize),
            Delete(usize),
            AddPaintLayer,
            AddFillLayer,
            AddFolder,
            Cut(usize),
            Copy(usize),
            Paste,
            SetColorTag(usize, Option<u8>),
            SetBlendMode(usize, BlendMode),
        }

        let mut list_action: Option<LayerListAction> = None;

        // Layer Stack Items (Top to Bottom)
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in (0..self.layer_stack.layers.len()).rev() {
                let layer_id = self.layer_stack.layers[i].id;
                let is_folder = self.layer_stack.layers[i].layer_type == LayerType::Folder;
                let parent_id = self.layer_stack.layers[i].parent_folder_id;

                // Hide child layers if parent folder is collapsed
                let is_hidden = parent_id.map_or(false, |pid| {
                    self.layer_stack
                        .layers
                        .iter()
                        .find(|l| l.id == pid)
                        .map_or(false, |p| !p.folder_expanded)
                });
                if is_hidden {
                    continue;
                }

                ui.push_id(format!("layer_row_id_{}_{}", layer_id, i), |ui| {
                    let is_active = self.layer_stack.active_layer_index == i;
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 28.0),
                        egui::Sense::click_and_drag(),
                    );

                    if is_active {
                        ui.painter().rect_filled(rect, 2.0_f32, ACTIVE_ROW_FILL);
                        ui.painter()
                            .rect_stroke(rect, 2.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
                    }

                    let layer = &mut self.layer_stack.layers[i];
                    let has_mask = layer.mask.is_some();
                    let layer_name = layer.name.clone();
                    let layer_type = layer.layer_type;
                    let num_effects = layer.effects.len();
                    let folder_expanded = layer.folder_expanded;
                    let has_anchor = layer.anchor_point.is_some();
                    let layer_opacity = layer.opacity;

                    let indent = if parent_id.is_some() { 16.0 } else { 0.0 };

                    // Color tag indicator (reference PBR suite color tags)
                    if let Some(tag) = layer.color_tag {
                        let tag_col = match tag {
                            1 => Color32::from_rgb(230, 60, 60),  // Red
                            2 => Color32::from_rgb(240, 140, 40), // Orange
                            3 => Color32::from_rgb(240, 210, 40), // Yellow
                            4 => Color32::from_rgb(60, 200, 100), // Green
                            5 => Color32::from_rgb(40, 200, 220), // Cyan
                            6 => Color32::from_rgb(60, 120, 240), // Blue
                            7 => Color32::from_rgb(180, 80, 220), // Purple
                            _ => Color32::TRANSPARENT,
                        };
                        ui.painter().rect_filled(
                            Rect::from_min_size(rect.min, vec2(3.5, rect.height())),
                            1.0,
                            tag_col,
                        );
                    }

                    // Eye toggle
                    let eye_icon = if layer.visible { "[O]" } else { "[-]" };
                    let eye_rect = Rect::from_min_size(
                        pos2(rect.left() + indent + 6.0, rect.top() + 6.0),
                        vec2(16.0, 16.0),
                    );
                    ui.painter().text(
                        eye_rect.center(),
                        Align2::CENTER_CENTER,
                        eye_icon,
                        egui::FontId::monospace(11.0),
                        TEXT_PRIMARY,
                    );

                    let mut cur_x = rect.left() + indent + 24.0;

                    // If folder, render expand/collapse arrow
                    if is_folder {
                        let arrow = if folder_expanded { "▼" } else { "▶" };
                        let arrow_rect =
                            Rect::from_min_size(pos2(cur_x, rect.top() + 6.0), vec2(14.0, 16.0));
                        ui.painter().text(
                            arrow_rect.center(),
                            Align2::CENTER_CENTER,
                            arrow,
                            egui::FontId::proportional(11.0),
                            Color32::from_rgb(255, 215, 0),
                        );
                        cur_x += 16.0;
                    }

                    // Thumbnail 1: Content/Material
                    let mat_thumb_rect =
                        Rect::from_min_size(pos2(cur_x, rect.top() + 5.0), vec2(18.0, 18.0));
                    let (badge_char, badge_color) = match layer_type {
                        LayerType::Paint => ("P", Color32::from_rgb(100, 200, 255)),
                        LayerType::Fill => ("F", SMART_MAT_ORANGE),
                        LayerType::Folder => ("D", Color32::from_rgb(255, 215, 0)),
                    };
                    ui.painter()
                        .rect_filled(mat_thumb_rect, 2.0, Color32::from_rgb(35, 35, 40));
                    ui.painter().text(
                        mat_thumb_rect.center(),
                        Align2::CENTER_CENTER,
                        badge_char,
                        egui::FontId::monospace(10.0),
                        badge_color,
                    );
                    if is_active && !self.paint_target_mask {
                        ui.painter().rect_stroke(
                            mat_thumb_rect,
                            2.0,
                            Stroke::new(1.5_f32, ACTIVE_BLUE),
                        );
                    } else {
                        ui.painter().rect_stroke(
                            mat_thumb_rect,
                            2.0,
                            Stroke::new(0.8_f32, Color32::from_rgb(80, 80, 85)),
                        );
                    }
                    cur_x += 22.0;

                    // Thumbnail 2: Mask (if present)
                    let mask_thumb_rect = if has_mask {
                        let mtr =
                            Rect::from_min_size(pos2(cur_x, rect.top() + 5.0), vec2(18.0, 18.0));
                        ui.painter().rect_filled(mtr, 2.0, Color32::BLACK);
                        ui.painter().text(
                            mtr.center(),
                            Align2::CENTER_CENTER,
                            "M",
                            egui::FontId::monospace(9.0),
                            Color32::WHITE,
                        );
                        if is_active && self.paint_target_mask {
                            ui.painter().rect_stroke(
                                mtr,
                                2.0,
                                Stroke::new(1.8_f32, Color32::from_rgb(100, 230, 255)),
                            );
                        } else {
                            ui.painter().rect_stroke(
                                mtr,
                                2.0,
                                Stroke::new(0.8_f32, Color32::from_rgb(140, 140, 150)),
                            );
                        }
                        cur_x += 22.0;
                        Some(mtr)
                    } else {
                        None
                    };

                    // Layer name text
                    ui.painter().text(
                        pos2(cur_x, rect.center().y),
                        Align2::LEFT_CENTER,
                        &layer_name,
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );

                    // Right side badges: Opacity, Anchor, Effects
                    let mut right_x = rect.right() - 8.0;
                    ui.painter().text(
                        pos2(right_x, rect.center().y),
                        Align2::RIGHT_CENTER,
                        format!("{:.0}%", layer_opacity * 100.0),
                        egui::FontId::proportional(11.0),
                        TEXT_DIMMED,
                    );
                    right_x -= 38.0;

                    if has_anchor {
                        let a_rect = Rect::from_min_size(
                            pos2(right_x - 14.0, rect.top() + 6.0),
                            vec2(14.0, 14.0),
                        );
                        ui.painter().rect_filled(a_rect, 1.0, SMART_MAT_ORANGE);
                        ui.painter().text(
                            a_rect.center(),
                            Align2::CENTER_CENTER,
                            "A",
                            egui::FontId::proportional(9.0),
                            Color32::WHITE,
                        );
                        right_x -= 18.0;
                    }

                    if num_effects > 0 {
                        let fx_rect = Rect::from_min_size(
                            pos2(right_x - 22.0, rect.top() + 6.0),
                            vec2(20.0, 14.0),
                        );
                        ui.painter()
                            .rect_filled(fx_rect, 2.0, Color32::from_rgb(130, 70, 200));
                        ui.painter().text(
                            fx_rect.center(),
                            Align2::CENTER_CENTER,
                            format!("+{}", num_effects),
                            egui::FontId::proportional(9.0),
                            Color32::WHITE,
                        );
                    }

                    // Right-Click Context Menu (reference PBR suite 1:1 matching)
                    resp.context_menu(|ui| {
                        ui.set_min_width(220.0);
                        ui.label(
                            egui::RichText::new(&layer_name)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.separator();

                        // ── 1. Mask Operations ──────────────────────────────
                        if has_mask {
                            if ui.button("Toggle mask").clicked() {
                                list_action = Some(LayerListAction::InvertMask(i));
                                ui.close_menu();
                            }
                            if ui.button("Invert mask").clicked() {
                                list_action = Some(LayerListAction::InvertMask(i));
                                ui.close_menu();
                            }
                            if ui.button("Clear mask").clicked() {
                                list_action = Some(LayerListAction::ClearMask(i));
                                ui.close_menu();
                            }
                            if ui.button("Remove mask").clicked() {
                                list_action = Some(LayerListAction::RemoveMask(i));
                                ui.close_menu();
                            }
                        } else {
                            if ui.button("Add white mask").clicked() {
                                list_action = Some(LayerListAction::AddWhiteMask(i));
                                ui.close_menu();
                            }
                            if ui.button("Add black mask").clicked() {
                                list_action = Some(LayerListAction::AddBlackMask(i));
                                ui.close_menu();
                            }
                            if ui.button("Add bitmap mask...").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_title("Open Bitmap Mask")
                                    .add_filter(
                                        "Image",
                                        &["png", "jpg", "jpeg", "webp", "bmp", "tga"],
                                    )
                                    .pick_file()
                                {
                                    list_action = Some(LayerListAction::AddBitmapMask(i, p));
                                }
                                ui.close_menu();
                            }
                            if ui.button("Add mask with color selection").clicked() {
                                list_action =
                                    Some(LayerListAction::AddGenerator(i, "Cavity Dirt".into()));
                                ui.close_menu();
                            }
                            if ui.button("Add mask with height combination").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "Metal Edge Wear".into(),
                                ));
                                ui.close_menu();
                            }
                        }
                        ui.separator();

                        // ── 2. Layer Cut/Copy/Paste/Duplicate/Remove ───────
                        if ui.button("Cut layer(s)\tCtrl+X").clicked() {
                            list_action = Some(LayerListAction::Cut(i));
                            ui.close_menu();
                        }
                        if ui.button("Copy layer(s)\tCtrl+C").clicked() {
                            list_action = Some(LayerListAction::Copy(i));
                            ui.close_menu();
                        }
                        if self.layer_clipboard.is_some() {
                            if ui.button("Paste layer(s)\tCtrl+V").clicked() {
                                list_action = Some(LayerListAction::Paste);
                                ui.close_menu();
                            }
                        }
                        if ui.button("Duplicate layer(s)\tCtrl+D").clicked() {
                            list_action = Some(LayerListAction::Duplicate(i));
                            ui.close_menu();
                        }
                        if ui.button("Group layer(s)\tCtrl+G").clicked() {
                            list_action = Some(LayerListAction::AddFolder);
                            ui.close_menu();
                        }
                        if ui
                            .button(
                                egui::RichText::new("Remove layer(s)\tDel")
                                    .color(Color32::from_rgb(255, 90, 90)),
                            )
                            .clicked()
                        {
                            list_action = Some(LayerListAction::Delete(i));
                            ui.close_menu();
                        }
                        ui.separator();

                        // ── 3. Color Labels Palette (8 swatches) ───────────
                        ui.horizontal(|ui| {
                            ui.label("Color Tag:");
                            let swatches: [(Option<u8>, Color32); 8] = [
                                (None, Color32::from_rgb(50, 50, 55)),
                                (Some(1), Color32::from_rgb(230, 60, 60)),
                                (Some(2), Color32::from_rgb(240, 140, 40)),
                                (Some(3), Color32::from_rgb(240, 210, 40)),
                                (Some(4), Color32::from_rgb(60, 200, 100)),
                                (Some(5), Color32::from_rgb(40, 200, 220)),
                                (Some(6), Color32::from_rgb(60, 120, 240)),
                                (Some(7), Color32::from_rgb(180, 80, 220)),
                            ];
                            for (tag, col) in swatches {
                                let (s_rect, s_resp) =
                                    ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::click());
                                ui.painter().rect_filled(s_rect, 2.0, col);
                                if s_resp.clicked() {
                                    list_action = Some(LayerListAction::SetColorTag(i, tag));
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();

                        // ── 4. Blending Options (All 29 reference PBR suite Blend Modes) ─
                        ui.menu_button("Blending options", |ui| {
                            let modes = [
                                BlendMode::Normal,
                                BlendMode::PassThrough,
                                BlendMode::Multiply,
                                BlendMode::Screen,
                                BlendMode::Overlay,
                                BlendMode::Add,
                                BlendMode::Subtract,
                                BlendMode::Darken,
                                BlendMode::Lighten,
                                BlendMode::ColorDodge,
                                BlendMode::ColorBurn,
                                BlendMode::LinearBurn,
                                BlendMode::HardLight,
                                BlendMode::SoftLight,
                                BlendMode::Difference,
                                BlendMode::Exclusion,
                                BlendMode::Color,
                                BlendMode::Luminosity,
                                BlendMode::Hue,
                                BlendMode::Saturation,
                                BlendMode::VividLight,
                                BlendMode::LinearLight,
                                BlendMode::PinLight,
                                BlendMode::HardMix,
                                BlendMode::DarkerColor,
                                BlendMode::LighterColor,
                                BlendMode::Divide,
                                BlendMode::RNM,
                                BlendMode::Whiteout,
                            ];
                            for mode in modes {
                                if ui.button(mode.name()).clicked() {
                                    list_action = Some(LayerListAction::SetBlendMode(i, mode));
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();

                        // ── 5. Effects Submenu (reference PBR suite style) ───
                        ui.menu_button("⚡ Add generator", |ui| {
                            if ui.button("Metal Edge Wear").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "Metal Edge Wear".into(),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Dirt Crevices").clicked() {
                                list_action =
                                    Some(LayerListAction::AddGenerator(i, "Dirt Crevices".into()));
                                ui.close_menu();
                            }
                            if ui.button("Dust Accumulation").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "Dust Accumulation".into(),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Position Gradient").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "Position Gradient".into(),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("World Normal Orientation").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "World Normal Orientation".into(),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Thickness / Subsurface").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "Thickness / Subsurface".into(),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Color / ID Selection").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(
                                    i,
                                    "Color / ID Selection".into(),
                                ));
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("🔮 Add filter", |ui| {
                            if ui.button("Levels").clicked() {
                                list_action = Some(LayerListAction::AddFilter(
                                    i,
                                    FilterEffect::Levels {
                                        min_in: 0.0,
                                        max_in: 1.0,
                                        gamma: 1.0,
                                        min_out: 0.0,
                                        max_out: 1.0,
                                        channel: FilterTargetChannel::All,
                                    },
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Gaussian Blur").clicked() {
                                list_action = Some(LayerListAction::AddFilter(
                                    i,
                                    FilterEffect::Blur {
                                        radius: 3.0,
                                        channel: FilterTargetChannel::All,
                                    },
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Sharpen").clicked() {
                                list_action = Some(LayerListAction::AddFilter(
                                    i,
                                    FilterEffect::Sharpen {
                                        strength: 1.5,
                                        channel: FilterTargetChannel::All,
                                    },
                                ));
                                ui.close_menu();
                            }
                            if ui.button("HSL Adjustment").clicked() {
                                list_action = Some(LayerListAction::AddFilter(
                                    i,
                                    FilterEffect::HSL {
                                        hue_shift: 0.0,
                                        sat_mult: 1.0,
                                        light_offset: 0.0,
                                    },
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Invert").clicked() {
                                list_action = Some(LayerListAction::AddFilter(
                                    i,
                                    FilterEffect::Invert {
                                        channel: FilterTargetChannel::All,
                                    },
                                ));
                                ui.close_menu();
                            }
                        });
                        if ui.button("📊 Add levels").clicked() {
                            list_action = Some(LayerListAction::AddLevels(i));
                            ui.close_menu();
                        }
                        if ui.button("⚓ Add anchor point").clicked() {
                            list_action = Some(LayerListAction::AddAnchor(i));
                            ui.close_menu();
                        }
                        if ui.button("➕ Add paint").clicked() {
                            list_action = Some(LayerListAction::AddPaintLayer);
                            ui.close_menu();
                        }
                        if ui.button("🎨 Add fill").clicked() {
                            list_action = Some(LayerListAction::AddFillLayer);
                            ui.close_menu();
                        }
                        if layer_type == LayerType::Paint {
                            if ui.button("🧹 Clear layer").clicked() {
                                list_action = Some(LayerListAction::ClearLayer(i));
                                ui.close_menu();
                            }
                        }
                    });

                    if resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            if pos.x <= rect.left() + indent + 20.0 {
                                list_action = Some(LayerListAction::ToggleVisibility(i));
                            } else if is_folder && pos.x <= rect.left() + indent + 40.0 {
                                list_action = Some(LayerListAction::ToggleFolder(i));
                            } else if let Some(mtr) = mask_thumb_rect {
                                if pos.x >= mtr.left() && pos.x <= mtr.right() {
                                    list_action = Some(LayerListAction::Select(i, true));
                                } else {
                                    list_action = Some(LayerListAction::Select(i, false));
                                }
                            } else {
                                list_action = Some(LayerListAction::Select(i, false));
                            }
                        } else {
                            list_action = Some(LayerListAction::Select(i, false));
                        }
                    }

                    // If mask exists, render sub-stack effects
                    if is_active {
                        if let Some(ref mut mask) = layer.mask {
                            ui.indent("mask_indent", |ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(ACTIVE_BLUE, "↳ Mask Sub-Stack:");
                                    ui.menu_button("+ Generator", |ui| {
                                        let gens = [
                                            "Metal Edge Wear",
                                            "Dirt Crevices",
                                            "Dust Accumulation",
                                            "Position Gradient",
                                            "World Normal Orientation",
                                            "Thickness / Subsurface",
                                            "Color / ID Selection",
                                        ];
                                        for g in gens {
                                            if ui.button(g).clicked() {
                                                list_action = Some(LayerListAction::AddGenerator(
                                                    i,
                                                    g.into(),
                                                ));
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                    if ui.small_button("+ Fill Effect").clicked() {
                                        list_action = Some(LayerListAction::AddFillEffect(i));
                                    }
                                    if ui.small_button("+ Levels").clicked() {
                                        list_action = Some(LayerListAction::AddLevels(i));
                                    }
                                });

                                let mut eff_rem = None;
                                for (eff_i, eff) in mask.effects.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        match eff {
                                            MaskEffect::Generator {
                                                name,
                                                wear_level,
                                                contrast,
                                                grunge_amount,
                                                ..
                                            } => {
                                                ui.label(format!("  • ⚡ {}:", name));
                                                ui.add(
                                                    egui::Slider::new(wear_level, 0.0..=2.0)
                                                        .text("Level")
                                                        .show_value(false),
                                                );
                                                ui.add(
                                                    egui::Slider::new(contrast, 0.1..=4.0)
                                                        .text("Contrast")
                                                        .show_value(false),
                                                );
                                                ui.add(
                                                    egui::Slider::new(grunge_amount, 0.0..=1.0)
                                                        .text("Grunge")
                                                        .show_value(false),
                                                );
                                            }
                                            MaskEffect::Fill {
                                                value,
                                                noise_frequency,
                                                ..
                                            } => {
                                                ui.label("  • 🎨 Fill:");
                                                ui.add(
                                                    egui::Slider::new(value, 0.0..=1.0)
                                                        .text("Val")
                                                        .show_value(false),
                                                );
                                                ui.add(
                                                    egui::Slider::new(noise_frequency, 0.0..=10.0)
                                                        .text("Noise")
                                                        .show_value(false),
                                                );
                                            }
                                            MaskEffect::Levels {
                                                black_in,
                                                white_in,
                                                gamma,
                                            } => {
                                                ui.label("  • 📊 Levels:");
                                                ui.add(
                                                    egui::Slider::new(black_in, 0.0..=1.0)
                                                        .text("In")
                                                        .show_value(false),
                                                );
                                                ui.add(
                                                    egui::Slider::new(white_in, 0.0..=1.0)
                                                        .text("Out")
                                                        .show_value(false),
                                                );
                                                ui.add(
                                                    egui::Slider::new(gamma, 0.1..=3.0)
                                                        .text("γ")
                                                        .show_value(false),
                                                );
                                            }
                                            MaskEffect::Invert => {
                                                ui.label("  • 🔄 Invert");
                                            }
                                            MaskEffect::Paint { .. } => {
                                                ui.label("  • 🖌 Paint");
                                            }
                                        }
                                        if ui.small_button("✕").clicked() {
                                            eff_rem = Some(eff_i);
                                        }
                                    });
                                }
                                if let Some(r_idx) = eff_rem {
                                    mask.effects.remove(r_idx);
                                }
                            });
                        }
                    }
                });
            }
        });

        let mut recomposite_needed = false;

        if let Some(action) = list_action {
            match action {
                LayerListAction::Select(i, is_mask) => {
                    self.layer_stack.active_layer_index = i;
                    self.paint_target_mask = is_mask;
                    if is_mask {
                        self.status_message = format!(
                            "Editing Mask for {}",
                            self.layer_stack
                                .layers
                                .get(i)
                                .map(|l| l.name.as_str())
                                .unwrap_or("")
                        );
                    } else {
                        self.status_message = format!(
                            "Selected Layer: {}",
                            self.layer_stack
                                .layers
                                .get(i)
                                .map(|l| l.name.as_str())
                                .unwrap_or("")
                        );
                    }
                }
                LayerListAction::ToggleVisibility(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.visible = !l.visible;
                        recomposite_needed = true;
                    }
                }
                LayerListAction::ToggleFolder(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.folder_expanded = !l.folder_expanded;
                    }
                }
                LayerListAction::AddBlackMask(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.add_black_mask();
                        recomposite_needed = true;
                        self.status_message = format!("Added Black Mask to {}", l.name);
                    }
                }
                LayerListAction::AddWhiteMask(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.add_white_mask();
                        recomposite_needed = true;
                        self.status_message = format!("Added White Mask to {}", l.name);
                    }
                }
                LayerListAction::AddBitmapMask(i, path) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.add_bitmap_mask(&path);
                        recomposite_needed = true;
                        self.status_message =
                            format!("Added Bitmap Mask ({}) to {}", path.display(), l.name);
                    }
                }
                LayerListAction::AddPaintLayer => {
                    let id = self.layer_stack.layers.len() + 1;
                    self.layer_stack.layers.push(Layer::new_paint(
                        id,
                        format!("Paint Layer {}", id),
                        self.layer_stack.width,
                        self.layer_stack.height,
                    ));
                    self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                    self.paint_target_mask = false;
                    recomposite_needed = true;
                    self.status_message = format!("Added Paint Layer {}", id);
                }
                LayerListAction::AddFillLayer => {
                    let id = self.layer_stack.layers.len() + 1;
                    self.layer_stack.layers.push(Layer::new_fill(
                        id,
                        format!("Fill Layer {}", id),
                        self.brush.color,
                        self.brush.metallic,
                        self.brush.roughness,
                        self.layer_stack.width,
                        self.layer_stack.height,
                    ));
                    self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                    self.paint_target_mask = false;
                    recomposite_needed = true;
                    self.status_message = format!("Added Fill Layer {}", id);
                }
                LayerListAction::AddFolder => {
                    let id = self.layer_stack.layers.len() + 1;
                    self.layer_stack.layers.push(Layer::new_folder(
                        id,
                        format!("Folder {}", id),
                        self.layer_stack.width,
                        self.layer_stack.height,
                    ));
                    self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                    self.paint_target_mask = false;
                    recomposite_needed = true;
                    self.status_message = format!("Added Folder {}", id);
                }
                LayerListAction::InvertMask(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        if let Some(ref mut m) = l.mask {
                            m.invert();
                            recomposite_needed = true;
                            self.status_message = format!("Inverted Mask on {}", l.name);
                        }
                    }
                }
                LayerListAction::ClearMask(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        if let Some(ref mut m) = l.mask {
                            m.clear();
                            recomposite_needed = true;
                            self.status_message = format!("Cleared Mask on {}", l.name);
                        }
                    }
                }
                LayerListAction::RemoveMask(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.mask = None;
                        recomposite_needed = true;
                        self.status_message = format!("Removed Mask from {}", l.name);
                    }
                }
                LayerListAction::Cut(i) => {
                    if i < self.layer_stack.layers.len() {
                        self.layer_clipboard = Some(self.layer_stack.layers.remove(i));
                        if self.layer_stack.active_layer_index >= self.layer_stack.layers.len()
                            && !self.layer_stack.layers.is_empty()
                        {
                            self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                        }
                        recomposite_needed = true;
                        self.status_message = "Cut Layer to Clipboard".into();
                    }
                }
                LayerListAction::Copy(i) => {
                    if let Some(l) = self.layer_stack.layers.get(i) {
                        self.layer_clipboard = Some(l.clone());
                        self.status_message = format!("Copied Layer: {}", l.name);
                    }
                }
                LayerListAction::Paste => {
                    if let Some(ref l) = self.layer_clipboard {
                        let mut new_l = l.clone();
                        new_l.id = self.layer_stack.layers.len() + 1;
                        new_l.name = format!("{} (Paste)", l.name);
                        let insert_at = (self.layer_stack.active_layer_index + 1)
                            .min(self.layer_stack.layers.len());
                        self.layer_stack.layers.insert(insert_at, new_l);
                        self.layer_stack.active_layer_index = insert_at;
                        recomposite_needed = true;
                        self.status_message = format!("Pasted Layer: {}", l.name);
                    }
                }
                LayerListAction::SetColorTag(i, tag) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.color_tag = tag;
                    }
                }
                LayerListAction::SetBlendMode(i, mode) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.blend_mode = mode;
                        recomposite_needed = true;
                        self.status_message = format!("Set Blend Mode: {:?}", mode);
                    }
                }
                LayerListAction::AddFilter(i, effect) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.effects.push(effect);
                        l.apply_effects();
                        recomposite_needed = true;
                        self.status_message = format!("Added Filter to {}", l.name);
                    }
                }
                LayerListAction::AddGenerator(i, gen_name) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        if l.mask.is_none() {
                            l.add_black_mask();
                        }
                        if let Some(ref mut m) = l.mask {
                            let (use_curv, use_ao, default_axis) = match gen_name.as_str() {
                                "Dirt Crevices" => (true, true, 1),
                                "Curvature Wear" => (true, false, 1),
                                "Dust Accumulation" => (false, true, 1),
                                "Position Gradient" => (false, false, 1),
                                "World Normal Orientation" => (false, false, 1),
                                "Thickness / Subsurface" => (false, false, 1),
                                "Color / ID Selection" => (false, false, 1),
                                _ => (true, true, 1),
                            };
                            m.effects.push(MaskEffect::Generator {
                                name: gen_name.clone(),
                                wear_level: 0.8,
                                contrast: 1.2,
                                dirt_amount: 0.5,
                                invert: false,
                                use_ao,
                                use_curvature: use_curv,
                                anchor_name: None,
                                blend_mode: MaskBlendMode::Multiply,
                                opacity: 1.0,
                                grunge_amount: 0.35,
                                grunge_scale: 6.0,
                                axis: default_axis,
                                normal_direction: [0.0, 1.0, 0.0],
                                target_color: [1.0, 0.0, 0.0, 1.0],
                                tolerance: 0.2,
                            });
                            m.recompute(
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                                self.baked_maps.world_space_normal.as_deref(),
                                self.baked_maps.position.as_deref(),
                                self.baked_maps.thickness.as_deref(),
                                self.baked_maps.id.as_deref(),
                            );
                            recomposite_needed = true;
                            self.status_message =
                                format!("Added Generator ({}) to {}", gen_name, l.name);
                        }
                    }
                }
                LayerListAction::AddFillEffect(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        if l.mask.is_none() {
                            l.add_black_mask();
                        }
                        if let Some(ref mut m) = l.mask {
                            m.effects.push(MaskEffect::Fill {
                                value: 0.5,
                                noise_frequency: 1.0,
                                blend_mode: MaskBlendMode::Normal,
                                opacity: 1.0,
                            });
                            m.recompute(
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                                self.baked_maps.world_space_normal.as_deref(),
                                self.baked_maps.position.as_deref(),
                                self.baked_maps.thickness.as_deref(),
                                self.baked_maps.id.as_deref(),
                            );
                            recomposite_needed = true;
                            self.status_message = format!("Added Fill Effect to {}", l.name);
                        }
                    }
                }
                LayerListAction::AddLevels(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        if l.mask.is_none() {
                            l.add_black_mask();
                        }
                        if let Some(ref mut m) = l.mask {
                            m.effects.push(MaskEffect::Levels {
                                black_in: 0.1,
                                white_in: 0.9,
                                gamma: 1.2,
                            });
                            m.recompute(
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                                self.baked_maps.world_space_normal.as_deref(),
                                self.baked_maps.position.as_deref(),
                                self.baked_maps.thickness.as_deref(),
                                self.baked_maps.id.as_deref(),
                            );
                            recomposite_needed = true;
                            self.status_message = format!("Added Levels Effect to {}", l.name);
                        }
                    }
                }
                LayerListAction::AddAnchor(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.anchor_point = Some(AnchorPoint::new(
                            1,
                            format!("Anchor_{}", l.name),
                            l.id,
                            crate::painting::AnchorChannel::All,
                        ));
                        self.status_message = format!("Added Anchor Point to {}", l.name);
                    }
                }
                LayerListAction::Duplicate(i) => {
                    if i < self.layer_stack.layers.len() {
                        let mut dup = self.layer_stack.layers[i].clone();
                        dup.id = self.layer_stack.layers.len() + 1;
                        dup.name = format!("{} (Copy)", dup.name);
                        self.layer_stack.layers.insert(i + 1, dup);
                        self.layer_stack.active_layer_index = i + 1;
                        recomposite_needed = true;
                        self.status_message = format!("Duplicated Layer {}", i + 1);
                    }
                }
                LayerListAction::ClearLayer(i) => {
                    if let Some(l) = self.layer_stack.layers.get_mut(i) {
                        l.clear();
                        recomposite_needed = true;
                        self.status_message = format!("Cleared Content of {}", l.name);
                    }
                }
                LayerListAction::Delete(i) => {
                    if self.layer_stack.layers.len() > 1 {
                        self.layer_stack.layers.remove(i);
                        self.layer_stack.active_layer_index = self
                            .layer_stack
                            .active_layer_index
                            .min(self.layer_stack.layers.len() - 1);
                        recomposite_needed = true;
                        self.status_message = "Deleted Layer".into();
                    }
                }
            }
        }

        // Layer Properties Inspector (for the currently selected layer)
        let active_idx = self.layer_stack.active_layer_index;
        let mut delete_layer = None;

        if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
            ui.separator();
            ui.collapsing(
                egui::RichText::new("PROPERTIES - CURRENT LAYER")
                    .strong()
                    .color(ACTIVE_BLUE),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut active_layer.name);
                    });

                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut active_layer.visible, "Visible").changed() {
                            recomposite_needed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Opacity:");
                        if ui
                            .add(egui::Slider::new(&mut active_layer.opacity, 0.0..=1.0))
                            .changed()
                        {
                            recomposite_needed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Blending:");
                        egui::ComboBox::from_id_salt("layer_blend_mode")
                            .selected_text(active_layer.blend_mode.name())
                            .show_ui(ui, |ui| {
                                let modes = [
                                    BlendMode::Normal,
                                    BlendMode::PassThrough,
                                    BlendMode::Multiply,
                                    BlendMode::Screen,
                                    BlendMode::Overlay,
                                    BlendMode::Add,
                                    BlendMode::Subtract,
                                    BlendMode::Darken,
                                    BlendMode::Lighten,
                                    BlendMode::ColorDodge,
                                    BlendMode::ColorBurn,
                                    BlendMode::LinearBurn,
                                    BlendMode::HardLight,
                                    BlendMode::SoftLight,
                                    BlendMode::Difference,
                                    BlendMode::Exclusion,
                                    BlendMode::Color,
                                    BlendMode::Luminosity,
                                    BlendMode::Hue,
                                    BlendMode::Saturation,
                                    BlendMode::VividLight,
                                    BlendMode::LinearLight,
                                    BlendMode::PinLight,
                                    BlendMode::HardMix,
                                    BlendMode::DarkerColor,
                                    BlendMode::LighterColor,
                                    BlendMode::Divide,
                                    BlendMode::RNM,
                                    BlendMode::Whiteout,
                                ];
                                for m in modes {
                                    if ui
                                        .selectable_value(&mut active_layer.blend_mode, m, m.name())
                                        .clicked()
                                    {
                                        recomposite_needed = true;
                                    }
                                }
                            });
                    });

                    // Mask Operations
                    ui.horizontal(|ui| {
                        if active_layer.mask.is_some() {
                            if ui.small_button("Invert Mask").clicked() {
                                if let Some(ref mut m) = active_layer.mask {
                                    for v in &mut m.buffer {
                                        *v = 1.0 - *v;
                                    }
                                    recomposite_needed = true;
                                }
                            }
                            if ui.small_button("Remove Mask").clicked() {
                                active_layer.mask = None;
                                recomposite_needed = true;
                            }
                        } else {
                            if ui.small_button("+ Black Mask").clicked() {
                                active_layer.add_black_mask();
                                recomposite_needed = true;
                            }
                            if ui.small_button("+ White Mask").clicked() {
                                active_layer.add_white_mask();
                                recomposite_needed = true;
                            }
                        }
                    });

                    // Per-Layer Channel Blends & Overrides (reference PBR suite Parity)
                    ui.collapsing("CHANNEL BLEND & ROUTING (CHANNEL PARITY)", |ui| {
                        let mut ch_row = |name: &str,
                                          enabled: &mut bool,
                                          blend: &mut BlendMode,
                                          opacity: &mut f32,
                                          id_salt: &str| {
                            ui.horizontal(|ui| {
                                if ui.checkbox(enabled, name).changed() {
                                    recomposite_needed = true;
                                }
                                egui::ComboBox::from_id_salt(id_salt)
                                    .selected_text(blend.name())
                                    .width(110.0)
                                    .show_ui(ui, |ui| {
                                        let modes = [
                                            BlendMode::Normal,
                                            BlendMode::Multiply,
                                            BlendMode::Screen,
                                            BlendMode::Overlay,
                                            BlendMode::Add,
                                            BlendMode::Subtract,
                                            BlendMode::Darken,
                                            BlendMode::Lighten,
                                            BlendMode::ColorDodge,
                                            BlendMode::ColorBurn,
                                            BlendMode::LinearBurn,
                                            BlendMode::HardLight,
                                            BlendMode::SoftLight,
                                            BlendMode::Difference,
                                            BlendMode::Exclusion,
                                            BlendMode::VividLight,
                                            BlendMode::LinearLight,
                                            BlendMode::PinLight,
                                            BlendMode::HardMix,
                                            BlendMode::DarkerColor,
                                            BlendMode::LighterColor,
                                            BlendMode::Divide,
                                            BlendMode::RNM,
                                            BlendMode::Whiteout,
                                        ];
                                        for m in modes {
                                            if ui.selectable_value(blend, m, m.name()).clicked() {
                                                recomposite_needed = true;
                                            }
                                        }
                                    });
                                if ui
                                    .add(egui::Slider::new(opacity, 0.0..=1.0).show_value(false))
                                    .changed()
                                {
                                    recomposite_needed = true;
                                }
                            });
                        };

                        ch_row(
                            "Base Color",
                            &mut active_layer.channel_color_enabled,
                            &mut active_layer.channel_color_blend,
                            &mut active_layer.channel_color_opacity,
                            "ch_color_blend",
                        );
                        ch_row(
                            "Metallic",
                            &mut active_layer.channel_metallic_enabled,
                            &mut active_layer.channel_metallic_blend,
                            &mut active_layer.channel_metallic_opacity,
                            "ch_met_blend",
                        );
                        ch_row(
                            "Roughness",
                            &mut active_layer.channel_roughness_enabled,
                            &mut active_layer.channel_roughness_blend,
                            &mut active_layer.channel_roughness_opacity,
                            "ch_rough_blend",
                        );
                        ch_row(
                            "Normal",
                            &mut active_layer.channel_normal_enabled,
                            &mut active_layer.channel_normal_blend,
                            &mut active_layer.channel_normal_opacity,
                            "ch_norm_blend",
                        );
                        ch_row(
                            "Height",
                            &mut active_layer.channel_height_enabled,
                            &mut active_layer.channel_height_blend,
                            &mut active_layer.channel_height_opacity,
                            "ch_height_blend",
                        );
                    });

                    // Layer Filter & Effects Stack
                    ui.collapsing(
                        format!("FILTER & EFFECT STACK ({})", active_layer.effects.len()),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.menu_button("+ Add Filter...", |ui| {
                                    if ui.button("Levels").clicked() {
                                        active_layer.effects.push(FilterEffect::Levels {
                                            min_in: 0.0,
                                            max_in: 1.0,
                                            gamma: 1.0,
                                            min_out: 0.0,
                                            max_out: 1.0,
                                            channel: FilterTargetChannel::All,
                                        });
                                        active_layer.apply_effects();
                                        recomposite_needed = true;
                                        ui.close_menu();
                                    }
                                    if ui.button("Gaussian Blur").clicked() {
                                        active_layer.effects.push(FilterEffect::Blur {
                                            radius: 3.0,
                                            channel: FilterTargetChannel::All,
                                        });
                                        active_layer.apply_effects();
                                        recomposite_needed = true;
                                        ui.close_menu();
                                    }
                                    if ui.button("Sharpen").clicked() {
                                        active_layer.effects.push(FilterEffect::Sharpen {
                                            strength: 1.5,
                                            channel: FilterTargetChannel::All,
                                        });
                                        active_layer.apply_effects();
                                        recomposite_needed = true;
                                        ui.close_menu();
                                    }
                                    if ui.button("HSL Adjustment").clicked() {
                                        active_layer.effects.push(FilterEffect::HSL {
                                            hue_shift: 0.0,
                                            sat_mult: 1.0,
                                            light_offset: 0.0,
                                        });
                                        active_layer.apply_effects();
                                        recomposite_needed = true;
                                        ui.close_menu();
                                    }
                                    if ui.button("Invert").clicked() {
                                        active_layer.effects.push(FilterEffect::Invert {
                                            channel: FilterTargetChannel::All,
                                        });
                                        active_layer.apply_effects();
                                        recomposite_needed = true;
                                        ui.close_menu();
                                    }
                                });
                            });

                            let mut effect_to_remove = None;
                            let mut effects_changed = false;

                            for (eff_idx, effect) in active_layer.effects.iter_mut().enumerate() {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.strong(match effect {
                                            FilterEffect::Levels { .. } => "📊 Levels",
                                            FilterEffect::Blur { .. } => "💧 Gaussian Blur",
                                            FilterEffect::Sharpen { .. } => "🔪 Sharpen",
                                            FilterEffect::HSL { .. } => "🌈 HSL Adjustment",
                                            FilterEffect::Invert { .. } => "🔄 Invert",
                                            FilterEffect::GradientMap { .. } => "🎨 Gradient Map",
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("✕")
                                                    .on_hover_text("Remove filter")
                                                    .clicked()
                                                {
                                                    effect_to_remove = Some(eff_idx);
                                                }
                                            },
                                        );
                                    });

                                    match effect {
                                        FilterEffect::Levels {
                                            min_in,
                                            max_in,
                                            gamma,
                                            min_out,
                                            max_out,
                                            ..
                                        } => {
                                            ui.horizontal(|ui| {
                                                ui.label("In:");
                                                if ui
                                                    .add(
                                                        egui::Slider::new(min_in, 0.0..=1.0)
                                                            .text("Min"),
                                                    )
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                                if ui
                                                    .add(
                                                        egui::Slider::new(max_in, 0.0..=1.0)
                                                            .text("Max"),
                                                    )
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Gamma:");
                                                if ui
                                                    .add(egui::Slider::new(gamma, 0.1..=5.0))
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Out:");
                                                if ui
                                                    .add(
                                                        egui::Slider::new(min_out, 0.0..=1.0)
                                                            .text("Min"),
                                                    )
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                                if ui
                                                    .add(
                                                        egui::Slider::new(max_out, 0.0..=1.0)
                                                            .text("Max"),
                                                    )
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                        }
                                        FilterEffect::Blur { radius, .. } => {
                                            ui.horizontal(|ui| {
                                                ui.label("Radius:");
                                                if ui
                                                    .add(egui::Slider::new(radius, 1.0..=20.0))
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                        }
                                        FilterEffect::Sharpen { strength, .. } => {
                                            ui.horizontal(|ui| {
                                                ui.label("Strength:");
                                                if ui
                                                    .add(egui::Slider::new(strength, 0.1..=5.0))
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                        }
                                        FilterEffect::HSL {
                                            hue_shift,
                                            sat_mult,
                                            light_offset,
                                        } => {
                                            ui.horizontal(|ui| {
                                                ui.label("Hue:");
                                                if ui
                                                    .add(egui::Slider::new(
                                                        hue_shift,
                                                        -180.0..=180.0,
                                                    ))
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Saturation:");
                                                if ui
                                                    .add(egui::Slider::new(sat_mult, 0.0..=2.0))
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Lightness:");
                                                if ui
                                                    .add(egui::Slider::new(
                                                        light_offset,
                                                        -1.0..=1.0,
                                                    ))
                                                    .changed()
                                                {
                                                    effects_changed = true;
                                                }
                                            });
                                        }
                                        FilterEffect::Invert { .. } => {
                                            ui.label("Inverts pixel values across target channel.");
                                        }
                                        FilterEffect::GradientMap { .. } => {
                                            ui.label("Remaps luminance to gradient palette.");
                                        }
                                    }
                                });
                            }

                            if let Some(rem_idx) = effect_to_remove {
                                active_layer.effects.remove(rem_idx);
                                effects_changed = true;
                            }

                            if effects_changed {
                                active_layer.apply_effects();
                                recomposite_needed = true;
                            }
                        },
                    );

                    if active_layer.layer_type == LayerType::Fill {
                        ui.separator();
                        ui.label(egui::RichText::new("MATERIAL CHANNELS").strong());

                        ui.horizontal(|ui| {
                            ui.label("UV Tiling:");
                            ui.add(egui::Slider::new(&mut self.uv_scale, 0.25..=8.0));
                        });

                        let mut col = [
                            (active_layer.fill_color[0] * 255.0) as u8,
                            (active_layer.fill_color[1] * 255.0) as u8,
                            (active_layer.fill_color[2] * 255.0) as u8,
                        ];
                        ui.horizontal(|ui| {
                            ui.label("Base Color:");
                            if ui.color_edit_button_srgb(&mut col).changed() {
                                active_layer.fill_color[0] = col[0] as f32 / 255.0;
                                active_layer.fill_color[1] = col[1] as f32 / 255.0;
                                active_layer.fill_color[2] = col[2] as f32 / 255.0;
                                active_layer.update_fill();
                                recomposite_needed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Metallic:");
                            if ui
                                .add(egui::Slider::new(
                                    &mut active_layer.fill_metallic,
                                    0.0..=1.0,
                                ))
                                .changed()
                            {
                                active_layer.update_fill();
                                recomposite_needed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Roughness:");
                            if ui
                                .add(egui::Slider::new(
                                    &mut active_layer.fill_roughness,
                                    0.04..=1.0,
                                ))
                                .changed()
                            {
                                active_layer.update_fill();
                                recomposite_needed = true;
                            }
                        });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("🗑 Delete Layer")
                                    .color(Color32::from_rgb(255, 100, 100)),
                            )
                            .clicked()
                        {
                            delete_layer = Some(active_idx);
                        }
                    });
                },
            );
        }

        if let Some(idx) = delete_layer {
            if self.layer_stack.layers.len() > 1 {
                self.layer_stack.layers.remove(idx);
                self.layer_stack.active_layer_index = self
                    .layer_stack
                    .active_layer_index
                    .min(self.layer_stack.layers.len() - 1);
                recomposite_needed = true;
                self.status_message = "Deleted Layer".into();
            }
        }

        if recomposite_needed {
            self.recompute_composite();
        }
    }

    fn ui_display_settings(&mut self, ui: &mut Ui) {
        ui.heading("DISPLAY SETTINGS");
        ui.separator();

        ui.collapsing("CAMERA & TURNTABLE", |ui| {
            ui.add(egui::Slider::new(&mut self.camera_yaw, 0.0..=360.0).text("Yaw"));
            ui.add(egui::Slider::new(&mut self.camera_pitch, -85.0..=85.0).text("Pitch"));
            ui.add(egui::Slider::new(&mut self.camera_dist, 0.5..=10.0).text("Distance"));

            ui.separator();
            ui.horizontal(|ui| {
                let btn_text = if self.turntable_active {
                    "⏹ Stop Turntable (Y)"
                } else {
                    "▶ Start Turntable (Y)"
                };
                if ui.button(btn_text).clicked() {
                    self.turntable_active = !self.turntable_active;
                }
                ui.add(
                    egui::Slider::new(&mut self.turntable_speed, -180.0..=180.0)
                        .text("Speed (°/s)"),
                );
            });

            ui.separator();
            ui.label(
                RichText::new("Camera Bookmarks (Ctrl+1..9 to Save, Alt+1..9 to Recall)")
                    .strong()
                    .color(Color32::from_rgb(180, 180, 180)),
            );
            for slot in 1..=9 {
                ui.horizontal(|ui| {
                    if let Some(ref bm) = self.camera_bookmarks[slot] {
                        ui.label(format!(
                            "#{}: Yaw {:.0}°, Pitch {:.0}°, Dist {:.2}",
                            slot, bm.yaw, bm.pitch, bm.dist
                        ));
                        if ui.button("Recall").clicked() {
                            self.camera_yaw = bm.yaw;
                            self.camera_pitch = bm.pitch;
                            self.camera_dist = bm.dist;
                            self.status_message = format!("Restored Camera Bookmark {}", slot);
                        }
                        if ui.button("Overwrite").clicked() {
                            self.camera_bookmarks[slot] = Some(CameraBookmark {
                                name: format!("Bookmark {}", slot),
                                yaw: self.camera_yaw,
                                pitch: self.camera_pitch,
                                dist: self.camera_dist,
                            });
                            self.status_message = format!("Saved Camera Bookmark {}", slot);
                        }
                        if ui.button("✖").clicked() {
                            self.camera_bookmarks[slot] = None;
                        }
                    } else {
                        ui.label(format!("#{}: [Empty]", slot));
                        if ui.button("Save Here").clicked() {
                            self.camera_bookmarks[slot] = Some(CameraBookmark {
                                name: format!("Bookmark {}", slot),
                                yaw: self.camera_yaw,
                                pitch: self.camera_pitch,
                                dist: self.camera_dist,
                            });
                            self.status_message = format!("Saved Camera Bookmark {}", slot);
                        }
                    }
                });
            }
        });

        ui.collapsing("POST-PROCESSING (BLOOM, SSAO, DOF)", |ui| {
            ui.checkbox(&mut self.bloom_enabled, "Bloom Highlights");
            if self.bloom_enabled {
                ui.add(
                    egui::Slider::new(&mut self.bloom_intensity, 0.0..=3.0).text("Bloom Intensity"),
                );
                ui.add(
                    egui::Slider::new(&mut self.bloom_threshold, 0.0..=1.0).text("Bloom Threshold"),
                );
            }

            ui.separator();
            ui.checkbox(&mut self.ssao_enabled, "SSAO Contact Shadows");
            if self.ssao_enabled {
                ui.add(
                    egui::Slider::new(&mut self.ssao_intensity, 0.0..=2.0).text("SSAO Intensity"),
                );
            }

            ui.separator();
            ui.checkbox(&mut self.dof_enabled, "Depth of Field (DoF)");
            if self.dof_enabled {
                ui.add(
                    egui::Slider::new(&mut self.dof_focal_distance, 0.5..=6.0)
                        .text("Focal Distance"),
                );
                ui.add(egui::Slider::new(&mut self.dof_aperture, 0.01..=0.3).text("Aperture"));
            }
        });

        ui.collapsing("SUBMESH VISIBILITY & ISOLATION", |ui| {
            if self.mesh.submeshes.is_empty() {
                ui.label("No submeshes found (Single mesh object)");
            } else {
                ui.horizontal(|ui| {
                    if ui.button("Show All Submeshes").clicked() {
                        self.isolated_submesh = None;
                        if let Some(ref vp_mutex) = self.viewport_wgpu {
                            if let Ok(mut vp) = vp_mutex.lock() {
                                vp.isolate_submesh(None);
                            }
                        }
                        self.status_message = "All submeshes visible".to_string();
                    }
                });
                ui.separator();

                let mut to_toggle = None;
                let mut to_isolate = None;

                if let Some(ref vp_mutex) = self.viewport_wgpu {
                    if let Ok(vp) = vp_mutex.lock() {
                        for (i, sm) in vp.submeshes().iter().enumerate() {
                            ui.horizontal(|ui| {
                                let eye_text = if sm.visible { "👁" } else { "🚫" };
                                if ui.button(eye_text).clicked() {
                                    to_toggle = Some((i, !sm.visible));
                                }
                                ui.label(&sm.name);
                                let is_isolated = self.isolated_submesh == Some(i);
                                let iso_btn_text = if is_isolated { "Isolated" } else { "Isolate" };
                                if ui.button(iso_btn_text).clicked() {
                                    if is_isolated {
                                        to_isolate = Some(None);
                                    } else {
                                        to_isolate = Some(Some(i));
                                    }
                                }
                            });
                        }
                    }
                }

                if let Some((idx, vis)) = to_toggle {
                    if let Some(ref vp_mutex) = self.viewport_wgpu {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.set_submesh_visibility(idx, vis);
                        }
                    }
                }
                if let Some(target) = to_isolate {
                    self.isolated_submesh = target;
                    if let Some(ref vp_mutex) = self.viewport_wgpu {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.isolate_submesh(target);
                        }
                    }
                }
            }
        });

        ui.collapsing("ENVIRONMENT LIGHTING (HDR IBL & STUDIO PROBES)", |ui| {
            if !self.available_environments.is_empty() {
                let current_name = self
                    .available_environments
                    .get(self.selected_env_idx)
                    .map(|(n, _)| n.as_str())
                    .unwrap_or("Studio Tomoco");

                egui::ComboBox::from_label("Studio HDRI (.EXR)")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for (i, (env_name, env_path)) in
                            self.available_environments.iter().enumerate()
                        {
                            if ui
                                .selectable_value(&mut self.selected_env_idx, i, env_name)
                                .clicked()
                            {
                                if let Some(ref vp_mutex) = self.viewport_wgpu {
                                    if let (Some(ref device), Some(ref queue)) =
                                        (&self.wgpu_device, &self.wgpu_queue)
                                    {
                                        if let Ok(mut vp) = vp_mutex.lock() {
                                            if let Err(e) =
                                                vp.load_environment(device, queue, env_path)
                                            {
                                                eprintln!(
                                                    "[ERROR] Failed to load environment {}: {}",
                                                    env_name, e
                                                );
                                            } else {
                                                self.status_message = format!(
                                                    "Loaded HDRI Environment: {}",
                                                    env_name
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
            }

            ui.add(egui::Slider::new(&mut self.exposure, -3.0..=3.0).text("Exposure (EV)"));
            ui.add(egui::Slider::new(&mut self.light_intensity, 0.0..=4.0).text("Light Intensity"));
            ui.add(
                egui::Slider::new(&mut self.env_rotation, 0.0..=360.0).text("Rotation (Shift+RMB)"),
            );

            ui.separator();
            ui.checkbox(&mut self.skybox_visible, "Show Skybox Background");
            if self.skybox_visible {
                ui.add(egui::Slider::new(&mut self.skybox_blur, 0.0..=1.0).text("Skybox Blur"));
                ui.add(
                    egui::Slider::new(&mut self.skybox_intensity, 0.0..=3.0)
                        .text("Skybox Intensity"),
                );
            }

            ui.separator();
            ui.add(egui::Slider::new(&mut self.uv_scale, 0.1..=10.0).text("Material UV Tiling"));
            ui.checkbox(&mut self.show_wireframe, "Wireframe Overlay (F4)");
            ui.checkbox(&mut self.show_grid, "Ground Grid & World Axes");

            ui.separator();
            ui.label(RichText::new("Color Management / Tone Mapping").strong());
            egui::ComboBox::from_id_salt("display_color_mgmt")
                .selected_text(match self.color_management_mode {
                    0 => "ACES Filmic (reference PBR suite Look)",
                    1 => "sRGB Display (Raw Linear Transfer)",
                    _ => "Neutral Reinhard (Soft Highlight Shoulder)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.color_management_mode,
                        0,
                        "ACES Filmic (reference PBR suite Look)",
                    );
                    ui.selectable_value(
                        &mut self.color_management_mode,
                        1,
                        "sRGB Display (Raw Linear Transfer)",
                    );
                    ui.selectable_value(
                        &mut self.color_management_mode,
                        2,
                        "Neutral Reinhard (Soft Highlight Shoulder)",
                    );
                });
        });
    }

    fn ui_shader_settings(&mut self, ui: &mut Ui) {
        ui.heading("SHADER SETTINGS");
        ui.separator();
        ui.label("Shader: pbr-metal-rough (reference PBR suite Parity)");
        ui.label("BRDF: Cook-Torrance Microfacet GGX");
        ui.label("Specular D: D_GGX (Trowbridge-Reitz)");
        ui.label("Shadowing G: Smith-Schlick GGX");
        ui.label("Fresnel F: Schlick with Roughness correction");
        ui.label("IBL: Split-Sum Numerical 2D Integration LUT (512x512)");
        ui.label("Tone Mapping: ACES Filmic Curve (sRGB transfer)");
        ui.label("Color Profiles: ACES Filmic / sRGB Display / Neutral Reinhard");
        ui.label("Depth Buffer: 24-bit Hardware GL_DEPTH_TEST (GL_LEQUAL)");
        ui.label("Hardware Core: Native Vulkan / wgpu PBR Viewport");
        ui.label("Status: [OK] Active & Real-Time GPU Rasterized");
    }

    fn ui_history_panel(&mut self, ui: &mut Ui) {
        ui.heading("HISTORY");
        ui.separator();

        ui.horizontal(|ui| {
            let undo_enabled = self.history_manager.can_undo();
            if ui
                .add_enabled(undo_enabled, egui::Button::new("↶ Undo"))
                .clicked()
            {
                self.perform_undo();
            }
            let redo_enabled = self.history_manager.can_redo();
            if ui
                .add_enabled(redo_enabled, egui::Button::new("↷ Redo"))
                .clicked()
            {
                self.perform_redo();
            }
            if ui.button("Clear History").clicked() {
                self.history_manager.undo_stack.clear();
                self.history_manager.redo_stack.clear();
                self.status_message = "History cleared".into();
            }
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(
                RichText::new("Undo Stack (Active / Past)")
                    .strong()
                    .color(Color32::from_rgb(180, 180, 180)),
            );
            let mut rollback_to = None;
            let current_len = self.history_manager.undo_stack.len();
            for (idx, cmd) in self.history_manager.undo_stack.iter().enumerate().rev() {
                let is_latest = idx + 1 == current_len;
                let text = format!(
                    "{:02}  {} ({} tiles)",
                    idx + 1,
                    cmd.description,
                    cmd.tile_backups.len()
                );
                let row = if is_latest {
                    ui.selectable_label(true, RichText::new(text).color(ACTIVE_BLUE).strong())
                } else {
                    ui.selectable_label(false, text)
                };
                if row.clicked() && !is_latest {
                    rollback_to = Some(idx);
                }
            }

            if let Some(target_idx) = rollback_to {
                let steps = self.history_manager.undo_stack.len() - 1 - target_idx;
                for _ in 0..steps {
                    self.perform_undo();
                }
            }

            let mut roll_forward_to = None;
            if !self.history_manager.redo_stack.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new("Redo Stack (Undone actions - Click to Fast-Forward)")
                        .italics()
                        .color(Color32::from_rgb(120, 120, 120)),
                );
                for (r_idx, cmd) in self.history_manager.redo_stack.iter().enumerate().rev() {
                    let text = format!("↷  {} ({} tiles)", cmd.description, cmd.tile_backups.len());
                    if ui
                        .selectable_label(
                            false,
                            RichText::new(text).color(Color32::from_rgb(140, 140, 140)),
                        )
                        .clicked()
                    {
                        roll_forward_to = Some(r_idx);
                    }
                }
            }

            if let Some(target_r_idx) = roll_forward_to {
                let steps = self.history_manager.redo_stack.len() - target_r_idx;
                for _ in 0..steps {
                    self.perform_redo();
                }
            }

            if self.history_manager.undo_stack.is_empty()
                && self.history_manager.redo_stack.is_empty()
            {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("No brush or tool actions recorded yet.")
                        .italics()
                        .color(TEXT_DIMMED),
                );
            }
        });
    }

    fn ui_log_panel(&mut self, ui: &mut Ui) {
        ui.heading("LOG");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (time, lvl, msg) in &self.logs {
                ui.horizontal(|ui| {
                    ui.label(format!("[{}]", time));
                    ui.colored_label(
                        match lvl.as_str() {
                            "INFO" => Color32::from_rgb(58, 217, 115),
                            "WARN" => SMART_MAT_ORANGE,
                            _ => Color32::RED,
                        },
                        format!("[{}]", lvl),
                    );
                    ui.label(msg);
                });
            }
        });
    }

    // ------------------------------------------------------------------
    // Offline Path-Traced Visualisation (Render Mode)
    // ------------------------------------------------------------------

    /// Capture settings + build the traced scene from the paint session and
    /// spawn the background worker (same pattern as `bake_mesh_maps`).
    pub fn start_render_session(&mut self) {
        // A previous, still-running session is cancelled on Drop.
        self.render_state = None;

        let mut opts = self.render_options.clone();
        opts.spp = self.render_preset.spp(self.render_custom_spp);
        // Capture the current viewport orbit exactly as the artist sees it.
        opts.camera = crate::render::OrbitCamera {
            yaw_deg: self.camera_yaw,
            pitch_deg: self.camera_pitch,
            dist: self.camera_dist,
            fov_deg: 45.0,
        };
        self.render_options.spp = opts.spp;
        self.render_options.camera = opts.camera;

        let scene = crate::render::RenderScene::from_pbr(
            &self.mesh,
            &self.layer_stack.composite_color,
            &self.layer_stack.composite_roughness,
            &self.layer_stack.composite_metallic,
            &self.layer_stack.composite_normal,
            self.layer_stack.width,
            self.layer_stack.height,
        );

        self.render_film_handle = None;
        self.render_last_preview_passes = 0;
        self.render_logged = false;
        self.render_state = Some(crate::render::spawn_render(scene, opts));
        self.status_message = format!(
            "Started offline render: {}×{} @ {} spp (CPU path traced)",
            self.render_options.width, self.render_options.height, self.render_options.spp
        );
        self.logs.push((
            chrono::Local::now().format("%H:%M:%S").to_string(),
            "INFO".into(),
            format!(
                "Offline path-traced render started ({}x{}, {} spp, mesh {})",
                self.render_options.width,
                self.render_options.height,
                self.render_options.spp,
                self.mesh.name
            ),
        ));
    }

    /// Cancel + detach the worker. `RenderJob::drop` flips the cancel flag, so
    /// leaving Render mode never blocks the UI thread.
    pub fn stop_render_session(&mut self) {
        if let Some(job) = self.render_state.take() {
            if job.film.passes() > 0 && !job.progress.finished.load(Ordering::Relaxed) {
                self.status_message = format!("Render stopped at {} passes", job.film.passes());
            }
        }
    }

    pub fn ui_render_studio_mode(&mut self, ctx: &egui::Context) {
        // 1. Top Menu Bar (same studio style as the Baking Studio)
        egui::TopBottomPanel::top("render_top_menu").show(ctx, |ui| {
            if self.logo_texture.is_none() {
                let logo_bytes = include_bytes!("../../../assets/logo_icon.png");
                if let Ok(img) = image::load_from_memory(logo_bytes) {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        &rgba.into_raw(),
                    );
                    self.logo_texture = Some(ctx.load_texture(
                        "remoainter_logo",
                        color_img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }

            ui.horizontal(|ui| {
                if let Some(ref logo) = self.logo_texture {
                    ui.image(egui::load::SizedTexture::new(logo.id(), vec2(20.0, 20.0)));
                } else {
                    ui.colored_label(SMART_MAT_ORANGE, "RM");
                }
                ui.label(
                    egui::RichText::new("REMoAINTER")
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.separator();

                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Return to Painting Mode").clicked() {
                            self.show_render_mode = false;
                            ui.close_menu();
                        }
                        if ui.button("Exit (Alt+F4)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Mode", |ui| {
                        if ui.button("🎨 Painting (Edition)").clicked() {
                            self.show_render_mode = false;
                            ui.close_menu();
                        }
                        if ui.button("⚡ Bake Mesh Maps (Studio)").clicked() {
                            self.show_render_mode = false;
                            self.show_baker_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("📷 Visualisation (Render)").clicked() {
                            ui.close_menu();
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(SMART_MAT_ORANGE, "📷 OFFLINE RENDER STUDIO");
                });
            });
        });

        // 2. Right Control Panel: settings + session controls + progress
        egui::SidePanel::right("render_controls_panel")
            .exact_width(340.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("📷 Visualisation (Render)");
                    ui.label("Offline path-traced, CPU progressive");
                    ui.separator();

                    ui.label("Quality Preset:");
                    egui::ComboBox::from_id_salt("render_preset_combo")
                        .selected_text(self.render_preset.name())
                        .show_ui(ui, |ui| {
                            for preset in crate::render::QualityPreset::ALL {
                                let _ = ui.selectable_value(
                                    &mut self.render_preset,
                                    preset,
                                    preset.name(),
                                );
                            }
                        });
                    if self.render_preset != crate::render::QualityPreset::Custom {
                        self.render_options.spp =
                            self.render_preset.spp(self.render_custom_spp);
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Custom spp:");
                            ui.add(
                                egui::DragValue::new(&mut self.render_custom_spp)
                                    .range(1..=20000),
                            );
                        });
                        self.render_options.spp =
                            self.render_preset.spp(self.render_custom_spp);
                    }

                    ui.label("Film Size:");
                    egui::ComboBox::from_id_salt("render_film_size_combo")
                        .selected_text(format!(
                            "{}×{}",
                            self.render_options.width, self.render_options.height
                        ))
                        .show_ui(ui, |ui| {
                            for (w, h) in [(512u32, 512u32), (1024, 1024), (2048, 2048), (4096, 4096)] {
                                let _ = ui.selectable_value(
                                    &mut self.render_options.width,
                                    w,
                                    format!("{w}×{h}"),
                                );
                                let _ = ui.selectable_value(
                                    &mut self.render_options.height,
                                    h,
                                    format!("{w}×{h}"),
                                );
                            }
                        });

                    ui.separator();
                    ui.add(
                        egui::Slider::new(
                            &mut self.render_options.exposure_stops,
                            -6.0..=6.0,
                        )
                        .text("Exposure (stops)"),
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut self.render_options.env.intensity,
                            0.0..=4.0,
                        )
                        .text("Env Intensity"),
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut self.render_options.env.rotation_deg,
                            0.0..=360.0,
                        )
                        .text("Env Rotation (°)"),
                    );
                    if let crate::render::EnvKind::Studio {
                        sun_intensity,
                        sun_angle_deg,
                    } = &mut self.render_options.env.kind
                    {
                        ui.add(
                            egui::Slider::new(sun_intensity, 0.0..=64.0)
                                .text("Sun Intensity"),
                        );
                        ui.add(
                            egui::Slider::new(sun_angle_deg, 0.0..=360.0)
                                .text("Sun Angle (°)"),
                        );
                    }

                    ui.separator();
                    ui.label("Output PNG (auto-save on completion):");
                    ui.horizontal(|ui| {
                        let shown = self
                            .render_options
                            .output_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "<not set>".into());
                        ui.label(
                            egui::RichText::new(shown)
                                .monospace()
                                .color(TEXT_PRIMARY),
                        );
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Save Rendered Film As...")
                                .add_filter("PNG Image (*.png)", &["png"])
                                .set_file_name("render.png")
                                .save_file()
                            {
                                self.render_options.output_path = Some(path);
                            }
                        }
                        if ui.add_enabled(
                            self.render_options.output_path.is_some(),
                            egui::Button::new("Clear"),
                        ).clicked() {
                            self.render_options.output_path = None;
                        }
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("▶ Start Render").clicked() {
                            self.start_render_session();
                            ui.close_menu();
                        }
                        ui.separator();
                        if let Some(ref job) = self.render_state {
                            let finished =
                                job.progress.finished.load(Ordering::Relaxed);
                            if !finished {
                                let paused = job.progress.pause.load(Ordering::Relaxed);
                                if ui.button(if paused { "▶ Resume" } else { "⏸ Pause" }).clicked() {
                                    job.progress.pause.store(!paused, Ordering::Relaxed);
                                }
                                if ui.button("⏹ Stop").clicked() {
                                    job.progress.cancel.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                    });
                    if let Some(ref job) = self.render_state {
                        if ui
                            .add_enabled(
                                job.film.passes() > 0,
                                egui::Button::new("💾 Save Film PNG..."),
                            )
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Save Current Film As...")
                                .add_filter("PNG Image (*.png)", &["png"])
                                .set_file_name("render.png")
                                .save_file()
                            {
                                match crate::render::save_png(
                                    &job.film,
                                    self.render_options.exposure_stops,
                                    &path,
                                ) {
                                    Ok(()) => {
                                        self.status_message =
                                            format!("Saved render film: {}", path.display());
                                    }
                                    Err(err) => {
                                        self.status_message = format!("Film save failed: {err}");
                                    }
                                }
                            }
                        }
                    }

                    ui.separator();
                    if let Some(ref job) = self.render_state {
                        let passes = job.progress.passes.load(Ordering::Relaxed);
                        let target = self.render_options.spp.max(1);
                        let frac = (passes as f32 / target as f32).clamp(0.0, 1.0);
                        let paused = job.progress.pause.load(Ordering::Relaxed);
                        let finished = job.progress.finished.load(Ordering::Relaxed);
                        let saved = job.progress.saved.load(Ordering::Relaxed);
                        let state_txt = if finished {
                            if saved {
                                "Completed & saved"
                            } else if job.progress.cancel.load(Ordering::Relaxed) {
                                "Stopped"
                            } else {
                                "Completed"
                            }
                        } else if paused {
                            "Paused"
                        } else {
                            "Rendering"
                        };
                        ui.label(egui::RichText::new(state_txt).strong().color(ACTIVE_BLUE));
                        ui.add(
                            egui::ProgressBar::new(frac)
                                .text(format!("{} / {} passes", passes, target)),
                        );
                        ui.label(format!(
                            "Elapsed: {:.1} s | Film: {}×{}",
                            job.elapsed_ms() as f32 / 1000.0,
                            self.render_options.width,
                            self.render_options.height
                        ));
                    } else {
                        ui.colored_label(
                            TEXT_PRIMARY,
                            "Press ▶ Start Render to trace the current paint session\n(camera orbit captured at start).",
                        );
                    }
                });
            });

        // 3. Central Film Preview (progressive, throttled to ~250 ms)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.painter().rect_filled(
                ui.max_rect(),
                0.0_f32,
                Color32::from_rgb(26, 26, 28),
            );

            if let Some(ref job) = self.render_state {
                let passes = job.progress.passes.load(Ordering::Relaxed);
                let due = passes > 0 && self.render_last_preview_passes != passes;
                let throttle_ok = self
                    .render_last_preview
                    .elapsed()
                    .as_millis()
                    >= 250;
                if due && throttle_ok {
                    let snap = job.film.snapshot_rgba(self.render_options.exposure_stops);
                    let size = [snap.width, snap.height];
                    let color_img = egui::ColorImage {
                        size,
                        pixels: snap
                            .rgb
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|c| Color32::from_rgb(c[0], c[1], c[2]))
                            .collect(),
                    };
                    if let Some(ref mut handle) = self.render_film_handle {
                        handle.set(color_img, egui::TextureOptions::LINEAR);
                    } else {
                        self.render_film_handle = Some(ctx.load_texture(
                            "render_preview",
                            color_img,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    self.render_last_preview_passes = snap.passes;
                    self.render_last_preview = std::time::Instant::now();
                }

                if let Some(ref tex) = self.render_film_handle {
                    let avail = ui.available_size();
                    let tw = tex.size()[0] as f32;
                    let th = tex.size()[1] as f32;
                    let scale = (avail.x / tw).min(avail.y / th).min(1e6);
                    let sz = vec2(tw * scale, th * scale);
                    let (rect, _) = ui.allocate_exact_size(sz, egui::Sense::hover());
                    ui.painter().image(
                        tex.id(),
                        rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Progressive film — {} passes converged",
                            self.render_last_preview_passes
                        ))
                        .weak(),
                    );
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(
                        TEXT_PRIMARY,
                        "📷 Offline Render Studio — set options on the right, then Start Render.\nRGB path tracer, analytic studio env (HDRI/skybox files and denoiser are out of scope this pass).",
                    );
                });
            }
        });

        // 4. Completion logging & premultiplied repaint while the worker runs
        if let Some(ref job) = self.render_state {
            if !job.progress.finished.load(Ordering::Relaxed) {
                ctx.request_repaint();
            } else if !self.render_logged {
                self.render_logged = true;
                let passes = job.progress.passes.load(Ordering::Relaxed);
                let saved = job.progress.saved.load(Ordering::Relaxed);
                let stopped = job.progress.cancel.load(Ordering::Relaxed);
                self.status_message = if saved {
                    format!(
                        "Render finished: {} spp in {:.1} s — PNG saved",
                        passes,
                        job.elapsed_ms() as f32 / 1000.0
                    )
                } else if stopped {
                    format!("Render stopped at {passes} passes")
                } else {
                    format!(
                        "Render finished: {} spp in {:.1} s — no output path set (use Save Film PNG)",
                        passes,
                        job.elapsed_ms() as f32 / 1000.0
                    )
                };
                self.logs.push((
                    chrono::Local::now().format("%H:%M:%S").to_string(),
                    if stopped {
                        "WARN".into()
                    } else {
                        "INFO".into()
                    },
                    format!("Offline render session ended ({passes} passes)"),
                ));
            }
        }
    }

    pub fn ui_baker_studio_mode(&mut self, ctx: &egui::Context) {
        // 1. Top Menu Bar (Allegorithmic Studio style)
        egui::TopBottomPanel::top("baker_top_menu").show(ctx, |ui| {
            if self.logo_texture.is_none() {
                let logo_bytes = include_bytes!("../../../assets/logo_icon.png");
                if let Ok(img) = image::load_from_memory(logo_bytes) {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        &rgba.into_raw(),
                    );
                    self.logo_texture = Some(ctx.load_texture(
                        "remoainter_logo",
                        color_img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }

            ui.horizontal(|ui| {
                if let Some(ref logo) = self.logo_texture {
                    ui.image(egui::load::SizedTexture::new(logo.id(), vec2(20.0, 20.0)));
                } else {
                    ui.colored_label(SMART_MAT_ORANGE, "RM");
                }
                ui.label(
                    egui::RichText::new("REMoAINTER")
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.separator();

                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Return to Painting Mode").clicked() {
                            self.show_baker_dialog = false;
                            ui.close_menu();
                        }
                        if ui.button("Exit (Alt+F4)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Mode", |ui| {
                        if ui.button("🎨 Painting (Edition)").clicked() {
                            self.show_baker_dialog = false;
                            ui.close_menu();
                        }
                        if ui.button("⚡ Bake Mesh Maps (Studio)").clicked() {
                            self.show_baker_dialog = true;
                            ui.close_menu();
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(ACTIVE_BLUE, "⚡ BAKING STUDIO MODE");
                });
            });
        });

        // 2. Control Bar (Mode Switcher & Baking Controls)
        egui::TopBottomPanel::top("baker_control_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(false, "🎨 Painting").clicked() {
                    self.show_baker_dialog = false;
                }
                let _ = ui.selectable_label(true, "⚡ Bake Mesh Maps");
                ui.separator();

                ui.label("Resolution:");
                egui::ComboBox::from_id_salt("baker_studio_res_combo")
                    .selected_text(format!(
                        "{}×{}",
                        self.baker_options.resolution, self.baker_options.resolution
                    ))
                    .show_ui(ui, |ui| {
                        let _ =
                            ui.selectable_value(&mut self.baker_options.resolution, 512, "512×512");
                        let _ = ui.selectable_value(
                            &mut self.baker_options.resolution,
                            1024,
                            "1024×1024",
                        );
                        let _ = ui.selectable_value(
                            &mut self.baker_options.resolution,
                            2048,
                            "2048×2048",
                        );
                        let _ = ui.selectable_value(
                            &mut self.baker_options.resolution,
                            4096,
                            "4096×4096",
                        );
                    });

                ui.separator();
                ui.colored_label(Color32::WHITE, format!("Target: {}", self.mesh.name));
            });
        });

        // 3. Status Bar
        egui::TopBottomPanel::bottom("baker_status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::from_rgb(58, 217, 115), "● Ready to Bake");
                ui.label(format!(
                    "| Target: {} ({} triangles) | Output: {}×{}",
                    self.mesh.name,
                    self.mesh.indices.len() / 3,
                    self.baker_options.resolution,
                    self.baker_options.resolution
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("reference PBR suite Baking Studio 1:1");
                });
            });
        });

        // 4. Far-Left SidePanel: Texture Set List + Mesh Map Bakers + Baking Log
        egui::SidePanel::left("baker_left_panel")
            .exact_width(220.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.strong("TEXTURE SET LIST");
                ui.separator();

                let active_tab_bg = Color32::from_rgb(35, 65, 105);
                let (ts_rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
                ui.painter().rect_filled(ts_rect, 2.0, active_tab_bg);
                ui.painter()
                    .rect_stroke(ts_rect, 2.0, Stroke::new(1.0_f32, ACTIVE_BLUE));
                ui.painter().text(
                    pos2(ts_rect.left() + 8.0, ts_rect.center().y),
                    Align2::LEFT_CENTER,
                    format!("☑  {}", self.mesh.name),
                    egui::FontId::proportional(11.5),
                    Color32::WHITE,
                );
                ui.painter().text(
                    pos2(ts_rect.right() - 8.0, ts_rect.center().y),
                    Align2::RIGHT_CENTER,
                    format!(
                        "{}×{}",
                        self.baker_options.resolution, self.baker_options.resolution
                    ),
                    egui::FontId::proportional(10.0),
                    TEXT_DIMMED,
                );

                ui.add_space(8.0);
                ui.strong("MESH MAP BAKERS");
                ui.separator();

                // Common settings row
                let cs_active = self.baker_selected_map == 99;
                let (cs_rect, cs_resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::click());
                if cs_active || cs_resp.hovered() {
                    ui.painter().rect_filled(cs_rect, 2.0, ACTIVE_ROW_FILL);
                }
                if cs_active {
                    ui.painter()
                        .rect_stroke(cs_rect, 2.0, Stroke::new(1.0_f32, ACTIVE_BLUE));
                }
                ui.painter().text(
                    pos2(cs_rect.left() + 24.0, cs_rect.center().y),
                    Align2::LEFT_CENTER,
                    "⚙  Common settings",
                    egui::FontId::proportional(11.5),
                    Color32::WHITE,
                );
                if cs_resp.clicked() {
                    self.baker_selected_map = 99;
                }

                ui.add_space(2.0);

                let map_rows: &[(&str, usize, [u8; 3], bool)] = &[
                    (
                        "Normal",
                        2,
                        [120, 150, 255],
                        self.baked_maps.normal.is_some(),
                    ),
                    (
                        "World space normal",
                        3,
                        [80, 200, 200],
                        self.baked_maps.world_space_normal.is_some(),
                    ),
                    ("ID", 6, [230, 120, 80], self.baked_maps.id.is_some()),
                    (
                        "Ambient occlusion",
                        0,
                        [200, 200, 200],
                        self.baked_maps.ambient_occlusion.is_some(),
                    ),
                    (
                        "Curvature",
                        1,
                        [100, 220, 120],
                        self.baked_maps.curvature.is_some(),
                    ),
                    (
                        "Position",
                        4,
                        [220, 180, 80],
                        self.baked_maps.position.is_some(),
                    ),
                    (
                        "Thickness",
                        5,
                        [200, 80, 180],
                        self.baked_maps.thickness.is_some(),
                    ),
                    ("Height", 7, [240, 220, 80], false),
                    ("Bent normals", 8, [140, 200, 255], false),
                    ("Opacity", 9, [180, 180, 220], false),
                ];

                let enabled_flags = [
                    self.baker_options.bake_ao,
                    self.baker_options.bake_curvature,
                    self.baker_options.bake_normal,
                    self.baker_options.bake_world_space_normal,
                    self.baker_options.bake_position,
                    self.baker_options.bake_thickness,
                    self.baker_options.bake_id,
                    false,
                    false,
                    false,
                ];

                for (name, map_idx, color, is_baked) in map_rows {
                    let is_sel = self.baker_selected_map == *map_idx;
                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 22.0),
                        egui::Sense::click(),
                    );
                    if is_sel {
                        ui.painter().rect_filled(row_rect, 2.0, ACTIVE_ROW_FILL);
                        ui.painter()
                            .rect_stroke(row_rect, 2.0, Stroke::new(1.0_f32, ACTIVE_BLUE));
                    } else if row_resp.hovered() {
                        ui.painter()
                            .rect_filled(row_rect, 2.0, Color32::from_rgb(38, 38, 44));
                    }

                    // Checkbox
                    let cb_rect = Rect::from_min_size(
                        pos2(row_rect.left() + 4.0, row_rect.center().y - 6.0),
                        vec2(12.0, 12.0),
                    );
                    let enabled = enabled_flags.get(*map_idx).copied().unwrap_or(false);
                    ui.painter().rect_filled(
                        cb_rect,
                        2.0,
                        if enabled {
                            ACTIVE_BLUE
                        } else {
                            Color32::from_rgb(30, 30, 35)
                        },
                    );
                    ui.painter().rect_stroke(
                        cb_rect,
                        2.0,
                        Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 80)),
                    );
                    if enabled {
                        ui.painter().text(
                            cb_rect.center(),
                            Align2::CENTER_CENTER,
                            "✓",
                            egui::FontId::monospace(9.0),
                            Color32::WHITE,
                        );
                    }

                    // Colored icon dot
                    let dot_pos = pos2(row_rect.left() + 22.0, row_rect.center().y);
                    ui.painter().circle_filled(
                        dot_pos,
                        4.0,
                        Color32::from_rgb(color[0], color[1], color[2]),
                    );

                    // Name
                    ui.painter().text(
                        pos2(row_rect.left() + 32.0, row_rect.center().y),
                        Align2::LEFT_CENTER,
                        *name,
                        egui::FontId::proportional(11.0),
                        Color32::WHITE,
                    );

                    // Status dot
                    let (tag_text, tag_col) = if *is_baked {
                        ("●", Color32::from_rgb(100, 240, 150))
                    } else {
                        ("○", TEXT_DIMMED)
                    };
                    ui.painter().text(
                        pos2(row_rect.right() - 8.0, row_rect.center().y),
                        Align2::RIGHT_CENTER,
                        tag_text,
                        egui::FontId::monospace(10.0),
                        tag_col,
                    );

                    if row_resp.clicked() {
                        if let Some(p) = row_resp.interact_pointer_pos() {
                            if p.x < row_rect.left() + 18.0 {
                                match *map_idx {
                                    0 => self.baker_options.bake_ao = !self.baker_options.bake_ao,
                                    1 => {
                                        self.baker_options.bake_curvature =
                                            !self.baker_options.bake_curvature
                                    }
                                    2 => {
                                        self.baker_options.bake_normal =
                                            !self.baker_options.bake_normal
                                    }
                                    3 => {
                                        self.baker_options.bake_world_space_normal =
                                            !self.baker_options.bake_world_space_normal
                                    }
                                    4 => {
                                        self.baker_options.bake_position =
                                            !self.baker_options.bake_position
                                    }
                                    5 => {
                                        self.baker_options.bake_thickness =
                                            !self.baker_options.bake_thickness
                                    }
                                    6 => self.baker_options.bake_id = !self.baker_options.bake_id,
                                    _ => {}
                                }
                            } else {
                                self.baker_selected_map = *map_idx;
                            }
                        }
                    }
                }

                ui.add_space(8.0);
                ui.separator();

                // Bottom Baking Log inside Left Dock
                ui.strong("BAKING LOG");
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.baker_tab_idx == 0, "PROCESS")
                        .clicked()
                    {
                        self.baker_tab_idx = 0;
                    }
                    if ui
                        .selectable_label(self.baker_tab_idx == 1, "PARAMS")
                        .clicked()
                    {
                        self.baker_tab_idx = 1;
                    }
                    if ui
                        .selectable_label(self.baker_tab_idx == 2, "MATCH")
                        .clicked()
                    {
                        self.baker_tab_idx = 2;
                    }
                });

                egui::Frame::none()
                    .fill(Color32::from_rgb(22, 22, 26))
                    .rounding(3.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.set_min_height(90.0);
                        match self.baker_tab_idx {
                            0 => {
                                if self.is_baking {
                                    let prog = self
                                        .baking_progress
                                        .load(std::sync::atomic::Ordering::Relaxed);
                                    ui.add(
                                        egui::ProgressBar::new(prog as f32 / 100.0)
                                            .text(format!("Baking... {}%", prog)),
                                    );
                                } else {
                                    ui.colored_label(
                                        Color32::from_rgb(100, 240, 150),
                                        "● Ready to bake",
                                    );
                                    ui.colored_label(
                                        TEXT_DIMMED,
                                        format!(
                                            "Mesh: {}
Res: {}×{}",
                                            self.mesh.name,
                                            self.baker_options.resolution,
                                            self.baker_options.resolution
                                        ),
                                    );
                                }
                            }
                            1 => {
                                ui.label(format!("• Cage Front: {:.3}", self.baker_frontal_dist));
                                ui.label(format!("• Cage Rear: {:.3}", self.baker_rear_dist));
                                ui.label("• Tangents: MikkTSpace");
                            }
                            _ => {
                                ui.label(format!(
                                    "• Suffix: '{}' / '{}'",
                                    self.baker_low_suffix, self.baker_high_suffix
                                ));
                            }
                        }
                    });
            });

        // 5. Left-Center SidePanel: Mesh Map Settings
        egui::SidePanel::left("baker_settings_panel").exact_width(340.0).resizable(false).show(ctx, |ui| {
            let active_map_name = match self.baker_selected_map {
                0 => "Ambient occlusion",
                1 => "Curvature",
                2 => "Normal",
                3 => "World space normal",
                4 => "Position",
                5 => "Thickness",
                6 => "ID",
                7 => "Height",
                8 => "Bent normals",
                9 => "Opacity",
                _ => "Common settings",
            };

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.colored_label(ACTIVE_BLUE, &self.mesh.name);
                ui.label("▸");
                ui.colored_label(Color32::WHITE, active_map_name);
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.baker_selected_map == 99 {
                    // COMMON SETTINGS
                    ui.strong("OUTPUT SETTINGS");
                    egui::Grid::new("baker_out_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        ui.label("Output size");
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("baker_out_size_combo")
                                .selected_text(format!("{}", self.baker_options.resolution))
                                .show_ui(ui, |ui| {
                                    let _ = ui.selectable_value(&mut self.baker_options.resolution, 512, "512");
                                    let _ = ui.selectable_value(&mut self.baker_options.resolution, 1024, "1024");
                                    let _ = ui.selectable_value(&mut self.baker_options.resolution, 2048, "2048");
                                    let _ = ui.selectable_value(&mut self.baker_options.resolution, 4096, "4096");
                                });
                            ui.label("🔗");
                            ui.colored_label(TEXT_DIMMED, format!("{}", self.baker_options.resolution));
                        });
                        ui.end_row();

                        ui.label("Dilation width");
                        ui.add(egui::Slider::new(&mut self.baker_frontal_dist, 0.0..=0.50).show_value(true));
                        ui.end_row();

                        ui.label("Apply diffusion");
                        ui.checkbox(&mut self.baker_use_low_as_high, "");
                        ui.end_row();
                    });

                    ui.add_space(6.0);
                    if ui.button("Restore defaults").clicked() {
                        self.baker_options.resolution = 2048;
                        self.baker_frontal_dist = 0.01;
                        self.baker_rear_dist = 0.01;
                    }
                    ui.add_space(8.0);

                    ui.strong("HIGH DEFINITION MESHES");
                    ui.separator();

                    if let Some(ref hp) = self.baker_high_poly_mesh {
                        let hp_name = hp.name.clone();
                        let hp_tris = hp.indices.len() / 3;
                        let hp_verts = hp.vertices.len();
                        let mut remove_hp = false;
                        egui::Frame::none()
                            .fill(Color32::from_rgb(30, 45, 60))
                            .rounding(4.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(Color32::from_rgb(100, 240, 150), "● High Poly Active");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button(egui::RichText::new("× Remove").color(Color32::from_rgb(255, 100, 100))).clicked() {
                                            remove_hp = true;
                                        }
                                    });
                                });
                                ui.label(egui::RichText::new(&hp_name).color(Color32::WHITE).strong());
                                ui.label(format!("Triangles: {} | Vertices: {}", hp_tris, hp_verts));
                                ui.colored_label(TEXT_DIMMED, "High-to-low raycast projection enabled");
                            });
                        if remove_hp {
                            self.baker_high_poly_mesh = None;
                            self.baker_high_poly_path.clear();
                            self.baker_use_low_as_high = true;
                        }
                    } else {
                        ui.checkbox(&mut self.baker_use_low_as_high, "Use Low Poly Mesh as High Poly Mesh");
                        ui.add_space(4.0);

                        let add_hp_btn = egui::Button::new(
                            egui::RichText::new("📂 Add High-Poly Mesh (OBJ/FBX)...").color(Color32::WHITE).strong()
                        ).fill(Color32::from_rgb(45, 75, 115)).min_size(vec2(ui.available_width(), 28.0));

                        if ui.add(add_hp_btn).on_hover_text("Load high-poly geometry to project normals, curvature, and detail onto low-poly UVs").clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .set_title("Select High Poly Mesh for Baking")
                                .add_filter("3D Models (*.obj, *.fbx, *.gltf, *.glb)", &["obj", "OBJ", "fbx", "FBX", "gltf", "GLTF", "glb", "GLB"])
                                .pick_file()
                            {
                                match Mesh::load_file(&p) {
                                    Ok(mut hp) => {
                                        hp.normalize_to_unit();
                                        self.baker_high_poly_path = p.display().to_string();
                                        self.baker_high_poly_mesh = Some(hp);
                                        self.baker_use_low_as_high = false;
                                        self.status_message = format!("Loaded High Poly: {}", p.display());
                                    }
                                    Err(e) => {
                                        self.status_message = format!("Failed to load high poly mesh: {}", e);
                                    }
                                }
                            }
                        }
                    }

                    ui.add_space(6.0);
                    egui::Grid::new("baker_hp_params_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        ui.label("Cage");
                        egui::ComboBox::from_id_salt("baker_cage_combo")
                            .selected_text("Distance based")
                            .show_ui(ui, |ui| {
                                let _ = ui.selectable_label(true, "Distance based");
                                let _ = ui.selectable_label(false, "Cage mesh");
                            });
                        ui.end_row();

                        ui.label("Max frontal distance");
                        if ui.add(egui::Slider::new(&mut self.baker_frontal_dist, 0.001..=0.50).show_value(true)).changed() {
                            self.baker_options.max_frontal_distance = self.baker_frontal_dist;
                        }
                        ui.end_row();

                        ui.label("Max rear distance");
                        if ui.add(egui::Slider::new(&mut self.baker_rear_dist, 0.001..=0.50).show_value(true)).changed() {
                            self.baker_options.max_rear_distance = self.baker_rear_dist;
                        }
                        ui.end_row();

                        ui.label("Relative to bbox");
                        ui.checkbox(&mut self.baker_relative_to_bbox, "");
                        ui.end_row();

                        ui.label("Average normals");
                        ui.checkbox(&mut self.baker_average_normals, "");
                        ui.end_row();

                        ui.label("Ignore backfaces");
                        ui.checkbox(&mut self.baker_ignore_backfaces, "");
                        ui.end_row();

                        ui.label("Use Skew Correction");
                        if ui.checkbox(&mut self.baker_skew_enabled, "").on_hover_text("Shears the high-poly sample position using a 2-channel skew texture (USE_SKEW_MAP parity)").changed() {
                            self.baker_options.use_skew_correction = self.baker_skew_enabled && self.baker_skew_map.is_some();
                        }
                        ui.end_row();

                        ui.label("Skew texture");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Load Skew Texture…").clicked() {
                                    if let Some(path) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "tga", "bmp"]).pick_file() {
                                        match image::open(&path) {
                                            Ok(img) => {
                                                let rgba = img.to_rgba8();
                                                let (w, h) = (rgba.width(), rgba.height());
                                                let data: Vec<[f32; 2]> = rgba.into_raw().chunks_exact(4)
                                                    .map(|p| [(p[0] as f32 / 255.0) * 2.0 - 1.0, (p[1] as f32 / 255.0) * 2.0 - 1.0])
                                                    .collect();
                                                self.baker_skew_map = Some(std::sync::Arc::new(crate::baker::engine::SkewMap { width: w, height: h, data }));
                                                self.baker_skew_map_path = path.display().to_string();
                                                self.baker_options.skew_map = self.baker_skew_map.clone();
                                                self.baker_options.use_skew_correction = self.baker_skew_enabled && self.baker_skew_map.is_some();
                                                self.status_message = format!("Skew texture loaded: {}", path.display());
                                            }
                                            Err(e) => self.logs.push((chrono::Local::now().format("%H:%M:%S").to_string(), "ERROR".into(), format!("Skew texture load failed: {}", e))),
                                        }
                                    }
                                }
                                if self.baker_skew_map.is_some() {
                                    ui.colored_label(egui::Color32::from_rgb(100, 240, 150), "● Loaded");
                                } else if self.baker_skew_enabled {
                                    ui.colored_label(SMART_MAT_ORANGE, "Needs texture");
                                }
                            });
                            if !self.baker_skew_map_path.is_empty() {
                                ui.label(egui::RichText::new(&self.baker_skew_map_path).small().color(TEXT_DIMMED.clone()));
                            }
                        });
                        ui.end_row();

                        ui.label("Invert skew correction");
                        if ui.checkbox(&mut self.baker_skew_invert, "").changed() {
                            self.baker_options.skew_invert = self.baker_skew_invert;
                        }
                        ui.end_row();

                        ui.label("Skew strength");
                        if ui.add(egui::Slider::new(&mut self.baker_skew_strength, 0.0..=2.0).show_value(true)).changed() {
                            self.baker_options.skew_strength = self.baker_skew_strength;
                        }
                        ui.end_row();

                        ui.label("Antialiasing");
                        egui::ComboBox::from_id_salt("baker_aa_combo")
                            .selected_text(match self.baker_subsampling {
                                0 => "None",
                                1 => "Subsampling 2x2",
                                2 => "Subsampling 4x4",
                                _ => "Subsampling 8x8",
                            })
                            .show_ui(ui, |ui| {
                                let _ = ui.selectable_value(&mut self.baker_subsampling, 0, "None");
                                let _ = ui.selectable_value(&mut self.baker_subsampling, 1, "Subsampling 2x2");
                                let _ = ui.selectable_value(&mut self.baker_subsampling, 2, "Subsampling 4x4");
                                let _ = ui.selectable_value(&mut self.baker_subsampling, 3, "Subsampling 8x8");
                            });
                        ui.end_row();

                        ui.label("Match");
                        egui::ComboBox::from_id_salt("baker_match_combo")
                            .selected_text(if self.baker_options.match_by_name { "By Mesh Name" } else { "Always" })
                            .show_ui(ui, |ui| {
                                let _ = ui.selectable_value(&mut self.baker_options.match_by_name, false, "Always");
                                let _ = ui.selectable_value(&mut self.baker_options.match_by_name, true, "By Mesh Name");
                            });
                        ui.end_row();

                        if self.baker_options.match_by_name {
                            ui.label("Low poly mesh suffix");
                            ui.text_edit_singleline(&mut self.baker_low_suffix);
                            ui.end_row();

                            ui.label("High poly mesh suffix");
                            ui.text_edit_singleline(&mut self.baker_high_suffix);
                            ui.end_row();

                            // reference parity: nearest-shell fallback when the
                            // name suffixes cannot pair a low/high shell pair.
                            let _ = ui.checkbox(&mut self.baker_options.match_by_shell, "Match by Shell (name miss fallback)");
                            ui.end_row();
                        }

                        ui.label("Output Depth");
                        egui::ComboBox::from_id_salt("baker_depth_combo")
                            .selected_text(match self.baker_options.map_format {
                                crate::baker::engine::BakedMapFormat::U8 => "8-bit Unorm",
                                crate::baker::engine::BakedMapFormat::F16 => "16-bit Float (Half)",
                                crate::baker::engine::BakedMapFormat::F32 => "32-bit Float",
                            })
                            .show_ui(ui, |ui| {
                                use crate::baker::engine::BakedMapFormat;
                                let _ = ui.selectable_value(&mut self.baker_options.map_format, BakedMapFormat::F32, "32-bit Float");
                                let _ = ui.selectable_value(&mut self.baker_options.map_format, BakedMapFormat::F16, "16-bit Float (Half)");
                                let _ = ui.selectable_value(&mut self.baker_options.map_format, BakedMapFormat::U8, "8-bit Unorm");
                            });
                        ui.end_row();
                    });
                } else {
                    // MAP-SPECIFIC PARAMETERS
                    let (map_type, map_name, output_filename) = match self.baker_selected_map {
                        0 => (MapType::AmbientOcclusion, "Ambient Occlusion", format!("{}_ambient_occlusion.png", self.mesh.name)),
                        1 => (MapType::Curvature, "Curvature", format!("{}_curvature.png", self.mesh.name)),
                        2 => (MapType::Normal, "Normal (Tangent Space)", format!("{}_normal_base.png", self.mesh.name)),
                        3 => (MapType::WorldSpaceNormal, "World Space Normal", format!("{}_world_space_normals.png", self.mesh.name)),
                        4 => (MapType::Position, "Position", format!("{}_position.png", self.mesh.name)),
                        5 => (MapType::Thickness, "Thickness", format!("{}_thickness.png", self.mesh.name)),
                        6 => (MapType::Id, "ID Map", format!("{}_id.png", self.mesh.name)),
                        _ => (MapType::Normal, "Mesh Map", format!("{}_map.png", self.mesh.name)),
                    };

                    ui.horizontal(|ui| {
                        ui.strong(map_name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn = egui::Button::new(
                                egui::RichText::new("⚡ Bake This Map Only").strong().color(Color32::WHITE)
                            ).fill(Color32::from_rgb(45, 95, 175));
                            if ui.add(btn).clicked() {
                                self.bake_single_map(map_type);
                            }
                        });
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Output map filename:");
                        ui.colored_label(Color32::WHITE, output_filename);
                    });
                    ui.add_space(8.0);

                    match self.baker_selected_map {
                        0 => {
                            ui.strong("AMBIENT OCCLUSION PARAMETERS");
                            egui::Grid::new("baker_ao_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                                ui.label("Secondary Rays / Samples");
                                ui.add(egui::Slider::new(&mut self.baker_options.ao_samples, 8..=256));
                                ui.end_row();

                                ui.label("Max Search Distance");
                                ui.add(egui::Slider::new(&mut self.baker_options.ao_max_distance, 0.1..=10.0));
                                ui.end_row();

                                ui.label("Spread Angle");
                                ui.add(egui::Slider::new(&mut self.baker_options.ao_spread, 0.1..=1.0));
                                ui.end_row();

                                ui.label("Distribution");
                                ui.label("Cosine Weighted (Hemispherical)");
                                ui.end_row();

                                ui.label("Self-Occlusion");
                                egui::ComboBox::from_id_salt("ao_self_occ")
                                    .selected_text("Always")
                                    .show_ui(ui, |ui| {
                                        let _ = ui.selectable_label(true, "Always");
                                        let _ = ui.selectable_label(false, "Only Same Mesh Name");
                                    });
                                ui.end_row();
                            });
                        }
                        1 => {
                            ui.strong("CURVATURE PARAMETERS");
                            egui::Grid::new("baker_curv_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                                ui.label("Intensity");
                                ui.add(egui::Slider::new(&mut self.baker_options.curvature_intensity, 0.1..=3.0));
                                ui.end_row();

                                ui.label("Method");
                                ui.label("Smooth Sobel");
                                ui.end_row();
                            });
                        }
                        2 => {
                            ui.strong("NORMAL MAP PARAMETERS");
                            egui::Grid::new("baker_nrm_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                                ui.label("Tangent Basis");
                                ui.label("MikkTSpace 1.0 (DirectX / OpenGL standard)");
                                ui.end_row();

                                ui.label("Invert Green (DirectX)");
                                ui.checkbox(&mut self.baker_options.invert_normal_y, "Flip Y Channel");
                                ui.end_row();
                            });
                        }
                        3 => {
                            ui.strong("WORLD SPACE NORMAL PARAMETERS");
                            ui.label("Up Vector: Y-Up (OpenGL standard)");
                            ui.label("Coordinate Frame: Normalized World Space XYZ");
                        }
                        4 => {
                            ui.strong("POSITION PARAMETERS");
                            ui.label("Normalization: Full Bounding Box [0.0 to 1.0]");
                            ui.label("Bit Depth: RGB 16-bit Floating Point");
                        }
                        5 => {
                            ui.strong("THICKNESS PARAMETERS");
                            ui.label("Subsurface Rays: 32 Samples");
                            ui.label("Raymarch: Inverted Normal Penetration");
                        }
                        _ => {
                            ui.strong("ID MAP PARAMETERS");
                            ui.label("Color Source: Sub-Mesh & Material Groups");
                            ui.label("Palette: Distinct High-Contrast IDs");
                        }
                    }
                }
            });
        });

        // 6. Central Panel: 3D Viewport with Cage & Baking Visualization
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter();
            painter.rect_filled(rect, 0.0, Color32::from_rgb(26, 26, 28));

            self.render_3d_viewport(painter, rect, &response, ctx);

            // Floating Baking Visualization Panel (top-left)
            let vis_rect = Rect::from_min_size(
                pos2(rect.left() + 16.0, rect.top() + 14.0),
                vec2(240.0, 100.0),
            );
            painter.rect_filled(vis_rect, 4.0, Color32::from_black_alpha(220));
            painter.rect_stroke(
                vis_rect,
                4.0,
                Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 65)),
            );
            painter.text(
                pos2(vis_rect.left() + 12.0, vis_rect.top() + 12.0),
                Align2::LEFT_TOP,
                "⚡ Baking visualization",
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
            painter.text(
                pos2(vis_rect.left() + 12.0, vis_rect.top() + 34.0),
                Align2::LEFT_TOP,
                "● Show for selected Texture Set only",
                egui::FontId::proportional(10.5),
                Color32::from_rgb(180, 180, 190),
            );
            painter.text(
                pos2(vis_rect.left() + 12.0, vis_rect.top() + 54.0),
                Align2::LEFT_TOP,
                format!("● Cage: Distance-based ({:.3})", self.baker_frontal_dist),
                egui::FontId::proportional(10.5),
                ACTIVE_BLUE,
            );
            painter.text(
                pos2(vis_rect.left() + 12.0, vis_rect.top() + 74.0),
                Align2::LEFT_TOP,
                "● UV Seams / Projection: Active",
                egui::FontId::proportional(10.5),
                Color32::from_rgb(100, 240, 150),
            );

            // Floating Action Bar (bottom-right)
            let action_rect = Rect::from_min_size(
                pos2(rect.right() - 440.0, rect.bottom() - 46.0),
                vec2(420.0, 36.0),
            );
            painter.rect_filled(action_rect, 4.0, Color32::from_black_alpha(230));
            painter.rect_stroke(
                action_rect,
                4.0,
                Stroke::new(1.0_f32, Color32::from_rgb(65, 65, 75)),
            );

            let bake_btn_rect = Rect::from_min_size(
                pos2(action_rect.left() + 6.0, action_rect.top() + 4.0),
                vec2(210.0, 28.0),
            );
            let return_btn_rect = Rect::from_min_size(
                pos2(action_rect.left() + 224.0, action_rect.top() + 4.0),
                vec2(190.0, 28.0),
            );

            let mut bake_hovered = false;
            let mut return_hovered = false;
            if let Some(pos) = ctx.pointer_hover_pos() {
                bake_hovered = bake_btn_rect.contains(pos);
                return_hovered = return_btn_rect.contains(pos);
            }

            let is_baking = self.is_baking;
            let current_prog = self.baking_progress.load(Ordering::Relaxed);

            let bake_fill = if is_baking {
                if bake_hovered {
                    Color32::from_rgb(240, 70, 70)
                } else {
                    Color32::from_rgb(200, 50, 50)
                }
            } else if bake_hovered {
                Color32::from_rgb(65, 135, 235)
            } else {
                ACTIVE_BLUE
            };

            painter.rect_filled(bake_btn_rect, 3.0, bake_fill);
            painter.rect_filled(
                return_btn_rect,
                3.0,
                if return_hovered {
                    Color32::from_rgb(55, 55, 60)
                } else {
                    Color32::from_rgb(40, 40, 45)
                },
            );

            let bake_label = if is_baking {
                format!("⏹ Cancel Bake ({}%)", current_prog)
            } else {
                "⚡ Bake Selected Textures".to_string()
            };
            painter.text(
                bake_btn_rect.center(),
                Align2::CENTER_CENTER,
                bake_label,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
            painter.text(
                return_btn_rect.center(),
                Align2::CENTER_CENTER,
                "Return to painting mode",
                egui::FontId::proportional(11.5),
                Color32::WHITE,
            );

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if bake_btn_rect.contains(pos) {
                        if is_baking {
                            self.cancel_baking.store(true, Ordering::Relaxed);
                        } else {
                            self.bake_mesh_maps();
                        }
                    } else if return_btn_rect.contains(pos) {
                        self.show_baker_dialog = false;
                    }
                }
            }
        });
    }

    fn ui_export_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_export_dialog;
        let mut do_export = false;
        let mut close_dialog = false;

        egui::Window::new("Export Textures — REMoAINTER")
            .open(&mut show)
            .default_width(650.0)
            .default_height(480.0)
            .min_width(550.0)
            .resizable(true)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Target Mesh:").strong());
                    ui.colored_label(Color32::WHITE, &self.mesh.name);
                    ui.label("|");
                    ui.label("Resolution:");
                    ui.colored_label(ACTIVE_BLUE, format!("{}×{}", self.layer_stack.width, self.layer_stack.height));
                });
                ui.separator();

                egui::Grid::new("export_settings_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        // Output Directory
                        ui.label(egui::RichText::new("Output Directory:").strong());
                        ui.horizontal(|ui| {
                            let dir_str = self.export_output_dir.display().to_string();
                            ui.label(egui::RichText::new(&dir_str).monospace().color(TEXT_PRIMARY));
                            if ui.button("Browse...").clicked() {
                                if let Some(folder) = rfd::FileDialog::new()
                                    .set_title("Select Export Directory")
                                    .pick_folder()
                                {
                                    self.export_output_dir = folder;
                                }
                            }
                        });
                        ui.end_row();

                        // Export Preset
                        ui.label(egui::RichText::new("Output Template / Preset:").strong());
                        let preset_names = [
                            "Unreal Engine 4/5 (Packed ORM: BaseColor, ORM, Normal DirectX)",
                            "Unity Universal Render Pipeline (Metallic / Smoothness)",
                            "Unity High Definition Render Pipeline (MaskMap)",
                            "glTF 2.0 (Metallic-Roughness Standard)",
                            "Generic PBR (Individual Split Maps)",
                            "VFX / Film Studio (32-bit OpenEXR)",
                        ];
                        egui::ComboBox::from_id_salt("export_preset_combo")
                            .selected_text(preset_names.get(self.export_preset_idx).copied().unwrap_or("Custom"))
                            .width(380.0)
                            .show_ui(ui, |ui| {
                                for (i, name) in preset_names.iter().enumerate() {
                                    ui.selectable_value(&mut self.export_preset_idx, i, *name);
                                }
                            });
                        ui.end_row();

                        // Export Resolution Override
                        ui.label(egui::RichText::new("Export Resolution:").strong());
                        egui::ComboBox::from_id_salt("export_res_combo")
                            .selected_text(format!("{}×{}", self.export_resolution, self.export_resolution))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.export_resolution, 512, "512×512");
                                ui.selectable_value(&mut self.export_resolution, 1024, "1024×1024");
                                ui.selectable_value(&mut self.export_resolution, 2048, "2048×2048");
                                ui.selectable_value(&mut self.export_resolution, 4096, "4096×4096");
                            });
                        ui.end_row();

                        // File Format
                        ui.label(egui::RichText::new("File Format:").strong());
                        let formats = [
                            "Auto (Preset Default)",
                            "PNG (8-bit)",
                            "PNG (16-bit High-Precision)",
                            "TGA (8-bit Uncompressed)",
                            "OpenEXR (32-bit Float)",
                            "JPEG (8-bit Compressed)",
                        ];
                        egui::ComboBox::from_id_salt("export_fmt_combo")
                            .selected_text(formats.get(self.export_file_format).copied().unwrap_or("PNG"))
                            .show_ui(ui, |ui| {
                                for (i, name) in formats.iter().enumerate() {
                                    ui.selectable_value(&mut self.export_file_format, i, *name);
                                }
                            });
                        ui.end_row();

                        // Dilation / Seam Padding
                        ui.label(egui::RichText::new("Dilation / Seam Padding:").strong());
                        ui.horizontal(|ui| {
                            ui.add(egui::Slider::new(&mut self.export_padding, 0..=64).text("pixels"));
                        });
                        ui.end_row();

                        // Padding algorithm (reference parity: transparent / color / diffusion / passthrough)
                        ui.label(egui::RichText::new("Padding Algorithm:").strong());
                        let padding_modes = ["Color (Dilate)", "Transparent", "Diffusion", "Passthrough"];
                        egui::ComboBox::from_id_salt("export_padding_algo_combo")
                            .selected_text(padding_modes.get(self.export_padding_algorithm as usize).copied().unwrap_or("Color"))
                            .show_ui(ui, |ui| {
                                for (i, name) in padding_modes.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.export_padding_algorithm,
                                        i as u32,
                                        *name,
                                    );
                                }
                            });
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.separator();

                // Channel Packing Overview Card
                ui.strong("PACKED MAP SUMMARY");
                egui::Frame::none()
                    .fill(Color32::from_rgb(26, 26, 30))
                    .rounding(4.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        match self.export_preset_idx {
                            0 => {
                                ui.label(egui::RichText::new("• BaseColor: RGB (sRGB Color)").color(Color32::from_rgb(228, 110, 60)));
                                ui.label(egui::RichText::new("• ORM Packed: R = Ambient Occlusion, G = Roughness, B = Metallic").color(Color32::from_rgb(100, 240, 150)));
                                ui.label(egui::RichText::new("• Normal: Tangent Space (DirectX Inverted -Y)").color(Color32::from_rgb(140, 120, 255)));
                            }
                            1 => {
                                ui.label(egui::RichText::new("• AlbedoTransparency: RGB (Albedo) + A (Opacity)").color(Color32::from_rgb(228, 110, 60)));
                                ui.label(egui::RichText::new("• MetallicSmoothness: R (Metallic) + A (Smoothness = 1 - Roughness)").color(Color32::from_rgb(150, 200, 255)));
                                ui.label(egui::RichText::new("• Normal: Tangent Space (OpenGL +Y)").color(Color32::from_rgb(140, 120, 255)));
                            }
                            2 => {
                                ui.label(egui::RichText::new("• BaseColor: RGB (sRGB)").color(Color32::from_rgb(228, 110, 60)));
                                ui.label(egui::RichText::new("• MaskMap: R = Metallic, G = AO, B = Thickness/Detail, A = Smoothness").color(Color32::from_rgb(100, 240, 150)));
                                ui.label(egui::RichText::new("• Normal: Tangent Space (OpenGL +Y)").color(Color32::from_rgb(140, 120, 255)));
                            }
                            3 => {
                                ui.label(egui::RichText::new("• baseColorTexture: RGB (sRGB) + A (Opacity)").color(Color32::from_rgb(228, 110, 60)));
                                ui.label(egui::RichText::new("• metallicRoughnessTexture: G = Roughness, B = Metallic (R = AO)").color(Color32::from_rgb(100, 240, 150)));
                                ui.label(egui::RichText::new("• normalTexture: Tangent Space (OpenGL +Y)").color(Color32::from_rgb(140, 120, 255)));
                            }
                            5 => {
                                ui.label(egui::RichText::new("• BaseColor: 32-bit Float Linear OpenEXR (RGBA)").color(Color32::from_rgb(228, 110, 60)));
                                ui.label(egui::RichText::new("• Normal: 32-bit Float Linear OpenEXR (RGB)").color(Color32::from_rgb(140, 120, 255)));
                                ui.label(egui::RichText::new("• ORM: 32-bit Float Linear OpenEXR (R=AO, G=Roughness, B=Metallic)").color(Color32::from_rgb(100, 240, 150)));
                            }
                            _ => {
                                ui.label("• BaseColor: RGB 8-bit");
                                ui.label("• Metallic: Grayscale L8");
                                ui.label("• Roughness: Grayscale L8");
                                ui.label("• Normal: RGB Tangent Space (OpenGL 16-bit)");
                                ui.label("• Height: Grayscale L16");
                                ui.label("• Ambient Occlusion: Grayscale L8");
                            }
                        }
                    });

                if !self.export_status.is_empty() {
                    ui.add_space(6.0);
                    ui.colored_label(Color32::from_rgb(100, 240, 150), &self.export_status);
                }

                ui.add_space(8.0);
                ui.separator();

                // Modal Footer
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let export_btn = egui::Button::new(
                            egui::RichText::new("📤 Export Textures")
                                .strong()
                                .color(Color32::WHITE)
                                .size(13.0)
                        ).fill(ACTIVE_BLUE).min_size(vec2(160.0, 30.0));

                        if ui.add(export_btn).clicked() {
                            do_export = true;
                        }
                    });
                });
            });

        if do_export {
            self.export_textures();
        }
        if close_dialog {
            show = false;
        }
        self.show_export_dialog = show;
    }

    fn ui_texture_importer_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_texture_importer_dialog;
        let mut close_dialog = false;
        egui::Window::new("Import PBR Texture Set — REMoAINTER")
            .open(&mut show)
            .default_width(620.0)
            .resizable(true)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Texture Set / Channel Auto-Matching");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("📄 Pick Files...")
                            .on_hover_text("Select multiple image files to auto-match by suffix")
                            .clicked()
                        {
                            if let Some(files) = rfd::FileDialog::new().pick_files() {
                                self.import_texture_set = PbrTextureSet::from_files(&files);
                                self.import_flip_normal_y =
                                    self.import_texture_set.is_directx_normal;
                            }
                        }
                        if ui
                            .button("📁 Pick Folder...")
                            .on_hover_text("Select folder containing texture pack to auto-match")
                            .clicked()
                        {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.import_texture_set = PbrTextureSet::from_folder(&folder);
                                self.import_flip_normal_y =
                                    self.import_texture_set.is_directx_normal;
                            }
                        }
                    });
                });
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Material Name:");
                    ui.text_edit_singleline(&mut self.import_texture_set.name);
                });
                ui.add_space(6.0);

                egui::Grid::new("pbr_import_grid")
                    .num_columns(3)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        // Base Color
                        ui.label(egui::RichText::new("Base Color / Albedo:").strong());
                        let bc_str = self
                            .import_texture_set
                            .base_color
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("[Not Assigned]");
                        ui.colored_label(
                            if self.import_texture_set.base_color.is_some() {
                                Color32::from_rgb(58, 217, 115)
                            } else {
                                TEXT_DIMMED
                            },
                            bc_str,
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Browse...").clicked() {
                                if let Some(f) = rfd::FileDialog::new().pick_file() {
                                    self.import_texture_set.base_color = Some(f);
                                }
                            }
                            if self.import_texture_set.base_color.is_some()
                                && ui.button("✕").clicked()
                            {
                                self.import_texture_set.base_color = None;
                            }
                        });
                        ui.end_row();

                        // Normal
                        ui.label(egui::RichText::new("Normal Map:").strong());
                        let n_str = self
                            .import_texture_set
                            .normal
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("[Not Assigned]");
                        ui.colored_label(
                            if self.import_texture_set.normal.is_some() {
                                Color32::from_rgb(58, 217, 115)
                            } else {
                                TEXT_DIMMED
                            },
                            n_str,
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Browse...").clicked() {
                                if let Some(f) = rfd::FileDialog::new().pick_file() {
                                    self.import_texture_set.normal = Some(f);
                                }
                            }
                            if self.import_texture_set.normal.is_some() && ui.button("✕").clicked()
                            {
                                self.import_texture_set.normal = None;
                            }
                            ui.checkbox(&mut self.import_flip_normal_y, "DirectX (Flip Y)");
                        });
                        ui.end_row();

                        // Roughness
                        ui.label(egui::RichText::new("Roughness:").strong());
                        let r_str = self
                            .import_texture_set
                            .roughness
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("[Not Assigned]");
                        ui.colored_label(
                            if self.import_texture_set.roughness.is_some() {
                                Color32::from_rgb(58, 217, 115)
                            } else {
                                TEXT_DIMMED
                            },
                            r_str,
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Browse...").clicked() {
                                if let Some(f) = rfd::FileDialog::new().pick_file() {
                                    self.import_texture_set.roughness = Some(f);
                                }
                            }
                            if self.import_texture_set.roughness.is_some()
                                && ui.button("✕").clicked()
                            {
                                self.import_texture_set.roughness = None;
                            }
                        });
                        ui.end_row();

                        // Metallic
                        ui.label(egui::RichText::new("Metallic:").strong());
                        let m_str = self
                            .import_texture_set
                            .metallic
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("[Not Assigned]");
                        ui.colored_label(
                            if self.import_texture_set.metallic.is_some() {
                                Color32::from_rgb(58, 217, 115)
                            } else {
                                TEXT_DIMMED
                            },
                            m_str,
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Browse...").clicked() {
                                if let Some(f) = rfd::FileDialog::new().pick_file() {
                                    self.import_texture_set.metallic = Some(f);
                                }
                            }
                            if self.import_texture_set.metallic.is_some()
                                && ui.button("✕").clicked()
                            {
                                self.import_texture_set.metallic = None;
                            }
                        });
                        ui.end_row();

                        // Ambient Occlusion
                        ui.label(egui::RichText::new("Ambient Occlusion:").strong());
                        let ao_str = self
                            .import_texture_set
                            .ambient_occlusion
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("[Not Assigned]");
                        ui.colored_label(
                            if self.import_texture_set.ambient_occlusion.is_some() {
                                Color32::from_rgb(58, 217, 115)
                            } else {
                                TEXT_DIMMED
                            },
                            ao_str,
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Browse...").clicked() {
                                if let Some(f) = rfd::FileDialog::new().pick_file() {
                                    self.import_texture_set.ambient_occlusion = Some(f);
                                }
                            }
                            if self.import_texture_set.ambient_occlusion.is_some()
                                && ui.button("✕").clicked()
                            {
                                self.import_texture_set.ambient_occlusion = None;
                            }
                        });
                        ui.end_row();

                        // Height
                        ui.label(egui::RichText::new("Height / Displacement:").strong());
                        let h_str = self
                            .import_texture_set
                            .height
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("[Not Assigned]");
                        ui.colored_label(
                            if self.import_texture_set.height.is_some() {
                                Color32::from_rgb(58, 217, 115)
                            } else {
                                TEXT_DIMMED
                            },
                            h_str,
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Browse...").clicked() {
                                if let Some(f) = rfd::FileDialog::new().pick_file() {
                                    self.import_texture_set.height = Some(f);
                                }
                            }
                            if self.import_texture_set.height.is_some() && ui.button("✕").clicked()
                            {
                                self.import_texture_set.height = None;
                            }
                        });
                        ui.end_row();
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("✔ Apply to Active Layer").strong())
                        .clicked()
                    {
                        if let Some(active) = self
                            .layer_stack
                            .layers
                            .get_mut(self.layer_stack.active_layer_index)
                        {
                            let _ = self
                                .import_texture_set
                                .load_into_layer(active, self.import_flip_normal_y);
                            if let Some(ao) = self
                                .import_texture_set
                                .load_ao_map(self.layer_stack.width, self.layer_stack.height)
                            {
                                self.baked_maps.ambient_occlusion = Some(ao);
                            }
                            self.recompute_composite();
                            self.status_message =
                                format!("Applied Texture Set: {}", self.import_texture_set.name);
                            self.history.push(format!(
                                "{:02}  Import Texture Set ({})",
                                self.history.len() + 1,
                                self.import_texture_set.name
                            ));
                        }
                        close_dialog = true;
                    }
                    if ui.button("➕ Create Fill Layer & Apply").clicked() {
                        let id = self.layer_stack.layers.len() + 1;
                        let mut layer = Layer::new_fill(
                            id,
                            &self.import_texture_set.name,
                            [0.8, 0.8, 0.8, 1.0],
                            0.0,
                            0.5,
                            self.layer_stack.width,
                            self.layer_stack.height,
                        );
                        let _ = self
                            .import_texture_set
                            .load_into_layer(&mut layer, self.import_flip_normal_y);
                        if let Some(ao) = self
                            .import_texture_set
                            .load_ao_map(self.layer_stack.width, self.layer_stack.height)
                        {
                            self.baked_maps.ambient_occlusion = Some(ao);
                        }
                        self.layer_stack.layers.push(layer);
                        self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                        self.recompute_composite();
                        self.status_message =
                            format!("Created Fill Layer with: {}", self.import_texture_set.name);
                        self.history.push(format!(
                            "{:02}  Import Texture Set ({})",
                            self.history.len() + 1,
                            self.import_texture_set.name
                        ));
                        close_dialog = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            });
        if close_dialog {
            show = false;
        }
        self.show_texture_importer_dialog = show;
    }

    fn ui_new_project_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_new_project_dialog;
        let mut do_create = false;
        let mut close_dialog = false;

        egui::Window::new("New project")
            .open(&mut show)
            .default_width(520.0)
            .min_width(450.0)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                egui::Grid::new("new_project_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        // 1. Template
                        ui.label(egui::RichText::new("Template:").strong());
                        let templates = [
                            "PBR - Metallic / Roughness (Allegorithmic)",
                            "PBR - Metallic / Roughness Alpha-blend",
                            "PBR - Specular / Glossiness",
                        ];
                        egui::ComboBox::from_id_salt("new_proj_template")
                            .selected_text(
                                templates
                                    .get(self.new_proj_template_idx)
                                    .copied()
                                    .unwrap_or("PBR - Metallic / Roughness"),
                            )
                            .width(320.0)
                            .show_ui(ui, |ui| {
                                for (i, t) in templates.iter().enumerate() {
                                    ui.selectable_value(&mut self.new_proj_template_idx, i, *t);
                                }
                            });
                        ui.end_row();

                        // 2. File
                        ui.label(egui::RichText::new("File:").strong());
                        ui.horizontal(|ui| {
                            let disp_path = if self.new_proj_mesh_path.is_empty() {
                                "Default Material Sphere".to_string()
                            } else {
                                self.new_proj_mesh_path.clone()
                            };
                            ui.add(
                                egui::Label::new(egui::RichText::new(&disp_path).color(
                                    if self.new_proj_mesh_path.is_empty() {
                                        TEXT_DIMMED
                                    } else {
                                        Color32::WHITE
                                    },
                                ))
                                .truncate(),
                            );
                            if ui.button("Select...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_title("Select 3D Mesh for Project")
                                    .add_filter("3D Mesh", &["obj", "OBJ", "fbx", "FBX"])
                                    .pick_file()
                                {
                                    self.new_proj_mesh_path = path.display().to_string();
                                }
                            }
                        });
                        ui.end_row();

                        // 3. Document Resolution
                        ui.label(egui::RichText::new("Document resolution:").strong());
                        egui::ComboBox::from_id_salt("new_proj_res")
                            .selected_text(format!(
                                "{}×{}",
                                self.new_proj_resolution, self.new_proj_resolution
                            ))
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.new_proj_resolution, 512, "512");
                                ui.selectable_value(&mut self.new_proj_resolution, 1024, "1024");
                                ui.selectable_value(&mut self.new_proj_resolution, 2048, "2048");
                                ui.selectable_value(&mut self.new_proj_resolution, 4096, "4096");
                            });
                        ui.end_row();

                        // 4. Normal Map Format
                        ui.label(egui::RichText::new("Normal map format:").strong());
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.new_proj_normal_directx, true, "DirectX");
                            ui.radio_value(&mut self.new_proj_normal_directx, false, "OpenGL");
                        });
                        ui.end_row();

                        // 5. Tangent space
                        ui.label(egui::RichText::new("Compute tangent space:").strong());
                        let mut per_frag = true;
                        ui.checkbox(&mut per_frag, "Per fragment");
                        ui.end_row();

                        // 6. UV Tile (UDIM)
                        ui.label(egui::RichText::new("UV Tile settings:").strong());
                        ui.checkbox(&mut self.new_proj_udim, "Create a texture set per UV tile");
                        ui.end_row();

                        // 7. Cameras
                        ui.label(egui::RichText::new("Import cameras:").strong());
                        ui.checkbox(&mut self.new_proj_import_cameras, "");
                        ui.end_row();
                    });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ok_btn = egui::Button::new(
                            egui::RichText::new("   OK   ")
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(ACTIVE_BLUE)
                        .min_size(vec2(80.0, 26.0));
                        if ui.add(ok_btn).clicked() {
                            do_create = true;
                            close_dialog = true;
                        }

                        if ui
                            .add(egui::Button::new("Cancel").min_size(vec2(80.0, 26.0)))
                            .clicked()
                        {
                            close_dialog = true;
                        }
                    });
                });
            });

        if do_create {
            // Load selected mesh or default sphere
            let loaded_mesh = if !self.new_proj_mesh_path.is_empty() {
                match Mesh::load_obj(&self.new_proj_mesh_path) {
                    Ok(mut m) => {
                        m.normalize_to_unit();
                        m
                    }
                    Err(e) => {
                        self.status_message = format!("Failed to load mesh: {}", e);
                        Mesh::create_material_sphere(64, 64)
                    }
                }
            } else {
                Mesh::create_material_sphere(64, 64)
            };

            self.mesh = loaded_mesh;
            self.selected_model_idx = if self.new_proj_mesh_path.is_empty() {
                0
            } else {
                99
            };
            self.project_name = if !self.new_proj_mesh_path.is_empty() {
                std::path::Path::new(&self.new_proj_mesh_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string()
            } else {
                "Untitled Project".to_string()
            };
            self.current_project_path = None;
            self.mesh_source_path = if !self.new_proj_mesh_path.is_empty() {
                Some(self.new_proj_mesh_path.clone())
            } else {
                None
            };
            self.project_is_dirty = false;

            // Resize LayerStack and reset to fresh clean base PBR layers
            let res = self.new_proj_resolution;
            self.layer_stack.width = res;
            self.layer_stack.height = res;
            self.layer_stack.layers.clear();

            let base_mat = load_real_pbr_material(
                1,
                "Steel Painted",
                [0.75, 0.76, 0.78, 1.0],
                0.0,
                0.5,
                res,
                res,
            );
            let paint_layer = Layer::new_paint(2, "Paint Layer 1", res, res);
            self.layer_stack.layers.push(base_mat);
            self.layer_stack.layers.push(paint_layer);
            self.layer_stack.active_layer_index = 1;

            self.baker_options.resolution = res;
            self.baker_options.invert_normal_y = self.new_proj_normal_directx;

            // Fresh bake of all 6 PBR maps
            if !self.mesh.indices.is_empty() {
                self.baked_maps = BakerEngine::bake_all(
                    &self.mesh.vertices,
                    &self.mesh.normals,
                    &self.mesh.uvs,
                    &self.mesh.indices,
                    &self.mesh.submeshes,
                    &self.baker_options,
                    None,
                    None,
                    None,
                );
            }

            // Sync Vulkan Viewport mesh
            if let Some(ref vp_mutex) = self.viewport_wgpu {
                if let (Some(ref device), Some(ref queue)) = (&self.wgpu_device, &self.wgpu_queue) {
                    if let Ok(mut vp) = vp_mutex.lock() {
                        vp.set_mesh(device, queue, &self.mesh);
                    }
                }
            }

            self.recompute_composite();
            self.texture_needs_update = true;
            self.viewport_view_mode = 2; // Split 3D | 2D view
            self.status_message =
                format!("New Project created: {} ({}x{})", self.mesh.name, res, res);
            self.history.clear();
            self.history.push("01  New Project".into());
        }

        if close_dialog {
            show = false;
        }
        self.show_new_project_dialog = show;
    }

    fn ui_snapshots_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_snapshots_dialog;
        let mut to_restore: Option<usize> = None;
        let mut capture_now = false;

        egui::Window::new("Project Snapshots & Autosave Recovery")
            .open(&mut show)
            .default_width(520.0)
            .min_width(420.0)
            .resizable(true)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("PROJECT SNAPSHOTS");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("📷 Capture Snapshot").strong().color(Color32::WHITE)).clicked() {
                            capture_now = true;
                        }
                    });
                });
                ui.separator();

                // Autosave recovery section
                if let Some(autosave_path) = crate::project::ProjectManager::check_autosave() {
                    egui::Frame::none()
                        .fill(Color32::from_rgb(30, 45, 60))
                        .rounding(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(Color32::from_rgb(100, 240, 150), "● Autosave File Detected");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("Restore Autosave").clicked() {
                                        if let Ok((manifest, mesh, layer_stack, baked_maps, baker_options)) = crate::project::ProjectManager::load_project(&autosave_path) {
                                            self.project_name = manifest.project_name;
                                            self.mesh = mesh;
                                            self.layer_stack = layer_stack;
                                            self.baked_maps = baked_maps;
                                            self.baker_options = baker_options;
                                            self.texture_needs_update = true;
                                            if let Some(ref vp_mutex) = self.viewport_wgpu {
                                                if let (Some(ref device), Some(ref queue)) = (&self.wgpu_device, &self.wgpu_queue) {
                                                    if let Ok(mut vp) = vp_mutex.lock() {
                                                        vp.set_mesh(device, queue, &self.mesh);
                                                    }
                                                }
                                            }
                                            self.recompute_composite();
                                            self.status_message = "Restored project from Autosave".into();
                                            self.logs.push((
                                                chrono::Local::now().format("%H:%M:%S").to_string(),
                                                "INFO".into(),
                                                "Restored active session from ~/.remoainter/autosave.rmp".into(),
                                            ));
                                        }
                                    }
                                });
                            });
                            ui.label(egui::RichText::new(autosave_path.display().to_string()).size(11.0).color(TEXT_DIMMED));
                        });
                    ui.add_space(6.0);
                }

                ui.strong("SESSION SNAPSHOTS");
                if self.snapshots.is_empty() {
                    ui.colored_label(TEXT_DIMMED, "No project snapshots captured yet in this session.");
                } else {
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for (i, snap) in self.snapshots.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}.", i + 1));
                                ui.colored_label(Color32::WHITE, &snap.name);
                                ui.colored_label(TEXT_DIMMED, format!("({})", snap.timestamp));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("Restore").clicked() {
                                        to_restore = Some(i);
                                    }
                                });
                            });
                            ui.separator();
                        }
                    });
                }
            });

        if let Some(idx) = to_restore {
            self.restore_snapshot(idx);
        }
        if capture_now {
            let name = format!("Snapshot {}", self.snapshots.len() + 1);
            self.take_snapshot(&name);
        }
        self.show_snapshots_dialog = show;
    }

    fn ui_preferences_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_preferences_dialog;
        let mut close_dialog = false;
        egui::Window::new("⚙ Preferences — REMoAINTER 2026.1")
            .open(&mut show)
            .resizable(false)
            .default_width(480.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("APPLICATION PREFERENCES");
                ui.separator();

                egui::Grid::new("pref_grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Autosave Interval:");
                        ui.add(
                            egui::Slider::new(&mut self.config.autosave_interval_secs, 30..=600)
                                .suffix(" s"),
                        );
                        ui.end_row();

                        ui.label("Undo History Depth:");
                        ui.add(
                            egui::Slider::new(&mut self.config.undo_depth, 16..=256)
                                .suffix(" steps"),
                        );
                        ui.end_row();

                        ui.label("Default Export Path:");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.config.default_export_dir);
                            if ui.button("Browse...").clicked() {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    self.config.default_export_dir = folder.display().to_string();
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Default Bake Res:");
                        egui::ComboBox::from_id_salt("pref_bake_res")
                            .selected_text(format!(
                                "{}×{}",
                                self.config.default_bake_resolution,
                                self.config.default_bake_resolution
                            ))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.config.default_bake_resolution,
                                    512,
                                    "512×512",
                                );
                                ui.selectable_value(
                                    &mut self.config.default_bake_resolution,
                                    1024,
                                    "1024×1024",
                                );
                                ui.selectable_value(
                                    &mut self.config.default_bake_resolution,
                                    2048,
                                    "2048×2048",
                                );
                                ui.selectable_value(
                                    &mut self.config.default_bake_resolution,
                                    4096,
                                    "4096×4096",
                                );
                            });
                        ui.end_row();

                        ui.label("Compositing Engine:");
                        ui.label("CPU (built-in blend pipeline)");
                        ui.end_row();

                        ui.label("Color Management:");
                        egui::ComboBox::from_id_salt("pref_color_mgmt")
                            .selected_text(match self.config.color_management_mode {
                                0 => "ACES Filmic (Default)",
                                1 => "sRGB Display (Raw Linear Transfer)",
                                _ => "Neutral Reinhard (Soft Highlight Shoulder)",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.config.color_management_mode,
                                    0,
                                    "ACES Filmic (reference PBR suite Look)",
                                );
                                ui.selectable_value(
                                    &mut self.config.color_management_mode,
                                    1,
                                    "sRGB Display (Raw Linear Transfer)",
                                );
                                ui.selectable_value(
                                    &mut self.config.color_management_mode,
                                    2,
                                    "Neutral Reinhard (Soft Highlight Shoulder)",
                                );
                            });
                        ui.end_row();
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save & Apply").clicked() {
                        self.autosave_interval_secs = self.config.autosave_interval_secs;
                        self.history_manager.max_undo_steps = self.config.undo_depth;
                        self.export_output_dir = PathBuf::from(&self.config.default_export_dir);
                        self.color_management_mode = self.config.color_management_mode;
                        let _ = self.config.save();
                        self.status_message =
                            "Preferences saved to ~/.remoainter/config.json".into();
                        close_dialog = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Reset to Defaults").clicked() {
                            self.config = AppConfig::default();
                        }
                    });
                });
            });
        if close_dialog {
            show = false;
        }
        self.show_preferences_dialog = show;
    }

    fn ui_hotkey_editor_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_hotkeys_dialog;
        let mut close_dialog = false;
        egui::Window::new("⌨ Hotkey Editor — REMoAINTER")
            .open(&mut show)
            .default_width(520.0)
            .default_height(400.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("KEYBOARD SHORTCUTS TABLE");
                ui.label(
                    RichText::new("reference PBR suite 1:1 Parity Keybindings").color(TEXT_DIMMED),
                );
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        egui::Grid::new("hotkey_grid")
                            .num_columns(3)
                            .striped(true)
                            .spacing([24.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Command").strong());
                                ui.label(RichText::new("Keybinding").strong());
                                ui.label(RichText::new("Scope").strong());
                                ui.end_row();

                                let mut sorted_keys: Vec<_> =
                                    self.config.custom_hotkeys.keys().cloned().collect();
                                sorted_keys.sort();

                                for cmd in sorted_keys {
                                    let key = self
                                        .config
                                        .custom_hotkeys
                                        .get(&cmd)
                                        .cloned()
                                        .unwrap_or_default();
                                    ui.label(&cmd);
                                    ui.colored_label(ACTIVE_BLUE, &key);
                                    ui.label("Global");
                                    ui.end_row();
                                }
                            });
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Restore Defaults").clicked() {
                        self.config.custom_hotkeys = AppConfig::default().custom_hotkeys;
                        let _ = self.config.save();
                        self.status_message = "Restored default hotkeys".into();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close_dialog = true;
                        }
                    });
                });
            });
        if close_dialog {
            show = false;
        }
        self.show_hotkeys_dialog = show;
    }

    fn ui_shortcuts_help_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_shortcuts_dialog;
        let mut close_dialog = false;
        egui::Window::new("📖 Documentation & Shortcuts Reference")
            .open(&mut show)
            .default_width(540.0)
            .default_height(480.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("REMoAINTER 2026.1 SHORTCUT CHEATSHEET");
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.collapsing("3D Viewport Navigation", |ui| {
                        egui::Grid::new("nav_grid")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                ui.colored_label(ACTIVE_BLUE, "Alt + LMB");
                                ui.label("Orbit camera around 3D mesh target");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Alt + MMB");
                                ui.label("Pan camera horizontally and vertically");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Alt + RMB / Wheel");
                                ui.label("Smooth zoom camera distance");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Shift + RMB");
                                ui.label("Rotate HDR Studio Environment lighting");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "F");
                                ui.label("Frame 3D Mesh to viewport center");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Y");
                                ui.label("Toggle Turntable continuous showcase orbit");
                                ui.end_row();
                            });
                    });

                    ui.collapsing("Tool Selection", |ui| {
                        egui::Grid::new("tool_grid")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                ui.colored_label(ACTIVE_BLUE, "1");
                                ui.label("Paint Brush Tool");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "2");
                                ui.label("Physical Eraser Tool");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "3");
                                ui.label("Projection Painting Tool");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "4");
                                ui.label("Polygon Fill (Triangle / UV Island / Mesh)");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "5");
                                ui.label("Smudge Tool");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "6");
                                ui.label("Clone Stamp (Alt+LMB to set source)");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "I");
                                ui.label("Color Eyedropper / Surface Material Sampler");
                                ui.end_row();
                            });
                    });

                    ui.collapsing("Display Channels & View Modes", |ui| {
                        egui::Grid::new("ch_grid")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                ui.colored_label(ACTIVE_BLUE, "M");
                                ui.label("Material Mode (Full Lit PBR GGX)");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "C");
                                ui.label("Cycle Solo PBR Channels (Color, Rough, Metal, Normal)");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "B");
                                ui.label("Cycle Mesh Maps (AO, Curvature, Position, Thickness)");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "F1");
                                ui.label("Split 3D | 2D View");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "F2");
                                ui.label("3D Viewport Only");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "F3");
                                ui.label("2D UV Texture Map View Only");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "F4");
                                ui.label("Toggle Wireframe Overlay");
                                ui.end_row();
                            });
                    });

                    ui.collapsing("Project, Baking & Export", |ui| {
                        egui::Grid::new("proj_grid")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + N");
                                ui.label("New Project Dialog");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + O");
                                ui.label("Open .rmp Project Archive");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + S");
                                ui.label("Save Active Project");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + Shift + S");
                                ui.label("Save Project As");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + R");
                                ui.label("Reload 3D Mesh Geometry from Source");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + Shift + B");
                                ui.label("Bake Mesh Maps Studio");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + Shift + E");
                                ui.label("Export Textures (Unreal/Unity/glTF)");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + 1..9");
                                ui.label("Save Camera Bookmark slot 1..9");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Alt + 1..9");
                                ui.label("Recall Camera Bookmark slot 1..9");
                                ui.end_row();
                            });
                    });

                    ui.collapsing("Layers & History", |ui| {
                        egui::Grid::new("layer_grid")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + Z");
                                ui.label("Undo last paint stroke or property change");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + Y");
                                ui.label("Redo undone action");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + G");
                                ui.label("Group selected layer into Folder");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + C");
                                ui.label("Copy active layer to clipboard");
                                ui.end_row();
                                ui.colored_label(ACTIVE_BLUE, "Ctrl + V");
                                ui.label("Paste layer from clipboard");
                                ui.end_row();
                            });
                    });
                });

                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close_dialog = true;
                    }
                });
            });
        if close_dialog {
            show = false;
        }
        self.show_shortcuts_dialog = show;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_bookmarks_save_restore() {
        let mut bookmarks: [Option<CameraBookmark>; 10] = Default::default();

        bookmarks[1] = Some(CameraBookmark {
            name: "Front Angle".into(),
            yaw: 45.0,
            pitch: 15.0,
            dist: 2.5,
        });

        bookmarks[2] = Some(CameraBookmark {
            name: "Top Down".into(),
            yaw: 0.0,
            pitch: 85.0,
            dist: 1.8,
        });

        assert!(bookmarks[1].is_some());
        assert!(bookmarks[2].is_some());
        assert!(bookmarks[3].is_none());

        let bm1 = bookmarks[1].as_ref().unwrap();
        assert_eq!(bm1.yaw, 45.0);
        assert_eq!(bm1.pitch, 15.0);
        assert_eq!(bm1.dist, 2.5);

        let bm2 = bookmarks[2].as_ref().unwrap();
        assert_eq!(bm2.yaw, 0.0);
        assert_eq!(bm2.pitch, 85.0);
        assert_eq!(bm2.dist, 1.8);
    }

    #[test]
    fn test_turntable_rotation_step() {
        let mut yaw: f32 = 350.0;
        let speed: f32 = 45.0;
        let dt: f32 = 0.5;

        yaw = (yaw + speed * dt).rem_euclid(360.0);
        assert!((yaw - 12.5).abs() < 1e-4);
    }

    #[test]
    fn test_layer_copy_paste_clipboard() {
        let mut app = RemoainterApp::default();
        let initial_count = app.layer_stack.layers.len();
        assert!(initial_count >= 1);

        app.copy_active_layer();
        assert!(app.layer_clipboard.is_some());

        app.paste_layer();
        assert_eq!(app.layer_stack.layers.len(), initial_count + 1);
    }

    #[test]
    fn test_layer_cut_clipboard() {
        let mut app = RemoainterApp::default();
        let initial_count = app.layer_stack.layers.len();
        assert!(initial_count >= 2);

        app.cut_active_layer();
        assert!(app.layer_clipboard.is_some());
        assert_eq!(app.layer_stack.layers.len(), initial_count - 1);

        app.paste_layer();
        assert_eq!(app.layer_stack.layers.len(), initial_count);
    }

    #[test]
    fn test_reset_layout_state() {
        let mut app = RemoainterApp::default();
        app.active_right_panel = 6;
        app.viewport_view_mode = 0;
        app.camera_yaw = 180.0;
        app.active_tool = ToolMode::Eraser;

        app.reset_layout();
        assert_eq!(app.active_right_panel, 2);
        assert_eq!(app.viewport_view_mode, 2);
        assert_eq!(app.camera_yaw, 35.0);
        assert_eq!(app.active_tool, ToolMode::Paint);
    }

    #[test]
    fn test_shelf_assets_and_environments_are_discovered() {
        let assets = scan_shelf_assets();
        assert!(
            !assets.is_empty(),
            "shelf_assets must discover bundled materials"
        );
        assert!(
            assets.iter().any(|a| a.category == 2),
            "must have materials (cat 2)"
        );
        assert!(
            assets.iter().any(|a| a.category == 1),
            "must have smart materials (cat 1)"
        );
        assert!(
            assets.iter().any(|a| a.category == 3),
            "must have smart masks (cat 3)"
        );
        assert!(
            assets.iter().any(|a| a.category == 4),
            "must have alphas (cat 4)"
        );

        let envs = list_available_environments();
        assert!(
            !envs.is_empty(),
            "environments must discover bundled HDR/PNG environments"
        );
    }
}
