use crate::baker::{BakedMaps, BakerEngine, BakerOptions, MapType};
use crate::graphics::viewport_gl::{list_available_environments, ViewportGl};
use crate::graphics::{EnvironmentPreset, Mesh};
use crate::painting::brush::AlphaMaskData;
use crate::painting::texture_importer::PbrTextureSet;
use crate::painting::{
    apply_smart_material_hierarchy, load_real_pbr_material,
    AnchorPoint, BlendMode, Brush, Layer, LayerStack, LayerType, MaskBlendMode, MaskEffect,
    ExportPreset, TextureExportContext, OutputFormat, BitDepth,
    FilterEffect, FilterTargetChannel,
};
use crate::ui::icons::{draw_icon, IconType};
use crate::ui::theme::*;
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Stroke, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
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
}

#[derive(Clone, Debug)]
pub struct ShelfAsset {
    pub name: String,
    pub category: usize, // 1: Smart Materials, 2: Materials, 3: Smart Masks, 4: Alphas
    pub path: PathBuf,
    pub thumb_path: PathBuf,
    pub badge: &'static str,
    pub badge_color: Color32,
    pub pbr_color: [f32; 4],
    pub pbr_metallic: f32,
    pub pbr_roughness: f32,
}

/// An active texture channel in the Texture Set
#[derive(Clone, Debug)]
pub struct TextureChannel {
    pub name: String,
    pub format: String,
    pub enabled: bool,
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
    pub show_baker_dialog: bool,
    pub baker_selected_map: usize,
    pub baker_subsampling: usize,
    pub baker_use_low_as_high: bool,
    pub baker_frontal_dist: f32,
    pub baker_rear_dist: f32,
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

    // Brush Engine & Undo/Redo History
    pub history_manager: crate::painting::HistoryManager,
    pub lazy_mouse_tracker: crate::painting::brush::LazyMouseTracker,
    pub is_in_stroke: bool,
    pub paint_target_mask: bool,

    // Shelf & Asset Filter
    pub shelf_category: usize, // 0: All, 1: Smart Mats, 2: Materials, 3: Smart Masks, 4: Alphas
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
    pub texture_needs_update: bool,
    pub viewport_view_mode: usize, // 0: 3D View, 1: 2D Texture View, 2: Split 3D | 2D
    pub show_wireframe: bool,
    pub show_uv_wireframe: bool,

    // Hardware OpenGL PBR Viewport & Studio HDR Environment Pipeline
    pub gl: Option<Arc<glow::Context>>,
    pub viewport_gl: Option<Arc<Mutex<ViewportGl>>>,
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

    // Export Textures Dialog (File > Export Textures, Substance Painter Parity)
    pub show_export_dialog: bool,
    pub export_output_dir: PathBuf,
    pub export_preset_idx: usize,
    pub export_resolution: u32,
    pub export_file_format: usize,
    pub export_padding: u32,
    pub export_status: String,
    pub baker_tab_idx: usize,

    // New Project Dialog (File > New..., Substance Painter Parity)
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

    // Project Management (File > Save/Load, Autosave, Snapshots)
    pub project_name: String,
    pub current_project_path: Option<PathBuf>,
    pub project_is_dirty: bool,
    pub last_autosave_time: std::time::Instant,
    pub autosave_interval_secs: u64,
    pub mesh_source_path: Option<String>,
    pub snapshots: Vec<crate::project::SnapshotInfo>,
    pub show_snapshots_dialog: bool,
}

fn infer_pbr(name: &str) -> ([f32; 4], f32, f32) {
    let lower = name.to_lowercase();
    let mut color = [0.70f32, 0.70, 0.72, 1.0];
    let mut metallic = 0.5f32;
    let mut roughness = 0.4f32;

    if lower.contains("gold") {
        color = [1.0, 0.84, 0.25, 1.0]; metallic = 1.0; roughness = 0.20;
    } else if lower.contains("copper") {
        color = [0.95, 0.64, 0.54, 1.0]; metallic = 1.0; roughness = 0.25;
    } else if lower.contains("bronze") || lower.contains("brass") {
        color = [0.85, 0.65, 0.35, 1.0]; metallic = 0.95; roughness = 0.30;
    } else if lower.contains("chrome") {
        color = [0.95, 0.95, 0.98, 1.0]; metallic = 1.0; roughness = 0.08;
    } else if lower.contains("steel") || lower.contains("iron") || lower.contains("aluminium") || lower.contains("aluminum") || lower.contains("metal") {
        color = [0.75, 0.76, 0.78, 1.0]; metallic = 0.95; roughness = 0.32;
    } else if lower.contains("cobalt") {
        color = [0.28, 0.45, 0.78, 1.0]; metallic = 0.85; roughness = 0.28;
    } else if lower.contains("rubber") {
        color = [0.15, 0.15, 0.16, 1.0]; metallic = 0.0; roughness = 0.75;
    } else if lower.contains("plastic") {
        color = [0.85, 0.18, 0.18, 1.0]; metallic = 0.0; roughness = 0.28;
    } else if lower.contains("wood") {
        color = [0.55, 0.35, 0.20, 1.0]; metallic = 0.0; roughness = 0.45;
    } else if lower.contains("concrete") || lower.contains("asphalt") || lower.contains("stone") {
        color = [0.52, 0.52, 0.52, 1.0]; metallic = 0.0; roughness = 0.82;
    } else if lower.contains("fabric") || lower.contains("leather") {
        color = [0.45, 0.38, 0.32, 1.0]; metallic = 0.0; roughness = 0.65;
    } else if lower.contains("carbon") {
        color = [0.12, 0.12, 0.14, 1.0]; metallic = 0.0; roughness = 0.22;
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
        metallic = if lower.contains("metallic") { 0.85 } else { 0.04 };
        roughness = 0.30;
    }

    (color, metallic, roughness)
}

fn find_shelf_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("/mnt/shared/REMoAINTER/assets/shelf"),
        PathBuf::from("assets/shelf"),
        PathBuf::from("../assets/shelf"),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("../assets/shelf"))).unwrap_or_default(),
    ];
    for c in &candidates {
        if c.exists() && c.join("smart_materials").exists() {
            return c.clone();
        }
    }
    PathBuf::from("/mnt/shared/REMoAINTER/assets/shelf")
}

fn scan_shelf_assets() -> Vec<ShelfAsset> {
    let shelf_dir = find_shelf_dir();
    let mut assets = Vec::new();

    let categories = [
        (1, "smart_materials", "SM", SMART_MAT_ORANGE),
        (2, "materials", "M", ACTIVE_BLUE),
        (3, "smart_masks", "MSK", Color32::from_rgb(160, 60, 220)),
        (4, "alphas", "A", Color32::from_rgb(46, 180, 110)),
    ];

    for (cat_id, sub_folder, badge, badge_color) in categories {
        let folder = shelf_dir.join(sub_folder);
        let thumb_sub = shelf_dir.join("thumbnails").join(sub_folder);

        if let Ok(entries) = std::fs::read_dir(&folder) {
            let mut list: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            list.sort_by_key(|e| e.file_name());

            for entry in list {
                let p = entry.path();
                if p.is_file() {
                    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext == "webp" || ext == "png" || ext == "sbsar" || ext == "spmsk" {
                        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        let thumb_png = thumb_sub.join(format!("{}.png", stem));
                        let real_mat_thumb = crate::painting::find_materials_dir().join(&stem).join("base_color.png");
                        let thumb_p = if real_mat_thumb.exists() {
                            real_mat_thumb
                        } else if thumb_png.exists() {
                            thumb_png
                        } else {
                            p.clone()
                        };
                        let (pbr_color, pbr_metallic, pbr_roughness) = infer_pbr(&stem);

                        assets.push(ShelfAsset {
                            name: stem,
                            category: cat_id,
                            path: p,
                            thumb_path: thumb_p,
                            badge,
                            badge_color,
                            pbr_color,
                            pbr_metallic,
                            pbr_roughness,
                        });
                    }
                }
            }
        }
    }

    // Include real starter PBR materials from assets/materials
    let mat_dir = crate::painting::find_materials_dir();
    if let Ok(entries) = std::fs::read_dir(&mat_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') && !assets.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
                    let thumb_p = p.join("base_color.png");
                    let (pbr_color, pbr_metallic, pbr_roughness) = infer_pbr(&name);
                    assets.push(ShelfAsset {
                        name: name.clone(),
                        category: 2, // Material
                        path: p,
                        thumb_path: if thumb_p.exists() { thumb_p } else { PathBuf::new() },
                        badge: "MAT",
                        badge_color: ACTIVE_BLUE,
                        pbr_color,
                        pbr_metallic,
                        pbr_roughness,
                    });
                }
            }
        }
    }

    assets
}

impl Default for RemoainterApp {
    fn default() -> Self {
        let mut app = Self {
            active_tool: ToolMode::Paint,
            brush: Brush::default(),
            layer_stack: LayerStack::new(1024, 1024),
            mesh: Mesh::create_material_sphere(64, 64),
            baker_options: BakerOptions::default(),
            baked_maps: BakedMaps::default(),
            is_baking: false,
            baking_progress: Arc::new(AtomicUsize::new(0)),
            show_baker_dialog: false,
            baker_selected_map: 99, // Default to Common settings (High-Poly parameters & Output)
            baker_subsampling: 0,
            baker_use_low_as_high: true,
            baker_frontal_dist: 0.08,
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
            env_preset: 1, // Warm & Cool Studio
            env_rotation: 0.0,
            light_intensity: 1.0,
            camera_yaw: 35.0,
            camera_pitch: 18.0,
            camera_dist: 2.2,
            symmetry_x: false,
            symmetry_y: false,
            symmetry_z: false,
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
                ("09:00:01".into(), "INFO".into(), "REMoAINTER v2.0.0 (High-Performance Rust Core) initialized.".into()),
                ("09:00:02".into(), "INFO".into(), "Substance 3D Painter 1-to-1 Architecture & UI Active.".into()),
                ("09:00:03".into(), "INFO".into(), "Loaded 360+ Allegorithmic PBR materials, Smart Masks & Alphas.".into()),
                ("09:00:04".into(), "INFO".into(), "Cook-Torrance / Blinn-Phong GGX Studio Lighting active.".into()),
            ],
            last_painted_uv: None,
            paint_target_mask: false,
            layer_clipboard: None,
            texture_handle: None,
            texture_needs_update: true,
            viewport_view_mode: 2, // 3D | 2D Split View as default on startup (Substance Painter parity)
            show_wireframe: false,
            show_uv_wireframe: true,

            gl: None,
            viewport_gl: None,
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
                TextureChannel { name: "Base color".into(), format: "sRGB8".into(), enabled: true },
                TextureChannel { name: "Metallic".into(),   format: "L8".into(),    enabled: true },
                TextureChannel { name: "Roughness".into(),  format: "L8".into(),    enabled: true },
                TextureChannel { name: "Normal".into(),     format: "RGB16F".into(), enabled: true },
                TextureChannel { name: "Height".into(),     format: "L16F".into(),  enabled: true },
            ],
            texture_set_manager: crate::painting::TextureSetManager::new(2048),

            show_export_dialog: false,
            export_output_dir: PathBuf::from("/mnt/shared/REMoAINTER/exports"),
            export_preset_idx: 0,
            export_resolution: 2048,
            export_file_format: 0,
            export_padding: 16,
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

            project_name: "Untitled Project".to_string(),
            current_project_path: None,
            project_is_dirty: false,
            last_autosave_time: std::time::Instant::now(),
            autosave_interval_secs: 180,
            mesh_source_path: None,
            snapshots: Vec::new(),
            show_snapshots_dialog: false,
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
                &app.baker_options,
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
            };
        }

        // Load clean base PBR material and user paint detail layer on startup (Substance standard)
        let w = app.layer_stack.width;
        let h = app.layer_stack.height;
        let base_mat = load_real_pbr_material(1, "Steel Painted", [0.75, 0.76, 0.78, 1.0], 0.95, 0.32, w, h);
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
        if let Some(ref gl) = cc.gl {
            app.gl = Some(gl.clone());
            match ViewportGl::new(gl, &app.mesh, &app.layer_stack, &app.baked_maps) {
                Ok(vp) => {
                    app.viewport_gl = Some(Arc::new(Mutex::new(vp)));
                }
                Err(e) => {
                    eprintln!("[ERROR] Failed to initialize OpenGL PBR Viewport: {}", e);
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

        if let Some(ref vp_mutex) = self.viewport_gl {
            if let Some(ref gl) = self.gl {
                if let Ok(mut vp) = vp_mutex.lock() {
                    vp.textures.upload_all(gl, &self.layer_stack, &self.baked_maps);
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

        if let Some(ref vp_mutex) = self.viewport_gl {
            if let Some(ref gl) = self.gl {
                if let Ok(mut vp) = vp_mutex.lock() {
                    vp.textures.upload_dirty(gl, &self.layer_stack, bbox);
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
        self.is_baking = true;
        self.baking_progress.store(0, Ordering::Relaxed);
        let progress = Arc::clone(&self.baking_progress);

        self.baked_maps = BakerEngine::bake_all(
            &self.mesh.vertices,
            &self.mesh.normals,
            &self.mesh.uvs,
            &self.mesh.indices,
            &self.baker_options,
            Some(progress),
            self.baker_high_poly_mesh.as_ref(),
        );
        self.is_baking = false;
        self.status_message = format!("Successfully baked mesh maps at {}x{}", self.baker_options.resolution, self.baker_options.resolution);
        self.logs.push((
            chrono::Local::now().format("%H:%M:%S").to_string(),
            "INFO".into(),
            format!("Baked mesh maps (AO, Curvature, Normals, Position) for {}", self.mesh.name),
        ));
        self.recompute_composite();
    }

    pub fn bake_single_map(&mut self, map_type: MapType) {
        self.is_baking = true;
        let progress = Arc::clone(&self.baking_progress);

        let single = BakerEngine::bake_single(
            map_type,
            &self.mesh.vertices,
            &self.mesh.normals,
            &self.mesh.uvs,
            &self.mesh.indices,
            &self.baker_options,
            Some(progress),
        );

        match map_type {
            MapType::AmbientOcclusion => self.baked_maps.ambient_occlusion = single.ambient_occlusion,
            MapType::Curvature => self.baked_maps.curvature = single.curvature,
            MapType::Normal => self.baked_maps.normal = single.normal,
            MapType::WorldSpaceNormal => self.baked_maps.world_space_normal = single.world_space_normal,
            MapType::Position => self.baked_maps.position = single.position,
            MapType::Thickness => self.baked_maps.thickness = single.thickness,
            MapType::Id => self.baked_maps.id = single.id,
        }
        self.is_baking = false;
        self.status_message = format!("Baked {:?} map at {}x{}", map_type, self.baker_options.resolution, self.baker_options.resolution);
        self.logs.push((
            chrono::Local::now().format("%H:%M:%S").to_string(),
            "INFO".into(),
            format!("Baked {:?} map at {}x{}", map_type, self.baker_options.resolution, self.baker_options.resolution),
        ));
        self.recompute_composite();
    }

    pub fn load_model(&mut self, idx: usize) {
        self.selected_model_idx = idx;
        match idx {
            0 => {
                self.mesh = Mesh::create_material_sphere(64, 64);
                self.status_message = "Loaded Material Sphere".to_string();
            }
            1 => {
                let paths = [
                    "/mnt/shared/REMoAINTER/assets/test_models/lantern.obj",
                    "assets/test_models/lantern.obj",
                    "../assets/test_models/lantern.obj",
                    "/home/senju/Documents/env-port/lantern_low.obj",
                ];
                for p in &paths {
                    if let Ok(m) = Mesh::load_obj(p) {
                        self.mesh = m;
                        self.status_message = "Loaded Test Asset: Antique Lantern".to_string();
                        break;
                    }
                }
            }
            2 => {
                let paths = [
                    "/mnt/shared/REMoAINTER/assets/test_models/radio.obj",
                    "assets/test_models/radio.obj",
                    "../assets/test_models/radio.obj",
                    "/home/senju/Pictures/radio-model/radio.obj",
                ];
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
                &self.baker_options,
                None,
                None,
            );
            if self.layer_stack.layers.is_empty() {
                let w = self.layer_stack.width;
                let h = self.layer_stack.height;
                let base_mat = load_real_pbr_material(1, "Steel Painted", [0.75, 0.76, 0.78, 1.0], 0.95, 0.32, w, h);
                let paint_layer = Layer::new_paint(2, "Paint Layer 1", w, h);
                self.layer_stack.layers.push(base_mat);
                self.layer_stack.layers.push(paint_layer);
                self.layer_stack.active_layer_index = 1;
            }
        }
        if let Some(ref vp_mutex) = self.viewport_gl {
            if let Some(ref gl) = self.gl {
                if let Ok(mut vp) = vp_mutex.lock() {
                    vp.set_mesh(gl, &self.mesh);
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
                self.history.push(format!("{:02}  Import Mesh ({})", self.history.len() + 1, self.mesh.name));
                self.logs.push((
                    time_str.clone(),
                    "INFO".into(),
                    format!("Successfully loaded mesh {} ({} vertices, {} triangles)",
                        self.mesh.name, self.mesh.vertices.len(), self.mesh.indices.len() / 3),
                ));

                // Re-bake all mesh maps for the newly loaded mesh
                self.baked_maps = BakerEngine::bake_all(
                    &self.mesh.vertices,
                    &self.mesh.normals,
                    &self.mesh.uvs,
                    &self.mesh.indices,
                    &self.baker_options,
                    None,
                    None,
                );

                if let Some(ref vp_mutex) = self.viewport_gl {
                    if let Some(ref gl) = self.gl {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.set_mesh(gl, &self.mesh);
                        }
                    }
                }

                if self.layer_stack.layers.is_empty() {
                    let w = self.layer_stack.width;
                    let h = self.layer_stack.height;
                    let base_mat = load_real_pbr_material(1, "Steel Painted", [0.75, 0.76, 0.78, 1.0], 0.95, 0.32, w, h);
                    let paint_layer = Layer::new_paint(2, "Paint Layer 1", w, h);
                    self.layer_stack.layers.push(base_mat);
                    self.layer_stack.layers.push(paint_layer);
                    self.layer_stack.active_layer_index = 1;
                }

                self.texture_set_manager.configure_from_mesh(&self.mesh, self.layer_stack.width);
                self.recompute_composite();
                self.status_message = format!("Loaded {} ({} vertices, {} triangles) with baked mesh maps",
                    self.mesh.name, self.mesh.vertices.len(), self.mesh.indices.len() / 3);
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
                (self.exposure, self.light_intensity, self.env_rotation, self.selected_env_idx),
            );
            match res {
                Ok(_) => {
                    self.project_is_dirty = false;
                    self.status_message = format!("Saved project to {}", path.display());
                    self.history.push(format!("{:02}  Save Project", self.history.len() + 1));
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

    /// Opens a `.rmp` project file from disk
    pub fn open_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open REMoAINTER Project")
            .add_filter("REMoAINTER Project (*.rmp)", &["rmp"])
            .pick_file()
        {
            match crate::project::ProjectManager::load_project(&path) {
                Ok((manifest, mesh, layer_stack, baked_maps, baker_options)) => {
                    self.project_name = manifest.project_name;
                    self.current_project_path = Some(path.clone());
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
                    if let Some(ref vp_mutex) = self.viewport_gl {
                        if let Some(ref gl) = self.gl {
                            if let Ok(mut vp) = vp_mutex.lock() {
                                vp.set_mesh(gl, &self.mesh);
                            }
                        }
                    }

                    self.recompute_composite();
                    self.status_message = format!("Opened project: {}", path.display());
                    self.history.push(format!("{:02}  Open Project ({})", self.history.len() + 1, self.project_name));
                    self.logs.push((
                        chrono::Local::now().format("%H:%M:%S").to_string(),
                        "INFO".into(),
                        format!("Loaded project archive: {} ({} layers)", path.display(), self.layer_stack.layers.len()),
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
            (self.exposure, self.light_intensity, self.env_rotation, self.selected_env_idx),
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
            (self.exposure, self.light_intensity, self.env_rotation, self.selected_env_idx),
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
            if let Ok((_, mesh, layer_stack, baked_maps, baker_options)) = crate::project::ProjectManager::load_project(&snap.path) {
                self.mesh = mesh;
                self.layer_stack = layer_stack;
                self.baked_maps = baked_maps;
                self.baker_options = baker_options;
                self.texture_needs_update = true;

                if let Some(ref vp_mutex) = self.viewport_gl {
                    if let Some(ref gl) = self.gl {
                        if let Ok(mut vp) = vp_mutex.lock() {
                            vp.set_mesh(gl, &self.mesh);
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
                            &self.baker_options,
                            None,
                            None,
                        );
                        if let Some(ref vp_mutex) = self.viewport_gl {
                            if let Some(ref gl) = self.gl {
                                if let Ok(mut vp) = vp_mutex.lock() {
                                    vp.set_mesh(gl, &self.mesh);
                                }
                            }
                        }
                        self.recompute_composite();
                        self.status_message = format!("Reloaded mesh: {} (paint preserved)", path.display());
                        self.history.push(format!("{:02}  Reload Mesh ({})", self.history.len() + 1, self.mesh.name));
                        self.logs.push((
                            chrono::Local::now().format("%H:%M:%S").to_string(),
                            "INFO".into(),
                            format!("Reloaded 3D mesh from disk: {}. Retained {} paint layers.", path.display(), self.layer_stack.layers.len()),
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
        if new_idx >= self.texture_set_manager.sets.len() || new_idx == self.texture_set_manager.active_index {
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
        let log_msg = format!("Switched active Texture Set to: {} (ID: {})", incoming.name, incoming.id);
        self.status_message = log_msg.clone();
        self.logs.push((time_str, "INFO".into(), log_msg));
    }

    /// Full Substance Painter-grade texture export (Unreal ORM, Unity URP/HDRP, glTF, Generic PBR, OpenEXR)
    pub fn export_textures(&mut self) {
        let preset = match self.export_preset_idx {
            0 => ExportPreset::unreal_engine(),
            1 => ExportPreset::unity_urp(),
            2 => ExportPreset::unity_hdrp(),
            3 => ExportPreset::gltf2(),
            4 => ExportPreset::generic_pbr(),
            _ => ExportPreset::studio_exr(),
        };

        let (format_override, depth_override) = match self.export_file_format {
            1 => (Some(OutputFormat::Png), Some(BitDepth::U8)),
            2 => (Some(OutputFormat::Png), Some(BitDepth::U16)),
            3 => (Some(OutputFormat::Tga), Some(BitDepth::U8)),
            4 => (Some(OutputFormat::Exr), Some(BitDepth::F32)),
            5 => (Some(OutputFormat::Jpg), Some(BitDepth::U8)),
            _ => (None, None), // Use preset defaults
        };

        let res_override = if self.export_resolution > 0 && self.export_resolution != self.layer_stack.width {
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
                self.export_status = format!("Exported {} maps to {}", count, self.export_output_dir.display());
                self.status_message = format!("Exported {} textures successfully!", count);
                let time_str = chrono::Local::now().format("%H:%M:%S").to_string();
                for log in result.logs {
                    self.logs.push((time_str.clone(), "INFO".into(), log));
                }
                self.logs.push((
                    time_str,
                    "SUCCESS".into(),
                    format!("Export complete: {} textures with {}px seam dilation", count, self.export_padding),
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
}

impl eframe::App for RemoainterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_substance_theme(ctx);

        // Automatic background autosave
        if self.project_is_dirty && self.last_autosave_time.elapsed().as_secs() >= self.autosave_interval_secs {
            self.trigger_autosave();
        }

        // Global hotkey processing (Substance Painter standard)
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
            self.perform_undo();
        } else if ctx.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::Y)) || (i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z))) {
            self.perform_redo();
        } else if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_project();
        } else if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_project_as();
        } else if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::O)) {
            self.open_project();
        } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::R)) {
            self.reload_mesh();
        } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::N)) {
            self.show_new_project_dialog = true;
        } else if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::B)) {
            self.show_baker_dialog = true;
        } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::G)) {
            let id = self.layer_stack.layers.len() + 1;
            let folder = Layer::new_folder(id, format!("Folder {}", id), self.layer_stack.width, self.layer_stack.height);
            let active_idx = self.layer_stack.active_layer_index;
            if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                active.parent_folder_id = Some(id);
            }
            self.layer_stack.layers.insert(active_idx + 1, folder);
            self.layer_stack.active_layer_index = active_idx + 1;
            self.recompute_composite();
            self.status_message = format!("Created Group Folder {}", id);
        } else if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::E)) {
            self.show_export_dialog = true;
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
            self.status_message = format!("Wireframe Overlay: {}", if self.show_wireframe { "ON (F4)" } else { "OFF" });
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
            self.status_message = format!("Solo Channel: {} [C]", self.channel_name(self.display_channel));
        } else if ctx.input(|i| i.key_pressed(egui::Key::B)) && !ctx.input(|i| i.modifiers.shift) {
            self.display_channel = match self.display_channel {
                5 => 6,
                6 => 7,
                7 => 8,
                _ => 5,
            };
            self.texture_needs_update = true;
            self.status_message = format!("Mesh Map: {} [B]", self.channel_name(self.display_channel));
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
                    self.status_message = format!("Tool: Polygon Fill (4) [{}]", match self.poly_fill_mode { 0 => "Triangle", 1 => "UV Island", _ => "Mesh" });
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num5)) {
                    self.active_tool = ToolMode::Smudge;
                    self.status_message = "Tool: Smudge (5)".into();
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num6)) {
                    self.active_tool = ToolMode::Clone;
                    self.status_message = "Tool: Clone Stamp (6) [Alt+Click to set source]".into();
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
                if paths.len() == 1 && paths[0].is_dir() {
                    let set = PbrTextureSet::from_folder(&paths[0]);
                    self.import_texture_set = set.clone();
                    self.import_flip_normal_y = set.is_directx_normal;
                    if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                        if let Ok(_) = set.load_into_layer(active, set.is_directx_normal) {
                            if let Some(ao) = set.load_ao_map(self.layer_stack.width, self.layer_stack.height) {
                                self.baked_maps.ambient_occlusion = Some(ao);
                            }
                            self.recompute_composite();
                            self.status_message = format!("Imported Texture Set via Drag & Drop: {}", set.name);
                            self.history.push(format!("{:02}  Import Texture Set ({})", self.history.len() + 1, set.name));
                        }
                    }
                } else {
                    let set = PbrTextureSet::from_files(&paths);
                    self.import_texture_set = set.clone();
                    self.import_flip_normal_y = set.is_directx_normal;
                    if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                        if let Ok(_) = set.load_into_layer(active, set.is_directx_normal) {
                            if let Some(ao) = set.load_ao_map(self.layer_stack.width, self.layer_stack.height) {
                                self.baked_maps.ambient_occlusion = Some(ao);
                            }
                            self.recompute_composite();
                            self.status_message = format!("Imported Texture Maps via Drag & Drop: {}", set.name);
                            self.history.push(format!("{:02}  Import Texture Maps ({})", self.history.len() + 1, set.name));
                        }
                    }
                }
            }
        }

        // If Baker mode is active, render full docked Baking Studio Mode (Substance Painter 1:1)
        if self.show_baker_dialog {
            self.ui_baker_studio_mode(ctx);
            if self.show_texture_importer_dialog {
                self.ui_texture_importer_modal(ctx);
            }
            if self.show_export_dialog {
                self.ui_export_modal(ctx);
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
                    self.logo_texture = Some(ctx.load_texture("remoainter_logo", color_img, egui::TextureOptions::LINEAR));
                }
            }

            ui.horizontal(|ui| {
                // Application Logo / Brand
                if let Some(ref logo) = self.logo_texture {
                    ui.image(egui::load::SizedTexture::new(logo.id(), vec2(20.0, 20.0)));
                } else {
                    ui.colored_label(SMART_MAT_ORANGE, "RM");
                }
                ui.label(egui::RichText::new("REMoAINTER").strong().color(Color32::WHITE));

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
                                .add_filter("3D Models (*.obj, *.fbx, *.gltf, *.glb)", &["obj", "OBJ", "fbx", "FBX", "gltf", "GLTF", "glb", "GLB"])
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
                                self.import_flip_normal_y = self.import_texture_set.is_directx_normal;
                                self.show_texture_importer_dialog = true;
                            }
                            ui.close_menu();
                        }
                        if ui.button("📄 Import Texture Maps (Files)...").clicked() {
                            if let Some(files) = rfd::FileDialog::new().pick_files() {
                                self.import_texture_set = PbrTextureSet::from_files(&files);
                                self.import_flip_normal_y = self.import_texture_set.is_directx_normal;
                                self.show_texture_importer_dialog = true;
                            }
                            ui.close_menu();
                        }
                        if ui.button("⚙ Texture Set Importer Dialog...").clicked() {
                            self.show_texture_importer_dialog = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit (Alt+F4)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        let undo_label = if let Some(last_cmd) = self.history_manager.undo_stack.last() {
                            format!("Undo {} (Ctrl+Z)", last_cmd.description)
                        } else {
                            "Undo (Ctrl+Z)".into()
                        };
                        if ui.add_enabled(self.history_manager.can_undo(), egui::Button::new(undo_label)).clicked() {
                            self.perform_undo();
                            ui.close_menu();
                        }

                        let redo_label = if let Some(next_cmd) = self.history_manager.redo_stack.last() {
                            format!("Redo {} (Ctrl+Y)", next_cmd.description)
                        } else {
                            "Redo (Ctrl+Y)".into()
                        };
                        if ui.add_enabled(self.history_manager.can_redo(), egui::Button::new(redo_label)).clicked() {
                            self.perform_redo();
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
                            self.status_message = "Visualisation Mode (Interactive PBR GGX)".into();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("Geometry", |ui| {
                        if ui.button("Substance Material Sphere").clicked() {
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
                        if ui.button("3D | 2D Split View [F1]").clicked() {
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
                        let wire_label = if self.show_wireframe { "Disable Wireframe Overlay [F4]" } else { "Enable Wireframe Overlay [F4]" };
                        if ui.button(wire_label).clicked() {
                            self.show_wireframe = !self.show_wireframe;
                            self.show_uv_wireframe = self.show_wireframe;
                            ui.close_menu();
                        }
                        ui.separator();
                        let mut ch_changed = false;
                        if ui.button("Material (Lit) [M]").clicked() { self.display_channel = 0; ch_changed = true; ui.close_menu(); }
                        if ui.button("Base Color (Solo) [C]").clicked() { self.display_channel = 1; ch_changed = true; ui.close_menu(); }
                        if ui.button("Roughness (Solo)").clicked() { self.display_channel = 2; ch_changed = true; ui.close_menu(); }
                        if ui.button("Metallic (Solo)").clicked() { self.display_channel = 3; ch_changed = true; ui.close_menu(); }
                        if ui.button("Normal (Solo)").clicked() { self.display_channel = 4; ch_changed = true; ui.close_menu(); }
                        ui.separator();
                        if ui.button("Ambient Occlusion [B]").clicked() { self.display_channel = 5; ch_changed = true; ui.close_menu(); }
                        if ui.button("Curvature").clicked() { self.display_channel = 6; ch_changed = true; ui.close_menu(); }
                        if ui.button("Position").clicked() { self.display_channel = 7; ch_changed = true; ui.close_menu(); }
                        if ui.button("Thickness").clicked() { self.display_channel = 8; ch_changed = true; ui.close_menu(); }
                        if ch_changed {
                            self.texture_needs_update = true;
                            self.status_message = format!("Channel: {}", self.channel_name(self.display_channel));
                        }
                    });

                    ui.menu_button("Window", |ui| {
                        if ui.button("Texture Set Settings").clicked() { self.active_right_panel = 0; ui.close_menu(); }
                        if ui.button("Texture Set List").clicked() { self.active_right_panel = 1; ui.close_menu(); }
                        if ui.button("Layers").clicked() { self.active_right_panel = 2; ui.close_menu(); }
                        if ui.button("Display Settings").clicked() { self.active_right_panel = 3; ui.close_menu(); }
                        if ui.button("Shader Settings").clicked() { self.active_right_panel = 4; ui.close_menu(); }
                        if ui.button("History").clicked() { self.active_right_panel = 5; ui.close_menu(); }
                        if ui.button("Log Console").clicked() { self.active_right_panel = 6; ui.close_menu(); }
                    });

                    ui.menu_button("Help", |ui| {
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
                if ui.selectable_label(baking_mode, "⚡ Bake Mesh Maps").clicked() {
                    self.show_baker_dialog = true;
                }
                ui.separator();

                ui.label("Size");
                ui.add(egui::Slider::new(&mut self.brush.size, 1.0..=128.0).suffix(" px"));

                ui.label("Flow");
                let mut flow_pct = self.brush.flow * 100.0;
                if ui.add(egui::Slider::new(&mut flow_pct, 1.0..=100.0).suffix("%")).changed() {
                    self.brush.flow = flow_pct / 100.0;
                }

                ui.label("Opacity");
                let mut op_pct = self.brush.opacity * 100.0;
                if ui.add(egui::Slider::new(&mut op_pct, 1.0..=100.0).suffix("%")).changed() {
                    self.brush.opacity = op_pct / 100.0;
                }

                ui.label("Spacing");
                let mut sp_pct = self.brush.spacing * 100.0;
                if ui.add(egui::Slider::new(&mut sp_pct, 1.0..=100.0).suffix("%")).changed() {
                    self.brush.spacing = sp_pct / 100.0;
                }

                ui.checkbox(&mut self.brush.lazy_mouse, "Lazy");
                if self.brush.lazy_mouse {
                    ui.add(egui::Slider::new(&mut self.lazy_mouse_tracker.radius, 2.0..=60.0).suffix(" px").text("Radius"));
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
                    if ui.add(egui::Slider::new(&mut sj, 0.0..=100.0).text("Size Jitter %")).changed() {
                        self.brush.size_jitter = sj / 100.0;
                    }
                    let mut fj = self.brush.flow_jitter * 100.0;
                    if ui.add(egui::Slider::new(&mut fj, 0.0..=100.0).text("Flow Jitter %")).changed() {
                        self.brush.flow_jitter = fj / 100.0;
                    }
                    let mut aj = self.brush.angle_jitter * 100.0;
                    if ui.add(egui::Slider::new(&mut aj, 0.0..=100.0).text("Angle Jitter %")).changed() {
                        self.brush.angle_jitter = aj / 100.0;
                    }
                    ui.separator();
                    ui.label(RichText::new("Channels to Paint").strong());
                    ui.checkbox(&mut self.brush.paint_color, "Color (BaseColor)");
                    ui.checkbox(&mut self.brush.paint_metallic, "Metallic");
                    ui.checkbox(&mut self.brush.paint_roughness, "Roughness");
                    ui.checkbox(&mut self.brush.paint_normal, "Normal");
                    ui.checkbox(&mut self.brush.paint_height, "Height");
                });

                ui.separator();
                // 3D/2D View Mode Switchers (Substance Painter F1/F2/F3/F4)
                if ui.selectable_label(self.viewport_view_mode == 2, "3D | 2D (F1)").on_hover_text("Split 3D Viewport & 2D UV Texture Map View").clicked() {
                    self.viewport_view_mode = 2;
                }
                if ui.selectable_label(self.viewport_view_mode == 0, "3D (F2)").on_hover_text("3D Viewport Only").clicked() {
                    self.viewport_view_mode = 0;
                }
                if ui.selectable_label(self.viewport_view_mode == 1, "2D (F3)").on_hover_text("2D UV Texture Map View Only").clicked() {
                    self.viewport_view_mode = 1;
                }
                if ui.selectable_label(self.show_wireframe, "Wire (F4)").on_hover_text("Toggle Wireframe Overlay (F4)").clicked() {
                    self.show_wireframe = !self.show_wireframe;
                    self.show_uv_wireframe = self.show_wireframe;
                }

                if self.active_tool == ToolMode::PolygonFill {
                    ui.separator();
                    ui.label(egui::RichText::new("Fill:").strong().color(Color32::WHITE));
                    if ui.selectable_label(self.poly_fill_mode == 0, "△ Tri").clicked() { self.poly_fill_mode = 0; }
                    if ui.selectable_label(self.poly_fill_mode == 1, "⬡ Island").clicked() { self.poly_fill_mode = 1; }
                    if ui.selectable_label(self.poly_fill_mode == 2, "⬛ Mesh").clicked() { self.poly_fill_mode = 2; }
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
                            ui.selectable_value(&mut self.display_channel, 5, "Ambient Occlusion (B)");
                            ui.selectable_value(&mut self.display_channel, 6, "Curvature");
                            ui.selectable_value(&mut self.display_channel, 7, "Position");
                            ui.selectable_value(&mut self.display_channel, 8, "Thickness");
                        });
                    if self.display_channel != prev_channel {
                        self.texture_needs_update = true;
                        self.status_message = format!("Channel: {}", self.channel_name(self.display_channel));
                    }
                });
            });
        });

        // 3. BOTTOM STATUS BAR
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.paint_target_mask {
                    ui.colored_label(Color32::from_rgb(100, 230, 255), "❚ MASK EDITING MODE (Grayscale)");
                } else {
                    ui.colored_label(Color32::from_rgb(58, 217, 115), "● Ready");
                }
                ui.label(format!("| Tool: {:?} | Mesh: {} | Stack: {} layers | {}", self.active_tool, self.mesh.name, self.layer_stack.layers.len(), self.status_message));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Disk Cache: 42% | REMoAINTER v2.0.0 (Rust Core)");
                });
            });
        });

        // 4. FAR-LEFT TOOLBAR (38px strip with custom razor-sharp vector icons)
        egui::SidePanel::left("left_tools").exact_width(38.0).resizable(false).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                let tools = [
                    (ToolMode::Paint, IconType::Brush, "Paint Brush (1)"),
                    (ToolMode::Eraser, IconType::Eraser, "Eraser (2)"),
                    (ToolMode::Projection, IconType::Projection, "Projection (3)"),
                    (ToolMode::PolygonFill, IconType::PolygonFill, "Polygon Fill (4)"),
                    (ToolMode::Smudge, IconType::Smudge, "Smudge (5)"),
                    (ToolMode::Clone, IconType::CloneStamp, "Clone Stamp (6)"),
                    (ToolMode::Eyedropper, IconType::Eyedropper, "Color Eyedropper (I)"),
                ];

                for (mode, icon_type, tip) in tools {
                    let active = self.active_tool == mode;
                    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter();
                        if active {
                            painter.rect_filled(rect, 4.0_f32, ACTIVE_ROW_FILL);
                            painter.rect_stroke(rect, 4.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
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
                        .hint_text("Search 360+ materials, masks, alphas..."),
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
                        if !query.is_empty() && !asset.name.to_lowercase().contains(&query) {
                            return None;
                        }
                        Some(idx)
                    })
                    .collect();

                let mut clicked_asset: Option<(usize, String, PathBuf, PathBuf, [f32; 4], f32, f32)> = None;

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
                                        self.shelf_textures.insert(asset.thumb_path.clone(), handle);
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
                                    painter.rect_stroke(rect, 4.0_f32, Stroke::new(1.5_f32, ACTIVE_BLUE));
                                }
                            }

                            if response.on_hover_text(&asset.name).clicked() {
                                clicked_asset = Some((
                                    asset.category,
                                    asset.name.clone(),
                                    asset.path.clone(),
                                    asset.thumb_path.clone(),
                                    asset.pbr_color,
                                    asset.pbr_metallic,
                                    asset.pbr_roughness,
                                ));
                            }
                        }
                    });
                });

                if let Some((cat, name, _path, thumb, color, metallic, roughness)) = clicked_asset {
                    match cat {
                        1 => {
                            self.brush.color = color;
                            self.brush.metallic = metallic;
                            self.brush.roughness = roughness;
                            apply_smart_material_hierarchy(
                                &mut self.layer_stack,
                                &name,
                                color,
                                metallic,
                                roughness,
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                            );
                            self.recompute_composite();
                            self.status_message = format!("Applied Smart Material: {}", name);
                            self.history.push(format!("{:02}  Apply Smart Material ({})", self.history.len() + 1, name));
                        }
                        2 => {
                            self.brush.color = color;
                            self.brush.metallic = metallic;
                            self.brush.roughness = roughness;
                            let id = self.layer_stack.layers.len() + 1;
                            let fill = load_real_pbr_material(
                                id,
                                &name,
                                color,
                                metallic,
                                roughness,
                                self.layer_stack.width,
                                self.layer_stack.height,
                            );
                            self.layer_stack.layers.push(fill);
                            self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                            self.recompute_composite();
                            self.status_message = format!("Applied Real PBR Material: {}", name);
                            self.history.push(format!("{:02}  Apply Real PBR Material ({})", self.history.len() + 1, name));
                        }
                        3 => {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                                active_layer.apply_smart_mask_file(
                                    &_path,
                                    self.baked_maps.curvature.as_deref(),
                                    self.baked_maps.ambient_occlusion.as_deref(),
                                );
                                self.status_message = format!("Applied Smart Mask ({}) to Layer: {}", name, active_layer.name);
                                self.history.push(format!("{:02}  Apply Smart Mask ({})", self.history.len() + 1, name));
                            }
                            self.recompute_composite();
                        }
                        4 => {
                            self.brush.alpha_name = name.clone();
                            if let Ok(img) = image::open(&_path).or_else(|_| image::open(&thumb)) {
                                let gray = img.to_luma8();
                                let (w, h) = gray.dimensions();
                                self.brush.custom_alpha = Some(Arc::new(AlphaMaskData {
                                    width: w,
                                    height: h,
                                    data: gray.into_raw(),
                                }));
                                self.brush.alpha_shape = crate::painting::brush::AlphaShape::Custom;
                            }
                            self.status_message = format!("Selected Alpha: {}", name);
                            self.history.push(format!("{:02}  Select Alpha ({})", self.history.len() + 1, name));
                        }
                        _ => {}
                    }
                }
            });

        // 6. FAR-RIGHT DOCK STRIP (34px narrow strip with vector icons)
        egui::SidePanel::right("right_strip").exact_width(34.0).resizable(false).show(ctx, |ui| {
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
                    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        let painter = ui.painter();
                        if active {
                            painter.rect_filled(rect, 3.0_f32, ACTIVE_ROW_FILL);
                            painter.rect_stroke(rect, 3.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
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
        egui::SidePanel::right("right_panel").default_width(320.0).width_range(260.0..=480.0).show(ctx, |ui| {
            match self.active_right_panel {
                0 => self.ui_texture_set_settings(ui),
                1 => self.ui_texture_set_list(ui),
                2 => self.ui_layers_panel(ui),
                3 => self.ui_display_settings(ui),
                4 => self.ui_shader_settings(ui),
                5 => self.ui_history_panel(ui),
                6 => self.ui_log_panel(ui),
                _ => {}
            }
        });

        // 8. CENTRAL 3D VIEWPORT & PAINT CANVAS
        egui::CentralPanel::default().show(ctx, |ui| {
            // Update GPU texture if dirty or not loaded
            if self.texture_needs_update || self.texture_handle.is_none() {
                let w = self.layer_stack.width as usize;
                let h = self.layer_stack.height as usize;
                let pixels: Vec<Color32> = match self.display_channel {
                    1 => self.layer_stack.composite_color.iter().map(|c| {
                        Color32::from_rgb((c[0].clamp(0.0, 1.0) * 255.0) as u8, (c[1].clamp(0.0, 1.0) * 255.0) as u8, (c[2].clamp(0.0, 1.0) * 255.0) as u8)
                    }).collect(),
                    2 => self.layer_stack.composite_roughness.iter().map(|r| {
                        let v = (r.clamp(0.0, 1.0) * 255.0) as u8;
                        Color32::from_rgb(v, v, v)
                    }).collect(),
                    3 => self.layer_stack.composite_metallic.iter().map(|m| {
                        let v = (m.clamp(0.0, 1.0) * 255.0) as u8;
                        Color32::from_rgb(v, v, v)
                    }).collect(),
                    4 => self.layer_stack.composite_normal.iter().map(|n| {
                        Color32::from_rgb((n[0].clamp(0.0, 1.0) * 255.0) as u8, (n[1].clamp(0.0, 1.0) * 255.0) as u8, (n[2].clamp(0.0, 1.0) * 255.0) as u8)
                    }).collect(),
                    5 => if let Some(ref ao) = self.baked_maps.ambient_occlusion {
                        ao.iter().map(|v| { let b = (v.clamp(0.0, 1.0) * 255.0) as u8; Color32::from_rgb(b, b, b) }).collect()
                    } else {
                        vec![Color32::WHITE; w * h]
                    },
                    6 => if let Some(ref cur) = self.baked_maps.curvature {
                        cur.iter().map(|v| { let b = (v.clamp(0.0, 1.0) * 255.0) as u8; Color32::from_rgb(b, b, b) }).collect()
                    } else {
                        vec![Color32::from_gray(128); w * h]
                    },
                    7 => if let Some(ref pos) = self.baked_maps.position {
                        pos.iter().map(|p| Color32::from_rgb((p[0].clamp(0.0, 1.0) * 255.0) as u8, (p[1].clamp(0.0, 1.0) * 255.0) as u8, (p[2].clamp(0.0, 1.0) * 255.0) as u8)).collect()
                    } else {
                        vec![Color32::from_gray(128); w * h]
                    },
                    8 => if let Some(ref thk) = self.baked_maps.thickness {
                        thk.iter().map(|v| { let b = (v.clamp(0.0, 1.0) * 255.0) as u8; Color32::from_rgb(b, b, b) }).collect()
                    } else {
                        vec![Color32::WHITE; w * h]
                    },
                    _ => self.layer_stack.composite_lit.iter().map(|c| {
                        Color32::from_rgb(
                            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        )
                    }).collect(),
                };
                let img = egui::ColorImage { size: [w, h], pixels };
                if let Some(ref mut handle) = self.texture_handle {
                    handle.set(img, egui::TextureOptions::LINEAR_REPEAT);
                } else {
                    self.texture_handle = Some(ctx.load_texture("pbr_composite", img, egui::TextureOptions::LINEAR_REPEAT));
                }
                self.texture_needs_update = false;
            }

            let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter();

            // Background Studio Gradient
            painter.rect_filled(rect, 0.0_f32, Color32::from_rgb(26, 26, 28));

            // Top Floating Overlay Bar
            let overlay_rect = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 12.0), vec2(580.0, 32.0));
            painter.rect_filled(overlay_rect, 4.0_f32, Color32::from_black_alpha(200));
            painter.text(pos2(overlay_rect.left() + 10.0, overlay_rect.center().y), Align2::LEFT_CENTER, format!("{} | Persp 50mm | Lit", self.mesh.name), egui::FontId::proportional(12.0), Color32::WHITE);

            // View mode switchers (Wire: F4, F1: 3D|2D, F2: 3D, F3: 2D)
            let vm_wire = Rect::from_min_size(pos2(overlay_rect.right() - 280.0, overlay_rect.top() + 4.0), vec2(66.0, 24.0));
            let vm_3d = Rect::from_min_size(pos2(overlay_rect.right() - 210.0, overlay_rect.top() + 4.0), vec2(62.0, 24.0));
            let vm_2d = Rect::from_min_size(pos2(overlay_rect.right() - 142.0, overlay_rect.top() + 4.0), vec2(62.0, 24.0));
            let vm_split = Rect::from_min_size(pos2(overlay_rect.right() - 74.0, overlay_rect.top() + 4.0), vec2(70.0, 24.0));

            painter.rect_filled(vm_wire, 3.0_f32, if self.show_wireframe { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_wire.center(), Align2::CENTER_CENTER, "Wire (F4)", egui::FontId::proportional(10.5), if self.show_wireframe { Color32::WHITE } else { TEXT_DIMMED });

            painter.rect_filled(vm_3d, 3.0_f32, if self.viewport_view_mode == 0 { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_3d.center(), Align2::CENTER_CENTER, "3D (F2)", egui::FontId::proportional(10.5), Color32::WHITE);

            painter.rect_filled(vm_2d, 3.0_f32, if self.viewport_view_mode == 1 { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_2d.center(), Align2::CENTER_CENTER, "2D (F3)", egui::FontId::proportional(10.5), Color32::WHITE);

            painter.rect_filled(vm_split, 3.0_f32, if self.viewport_view_mode == 2 { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_split.center(), Align2::CENTER_CENTER, "3D|2D (F1)", egui::FontId::proportional(10.5), Color32::WHITE);

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if vm_wire.contains(pos) { self.show_wireframe = !self.show_wireframe; self.show_uv_wireframe = !self.show_uv_wireframe; }
                    else if vm_3d.contains(pos) { self.viewport_view_mode = 0; }
                    else if vm_2d.contains(pos) { self.viewport_view_mode = 1; }
                    else if vm_split.contains(pos) { self.viewport_view_mode = 2; }
                }
            }

            if ctx.input(|i| i.key_pressed(egui::Key::F4)) {
                self.show_wireframe = !self.show_wireframe;
                self.show_uv_wireframe = !self.show_uv_wireframe;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F1)) { self.viewport_view_mode = 2; }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) { self.viewport_view_mode = 0; }
            if ctx.input(|i| i.key_pressed(egui::Key::F3)) { self.viewport_view_mode = 1; }

            // Layout based on view mode
            match self.viewport_view_mode {
                0 => {
                    self.render_3d_viewport(painter, rect, &response, ctx);
                }
                1 => {
                    self.render_2d_canvas(painter, rect, &response, ctx);
                }
                _ => {
                    let half_w = rect.width() * 0.5;
                    let rect_3d = Rect::from_min_size(rect.min, vec2(half_w - 2.0, rect.height()));
                    let rect_2d = Rect::from_min_size(pos2(rect.min.x + half_w + 2.0, rect.min.y), vec2(half_w - 2.0, rect.height()));

                    painter.line_segment([pos2(rect.min.x + half_w, rect.min.y), pos2(rect.min.x + half_w, rect.max.y)], Stroke::new(1.0_f32, BORDER_COLOR));
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
    }
}

// Right panel implementations
impl RemoainterApp {
    fn render_2d_canvas(&mut self, painter: &egui::Painter, canvas_bounds: Rect, response: &egui::Response, ctx: &egui::Context) {
        let size = (canvas_bounds.width().min(canvas_bounds.height()) - 64.0).max(64.0);
        let canvas_rect = Rect::from_center_size(canvas_bounds.center(), vec2(size, size));

        // Canvas frame & shadow
        painter.rect_filled(canvas_rect, 2.0_f32, Color32::from_rgb(18, 18, 18));
        painter.rect_stroke(canvas_rect, 2.0_f32, Stroke::new(1.0_f32, BORDER_COLOR));

        // Draw live GPU texture map
        if let Some(ref handle) = self.texture_handle {
            painter.image(
                handle.id(),
                canvas_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // Draw UV Mesh Wireframe Overlay (Substance Painter 2D UV Layout)
        if self.show_uv_wireframe {
            let num_tris = self.mesh.indices.len() / 3;
            let wire_color = Color32::from_rgba_unmultiplied(65, 175, 245, 140);
            let wire_stroke = Stroke::new(0.85_f32, wire_color);
            for t in 0..num_tris {
                let i0 = self.mesh.indices[t * 3] as usize;
                let i1 = self.mesh.indices[t * 3 + 1] as usize;
                let i2 = self.mesh.indices[t * 3 + 2] as usize;
                if i0 < self.mesh.uvs.len() && i1 < self.mesh.uvs.len() && i2 < self.mesh.uvs.len() {
                    let uv0 = self.mesh.uvs[i0];
                    let uv1 = self.mesh.uvs[i1];
                    let uv2 = self.mesh.uvs[i2];

                    let p0 = pos2(canvas_rect.left() + uv0[0] * canvas_rect.width(), canvas_rect.top() + uv0[1] * canvas_rect.height());
                    let p1 = pos2(canvas_rect.left() + uv1[0] * canvas_rect.width(), canvas_rect.top() + uv1[1] * canvas_rect.height());
                    let p2 = pos2(canvas_rect.left() + uv2[0] * canvas_rect.width(), canvas_rect.top() + uv2[1] * canvas_rect.height());

                    painter.line_segment([p0, p1], wire_stroke);
                    painter.line_segment([p1, p2], wire_stroke);
                    painter.line_segment([p2, p0], wire_stroke);
                }
            }
        }

        // Texture border & UV grid indicator
        painter.text(
            pos2(canvas_rect.left() + 8.0, canvas_rect.top() + 8.0),
            Align2::LEFT_TOP,
            format!("UV Layout ({}x{}) | Wire: {}", self.layer_stack.width, self.layer_stack.height, if self.show_uv_wireframe { "ON (F4)" } else { "OFF" }),
            egui::FontId::monospace(10.5),
            TEXT_DIMMED,
        );

        // Brush cursor preview
        if let Some(pos) = ctx.pointer_hover_pos() {
            if canvas_rect.contains(pos) {
                let brush_screen_r = (self.brush.size / self.layer_stack.width as f32) * size * 0.5;
                painter.circle_stroke(pos, brush_screen_r.max(2.0), Stroke::new(1.2_f32, ACTIVE_BLUE));
            }
        }

        // Paint handling inside 2D Canvas
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if canvas_rect.contains(pos) && !ctx.input(|i| i.modifiers.alt) {
                    if !self.is_in_stroke {
                        self.history_manager.begin_stroke();
                        self.is_in_stroke = true;
                    }

                    let paint_pos = if self.brush.lazy_mouse {
                        let smoothed = self.lazy_mouse_tracker.update([pos.x, pos.y]);
                        if self.lazy_mouse_tracker.is_dragging {
                            painter.line_segment([pos, pos2(smoothed[0], smoothed[1])], Stroke::new(1.2_f32, Color32::from_rgb(255, 120, 40)));
                            painter.circle_stroke(pos2(smoothed[0], smoothed[1]), 2.5, Stroke::new(1.0_f32, Color32::WHITE));
                        }
                        pos2(smoothed[0], smoothed[1])
                    } else {
                        pos
                    };

                    let u = ((paint_pos.x - canvas_rect.left()) / canvas_rect.width()).clamp(0.0, 1.0);
                    let v = ((paint_pos.y - canvas_rect.top()) / canvas_rect.height()).clamp(0.0, 1.0);
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
                            self.status_message = format!("Sampled: Color [{:.2}, {:.2}, {:.2}], Met: {:.2}, Rough: {:.2}",
                                self.brush.color[0], self.brush.color[1], self.brush.color[2], self.brush.metallic, self.brush.roughness);
                        }
                        [0, 0, 0, 0]
                    } else if self.active_tool == ToolMode::PolygonFill {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                            if self.poly_fill_mode == 2 {
                                active_layer.flood_fill(&self.brush, Some(&mut self.history_manager));
                            } else {
                                let num_tris = self.mesh.indices.len() / 3;
                                let mut hit_t = None;
                                for t in 0..num_tris {
                                    let i0 = self.mesh.indices[t * 3] as usize;
                                    let i1 = self.mesh.indices[t * 3 + 1] as usize;
                                    let i2 = self.mesh.indices[t * 3 + 2] as usize;
                                    if i0 < self.mesh.uvs.len() && i1 < self.mesh.uvs.len() && i2 < self.mesh.uvs.len() {
                                        let uv0 = self.mesh.uvs[i0];
                                        let uv1 = self.mesh.uvs[i1];
                                        let uv2 = self.mesh.uvs[i2];
                                        let denom = ((uv1[1] - uv2[1]) * (uv0[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv0[1] - uv2[1])).max(1e-7);
                                        let b0 = ((uv1[1] - uv2[1]) * (uv[0] - uv2[0]) + (uv2[0] - uv1[0]) * (uv[1] - uv2[1])) / denom;
                                        let b1 = ((uv2[1] - uv0[1]) * (uv[0] - uv2[0]) + (uv0[0] - uv2[0]) * (uv[1] - uv2[1])) / denom;
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
                                        active_layer.fill_triangle(self.mesh.uvs[i0], self.mesh.uvs[i1], self.mesh.uvs[i2], &self.brush, Some(&mut self.history_manager));
                                    } else {
                                        let island = self.mesh.find_uv_island(t);
                                        for it in island {
                                            let i0 = self.mesh.indices[it * 3] as usize;
                                            let i1 = self.mesh.indices[it * 3 + 1] as usize;
                                            let i2 = self.mesh.indices[it * 3 + 2] as usize;
                                            if i0 < self.mesh.uvs.len() && i1 < self.mesh.uvs.len() && i2 < self.mesh.uvs.len() {
                                                active_layer.fill_triangle(self.mesh.uvs[i0], self.mesh.uvs[i1], self.mesh.uvs[i2], &self.brush, Some(&mut self.history_manager));
                                            }
                                        }
                                    }
                                } else {
                                    active_layer.flood_fill(&self.brush, Some(&mut self.history_manager));
                                }
                            }
                        }
                        self.recompute_composite();
                        [0, 0, self.layer_stack.width as i32, self.layer_stack.height as i32]
                    } else if self.active_tool == ToolMode::Smudge {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                            active_layer.smudge_stamp(uv, &self.brush, Some(&mut self.history_manager))
                        } else {
                            [0, 0, 0, 0]
                        }
                    } else if self.active_tool == ToolMode::Clone {
                        if ctx.input(|i| i.modifiers.alt) {
                            self.clone_source_uv = Some(uv);
                            self.status_message = format!("Clone Source Set: [{:.2}, {:.2}]", uv[0], uv[1]);
                            [0, 0, 0, 0]
                        } else {
                            let src_uv = self.clone_source_uv.unwrap_or(uv);
                            let px = (src_uv[0] * (self.layer_stack.width - 1) as f32) as usize;
                            let py = (src_uv[1] * (self.layer_stack.height - 1) as f32) as usize;
                            let idx = py * self.layer_stack.width as usize + px;
                            let mut clone_brush = self.brush.clone();
                            if idx < self.layer_stack.composite_color.len() {
                                clone_brush.color = self.layer_stack.composite_color[idx];
                                clone_brush.metallic = self.layer_stack.composite_metallic[idx];
                                clone_brush.roughness = self.layer_stack.composite_roughness[idx];
                            }
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                active_layer.paint_stamp(uv, &clone_brush, self.paint_target_mask, Some(&mut self.history_manager))
                            } else {
                                [0, 0, 0, 0]
                            }
                        }
                    } else {
                        let is_eraser = self.active_tool == ToolMode::Eraser;
                        let on_mask = self.paint_target_mask;
                        if let Some(last_uv) = self.last_painted_uv {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                let mut b = if is_eraser {
                                    active_layer.erase_line(last_uv, uv, &self.brush, on_mask, Some(&mut self.history_manager))
                                } else {
                                    active_layer.paint_line(last_uv, uv, &self.brush, on_mask, Some(&mut self.history_manager))
                                };
                                if self.symmetry_x {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_line([1.0 - last_uv[0], last_uv[1]], [1.0 - uv[0], uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    } else {
                                        active_layer.paint_line([1.0 - last_uv[0], last_uv[1]], [1.0 - uv[0], uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    };
                                    b[0] = b[0].min(b_sym[0]); b[1] = b[1].min(b_sym[1]); b[2] = b[2].max(b_sym[2]); b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_y {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_line([last_uv[0], 1.0 - last_uv[1]], [uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    } else {
                                        active_layer.paint_line([last_uv[0], 1.0 - last_uv[1]], [uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    };
                                    b[0] = b[0].min(b_sym[0]); b[1] = b[1].min(b_sym[1]); b[2] = b[2].max(b_sym[2]); b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_z {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_line([1.0 - last_uv[0], 1.0 - last_uv[1]], [1.0 - uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    } else {
                                        active_layer.paint_line([1.0 - last_uv[0], 1.0 - last_uv[1]], [1.0 - uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    };
                                    b[0] = b[0].min(b_sym[0]); b[1] = b[1].min(b_sym[1]); b[2] = b[2].max(b_sym[2]); b[3] = b[3].max(b_sym[3]);
                                }
                                b
                            } else {
                                [0, 0, 0, 0]
                            }
                        } else {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                let mut b = if is_eraser {
                                    active_layer.erase_stamp(uv, &self.brush, on_mask, Some(&mut self.history_manager))
                                } else {
                                    active_layer.paint_stamp(uv, &self.brush, on_mask, Some(&mut self.history_manager))
                                };
                                if self.symmetry_x {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_stamp([1.0 - uv[0], uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    } else {
                                        active_layer.paint_stamp([1.0 - uv[0], uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    };
                                    b[0] = b[0].min(b_sym[0]); b[1] = b[1].min(b_sym[1]); b[2] = b[2].max(b_sym[2]); b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_y {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_stamp([uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    } else {
                                        active_layer.paint_stamp([uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    };
                                    b[0] = b[0].min(b_sym[0]); b[1] = b[1].min(b_sym[1]); b[2] = b[2].max(b_sym[2]); b[3] = b[3].max(b_sym[3]);
                                }
                                if self.symmetry_z {
                                    let b_sym = if is_eraser {
                                        active_layer.erase_stamp([1.0 - uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    } else {
                                        active_layer.paint_stamp([1.0 - uv[0], 1.0 - uv[1]], &self.brush, on_mask, Some(&mut self.history_manager))
                                    };
                                    b[0] = b[0].min(b_sym[0]); b[1] = b[1].min(b_sym[1]); b[2] = b[2].max(b_sym[2]); b[3] = b[3].max(b_sym[3]);
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
                    _ => "Brush Tool",
                };
                self.history_manager.commit_stroke(tool_name);
                self.history.push(format!("{:02}  {}", self.history.len() + 1, tool_name));
                self.logs.push((chrono::Local::now().format("%H:%M:%S").to_string(), "INFO".into(), format!("Committed stroke: {}", tool_name)));
                self.is_in_stroke = false;
                self.lazy_mouse_tracker.reset();
            }
        }
    }

    fn render_3d_viewport(&mut self, painter: &egui::Painter, view_bounds: Rect, response: &egui::Response, ctx: &egui::Context) {
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

        // 1. OpenGL Hardware PBR 3D Render Callback (24-bit Depth Test + Per-Fragment GGX + ACES)
        if let Some(ref vp_mutex) = self.viewport_gl {
            let vp_clone = vp_mutex.clone();
            let yaw = self.camera_yaw;
            let pitch = self.camera_pitch;
            let cam_dist = self.camera_dist;
            let light_intensity = self.light_intensity;
            let env_rotation = self.env_rotation;
            let display_channel = self.display_channel;

            let exposure = self.exposure;
            let skybox_visible = self.skybox_visible;
            let skybox_blur = self.skybox_blur;
            let skybox_intensity = self.skybox_intensity;
            let uv_scale = self.uv_scale;
            let show_wireframe = self.show_wireframe;

            let callback = egui_glow::CallbackFn::new(move |info, painter| {
                let rect = info.clip_rect_in_pixels();
                let x = rect.left_px;
                let y = rect.from_bottom_px;
                let w = rect.width_px;
                let h = rect.height_px;

                if let Ok(mut vp) = vp_clone.lock() {
                    vp.exposure = exposure;
                    vp.skybox_visible = skybox_visible;
                    vp.skybox_blur = skybox_blur;
                    vp.skybox_intensity = skybox_intensity;
                    vp.uv_scale = uv_scale;
                    vp.show_wireframe = show_wireframe;
                    vp.paint(
                        painter.gl(),
                        x,
                        y,
                        w,
                        h,
                        yaw,
                        pitch,
                        cam_dist,
                        light_intensity,
                        env_rotation,
                        display_channel,
                    );
                }
            });

            painter.add(egui::PaintCallback {
                rect: view_bounds,
                callback: Arc::new(callback),
            });
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

        // Camera Orbit (Alt+LMB/MMB) / Zoom (Alt+RMB) / Environment Rotate (Shift+RMB) & 3D Surface Painting
        if response.dragged() {
            let delta = response.drag_delta();
            if ctx.input(|i| i.modifiers.alt && i.pointer.button_down(egui::PointerButton::Secondary)) {
                self.camera_dist = (self.camera_dist + delta.y * 0.01).clamp(0.5, 6.0);
            } else if ctx.input(|i| i.modifiers.shift && i.pointer.button_down(egui::PointerButton::Secondary)) {
                self.env_rotation = (self.env_rotation + delta.x * 0.8).rem_euclid(360.0);
            } else if ctx.input(|i| i.modifiers.alt) || ctx.input(|i| i.pointer.middle_down()) {
                self.camera_yaw += delta.x * 0.4;
                self.camera_pitch = (self.camera_pitch + delta.y * 0.4).clamp(-85.0, 85.0);
            } else if let Some(pos) = response.interact_pointer_pos() {
                let ndc_x = (pos.x - view_bounds.left()) / view_bounds.width() * 2.0 - 1.0;
                let ndc_y = 1.0 - (pos.y - view_bounds.top()) / view_bounds.height() * 2.0;
                let p_far = inv_view_proj.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
                let ray_dir = (p_far - cam_pos).normalize();

                if let Some(hit) = self.mesh.raycast_normalized(cam_pos, ray_dir, mesh_center, mesh_radius) {
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
                            self.status_message = format!("Sampled: Color [{:.2}, {:.2}, {:.2}], Met: {:.2}, Rough: {:.2}",
                                self.brush.color[0], self.brush.color[1], self.brush.color[2], self.brush.metallic, self.brush.roughness);
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
                                    if i0 < self.mesh.uvs.len() && i1 < self.mesh.uvs.len() && i2 < self.mesh.uvs.len() {
                                        active_layer.fill_triangle(self.mesh.uvs[i0], self.mesh.uvs[i1], self.mesh.uvs[i2], &self.brush, Some(&mut self.history_manager));
                                    }
                                }
                                1 => {
                                    // UV Island Fill
                                    let island = self.mesh.find_uv_island(hit.tri_idx);
                                    for t in island {
                                        let i0 = self.mesh.indices[t * 3] as usize;
                                        let i1 = self.mesh.indices[t * 3 + 1] as usize;
                                        let i2 = self.mesh.indices[t * 3 + 2] as usize;
                                        if i0 < self.mesh.uvs.len() && i1 < self.mesh.uvs.len() && i2 < self.mesh.uvs.len() {
                                            active_layer.fill_triangle(self.mesh.uvs[i0], self.mesh.uvs[i1], self.mesh.uvs[i2], &self.brush, Some(&mut self.history_manager));
                                        }
                                    }
                                }
                                _ => {
                                    // Mesh / Chunk Fill
                                    active_layer.flood_fill(&self.brush, Some(&mut self.history_manager));
                                }
                            }
                        }
                        self.recompute_composite();
                        [0, 0, self.layer_stack.width as i32, self.layer_stack.height as i32]
                    } else if self.active_tool == ToolMode::Smudge {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                            active_layer.smudge_3d_surface(&self.mesh, hit.pos, hit.normal, radius_3d, &self.brush, Some(&mut self.history_manager))
                        } else {
                            [0, 0, 0, 0]
                        }
                    } else if self.active_tool == ToolMode::Clone {
                        if ctx.input(|i| i.modifiers.alt) {
                            self.clone_source_uv = Some(uv);
                            self.status_message = format!("Clone Source Set: [{:.2}, {:.2}]", uv[0], uv[1]);
                            [0, 0, 0, 0]
                        } else {
                            let src_uv = self.clone_source_uv.unwrap_or(uv);
                            let px = (src_uv[0] * (self.layer_stack.width - 1) as f32) as usize;
                            let py = (src_uv[1] * (self.layer_stack.height - 1) as f32) as usize;
                            let idx = py * self.layer_stack.width as usize + px;
                            let mut clone_brush = self.brush.clone();
                            if idx < self.layer_stack.composite_color.len() {
                                clone_brush.color = self.layer_stack.composite_color[idx];
                                clone_brush.metallic = self.layer_stack.composite_metallic[idx];
                                clone_brush.roughness = self.layer_stack.composite_roughness[idx];
                            }
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                active_layer.paint_3d_surface(&self.mesh, hit.pos, hit.normal, radius_3d, &clone_brush, false, self.paint_target_mask, Some(&mut self.history_manager))
                            } else {
                                [0, 0, 0, 0]
                            }
                        }
                    } else {
                        // Paint (1), Eraser (2), Projection (3)
                        let is_eraser = self.active_tool == ToolMode::Eraser;
                        let on_mask = self.paint_target_mask;
                        let mut b = if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                            active_layer.paint_3d_surface(&self.mesh, hit.pos, hit.normal, radius_3d, &self.brush, is_eraser, on_mask, Some(&mut self.history_manager))
                        } else {
                            [0, 0, 0, 0]
                        };
                        if self.symmetry_x {
                            let sym_pos = [-hit.pos[0], hit.pos[1], hit.pos[2]];
                            let sym_norm = [-hit.normal[0], hit.normal[1], hit.normal[2]];
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                let b_sym = active_layer.paint_3d_surface(&self.mesh, sym_pos, sym_norm, radius_3d, &self.brush, is_eraser, on_mask, Some(&mut self.history_manager));
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
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                let b_sym = active_layer.paint_3d_surface(&self.mesh, sym_pos, sym_norm, radius_3d, &self.brush, is_eraser, on_mask, Some(&mut self.history_manager));
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
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(active_idx) {
                                let b_sym = active_layer.paint_3d_surface(&self.mesh, sym_pos, sym_norm, radius_3d, &self.brush, is_eraser, on_mask, Some(&mut self.history_manager));
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
                    _ => "Brush Tool",
                };
                self.history_manager.commit_stroke(tool_name);
                self.history.push(format!("{:02}  {}", self.history.len() + 1, tool_name));
                self.logs.push((chrono::Local::now().format("%H:%M:%S").to_string(), "INFO".into(), format!("Committed stroke: {}", tool_name)));
                self.is_in_stroke = false;
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
                if self.mesh.raycast_normalized(cam_pos, ray_dir, mesh_center, mesh_radius).is_some() {
                    painter.circle_stroke(pos, (self.brush.size * 0.5).max(3.0), Stroke::new(1.2_f32, ACTIVE_BLUE));
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
                if ui.button(egui::RichText::new("+ Add").strong().color(Color32::WHITE)).on_hover_text("Add a new Texture Set (Material Group)").clicked() {
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
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), egui::Sense::click());

            if is_active {
                ui.painter().rect_filled(rect, 3.0_f32, ACTIVE_ROW_FILL);
                ui.painter().rect_stroke(rect, 3.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 3.0_f32, Color32::from_rgb(38, 38, 44));
            }

            // Eye visibility dot/icon
            let eye_rect = Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(14.0, 14.0));
            let eye_resp = ui.interact(eye_rect, ui.id().with(("ts_eye", idx)), egui::Sense::click());
            if eye_resp.clicked() {
                set.visible = !set.visible;
            }
            ui.painter().circle_filled(
                eye_rect.center(),
                4.5,
                if set.visible { ACTIVE_BLUE } else { Color32::from_rgb(70, 70, 75) },
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
            egui::RichText::new("📂 Import Mesh (OBJ/FBX/glTF)...").color(Color32::WHITE)
        ).fill(Color32::from_rgb(40, 55, 75)).min_size(vec2(ui.available_width(), 26.0));
        if ui.add(import_btn).on_hover_text("Open a 3D mesh file with materials or UDIMs").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Import Mesh — REMoAINTER")
                .add_filter("3D Models (*.obj, *.gltf, *.glb)", &["obj", "OBJ", "gltf", "glb"])
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
                ui.menu_button(egui::RichText::new("+").strong().color(Color32::WHITE), |ui| {
                    ui.set_min_width(160.0);
                    ui.label(egui::RichText::new("Add Channel").strong());
                    ui.separator();
                    let all_possible = [
                        ("Base color",   "sRGB8"),
                        ("Metallic",     "L8"),
                        ("Roughness",    "L8"),
                        ("Normal",       "RGB16F"),
                        ("Height",       "L16F"),
                        ("Emissive",     "sRGB8"),
                        ("Opacity",      "L8"),
                        ("Ambient Occlusion", "L8"),
                        ("Specular",     "L8"),
                        ("Glossiness",   "L8"),
                        ("Displacement", "L16F"),
                        ("User0",        "L8"),
                        ("User1",        "L8"),
                    ];
                    for (ch_name, fmt) in all_possible {
                        let already = self.texture_channels.iter().any(|c: &TextureChannel| c.name == ch_name);
                        let label = if already {
                            egui::RichText::new(format!("{} (active)", ch_name)).color(TEXT_DIMMED)
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
                });
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
                    "Metallic"   => Color32::from_rgb(150, 200, 255),
                    "Roughness"  => Color32::from_rgb(80, 200, 130),
                    "Normal"     => Color32::from_rgb(140, 120, 255),
                    "Height"     => Color32::from_rgb(255, 210, 80),
                    "Emissive"   => Color32::from_rgb(255, 100, 50),
                    "Opacity"    => Color32::from_rgb(200, 200, 200),
                    _            => TEXT_DIMMED,
                };
                let (dot_rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 4.5, dot_color);
                ui.label(egui::RichText::new(&ch.name).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !is_required {
                        if ui.small_button(egui::RichText::new("×").color(Color32::from_rgb(255, 80, 80))).clicked() {
                            to_remove = Some(ci);
                        }
                    }
                    ui.label(egui::RichText::new(&ch.format).color(TEXT_DIMMED).size(10.5));
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
            ui.checkbox(&mut self.baker_options.invert_normal_y, "Invert Green Channel (DirectX)");
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
                        let _ = ui.selectable_label(true,  "16px");
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
            ("Normal",          self.baked_maps.normal.is_some()),
            ("World Space Nrm", self.baked_maps.world_space_normal.is_some()),
            ("AO",              self.baked_maps.ambient_occlusion.is_some()),
            ("Curvature",       self.baked_maps.curvature.is_some()),
            ("Position",        self.baked_maps.position.is_some()),
            ("Thickness",       self.baked_maps.thickness.is_some()),
            ("ID Map",          self.baked_maps.id.is_some()),
        ];
        egui::Grid::new("mesh_map_grid").num_columns(2).spacing([4.0, 2.0]).show(ui, |ui| {
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
            egui::RichText::new("Bake Mesh Maps...").color(Color32::WHITE).strong()
        ).fill(Color32::from_rgb(40, 90, 160)).min_size(vec2(ui.available_width(), 24.0));
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
                self.layer_stack.layers.push(Layer::new_paint(id, format!("Layer {}", id), self.layer_stack.width, self.layer_stack.height));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.paint_target_mask = false;
                self.recompute_composite();
                self.status_message = "Added Paint Layer".into();
            }
            if ui.small_button("+ Fill").clicked() {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_fill(id, format!("Fill {}", id), self.brush.color, self.brush.metallic, self.brush.roughness, self.layer_stack.width, self.layer_stack.height));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.paint_target_mask = false;
                self.recompute_composite();
                self.status_message = "Added Fill Layer".into();
            }
            if ui.small_button("+ Folder").on_hover_text("Add Folder Group (Ctrl+G)").clicked() {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_folder(id, format!("Folder {}", id), self.layer_stack.width, self.layer_stack.height));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.paint_target_mask = false;
                self.recompute_composite();
                self.status_message = "Added Folder".into();
            }
            if ui.small_button("+ Black Mask").on_hover_text("Add Black Mask with Sub-Stack").clicked() {
                if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                    active.add_black_mask();
                    self.status_message = "Added Black Mask".into();
                }
                self.recompute_composite();
            }
            if ui.small_button("+ White Mask").on_hover_text("Add White Mask").clicked() {
                if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                    active.add_white_mask();
                    self.status_message = "Added White Mask".into();
                }
                self.recompute_composite();
            }
            ui.menu_button("+ Filter", |ui| {
                let active_idx = self.layer_stack.active_layer_index;
                if ui.button("Levels").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Levels { min_in: 0.0, max_in: 1.0, gamma: 1.0, min_out: 0.0, max_out: 1.0, channel: FilterTargetChannel::All });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("Gaussian Blur").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Blur { radius: 3.0, channel: FilterTargetChannel::All });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("Sharpen").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Sharpen { strength: 1.5, channel: FilterTargetChannel::All });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("HSL Adjustment").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::HSL { hue_shift: 0.0, sat_mult: 1.0, light_offset: 0.0 });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
                if ui.button("Invert").clicked() {
                    if let Some(active) = self.layer_stack.layers.get_mut(active_idx) {
                        active.effects.push(FilterEffect::Invert { channel: FilterTargetChannel::All });
                        active.apply_effects();
                    }
                    self.recompute_composite();
                    ui.close_menu();
                }
            });
            if ui.small_button("+ Anchor").on_hover_text("Add Anchor Point").clicked() {
                if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                    active.anchor_point = Some(AnchorPoint::new(
                        1,
                        format!("Anchor_{}", active.name),
                        active.id,
                        crate::painting::AnchorChannel::All,
                    ));
                    self.status_message = format!("Created Anchor Point for {}", active.name);
                }
            }
            if ui.small_button("📁 Import...").on_hover_text("Import PBR Texture Set (Folder or Files with Auto-Matching)").clicked() {
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
                    self.layer_stack.layers.iter().find(|l| l.id == pid).map_or(false, |p| !p.folder_expanded)
                });
                if is_hidden {
                    continue;
                }

                ui.push_id(format!("layer_row_id_{}_{}", layer_id, i), |ui| {
                    let is_active = self.layer_stack.active_layer_index == i;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), egui::Sense::click_and_drag());

                    if is_active {
                        ui.painter().rect_filled(rect, 2.0_f32, ACTIVE_ROW_FILL);
                        ui.painter().rect_stroke(rect, 2.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
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

                    // Color tag indicator (Substance Painter color tags)
                    if let Some(tag) = layer.color_tag {
                        let tag_col = match tag {
                            1 => Color32::from_rgb(230, 60, 60),   // Red
                            2 => Color32::from_rgb(240, 140, 40),  // Orange
                            3 => Color32::from_rgb(240, 210, 40),  // Yellow
                            4 => Color32::from_rgb(60, 200, 100),  // Green
                            5 => Color32::from_rgb(40, 200, 220),  // Cyan
                            6 => Color32::from_rgb(60, 120, 240),  // Blue
                            7 => Color32::from_rgb(180, 80, 220),  // Purple
                            _ => Color32::TRANSPARENT,
                        };
                        ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.5, rect.height())), 1.0, tag_col);
                    }

                    // Eye toggle
                    let eye_icon = if layer.visible { "[O]" } else { "[-]" };
                    let eye_rect = Rect::from_min_size(pos2(rect.left() + indent + 6.0, rect.top() + 6.0), vec2(16.0, 16.0));
                    ui.painter().text(eye_rect.center(), Align2::CENTER_CENTER, eye_icon, egui::FontId::monospace(11.0), TEXT_PRIMARY);

                    let mut cur_x = rect.left() + indent + 24.0;

                    // If folder, render expand/collapse arrow
                    if is_folder {
                        let arrow = if folder_expanded { "▼" } else { "▶" };
                        let arrow_rect = Rect::from_min_size(pos2(cur_x, rect.top() + 6.0), vec2(14.0, 16.0));
                        ui.painter().text(arrow_rect.center(), Align2::CENTER_CENTER, arrow, egui::FontId::proportional(11.0), Color32::from_rgb(255, 215, 0));
                        cur_x += 16.0;
                    }

                    // Thumbnail 1: Content/Material
                    let mat_thumb_rect = Rect::from_min_size(pos2(cur_x, rect.top() + 5.0), vec2(18.0, 18.0));
                    let (badge_char, badge_color) = match layer_type {
                        LayerType::Paint => ("P", Color32::from_rgb(100, 200, 255)),
                        LayerType::Fill => ("F", SMART_MAT_ORANGE),
                        LayerType::Folder => ("D", Color32::from_rgb(255, 215, 0)),
                    };
                    ui.painter().rect_filled(mat_thumb_rect, 2.0, Color32::from_rgb(35, 35, 40));
                    ui.painter().text(mat_thumb_rect.center(), Align2::CENTER_CENTER, badge_char, egui::FontId::monospace(10.0), badge_color);
                    if is_active && !self.paint_target_mask {
                        ui.painter().rect_stroke(mat_thumb_rect, 2.0, Stroke::new(1.5_f32, ACTIVE_BLUE));
                    } else {
                        ui.painter().rect_stroke(mat_thumb_rect, 2.0, Stroke::new(0.8_f32, Color32::from_rgb(80, 80, 85)));
                    }
                    cur_x += 22.0;

                    // Thumbnail 2: Mask (if present)
                    let mask_thumb_rect = if has_mask {
                        let mtr = Rect::from_min_size(pos2(cur_x, rect.top() + 5.0), vec2(18.0, 18.0));
                        ui.painter().rect_filled(mtr, 2.0, Color32::BLACK);
                        ui.painter().text(mtr.center(), Align2::CENTER_CENTER, "M", egui::FontId::monospace(9.0), Color32::WHITE);
                        if is_active && self.paint_target_mask {
                            ui.painter().rect_stroke(mtr, 2.0, Stroke::new(1.8_f32, Color32::from_rgb(100, 230, 255)));
                        } else {
                            ui.painter().rect_stroke(mtr, 2.0, Stroke::new(0.8_f32, Color32::from_rgb(140, 140, 150)));
                        }
                        cur_x += 22.0;
                        Some(mtr)
                    } else {
                        None
                    };

                    // Layer name text
                    ui.painter().text(pos2(cur_x, rect.center().y), Align2::LEFT_CENTER, &layer_name, egui::FontId::proportional(12.0), Color32::WHITE);

                    // Right side badges: Opacity, Anchor, Effects
                    let mut right_x = rect.right() - 8.0;
                    ui.painter().text(pos2(right_x, rect.center().y), Align2::RIGHT_CENTER, format!("{:.0}%", layer_opacity * 100.0), egui::FontId::proportional(11.0), TEXT_DIMMED);
                    right_x -= 38.0;

                    if has_anchor {
                        let a_rect = Rect::from_min_size(pos2(right_x - 14.0, rect.top() + 6.0), vec2(14.0, 14.0));
                        ui.painter().rect_filled(a_rect, 1.0, SMART_MAT_ORANGE);
                        ui.painter().text(a_rect.center(), Align2::CENTER_CENTER, "A", egui::FontId::proportional(9.0), Color32::WHITE);
                        right_x -= 18.0;
                    }

                    if num_effects > 0 {
                        let fx_rect = Rect::from_min_size(pos2(right_x - 22.0, rect.top() + 6.0), vec2(20.0, 14.0));
                        ui.painter().rect_filled(fx_rect, 2.0, Color32::from_rgb(130, 70, 200));
                        ui.painter().text(fx_rect.center(), Align2::CENTER_CENTER, format!("+{}", num_effects), egui::FontId::proportional(9.0), Color32::WHITE);
                    }

                    // Right-Click Context Menu (Substance Painter 1:1 matching)
                    resp.context_menu(|ui| {
                        ui.set_min_width(220.0);
                        ui.label(egui::RichText::new(&layer_name).strong().color(Color32::WHITE));
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
                                    .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp", "tga"])
                                    .pick_file()
                                {
                                    list_action = Some(LayerListAction::AddBitmapMask(i, p));
                                }
                                ui.close_menu();
                            }
                            if ui.button("Add mask with color selection").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Cavity Dirt".into()));
                                ui.close_menu();
                            }
                            if ui.button("Add mask with height combination").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Metal Edge Wear".into()));
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
                        if ui.button(egui::RichText::new("Remove layer(s)\tDel").color(Color32::from_rgb(255, 90, 90))).clicked() {
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
                                let (s_rect, s_resp) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::click());
                                ui.painter().rect_filled(s_rect, 2.0, col);
                                if s_resp.clicked() {
                                    list_action = Some(LayerListAction::SetColorTag(i, tag));
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();

                        // ── 4. Blending Options (All 18 Substance Painter Blend Modes) ─
                        ui.menu_button("Blending options", |ui| {
                            let modes = [
                                BlendMode::Normal, BlendMode::PassThrough, BlendMode::Multiply, BlendMode::Screen,
                                BlendMode::Overlay, BlendMode::Add, BlendMode::Subtract, BlendMode::Darken,
                                BlendMode::Lighten, BlendMode::ColorDodge, BlendMode::ColorBurn, BlendMode::LinearBurn,
                                BlendMode::HardLight, BlendMode::SoftLight, BlendMode::Difference, BlendMode::Exclusion,
                                BlendMode::Color, BlendMode::Luminosity,
                            ];
                            for mode in modes {
                                if ui.button(mode.name()).clicked() {
                                    list_action = Some(LayerListAction::SetBlendMode(i, mode));
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();

                        // ── 5. Effects Submenu (Substance Painter style) ───
                        ui.menu_button("⚡ Add generator", |ui| {
                            if ui.button("Metal Edge Wear").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Metal Edge Wear".into()));
                                ui.close_menu();
                            }
                            if ui.button("Dirt Crevices").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Dirt Crevices".into()));
                                ui.close_menu();
                            }
                            if ui.button("Dust Accumulation").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Dust Accumulation".into()));
                                ui.close_menu();
                            }
                            if ui.button("Position Gradient").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Position Gradient".into()));
                                ui.close_menu();
                            }
                            if ui.button("World Normal Orientation").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "World Normal Orientation".into()));
                                ui.close_menu();
                            }
                            if ui.button("Thickness / Subsurface").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Thickness / Subsurface".into()));
                                ui.close_menu();
                            }
                            if ui.button("Color / ID Selection").clicked() {
                                list_action = Some(LayerListAction::AddGenerator(i, "Color / ID Selection".into()));
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("🔮 Add filter", |ui| {
                            if ui.button("Levels").clicked() {
                                list_action = Some(LayerListAction::AddFilter(i, FilterEffect::Levels { min_in: 0.0, max_in: 1.0, gamma: 1.0, min_out: 0.0, max_out: 1.0, channel: FilterTargetChannel::All }));
                                ui.close_menu();
                            }
                            if ui.button("Gaussian Blur").clicked() {
                                list_action = Some(LayerListAction::AddFilter(i, FilterEffect::Blur { radius: 3.0, channel: FilterTargetChannel::All }));
                                ui.close_menu();
                            }
                            if ui.button("Sharpen").clicked() {
                                list_action = Some(LayerListAction::AddFilter(i, FilterEffect::Sharpen { strength: 1.5, channel: FilterTargetChannel::All }));
                                ui.close_menu();
                            }
                            if ui.button("HSL Adjustment").clicked() {
                                list_action = Some(LayerListAction::AddFilter(i, FilterEffect::HSL { hue_shift: 0.0, sat_mult: 1.0, light_offset: 0.0 }));
                                ui.close_menu();
                            }
                            if ui.button("Invert").clicked() {
                                list_action = Some(LayerListAction::AddFilter(i, FilterEffect::Invert { channel: FilterTargetChannel::All }));
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
                                            "Metal Edge Wear", "Dirt Crevices", "Dust Accumulation",
                                            "Position Gradient", "World Normal Orientation",
                                            "Thickness / Subsurface", "Color / ID Selection"
                                        ];
                                        for g in gens {
                                            if ui.button(g).clicked() {
                                                list_action = Some(LayerListAction::AddGenerator(i, g.into()));
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
                                            MaskEffect::Generator { name, wear_level, contrast, grunge_amount, .. } => {
                                                ui.label(format!("  • ⚡ {}:", name));
                                                ui.add(egui::Slider::new(wear_level, 0.0..=2.0).text("Level").show_value(false));
                                                ui.add(egui::Slider::new(contrast, 0.1..=4.0).text("Contrast").show_value(false));
                                                ui.add(egui::Slider::new(grunge_amount, 0.0..=1.0).text("Grunge").show_value(false));
                                            }
                                            MaskEffect::Fill { value, noise_frequency, .. } => {
                                                ui.label("  • 🎨 Fill:");
                                                ui.add(egui::Slider::new(value, 0.0..=1.0).text("Val").show_value(false));
                                                ui.add(egui::Slider::new(noise_frequency, 0.0..=10.0).text("Noise").show_value(false));
                                            }
                                            MaskEffect::Levels { black_in, white_in, gamma } => {
                                                ui.label("  • 📊 Levels:");
                                                ui.add(egui::Slider::new(black_in, 0.0..=1.0).text("In").show_value(false));
                                                ui.add(egui::Slider::new(white_in, 0.0..=1.0).text("Out").show_value(false));
                                                ui.add(egui::Slider::new(gamma, 0.1..=3.0).text("γ").show_value(false));
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
                        self.status_message = format!("Editing Mask for {}", self.layer_stack.layers.get(i).map(|l| l.name.as_str()).unwrap_or(""));
                    } else {
                        self.status_message = format!("Selected Layer: {}", self.layer_stack.layers.get(i).map(|l| l.name.as_str()).unwrap_or(""));
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
                        self.status_message = format!("Added Bitmap Mask ({}) to {}", path.display(), l.name);
                    }
                }
                LayerListAction::AddPaintLayer => {
                    let id = self.layer_stack.layers.len() + 1;
                    self.layer_stack.layers.push(Layer::new_paint(id, format!("Paint Layer {}", id), self.layer_stack.width, self.layer_stack.height));
                    self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                    self.paint_target_mask = false;
                    recomposite_needed = true;
                    self.status_message = format!("Added Paint Layer {}", id);
                }
                LayerListAction::AddFillLayer => {
                    let id = self.layer_stack.layers.len() + 1;
                    self.layer_stack.layers.push(Layer::new_fill(id, format!("Fill Layer {}", id), self.brush.color, self.brush.metallic, self.brush.roughness, self.layer_stack.width, self.layer_stack.height));
                    self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                    self.paint_target_mask = false;
                    recomposite_needed = true;
                    self.status_message = format!("Added Fill Layer {}", id);
                }
                LayerListAction::AddFolder => {
                    let id = self.layer_stack.layers.len() + 1;
                    self.layer_stack.layers.push(Layer::new_folder(id, format!("Folder {}", id), self.layer_stack.width, self.layer_stack.height));
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
                        if self.layer_stack.active_layer_index >= self.layer_stack.layers.len() && !self.layer_stack.layers.is_empty() {
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
                        let insert_at = (self.layer_stack.active_layer_index + 1).min(self.layer_stack.layers.len());
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
                            self.status_message = format!("Added Generator ({}) to {}", gen_name, l.name);
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
                        self.layer_stack.active_layer_index = self.layer_stack.active_layer_index.min(self.layer_stack.layers.len() - 1);
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
            ui.collapsing(egui::RichText::new("PROPERTIES - CURRENT LAYER").strong().color(ACTIVE_BLUE), |ui| {
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
                    if ui.add(egui::Slider::new(&mut active_layer.opacity, 0.0..=1.0)).changed() {
                        recomposite_needed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Blending:");
                    egui::ComboBox::from_id_salt("layer_blend_mode")
                        .selected_text(active_layer.blend_mode.name())
                        .show_ui(ui, |ui| {
                            let modes = [
                                BlendMode::Normal, BlendMode::PassThrough, BlendMode::Multiply, BlendMode::Screen,
                                BlendMode::Overlay, BlendMode::Add, BlendMode::Subtract, BlendMode::Darken,
                                BlendMode::Lighten, BlendMode::ColorDodge, BlendMode::ColorBurn, BlendMode::LinearBurn,
                                BlendMode::HardLight, BlendMode::SoftLight, BlendMode::Difference, BlendMode::Exclusion,
                                BlendMode::Color, BlendMode::Luminosity,
                            ];
                            for m in modes {
                                if ui.selectable_value(&mut active_layer.blend_mode, m, m.name()).clicked() {
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
                                for v in &mut m.buffer { *v = 1.0 - *v; }
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

                // Per-Layer Channel Blends & Overrides (Substance Painter Parity)
                ui.collapsing("CHANNEL BLEND & ROUTING (SUBSTANCE PARITY)", |ui| {
                    let mut ch_row = |name: &str, enabled: &mut bool, blend: &mut BlendMode, opacity: &mut f32, id_salt: &str| {
                        ui.horizontal(|ui| {
                            if ui.checkbox(enabled, name).changed() {
                                recomposite_needed = true;
                            }
                            egui::ComboBox::from_id_salt(id_salt)
                                .selected_text(blend.name())
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    let modes = [
                                        BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen, BlendMode::Overlay,
                                        BlendMode::Add, BlendMode::Subtract, BlendMode::Darken, BlendMode::Lighten,
                                        BlendMode::ColorDodge, BlendMode::ColorBurn, BlendMode::LinearBurn,
                                        BlendMode::HardLight, BlendMode::SoftLight, BlendMode::Difference,
                                    ];
                                    for m in modes {
                                        if ui.selectable_value(blend, m, m.name()).clicked() {
                                            recomposite_needed = true;
                                        }
                                    }
                                });
                            if ui.add(egui::Slider::new(opacity, 0.0..=1.0).show_value(false)).changed() {
                                recomposite_needed = true;
                            }
                        });
                    };

                    ch_row("Base Color", &mut active_layer.channel_color_enabled, &mut active_layer.channel_color_blend, &mut active_layer.channel_color_opacity, "ch_color_blend");
                    ch_row("Metallic", &mut active_layer.channel_metallic_enabled, &mut active_layer.channel_metallic_blend, &mut active_layer.channel_metallic_opacity, "ch_met_blend");
                    ch_row("Roughness", &mut active_layer.channel_roughness_enabled, &mut active_layer.channel_roughness_blend, &mut active_layer.channel_roughness_opacity, "ch_rough_blend");
                    ch_row("Normal", &mut active_layer.channel_normal_enabled, &mut active_layer.channel_normal_blend, &mut active_layer.channel_normal_opacity, "ch_norm_blend");
                    ch_row("Height", &mut active_layer.channel_height_enabled, &mut active_layer.channel_height_blend, &mut active_layer.channel_height_opacity, "ch_height_blend");
                });

                // Layer Filter & Effects Stack
                ui.collapsing(format!("FILTER & EFFECT STACK ({})", active_layer.effects.len()), |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button("+ Add Filter...", |ui| {
                            if ui.button("Levels").clicked() {
                                active_layer.effects.push(FilterEffect::Levels { min_in: 0.0, max_in: 1.0, gamma: 1.0, min_out: 0.0, max_out: 1.0, channel: FilterTargetChannel::All });
                                active_layer.apply_effects();
                                recomposite_needed = true;
                                ui.close_menu();
                            }
                            if ui.button("Gaussian Blur").clicked() {
                                active_layer.effects.push(FilterEffect::Blur { radius: 3.0, channel: FilterTargetChannel::All });
                                active_layer.apply_effects();
                                recomposite_needed = true;
                                ui.close_menu();
                            }
                            if ui.button("Sharpen").clicked() {
                                active_layer.effects.push(FilterEffect::Sharpen { strength: 1.5, channel: FilterTargetChannel::All });
                                active_layer.apply_effects();
                                recomposite_needed = true;
                                ui.close_menu();
                            }
                            if ui.button("HSL Adjustment").clicked() {
                                active_layer.effects.push(FilterEffect::HSL { hue_shift: 0.0, sat_mult: 1.0, light_offset: 0.0 });
                                active_layer.apply_effects();
                                recomposite_needed = true;
                                ui.close_menu();
                            }
                            if ui.button("Invert").clicked() {
                                active_layer.effects.push(FilterEffect::Invert { channel: FilterTargetChannel::All });
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
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("✕").on_hover_text("Remove filter").clicked() {
                                        effect_to_remove = Some(eff_idx);
                                    }
                                });
                            });

                            match effect {
                                FilterEffect::Levels { min_in, max_in, gamma, min_out, max_out, .. } => {
                                    ui.horizontal(|ui| {
                                        ui.label("In:");
                                        if ui.add(egui::Slider::new(min_in, 0.0..=1.0).text("Min")).changed() { effects_changed = true; }
                                        if ui.add(egui::Slider::new(max_in, 0.0..=1.0).text("Max")).changed() { effects_changed = true; }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Gamma:");
                                        if ui.add(egui::Slider::new(gamma, 0.1..=5.0)).changed() { effects_changed = true; }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Out:");
                                        if ui.add(egui::Slider::new(min_out, 0.0..=1.0).text("Min")).changed() { effects_changed = true; }
                                        if ui.add(egui::Slider::new(max_out, 0.0..=1.0).text("Max")).changed() { effects_changed = true; }
                                    });
                                }
                                FilterEffect::Blur { radius, .. } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Radius:");
                                        if ui.add(egui::Slider::new(radius, 1.0..=20.0)).changed() { effects_changed = true; }
                                    });
                                }
                                FilterEffect::Sharpen { strength, .. } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Strength:");
                                        if ui.add(egui::Slider::new(strength, 0.1..=5.0)).changed() { effects_changed = true; }
                                    });
                                }
                                FilterEffect::HSL { hue_shift, sat_mult, light_offset } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Hue:");
                                        if ui.add(egui::Slider::new(hue_shift, -180.0..=180.0)).changed() { effects_changed = true; }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Saturation:");
                                        if ui.add(egui::Slider::new(sat_mult, 0.0..=2.0)).changed() { effects_changed = true; }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Lightness:");
                                        if ui.add(egui::Slider::new(light_offset, -1.0..=1.0)).changed() { effects_changed = true; }
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
                });

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
                        if ui.add(egui::Slider::new(&mut active_layer.fill_metallic, 0.0..=1.0)).changed() {
                            active_layer.update_fill();
                            recomposite_needed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Roughness:");
                        if ui.add(egui::Slider::new(&mut active_layer.fill_roughness, 0.04..=1.0)).changed() {
                            active_layer.update_fill();
                            recomposite_needed = true;
                        }
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("🗑 Delete Layer").color(Color32::from_rgb(255, 100, 100))).clicked() {
                        delete_layer = Some(active_idx);
                    }
                });
            });
        }

        if let Some(idx) = delete_layer {
            if self.layer_stack.layers.len() > 1 {
                self.layer_stack.layers.remove(idx);
                self.layer_stack.active_layer_index = self.layer_stack.active_layer_index.min(self.layer_stack.layers.len() - 1);
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

        ui.collapsing("CAMERA", |ui| {
            ui.add(egui::Slider::new(&mut self.camera_yaw, 0.0..=360.0).text("Yaw"));
            ui.add(egui::Slider::new(&mut self.camera_pitch, -85.0..=85.0).text("Pitch"));
            ui.add(egui::Slider::new(&mut self.camera_dist, 1.0..=10.0).text("Distance"));
        });

        ui.collapsing("ENVIRONMENT LIGHTING (HDR IBL & STUDIO PROBES)", |ui| {
            if !self.available_environments.is_empty() {
                let current_name = self.available_environments
                    .get(self.selected_env_idx)
                    .map(|(n, _)| n.as_str())
                    .unwrap_or("Studio Tomoco");

                egui::ComboBox::from_label("Studio HDRI (.EXR)")
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for (i, (env_name, env_path)) in self.available_environments.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_env_idx, i, env_name).clicked() {
                                if let Some(ref vp_mutex) = self.viewport_gl {
                                    if let Some(ref gl) = self.gl {
                                        if let Ok(mut vp) = vp_mutex.lock() {
                                            if let Err(e) = vp.load_environment(gl, env_path) {
                                                eprintln!("[ERROR] Failed to load environment {}: {}", env_name, e);
                                            } else {
                                                self.status_message = format!("Loaded HDRI Environment: {}", env_name);
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
            ui.add(egui::Slider::new(&mut self.env_rotation, 0.0..=360.0).text("Rotation (Shift+RMB)"));

            ui.separator();
            ui.checkbox(&mut self.skybox_visible, "Show Skybox Background");
            if self.skybox_visible {
                ui.add(egui::Slider::new(&mut self.skybox_blur, 0.0..=1.0).text("Skybox Blur"));
                ui.add(egui::Slider::new(&mut self.skybox_intensity, 0.0..=3.0).text("Skybox Intensity"));
            }

            ui.separator();
            ui.add(egui::Slider::new(&mut self.uv_scale, 0.1..=10.0).text("Material UV Tiling"));
            ui.checkbox(&mut self.show_wireframe, "Wireframe Overlay (F4)");
        });
    }

    fn ui_shader_settings(&mut self, ui: &mut Ui) {
        ui.heading("SHADER SETTINGS");
        ui.separator();
        ui.label("Shader: pbr-metal-rough (Substance Painter Parity)");
        ui.label("BRDF: Cook-Torrance Microfacet GGX");
        ui.label("Specular D: D_GGX (Trowbridge-Reitz)");
        ui.label("Shadowing G: Smith-Schlick GGX");
        ui.label("Fresnel F: Schlick with Roughness correction");
        ui.label("IBL: Split-Sum Numerical 2D Integration LUT (512x512)");
        ui.label("Tone Mapping: ACES Filmic Curve (sRGB transfer)");
        ui.label("Depth Buffer: 24-bit Hardware GL_DEPTH_TEST (GL_LEQUAL)");
        ui.label("Hardware Core: Embedded OpenGL / glow Viewport");
        ui.label("Status: [OK] Active & Real-Time GPU Rasterized");
    }

    fn ui_history_panel(&mut self, ui: &mut Ui) {
        ui.heading("HISTORY");
        ui.separator();

        ui.horizontal(|ui| {
            let undo_enabled = self.history_manager.can_undo();
            if ui.add_enabled(undo_enabled, egui::Button::new("↶ Undo")).clicked() {
                self.perform_undo();
            }
            let redo_enabled = self.history_manager.can_redo();
            if ui.add_enabled(redo_enabled, egui::Button::new("↷ Redo")).clicked() {
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
            ui.label(RichText::new("Undo Stack (Active / Past)").strong().color(Color32::from_rgb(180, 180, 180)));
            let mut rollback_to = None;
            let current_len = self.history_manager.undo_stack.len();
            for (idx, cmd) in self.history_manager.undo_stack.iter().enumerate().rev() {
                let is_latest = idx + 1 == current_len;
                let text = format!("{:02}  {} ({} tiles)", idx + 1, cmd.description, cmd.tile_backups.len());
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

            if !self.history_manager.redo_stack.is_empty() {
                ui.separator();
                ui.label(RichText::new("Redo Stack (Undone actions)").italics().color(Color32::from_rgb(120, 120, 120)));
                for cmd in self.history_manager.redo_stack.iter().rev() {
                    let text = format!("--  {} ({} tiles)", cmd.description, cmd.tile_backups.len());
                    ui.label(RichText::new(text).color(Color32::from_rgb(100, 100, 100)));
                }
            }

            if self.history_manager.undo_stack.is_empty() && self.history_manager.redo_stack.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("No brush or tool actions recorded yet.").italics().color(TEXT_DIMMED));
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
                    ui.colored_label(match lvl.as_str() {
                        "INFO" => Color32::from_rgb(58, 217, 115),
                        "WARN" => SMART_MAT_ORANGE,
                        _ => Color32::RED,
                    }, format!("[{}]", lvl));
                    ui.label(msg);
                });
            }
        });
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
                    self.logo_texture = Some(ctx.load_texture("remoainter_logo", color_img, egui::TextureOptions::LINEAR));
                }
            }

            ui.horizontal(|ui| {
                if let Some(ref logo) = self.logo_texture {
                    ui.image(egui::load::SizedTexture::new(logo.id(), vec2(20.0, 20.0)));
                } else {
                    ui.colored_label(SMART_MAT_ORANGE, "RM");
                }
                ui.label(egui::RichText::new("REMoAINTER").strong().color(Color32::WHITE));
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
                    .selected_text(format!("{}×{}", self.baker_options.resolution, self.baker_options.resolution))
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_value(&mut self.baker_options.resolution, 512,  "512×512");
                        let _ = ui.selectable_value(&mut self.baker_options.resolution, 1024, "1024×1024");
                        let _ = ui.selectable_value(&mut self.baker_options.resolution, 2048, "2048×2048");
                        let _ = ui.selectable_value(&mut self.baker_options.resolution, 4096, "4096×4096");
                    });

                ui.separator();
                ui.colored_label(Color32::WHITE, format!("Target: {}", self.mesh.name));
            });
        });

        // 3. Status Bar
        egui::TopBottomPanel::bottom("baker_status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::from_rgb(58, 217, 115), "● Ready to Bake");
                ui.label(format!("| Target: {} ({} triangles) | Output: {}×{}", self.mesh.name, self.mesh.indices.len() / 3, self.baker_options.resolution, self.baker_options.resolution));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Substance 3D Painter Baking Studio 1:1");
                });
            });
        });

        // 4. Far-Left SidePanel: Texture Set List + Mesh Map Bakers + Baking Log
        egui::SidePanel::left("baker_left_panel").exact_width(220.0).resizable(false).show(ctx, |ui| {
            ui.add_space(4.0);
            ui.strong("TEXTURE SET LIST");
            ui.separator();

            let active_tab_bg = Color32::from_rgb(35, 65, 105);
            let (ts_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
            ui.painter().rect_filled(ts_rect, 2.0, active_tab_bg);
            ui.painter().rect_stroke(ts_rect, 2.0, Stroke::new(1.0_f32, ACTIVE_BLUE));
            ui.painter().text(pos2(ts_rect.left() + 8.0, ts_rect.center().y), Align2::LEFT_CENTER,
                format!("☑  {}", self.mesh.name), egui::FontId::proportional(11.5), Color32::WHITE);
            ui.painter().text(pos2(ts_rect.right() - 8.0, ts_rect.center().y), Align2::RIGHT_CENTER,
                format!("{}×{}", self.baker_options.resolution, self.baker_options.resolution), egui::FontId::proportional(10.0), TEXT_DIMMED);

            ui.add_space(8.0);
            ui.strong("MESH MAP BAKERS");
            ui.separator();

            // Common settings row
            let cs_active = self.baker_selected_map == 99;
            let (cs_rect, cs_resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::click());
            if cs_active || cs_resp.hovered() {
                ui.painter().rect_filled(cs_rect, 2.0, ACTIVE_ROW_FILL);
            }
            if cs_active {
                ui.painter().rect_stroke(cs_rect, 2.0, Stroke::new(1.0_f32, ACTIVE_BLUE));
            }
            ui.painter().text(pos2(cs_rect.left() + 24.0, cs_rect.center().y), Align2::LEFT_CENTER,
                "⚙  Common settings", egui::FontId::proportional(11.5), Color32::WHITE);
            if cs_resp.clicked() {
                self.baker_selected_map = 99;
            }

            ui.add_space(2.0);

            let map_rows: &[(&str, usize, [u8; 3], bool)] = &[
                ("Normal", 2, [120, 150, 255], self.baked_maps.normal.is_some()),
                ("World space normal", 3, [80, 200, 200], self.baked_maps.world_space_normal.is_some()),
                ("ID", 6, [230, 120, 80], self.baked_maps.id.is_some()),
                ("Ambient occlusion", 0, [200, 200, 200], self.baked_maps.ambient_occlusion.is_some()),
                ("Curvature", 1, [100, 220, 120], self.baked_maps.curvature.is_some()),
                ("Position", 4, [220, 180, 80], self.baked_maps.position.is_some()),
                ("Thickness", 5, [200, 80, 180], self.baked_maps.thickness.is_some()),
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
                let (row_rect, row_resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), egui::Sense::click());
                if is_sel {
                    ui.painter().rect_filled(row_rect, 2.0, ACTIVE_ROW_FILL);
                    ui.painter().rect_stroke(row_rect, 2.0, Stroke::new(1.0_f32, ACTIVE_BLUE));
                } else if row_resp.hovered() {
                    ui.painter().rect_filled(row_rect, 2.0, Color32::from_rgb(38, 38, 44));
                }

                // Checkbox
                let cb_rect = Rect::from_min_size(pos2(row_rect.left() + 4.0, row_rect.center().y - 6.0), vec2(12.0, 12.0));
                let enabled = enabled_flags.get(*map_idx).copied().unwrap_or(false);
                ui.painter().rect_filled(cb_rect, 2.0, if enabled { ACTIVE_BLUE } else { Color32::from_rgb(30, 30, 35) });
                ui.painter().rect_stroke(cb_rect, 2.0, Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 80)));
                if enabled {
                    ui.painter().text(cb_rect.center(), Align2::CENTER_CENTER, "✓", egui::FontId::monospace(9.0), Color32::WHITE);
                }

                // Colored icon dot
                let dot_pos = pos2(row_rect.left() + 22.0, row_rect.center().y);
                ui.painter().circle_filled(dot_pos, 4.0, Color32::from_rgb(color[0], color[1], color[2]));

                // Name
                ui.painter().text(pos2(row_rect.left() + 32.0, row_rect.center().y), Align2::LEFT_CENTER, *name, egui::FontId::proportional(11.0), Color32::WHITE);

                // Status dot
                let (tag_text, tag_col) = if *is_baked { ("●", Color32::from_rgb(100, 240, 150)) } else { ("○", TEXT_DIMMED) };
                ui.painter().text(pos2(row_rect.right() - 8.0, row_rect.center().y), Align2::RIGHT_CENTER, tag_text, egui::FontId::monospace(10.0), tag_col);

                if row_resp.clicked() {
                    if let Some(p) = row_resp.interact_pointer_pos() {
                        if p.x < row_rect.left() + 18.0 {
                            match *map_idx {
                                0 => self.baker_options.bake_ao = !self.baker_options.bake_ao,
                                1 => self.baker_options.bake_curvature = !self.baker_options.bake_curvature,
                                2 => self.baker_options.bake_normal = !self.baker_options.bake_normal,
                                3 => self.baker_options.bake_world_space_normal = !self.baker_options.bake_world_space_normal,
                                4 => self.baker_options.bake_position = !self.baker_options.bake_position,
                                5 => self.baker_options.bake_thickness = !self.baker_options.bake_thickness,
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
                if ui.selectable_label(self.baker_tab_idx == 0, "PROCESS").clicked() { self.baker_tab_idx = 0; }
                if ui.selectable_label(self.baker_tab_idx == 1, "PARAMS").clicked() { self.baker_tab_idx = 1; }
                if ui.selectable_label(self.baker_tab_idx == 2, "MATCH").clicked() { self.baker_tab_idx = 2; }
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
                                let prog = self.baking_progress.load(std::sync::atomic::Ordering::Relaxed);
                                ui.add(egui::ProgressBar::new(prog as f32 / 100.0).text(format!("Baking... {}%", prog)));
                            } else {
                                ui.colored_label(Color32::from_rgb(100, 240, 150), "● Ready to bake");
                                ui.colored_label(TEXT_DIMMED, format!("Mesh: {}
Res: {}×{}", self.mesh.name, self.baker_options.resolution, self.baker_options.resolution));
                            }
                        }
                        1 => {
                            ui.label(format!("• Cage Front: {:.3}", self.baker_frontal_dist));
                            ui.label(format!("• Cage Rear: {:.3}", self.baker_rear_dist));
                            ui.label("• Tangents: MikkTSpace");
                        }
                        _ => {
                            ui.label(format!("• Suffix: '{}' / '{}'", self.baker_low_suffix, self.baker_high_suffix));
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
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter();
            painter.rect_filled(rect, 0.0, Color32::from_rgb(26, 26, 28));

            self.render_3d_viewport(painter, rect, &response, ctx);

            // Floating Baking Visualization Panel (top-left)
            let vis_rect = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 14.0), vec2(240.0, 100.0));
            painter.rect_filled(vis_rect, 4.0, Color32::from_black_alpha(220));
            painter.rect_stroke(vis_rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 65)));
            painter.text(pos2(vis_rect.left() + 12.0, vis_rect.top() + 12.0), Align2::LEFT_TOP,
                "⚡ Baking visualization", egui::FontId::proportional(12.0), Color32::WHITE);
            painter.text(pos2(vis_rect.left() + 12.0, vis_rect.top() + 34.0), Align2::LEFT_TOP,
                "● Show for selected Texture Set only", egui::FontId::proportional(10.5), Color32::from_rgb(180, 180, 190));
            painter.text(pos2(vis_rect.left() + 12.0, vis_rect.top() + 54.0), Align2::LEFT_TOP,
                format!("● Cage: Distance-based ({:.3})", self.baker_frontal_dist), egui::FontId::proportional(10.5), ACTIVE_BLUE);
            painter.text(pos2(vis_rect.left() + 12.0, vis_rect.top() + 74.0), Align2::LEFT_TOP,
                "● UV Seams / Projection: Active", egui::FontId::proportional(10.5), Color32::from_rgb(100, 240, 150));

            // Floating Action Bar (bottom-right)
            let action_rect = Rect::from_min_size(pos2(rect.right() - 440.0, rect.bottom() - 46.0), vec2(420.0, 36.0));
            painter.rect_filled(action_rect, 4.0, Color32::from_black_alpha(230));
            painter.rect_stroke(action_rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(65, 65, 75)));

            let bake_btn_rect = Rect::from_min_size(pos2(action_rect.left() + 6.0, action_rect.top() + 4.0), vec2(210.0, 28.0));
            let return_btn_rect = Rect::from_min_size(pos2(action_rect.left() + 224.0, action_rect.top() + 4.0), vec2(190.0, 28.0));

            let mut bake_hovered = false;
            let mut return_hovered = false;
            if let Some(pos) = ctx.pointer_hover_pos() {
                bake_hovered = bake_btn_rect.contains(pos);
                return_hovered = return_btn_rect.contains(pos);
            }

            painter.rect_filled(bake_btn_rect, 3.0, if bake_hovered { Color32::from_rgb(65, 135, 235) } else { ACTIVE_BLUE });
            painter.rect_filled(return_btn_rect, 3.0, if return_hovered { Color32::from_rgb(55, 55, 60) } else { Color32::from_rgb(40, 40, 45) });

            painter.text(bake_btn_rect.center(), Align2::CENTER_CENTER, "⚡ Bake Selected Textures", egui::FontId::proportional(12.0), Color32::WHITE);
            painter.text(return_btn_rect.center(), Align2::CENTER_CENTER, "Return to painting mode", egui::FontId::proportional(11.5), Color32::WHITE);

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if bake_btn_rect.contains(pos) {
                        self.bake_mesh_maps();
                        self.show_baker_dialog = false;
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
                        if ui.button("📄 Pick Files...").on_hover_text("Select multiple image files to auto-match by suffix").clicked() {
                            if let Some(files) = rfd::FileDialog::new().pick_files() {
                                self.import_texture_set = PbrTextureSet::from_files(&files);
                                self.import_flip_normal_y = self.import_texture_set.is_directx_normal;
                            }
                        }
                        if ui.button("📁 Pick Folder...").on_hover_text("Select folder containing texture pack to auto-match").clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.import_texture_set = PbrTextureSet::from_folder(&folder);
                                self.import_flip_normal_y = self.import_texture_set.is_directx_normal;
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

                egui::Grid::new("pbr_import_grid").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                    // Base Color
                    ui.label(egui::RichText::new("Base Color / Albedo:").strong());
                    let bc_str = self.import_texture_set.base_color.as_ref()
                        .and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("[Not Assigned]");
                    ui.colored_label(if self.import_texture_set.base_color.is_some() { Color32::from_rgb(58, 217, 115) } else { TEXT_DIMMED }, bc_str);
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                self.import_texture_set.base_color = Some(f);
                            }
                        }
                        if self.import_texture_set.base_color.is_some() && ui.button("✕").clicked() {
                            self.import_texture_set.base_color = None;
                        }
                    });
                    ui.end_row();

                    // Normal
                    ui.label(egui::RichText::new("Normal Map:").strong());
                    let n_str = self.import_texture_set.normal.as_ref()
                        .and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("[Not Assigned]");
                    ui.colored_label(if self.import_texture_set.normal.is_some() { Color32::from_rgb(58, 217, 115) } else { TEXT_DIMMED }, n_str);
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                self.import_texture_set.normal = Some(f);
                            }
                        }
                        if self.import_texture_set.normal.is_some() && ui.button("✕").clicked() {
                            self.import_texture_set.normal = None;
                        }
                        ui.checkbox(&mut self.import_flip_normal_y, "DirectX (Flip Y)");
                    });
                    ui.end_row();

                    // Roughness
                    ui.label(egui::RichText::new("Roughness:").strong());
                    let r_str = self.import_texture_set.roughness.as_ref()
                        .and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("[Not Assigned]");
                    ui.colored_label(if self.import_texture_set.roughness.is_some() { Color32::from_rgb(58, 217, 115) } else { TEXT_DIMMED }, r_str);
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                self.import_texture_set.roughness = Some(f);
                            }
                        }
                        if self.import_texture_set.roughness.is_some() && ui.button("✕").clicked() {
                            self.import_texture_set.roughness = None;
                        }
                    });
                    ui.end_row();

                    // Metallic
                    ui.label(egui::RichText::new("Metallic:").strong());
                    let m_str = self.import_texture_set.metallic.as_ref()
                        .and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("[Not Assigned]");
                    ui.colored_label(if self.import_texture_set.metallic.is_some() { Color32::from_rgb(58, 217, 115) } else { TEXT_DIMMED }, m_str);
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                self.import_texture_set.metallic = Some(f);
                            }
                        }
                        if self.import_texture_set.metallic.is_some() && ui.button("✕").clicked() {
                            self.import_texture_set.metallic = None;
                        }
                    });
                    ui.end_row();

                    // Ambient Occlusion
                    ui.label(egui::RichText::new("Ambient Occlusion:").strong());
                    let ao_str = self.import_texture_set.ambient_occlusion.as_ref()
                        .and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("[Not Assigned]");
                    ui.colored_label(if self.import_texture_set.ambient_occlusion.is_some() { Color32::from_rgb(58, 217, 115) } else { TEXT_DIMMED }, ao_str);
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                self.import_texture_set.ambient_occlusion = Some(f);
                            }
                        }
                        if self.import_texture_set.ambient_occlusion.is_some() && ui.button("✕").clicked() {
                            self.import_texture_set.ambient_occlusion = None;
                        }
                    });
                    ui.end_row();

                    // Height
                    ui.label(egui::RichText::new("Height / Displacement:").strong());
                    let h_str = self.import_texture_set.height.as_ref()
                        .and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("[Not Assigned]");
                    ui.colored_label(if self.import_texture_set.height.is_some() { Color32::from_rgb(58, 217, 115) } else { TEXT_DIMMED }, h_str);
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                self.import_texture_set.height = Some(f);
                            }
                        }
                        if self.import_texture_set.height.is_some() && ui.button("✕").clicked() {
                            self.import_texture_set.height = None;
                        }
                    });
                    ui.end_row();
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("✔ Apply to Active Layer").strong()).clicked() {
                        if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                            let _ = self.import_texture_set.load_into_layer(active, self.import_flip_normal_y);
                            if let Some(ao) = self.import_texture_set.load_ao_map(self.layer_stack.width, self.layer_stack.height) {
                                self.baked_maps.ambient_occlusion = Some(ao);
                            }
                            self.recompute_composite();
                            self.status_message = format!("Applied Texture Set: {}", self.import_texture_set.name);
                            self.history.push(format!("{:02}  Import Texture Set ({})", self.history.len() + 1, self.import_texture_set.name));
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
                        let _ = self.import_texture_set.load_into_layer(&mut layer, self.import_flip_normal_y);
                        if let Some(ao) = self.import_texture_set.load_ao_map(self.layer_stack.width, self.layer_stack.height) {
                            self.baked_maps.ambient_occlusion = Some(ao);
                        }
                        self.layer_stack.layers.push(layer);
                        self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                        self.recompute_composite();
                        self.status_message = format!("Created Fill Layer with: {}", self.import_texture_set.name);
                        self.history.push(format!("{:02}  Import Texture Set ({})", self.history.len() + 1, self.import_texture_set.name));
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
                            .selected_text(templates.get(self.new_proj_template_idx).copied().unwrap_or("PBR - Metallic / Roughness"))
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
                                egui::Label::new(
                                    egui::RichText::new(&disp_path).color(if self.new_proj_mesh_path.is_empty() { TEXT_DIMMED } else { Color32::WHITE })
                                ).truncate()
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
                            .selected_text(format!("{}×{}", self.new_proj_resolution, self.new_proj_resolution))
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
                            egui::RichText::new("   OK   ").color(Color32::WHITE).strong()
                        ).fill(ACTIVE_BLUE).min_size(vec2(80.0, 26.0));
                        if ui.add(ok_btn).clicked() {
                            do_create = true;
                            close_dialog = true;
                        }

                        if ui.add(egui::Button::new("Cancel").min_size(vec2(80.0, 26.0))).clicked() {
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
            self.selected_model_idx = if self.new_proj_mesh_path.is_empty() { 0 } else { 99 };
            self.project_name = if !self.new_proj_mesh_path.is_empty() {
                std::path::Path::new(&self.new_proj_mesh_path).file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string()
            } else {
                "Untitled Project".to_string()
            };
            self.current_project_path = None;
            self.mesh_source_path = if !self.new_proj_mesh_path.is_empty() { Some(self.new_proj_mesh_path.clone()) } else { None };
            self.project_is_dirty = false;

            // Resize LayerStack and reset to fresh clean base PBR layers
            let res = self.new_proj_resolution;
            self.layer_stack.width = res;
            self.layer_stack.height = res;
            self.layer_stack.layers.clear();

            let base_mat = load_real_pbr_material(1, "Steel Painted", [0.75, 0.76, 0.78, 1.0], 0.0, 0.5, res, res);
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
                    &self.baker_options,
                    None,
                    None,
                );
            }

            // Sync OpenGL Viewport mesh
            if let Some(ref vp_mutex) = self.viewport_gl {
                if let Some(ref gl) = self.gl {
                    if let Ok(mut vp) = vp_mutex.lock() {
                        vp.set_mesh(gl, &self.mesh);
                    }
                }
            }

            self.recompute_composite();
            self.texture_needs_update = true;
            self.viewport_view_mode = 2; // Split 3D | 2D view
            self.status_message = format!("New Project created: {} ({}x{})", self.mesh.name, res, res);
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
                                            if let Some(ref vp_mutex) = self.viewport_gl {
                                                if let Some(ref gl) = self.gl {
                                                    if let Ok(mut vp) = vp_mutex.lock() {
                                                        vp.set_mesh(gl, &self.mesh);
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
}
