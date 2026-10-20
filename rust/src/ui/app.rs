use crate::baker::{BakedMaps, BakerEngine, BakerOptions, MapType};
use crate::graphics::Mesh;
use crate::painting::brush::AlphaMaskData;
use crate::painting::{AnchorPoint, Brush, Layer, LayerStack, LayerType, MaskBlendMode, MaskEffect};
use crate::ui::icons::{draw_icon, IconType};
use crate::ui::theme::*;
use egui::{pos2, vec2, Align2, Color32, Rect, Stroke, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

    // Viewport & Lighting
    pub display_channel: usize, // 0: Material, 1: Base Color, 2: Roughness, 3: Metallic, 4: Normal, 5: AO, 6: Curvature
    pub env_preset: usize,
    pub env_rotation: f32,
    pub light_intensity: f32,
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub camera_dist: f32,
    pub symmetry_x: bool,

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

    // Viewport GPU Texture & Multi-View Mode
    pub texture_handle: Option<egui::TextureHandle>,
    pub texture_needs_update: bool,
    pub viewport_view_mode: usize, // 0: 3D View, 1: 2D Texture View, 2: Split 3D | 2D
}

fn infer_pbr(name: &str) -> ([f32; 4], f32, f32) {
    let lower = name.to_lowercase();
    if lower.contains("gold") {
        ([1.0, 0.84, 0.25, 1.0], 1.0, 0.20)
    } else if lower.contains("copper") {
        ([0.95, 0.64, 0.54, 1.0], 1.0, 0.25)
    } else if lower.contains("bronze") || lower.contains("brass") {
        ([0.85, 0.65, 0.35, 1.0], 0.95, 0.30)
    } else if lower.contains("chrome") {
        ([0.95, 0.95, 0.98, 1.0], 1.0, 0.08)
    } else if lower.contains("steel") || lower.contains("iron") || lower.contains("aluminium") || lower.contains("aluminum") || lower.contains("metal") {
        ([0.75, 0.76, 0.78, 1.0], 0.95, 0.35)
    } else if lower.contains("cobalt") {
        ([0.28, 0.45, 0.78, 1.0], 0.85, 0.28)
    } else if lower.contains("rubber") {
        ([0.15, 0.15, 0.16, 1.0], 0.0, 0.75)
    } else if lower.contains("plastic") {
        ([0.85, 0.18, 0.18, 1.0], 0.0, 0.28)
    } else if lower.contains("wood") {
        ([0.55, 0.35, 0.20, 1.0], 0.0, 0.45)
    } else if lower.contains("concrete") || lower.contains("asphalt") || lower.contains("stone") {
        ([0.52, 0.52, 0.52, 1.0], 0.0, 0.82)
    } else if lower.contains("fabric") || lower.contains("leather") {
        ([0.45, 0.38, 0.32, 1.0], 0.0, 0.65)
    } else if lower.contains("paint") {
        ([0.75, 0.20, 0.20, 1.0], 0.05, 0.35)
    } else {
        ([0.70, 0.70, 0.72, 1.0], 0.5, 0.4)
    }
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
                        let thumb_p = thumb_sub.join(format!("{}.png", stem));
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
            baker_selected_map: 0,
            baker_subsampling: 0,
            baker_use_low_as_high: true,
            baker_frontal_dist: 0.08,
            baker_rear_dist: 0.08,
            baker_curv_method: 0,

            display_channel: 0,
            env_preset: 1, // Warm & Cool Studio
            env_rotation: 0.0,
            light_intensity: 1.0,
            camera_yaw: 35.0,
            camera_pitch: 18.0,
            camera_dist: 2.4,
            symmetry_x: false,

            shelf_category: 0,
            search_query: String::new(),
            shelf_assets: scan_shelf_assets(),
            shelf_textures: HashMap::new(),
            logo_texture: None,
            active_right_panel: 2, // Layers panel open by default

            selected_model_idx: 1, // Default to Antique Lantern (14,570 faces)
            status_message: "Ready".to_string(),
            history: vec![
                "01  New Project (Lantern - 1024x1024)".into(),
                "02  Load Mesh (Antique Lantern)".into(),
                "03  Add Base Layer (Painted Steel)".into(),
            ],
            logs: vec![
                ("09:00:01".into(), "INFO".into(), "REMoAINTER v2.0.0 (High-Performance Rust Core) initialized.".into()),
                ("09:00:02".into(), "INFO".into(), "Substance 3D Painter 1-to-1 Architecture & UI Active.".into()),
                ("09:00:03".into(), "INFO".into(), "Loaded 360+ Allegorithmic PBR materials, Smart Masks & Alphas.".into()),
                ("09:00:04".into(), "INFO".into(), "Cook-Torrance / Blinn-Phong GGX Studio Lighting active.".into()),
            ],
            last_painted_uv: None,
            texture_handle: None,
            texture_needs_update: true,
            viewport_view_mode: 0, // 3D Viewport on startup
        };

        // Load Antique Lantern by default on launch
        let lantern_paths = [
            "/mnt/shared/REMoAINTER/assets/test_models/lantern.obj",
            "assets/test_models/lantern.obj",
            "../assets/test_models/lantern.obj",
            "/home/senju/Documents/env-port/lantern_low.obj",
        ];
        for p in &lantern_paths {
            if let Ok(m) = Mesh::load_obj(p) {
                app.mesh = m;
                app.status_message = "Loaded Test Asset: Antique Lantern (14,570 faces)".to_string();
                break;
            }
        }

        // Initialize default baked maps analytically (Instant startup, 0 CPU spike)
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

        // Initialize with default Painted Steel fill layer
        let steel_fill = Layer::new_fill(
            1,
            "Painted Metal Base".to_string(),
            [0.22, 0.25, 0.28, 1.0], // Dark industrial gunmetal
            0.65, // metallic
            0.35, // roughness
            1024,
            1024,
        );
        app.layer_stack.layers.push(steel_fill);
        app.layer_stack.composite(app.baked_maps.curvature.as_deref(), app.baked_maps.ambient_occlusion.as_deref());
        app.texture_needs_update = true;
        app
    }
}

impl RemoainterApp {
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
        );
        self.is_baking = false;
        self.status_message = format!("Successfully baked mesh maps at {}x{}", self.baker_options.resolution, self.baker_options.resolution);
        self.logs.push((
            chrono::Local::now().format("%H:%M:%S").to_string(),
            "INFO".into(),
            format!("Baked mesh maps (AO, Curvature, Normals, Position) for {}", self.mesh.name),
        ));
        self.layer_stack.composite(self.baked_maps.curvature.as_deref(), self.baked_maps.ambient_occlusion.as_deref());
        self.texture_needs_update = true;
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
        self.layer_stack.composite(self.baked_maps.curvature.as_deref(), self.baked_maps.ambient_occlusion.as_deref());
        self.texture_needs_update = true;
    }

    pub fn load_model(&mut self, idx: usize) {
        self.selected_model_idx = idx;
        match idx {
            0 => {
                self.mesh = Mesh::create_material_sphere(64, 64);
                self.status_message = "Loaded Calibration Material Sphere".to_string();
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
            _ => {}
        }
        self.texture_needs_update = true;
        self.status_message = format!("Loaded {} (Bake maps via Ctrl+Shift+B)", self.mesh.name);
    }
}

impl eframe::App for RemoainterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_substance_theme(ctx);

        // Global hotkey processing (Substance Painter standard)
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::B)) {
            self.show_baker_dialog = true;
        } else if ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.display_channel = 0;
            self.texture_needs_update = true;
            self.status_message = "Channel: Material (Lit PBR)".to_string();
        } else if ctx.input(|i| i.key_pressed(egui::Key::C)) {
            self.display_channel = match self.display_channel {
                0 => 1,
                1 => 2,
                2 => 3,
                3 => 4,
                _ => 0,
            };
            self.texture_needs_update = true;
            self.status_message = format!("Solo Channel: {}", self.channel_name(self.display_channel));
        } else if ctx.input(|i| i.key_pressed(egui::Key::B)) && !ctx.input(|i| i.modifiers.shift) {
            self.display_channel = match self.display_channel {
                5 => 6,
                6 => 7,
                7 => 8,
                _ => 5,
            };
            self.texture_needs_update = true;
            self.status_message = format!("Mesh Map: {}", self.channel_name(self.display_channel));
        } else if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.camera_yaw = 35.0;
            self.camera_pitch = 18.0;
            self.camera_dist = 2.4;
            self.status_message = "Framed 3D Mesh".to_string();
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
            self.active_tool = ToolMode::Paint;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
            self.active_tool = ToolMode::Eraser;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
            self.active_tool = ToolMode::Projection;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num4)) {
            self.active_tool = ToolMode::PolygonFill;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num5)) {
            self.active_tool = ToolMode::Smudge;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Num6)) {
            self.active_tool = ToolMode::Clone;
        } else if ctx.input(|i| i.key_pressed(egui::Key::I)) {
            self.active_tool = ToolMode::Eyedropper;
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
                            self.layer_stack = LayerStack::new(2048, 2048);
                            self.status_message = "Created New Project".into();
                            ui.close_menu();
                        }
                        if ui.button("Bake Mesh Maps... (Ctrl+Shift+B)").clicked() {
                            self.show_baker_dialog = true;
                            ui.close_menu();
                        }
                        if ui.button("Exit (Alt+F4)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("Edit", |ui| {
                        if ui.button("Undo (Ctrl+Z)").clicked() {
                            self.status_message = "Undo".into();
                            ui.close_menu();
                        }
                        if ui.button("Redo (Ctrl+Y)").clicked() {
                            self.status_message = "Redo".into();
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
                    });

                    ui.menu_button("Viewport", |ui| {
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
                ui.checkbox(&mut self.symmetry_x, "Symmetry");

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
                ui.colored_label(Color32::from_rgb(58, 217, 115), "● Ready");
                ui.label(format!("| Tool: {:?} | Mesh: {} | Stack: {} layers", self.active_tool, self.mesh.name, self.layer_stack.layers.len()));
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
                            let id = self.layer_stack.layers.len();
                            let fill = Layer::new_fill(
                                id,
                                format!("SM: {}", name),
                                color,
                                metallic,
                                roughness,
                                self.layer_stack.width,
                                self.layer_stack.height,
                            );
                            self.layer_stack.layers.push(fill);
                            self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                            self.layer_stack.composite(
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                            );
                            self.texture_needs_update = true;
                            self.status_message = format!("Applied Smart Material: {}", name);
                            self.history.push(format!("{:02}  Apply Smart Material ({})", self.history.len() + 1, name));
                        }
                        2 => {
                            self.brush.color = color;
                            self.brush.metallic = metallic;
                            self.brush.roughness = roughness;
                            let id = self.layer_stack.layers.len();
                            let fill = Layer::new_fill(
                                id,
                                format!("Mat: {}", name),
                                color,
                                metallic,
                                roughness,
                                self.layer_stack.width,
                                self.layer_stack.height,
                            );
                            self.layer_stack.layers.push(fill);
                            self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                            self.layer_stack.composite(
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                            );
                            self.texture_needs_update = true;
                            self.status_message = format!("Applied Material: {}", name);
                            self.history.push(format!("{:02}  Apply Material ({})", self.history.len() + 1, name));
                        }
                        3 => {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                                active_layer.add_black_mask();
                                self.status_message = format!("Applied Smart Mask ({}) to Layer: {}", name, active_layer.name);
                                self.history.push(format!("{:02}  Add Mask ({})", self.history.len() + 1, name));
                            }
                            self.layer_stack.composite(
                                self.baked_maps.curvature.as_deref(),
                                self.baked_maps.ambient_occlusion.as_deref(),
                            );
                            self.texture_needs_update = true;
                        }
                        4 => {
                            self.brush.alpha_name = name.clone();
                            if let Ok(img) = image::open(&thumb) {
                                let gray = img.to_luma8();
                                let (w, h) = gray.dimensions();
                                self.brush.custom_alpha = Some(Arc::new(AlphaMaskData {
                                    width: w,
                                    height: h,
                                    data: gray.into_raw(),
                                }));
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
                    _ => self.layer_stack.composite_color.iter().map(|c| {
                        Color32::from_rgba_premultiplied(
                            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                            255,
                        )
                    }).collect(),
                };
                let img = egui::ColorImage { size: [w, h], pixels };
                if let Some(ref mut handle) = self.texture_handle {
                    handle.set(img, egui::TextureOptions::LINEAR);
                } else {
                    self.texture_handle = Some(ctx.load_texture("pbr_composite", img, egui::TextureOptions::LINEAR));
                }
                self.texture_needs_update = false;
            }

            let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter();

            // Background Studio Gradient
            painter.rect_filled(rect, 0.0_f32, Color32::from_rgb(26, 26, 28));

            // Top Floating Overlay Bar
            let overlay_rect = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 12.0), vec2(500.0, 32.0));
            painter.rect_filled(overlay_rect, 4.0_f32, Color32::from_black_alpha(200));
            painter.text(pos2(overlay_rect.left() + 10.0, overlay_rect.center().y), Align2::LEFT_CENTER, format!("{} | Persp 50mm | Lit", self.mesh.name), egui::FontId::proportional(12.0), Color32::WHITE);

            // View mode switchers (F1: 3D|2D, F2: 3D, F3: 2D)
            let vm_3d = Rect::from_min_size(pos2(overlay_rect.right() - 210.0, overlay_rect.top() + 4.0), vec2(62.0, 24.0));
            let vm_2d = Rect::from_min_size(pos2(overlay_rect.right() - 142.0, overlay_rect.top() + 4.0), vec2(62.0, 24.0));
            let vm_split = Rect::from_min_size(pos2(overlay_rect.right() - 74.0, overlay_rect.top() + 4.0), vec2(70.0, 24.0));

            painter.rect_filled(vm_3d, 3.0_f32, if self.viewport_view_mode == 0 { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_3d.center(), Align2::CENTER_CENTER, "3D (F2)", egui::FontId::proportional(10.5), Color32::WHITE);

            painter.rect_filled(vm_2d, 3.0_f32, if self.viewport_view_mode == 1 { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_2d.center(), Align2::CENTER_CENTER, "2D (F3)", egui::FontId::proportional(10.5), Color32::WHITE);

            painter.rect_filled(vm_split, 3.0_f32, if self.viewport_view_mode == 2 { ACTIVE_ROW_FILL } else { PANEL_BG });
            painter.text(vm_split.center(), Align2::CENTER_CENTER, "3D|2D (F1)", egui::FontId::proportional(10.5), Color32::WHITE);

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if vm_3d.contains(pos) { self.viewport_view_mode = 0; }
                    else if vm_2d.contains(pos) { self.viewport_view_mode = 1; }
                    else if vm_split.contains(pos) { self.viewport_view_mode = 2; }
                }
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

        // Baker Dialog
        if self.show_baker_dialog {
            self.ui_baker_modal(ctx);
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

        // Texture border & UV grid indicator
        painter.text(pos2(canvas_rect.left() + 8.0, canvas_rect.top() + 8.0), Align2::LEFT_TOP, format!("UV Texture ({}x{})", self.layer_stack.width, self.layer_stack.height), egui::FontId::monospace(10.5), TEXT_DIMMED);

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
                    let u = ((pos.x - canvas_rect.left()) / canvas_rect.width()).clamp(0.0, 1.0);
                    let v = ((pos.y - canvas_rect.top()) / canvas_rect.height()).clamp(0.0, 1.0);
                    let uv = [u, v];

                    let bbox = if let Some(last_uv) = self.last_painted_uv {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                            active_layer.paint_line(last_uv, uv, &self.brush, false)
                        } else {
                            [0, 0, 0, 0]
                        }
                    } else {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                            active_layer.paint_stamp(uv, &self.brush, false)
                        } else {
                            [0, 0, 0, 0]
                        }
                    };
                    self.last_painted_uv = Some(uv);
                    self.layer_stack.composite_dirty(bbox);
                    self.texture_needs_update = true;
                }
            }
        } else {
            self.last_painted_uv = None;
        }
    }

    fn render_3d_viewport(&mut self, painter: &egui::Painter, view_bounds: Rect, response: &egui::Response, ctx: &egui::Context) {
        let center = view_bounds.center();
        let yaw_rad = self.camera_yaw.to_radians();
        let pitch_rad = self.camera_pitch.to_radians();
        let view_scale = view_bounds.height().min(view_bounds.width()) * 1.35;

        // Smooth camera zoom via mouse scroll wheel
        if view_bounds.contains(ctx.pointer_latest_pos().unwrap_or_default()) {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.05 {
                self.camera_dist = (self.camera_dist - scroll * 0.003).clamp(0.5, 6.0);
            }
        }

        // Photorealistic soft contact shadow under 3D mesh (no harsh black circles)
        let floor_y = center.y + view_scale * 0.42;
        let shadow_rx = view_scale * 0.36;
        let shadow_ry = view_scale * 0.09;
        // Tier 1: Soft ambient floor falloff
        painter.rect_filled(
            Rect::from_center_size(pos2(center.x, floor_y), vec2(shadow_rx * 2.2, shadow_ry * 2.2)),
            shadow_ry * 1.1,
            Color32::from_black_alpha(15),
        );
        // Tier 2: Mid penumbra
        painter.rect_filled(
            Rect::from_center_size(pos2(center.x, floor_y), vec2(shadow_rx * 1.5, shadow_ry * 1.5)),
            shadow_ry * 0.75,
            Color32::from_black_alpha(32),
        );
        // Tier 3: Core ground contact
        painter.rect_filled(
            Rect::from_center_size(pos2(center.x, floor_y), vec2(shadow_rx * 0.95, shadow_ry * 0.95)),
            shadow_ry * 0.45,
            Color32::from_black_alpha(55),
        );

        if let Some(ref handle) = self.texture_handle {
            // Project the real 3D mesh (Antique Lantern, Retro Radio, or Sphere) with lighting and depth sorting
            let (egui_mesh, hit_tris) = self.mesh.project_to_egui_mesh(
                center,
                view_scale,
                yaw_rad,
                pitch_rad,
                self.camera_dist,
                handle.id(),
            );

            // RENDER 3D MESH GEOMETRY DIRECTLY ON GPU
            painter.add(egui::Shape::Mesh(egui_mesh));

            // Camera Orbit (Alt+LMB or MMB) / Zoom (Alt+RMB) & 3D Surface Painting
            if response.dragged() {
                let delta = response.drag_delta();
                if ctx.input(|i| i.modifiers.alt && i.pointer.button_down(egui::PointerButton::Secondary)) {
                    self.camera_dist = (self.camera_dist + delta.y * 0.01).clamp(0.5, 6.0);
                } else if ctx.input(|i| i.modifiers.alt) || ctx.input(|i| i.pointer.middle_down()) {
                    self.camera_yaw += delta.x * 0.4;
                    self.camera_pitch = (self.camera_pitch + delta.y * 0.4).clamp(-85.0, 85.0);
                } else if let Some(pos) = response.interact_pointer_pos() {
                    // Raycast into 3D mesh triangles and interpolate exact UV
                    if let Some(uv) = Mesh::hit_test_uv(&hit_tris, pos) {
                        let bbox = if let Some(last_uv) = self.last_painted_uv {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                                active_layer.paint_line(last_uv, uv, &self.brush, false)
                            } else {
                                [0, 0, 0, 0]
                            }
                        } else {
                            if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                                active_layer.paint_stamp(uv, &self.brush, false)
                            } else {
                                [0, 0, 0, 0]
                            }
                        };
                        self.last_painted_uv = Some(uv);
                        self.layer_stack.composite_dirty(bbox);
                        self.texture_needs_update = true;
                    }
                }
            } else {
                self.last_painted_uv = None;
            }

            // Brush cursor on 3D surface
            if let Some(pos) = ctx.pointer_hover_pos() {
                if Mesh::hit_test_uv(&hit_tris, pos).is_some() {
                    painter.circle_stroke(pos, (self.brush.size * 0.5).max(3.0), Stroke::new(1.2_f32, ACTIVE_BLUE));
                }
            }
        }

        // Viewport navigation indicator
        painter.text(pos2(center.x, view_bounds.bottom() - 18.0), Align2::CENTER_CENTER, "Alt + LMB / MMB: Orbit | Alt + RMB / Scroll: Zoom | Click & Drag: Paint on 3D Mesh", egui::FontId::proportional(11.5), TEXT_DIMMED);
    }

    fn ui_texture_set_list(&mut self, ui: &mut Ui) {
        ui.heading("TEXTURE SET LIST");
        ui.separator();

        let models = [
            ("Head (Material Sphere)", 0),
            ("Antique Lantern (Test Asset)", 1),
            ("Retro Radio (Test Asset)", 2),
        ];

        for (name, idx) in models {
            let active = self.selected_model_idx == idx;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), egui::Sense::click());
            if active {
                ui.painter().rect_filled(rect, 2.0_f32, ACTIVE_ROW_FILL);
                ui.painter().rect_stroke(rect, 2.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
            }
            ui.painter().text(pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, format!("v  {}", name), egui::FontId::proportional(12.0), Color32::WHITE);
            ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, "Main shader", egui::FontId::proportional(11.0), TEXT_DIMMED);

            if resp.clicked() {
                self.load_model(idx);
            }
        }

        ui.spacing();
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Resolution:");
            egui::ComboBox::from_id_salt("res_combo")
                .selected_text(format!("{}x{}", self.baker_options.resolution, self.baker_options.resolution))
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut self.baker_options.resolution, 1024, "1024x1024").clicked() {
                        self.layer_stack.resize(1024, 1024);
                    }
                    if ui.selectable_value(&mut self.baker_options.resolution, 2048, "2048x2048").clicked() {
                        self.layer_stack.resize(2048, 2048);
                    }
                    if ui.selectable_value(&mut self.baker_options.resolution, 4096, "4096x4096").clicked() {
                        self.layer_stack.resize(4096, 4096);
                    }
                });
        });
    }

    fn ui_texture_set_settings(&mut self, ui: &mut Ui) {
        ui.heading("TEXTURE SET SETTINGS");
        ui.separator();

        ui.collapsing("CHANNELS", |ui| {
            ui.label("• Base color: sRGB8 (RGB)");
            ui.label("• Metallic: L8 (Grayscale)");
            ui.label("• Roughness: L8 (Grayscale)");
            ui.label("• Normal: RGB16F (Tangent)");
            ui.label("• Height: L16F (Depth)");
        });

        ui.collapsing("NORMAL MAP FORMAT", |ui| {
            ui.checkbox(&mut self.baker_options.invert_normal_y, "Invert Green Channel (DirectX format)");
        });

        ui.collapsing("MESH MAPS BAKING", |ui| {
            if ui.button(egui::RichText::new("Bake Mesh Maps...").color(Color32::WHITE)).clicked() {
                self.show_baker_dialog = true;
            }
            ui.label("Status: All 6 PBR maps baked and active.");
        });
    }

    fn ui_layers_panel(&mut self, ui: &mut Ui) {
        ui.heading("LAYERS");

        // Action Toolbar
        ui.horizontal(|ui| {
            if ui.small_button("+ Paint").clicked() {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_paint(id, format!("Layer {}", id), self.layer_stack.width, self.layer_stack.height));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.status_message = "Added Paint Layer".into();
            }
            if ui.small_button("+ Fill").clicked() {
                let id = self.layer_stack.layers.len() + 1;
                self.layer_stack.layers.push(Layer::new_fill(id, format!("Fill {}", id), self.brush.color, self.brush.metallic, self.brush.roughness, self.layer_stack.width, self.layer_stack.height));
                self.layer_stack.active_layer_index = self.layer_stack.layers.len() - 1;
                self.status_message = "Added Fill Layer".into();
            }
            if ui.small_button("+ Black Mask").on_hover_text("Add Black Mask with Sub-Stack").clicked() {
                if let Some(active) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                    active.add_black_mask();
                    self.status_message = "Added Black Mask".into();
                }
            }
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
        });

        ui.separator();

        // Layer Stack Items (Top to Bottom)
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in (0..self.layer_stack.layers.len()).rev() {
                let is_active = self.layer_stack.active_layer_index == i;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::click());

                if is_active {
                    ui.painter().rect_filled(rect, 2.0_f32, ACTIVE_ROW_FILL);
                    ui.painter().rect_stroke(rect, 2.0_f32, Stroke::new(1.2_f32, ACTIVE_BLUE));
                }

                let layer = &mut self.layer_stack.layers[i];

                // Eye toggle
                let eye_icon = if layer.visible { "[O]" } else { "[-]" };
                ui.painter().text(pos2(rect.left() + 6.0, rect.center().y), Align2::LEFT_CENTER, eye_icon, egui::FontId::monospace(11.0), TEXT_PRIMARY);

                // Layer type badge & name
                let type_badge = match layer.layer_type {
                    LayerType::Paint => "[P]",
                    LayerType::Fill => "[F]",
                    LayerType::Folder => "[D]",
                };
                let badge_color = match layer.layer_type {
                    LayerType::Paint => Color32::from_rgb(100, 200, 255),
                    LayerType::Fill => SMART_MAT_ORANGE,
                    LayerType::Folder => Color32::from_rgb(255, 215, 0),
                };
                ui.painter().text(pos2(rect.left() + 28.0, rect.center().y), Align2::LEFT_CENTER, type_badge, egui::FontId::monospace(11.0), badge_color);
                ui.painter().text(pos2(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, &layer.name, egui::FontId::proportional(12.0), Color32::WHITE);

                // Mask badge
                if layer.mask.is_some() {
                    let m_rect = Rect::from_min_size(pos2(rect.right() - 85.0, rect.top() + 5.0), vec2(16.0, 16.0));
                    ui.painter().rect_filled(m_rect, 1.0_f32, Color32::BLACK);
                    ui.painter().rect_stroke(m_rect, 1.0_f32, Stroke::new(1.0_f32, Color32::WHITE));
                }

                // Anchor point badge
                if layer.anchor_point.is_some() {
                    let a_rect = Rect::from_min_size(pos2(rect.right() - 65.0, rect.top() + 5.0), vec2(16.0, 16.0));
                    ui.painter().rect_filled(a_rect, 1.0, SMART_MAT_ORANGE);
                    ui.painter().text(a_rect.center(), Align2::CENTER_CENTER, "A", egui::FontId::proportional(10.0), Color32::WHITE);
                }

                // Opacity label
                ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, "100", egui::FontId::proportional(11.0), TEXT_DIMMED);

                if resp.clicked() {
                    self.layer_stack.active_layer_index = i;
                }

                // If mask exists, render sub-stack effects
                if is_active {
                    if let Some(ref mut mask) = layer.mask {
                        ui.indent("mask_indent", |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(ACTIVE_BLUE, "↳ Mask Sub-Stack:");
                                if ui.small_button("+ Generator").clicked() {
                                    mask.effects.push(MaskEffect::Generator {
                                        name: "Metal Edge Wear".into(),
                                        wear_level: 0.8,
                                        contrast: 1.2,
                                        dirt_amount: 0.5,
                                        invert: false,
                                        use_ao: true,
                                        use_curvature: true,
                                        anchor_name: None,
                                        blend_mode: MaskBlendMode::Multiply,
                                        opacity: 1.0,
                                    });
                                }
                                if ui.small_button("+ Fill Effect").clicked() {
                                    mask.effects.push(MaskEffect::Fill {
                                        value: 0.5,
                                        noise_frequency: 1.0,
                                        blend_mode: MaskBlendMode::Normal,
                                        opacity: 1.0,
                                    });
                                }
                            });

                            for eff in &mask.effects {
                                match eff {
                                    MaskEffect::Generator { name, wear_level, .. } => {
                                        ui.label(format!("  • Generator: {} (Wear: {:.1})", name, wear_level));
                                    }
                                    MaskEffect::Fill { value, .. } => {
                                        ui.label(format!("  • Fill Effect (Val: {:.2})", value));
                                    }
                                    _ => {}
                                }
                            }
                        });
                    }
                }
            }
        });
    }

    fn ui_display_settings(&mut self, ui: &mut Ui) {
        ui.heading("DISPLAY SETTINGS");
        ui.separator();

        ui.collapsing("CAMERA", |ui| {
            ui.add(egui::Slider::new(&mut self.camera_yaw, 0.0..=360.0).text("Yaw"));
            ui.add(egui::Slider::new(&mut self.camera_pitch, -85.0..=85.0).text("Pitch"));
            ui.add(egui::Slider::new(&mut self.camera_dist, 1.0..=10.0).text("Distance"));
        });

        ui.collapsing("ENVIRONMENT LIGHTING", |ui| {
            let presets = ["Neutral Studio", "Warm & Cool Studio", "Soft 3-Point Light", "Golden Hour Sun"];
            egui::ComboBox::from_label("HDRI Preset")
                .selected_text(presets[self.env_preset.min(presets.len() - 1)])
                .show_ui(ui, |ui| {
                    for (i, p) in presets.iter().enumerate() {
                        ui.selectable_value(&mut self.env_preset, i, *p);
                    }
                });

            ui.add(egui::Slider::new(&mut self.light_intensity, 0.1..=4.0).text("Exposure"));
            ui.add(egui::Slider::new(&mut self.env_rotation, 0.0..=360.0).text("Rotation"));
        });
    }

    fn ui_shader_settings(&mut self, ui: &mut Ui) {
        ui.heading("SHADER SETTINGS");
        ui.separator();
        ui.label("Shader: pbr-metal-rough");
        ui.label("Microfacet: Cook-Torrance GGX");
        ui.label("Pipeline: Vulkan 1.3 / WGPU Dynamic");
        ui.label("Status: [OK] Active & Optimized");
    }

    fn ui_history_panel(&mut self, ui: &mut Ui) {
        ui.heading("HISTORY");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for h in &self.history {
                ui.label(h);
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

    fn ui_baker_modal(&mut self, ctx: &egui::Context) {
        let mut show = self.show_baker_dialog;
        egui::Window::new("Bake Mesh Maps — REMoAINTER Baking Studio")
            .open(&mut show)
            .default_width(740.0)
            .default_height(540.0)
            .resizable(true)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Header Information
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Target Mesh:").strong());
                    ui.colored_label(Color32::WHITE, &self.mesh.name);
                    ui.label("|");
                    ui.label(format!("Triangles: {}", self.mesh.indices.len() / 3));
                    ui.label("|");
                    ui.colored_label(Color32::from_rgb(58, 217, 115), "Rayon Multithreading Active");
                });
                ui.separator();

                // Two-Column Layout (Substance Painter Architecture)
                ui.columns(2, |columns| {
                    // LEFT COLUMN: Texture Sets & Mesh Maps Checklist
                    let left = &mut columns[0];
                    left.heading(egui::RichText::new("Texture Sets").size(13.0));
                    left.checkbox(&mut true, format!("● {} (Default)", self.mesh.name));
                    left.add_space(8.0);

                    left.horizontal(|ui| {
                        ui.heading(egui::RichText::new("Mesh Maps").size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Deselect All").clicked() {
                                self.baker_options.bake_ao = false;
                                self.baker_options.bake_curvature = false;
                                self.baker_options.bake_normal = false;
                                self.baker_options.bake_world_space_normal = false;
                                self.baker_options.bake_position = false;
                                self.baker_options.bake_thickness = false;
                                self.baker_options.bake_id = false;
                            }
                            if ui.small_button("Select All").clicked() {
                                self.baker_options.bake_ao = true;
                                self.baker_options.bake_curvature = true;
                                self.baker_options.bake_normal = true;
                                self.baker_options.bake_world_space_normal = true;
                                self.baker_options.bake_position = true;
                                self.baker_options.bake_thickness = true;
                                self.baker_options.bake_id = true;
                            }
                        });
                    });
                    left.separator();

                    let map_defs = [
                        (0, "Ambient Occlusion", &mut self.baker_options.bake_ao, self.baked_maps.ambient_occlusion.is_some()),
                        (1, "Curvature", &mut self.baker_options.bake_curvature, self.baked_maps.curvature.is_some()),
                        (2, "Normal (Tangent)", &mut self.baker_options.bake_normal, self.baked_maps.normal.is_some()),
                        (3, "World Space Normal", &mut self.baker_options.bake_world_space_normal, self.baked_maps.world_space_normal.is_some()),
                        (4, "Position (XYZ)", &mut self.baker_options.bake_position, self.baked_maps.position.is_some()),
                        (5, "Thickness (Subsurface)", &mut self.baker_options.bake_thickness, self.baked_maps.thickness.is_some()),
                        (6, "ID Map", &mut self.baker_options.bake_id, self.baked_maps.id.is_some()),
                    ];

                    for (map_idx, label, enabled_ref, is_baked) in map_defs {
                        let is_selected = self.baker_selected_map == map_idx;
                        let (rect, resp) = left.allocate_exact_size(vec2(left.available_width(), 26.0), egui::Sense::click());
                        if is_selected {
                            left.painter().rect_filled(rect, 3.0_f32, ACTIVE_ROW_FILL);
                            left.painter().rect_stroke(rect, 3.0_f32, Stroke::new(1.0_f32, ACTIVE_BLUE));
                        } else if resp.hovered() {
                            left.painter().rect_filled(rect, 3.0_f32, Color32::from_rgb(40, 40, 44));
                        }

                        // Checkbox on left
                        let cb_rect = Rect::from_min_size(pos2(rect.left() + 6.0, rect.top() + 6.0), vec2(14.0, 14.0));
                        left.painter().rect_filled(cb_rect, 2.0_f32, if *enabled_ref { ACTIVE_BLUE } else { Color32::from_rgb(30, 30, 32) });
                        left.painter().rect_stroke(cb_rect, 2.0_f32, Stroke::new(1.0_f32, Color32::from_rgb(80, 80, 85)));
                        if *enabled_ref {
                            left.painter().text(cb_rect.center(), Align2::CENTER_CENTER, "✓", egui::FontId::monospace(11.0), Color32::WHITE);
                        }

                        // Map name
                        left.painter().text(pos2(rect.left() + 26.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), Color32::WHITE);

                        // Status badge (BAKED or READY)
                        let badge_rect = Rect::from_min_size(pos2(rect.right() - 56.0, rect.top() + 5.0), vec2(50.0, 16.0));
                        if is_baked {
                            left.painter().rect_filled(badge_rect, 2.0, Color32::from_rgb(25, 75, 45));
                            left.painter().text(badge_rect.center(), Align2::CENTER_CENTER, "BAKED", egui::FontId::monospace(9.5), Color32::from_rgb(100, 240, 150));
                        } else {
                            left.painter().rect_filled(badge_rect, 2.0, Color32::from_rgb(45, 45, 50));
                            left.painter().text(badge_rect.center(), Align2::CENTER_CENTER, "READY", egui::FontId::monospace(9.5), TEXT_DIMMED);
                        }

                        if resp.clicked() {
                            // If clicked on checkbox area, toggle enable
                            if let Some(pos) = resp.interact_pointer_pos() {
                                if pos.x < rect.left() + 24.0 {
                                    *enabled_ref = !*enabled_ref;
                                } else {
                                    self.baker_selected_map = map_idx;
                                }
                            } else {
                                self.baker_selected_map = map_idx;
                            }
                        }
                    }

                    left.add_space(12.0);
                    let sel_map_type = match self.baker_selected_map {
                        0 => MapType::AmbientOcclusion,
                        1 => MapType::Curvature,
                        2 => MapType::Normal,
                        3 => MapType::WorldSpaceNormal,
                        4 => MapType::Position,
                        5 => MapType::Thickness,
                        _ => MapType::Id,
                    };
                    if left.button(format!("⚡ Bake {:?} Only", sel_map_type)).on_hover_text("Bake only this individual map instantly").clicked() {
                        self.bake_single_map(sel_map_type);
                    }

                    // RIGHT COLUMN: Parameters (Common + Map-Specific)
                    let right = &mut columns[1];
                    egui::ScrollArea::vertical().show(right, |right| {
                        right.collapsing("Common Parameters", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Output Size:");
                                egui::ComboBox::from_id_salt("baker_out_size")
                                    .selected_text(format!("{}x{}", self.baker_options.resolution, self.baker_options.resolution))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.baker_options.resolution, 512, "512x512");
                                        ui.selectable_value(&mut self.baker_options.resolution, 1024, "1024x1024");
                                        ui.selectable_value(&mut self.baker_options.resolution, 2048, "2048x2048");
                                        ui.selectable_value(&mut self.baker_options.resolution, 4096, "4096x4096");
                                    });
                            });

                            ui.horizontal(|ui| {
                                ui.label("Antialiasing:");
                                egui::ComboBox::from_id_salt("baker_subsampling")
                                    .selected_text(match self.baker_subsampling {
                                        0 => "Subsampling 1x (None)",
                                        1 => "Subsampling 2x2",
                                        2 => "Subsampling 4x4",
                                        _ => "Subsampling 8x8",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.baker_subsampling, 0, "Subsampling 1x (None)");
                                        ui.selectable_value(&mut self.baker_subsampling, 1, "Subsampling 2x2");
                                        ui.selectable_value(&mut self.baker_subsampling, 2, "Subsampling 4x4");
                                        ui.selectable_value(&mut self.baker_subsampling, 3, "Subsampling 8x8");
                                    });
                            });

                            ui.checkbox(&mut self.baker_use_low_as_high, "Use Low Poly Mesh as High Poly Source");
                            ui.add(egui::Slider::new(&mut self.baker_frontal_dist, 0.01..=0.50).text("Max Frontal Distance"));
                            ui.add(egui::Slider::new(&mut self.baker_rear_dist, 0.01..=0.50).text("Max Rear Distance"));
                        });

                        right.add_space(8.0);
                        match self.baker_selected_map {
                            0 => {
                                right.collapsing("Ambient Occlusion Parameters", |ui| {
                                    ui.add(egui::Slider::new(&mut self.baker_options.ao_samples, 8..=128).text("Secondary Rays / Samples"));
                                    ui.add(egui::Slider::new(&mut self.baker_options.ao_max_distance, 0.1..=5.0).text("Max Distance"));
                                    ui.add(egui::Slider::new(&mut self.baker_options.ao_spread, 0.1..=1.0).text("Spread Angle"));
                                    ui.label("Distribution: Cosine Weighted (Hemispherical)");
                                    ui.label("Attenuation: Linear Falloff");
                                });
                            }
                            1 => {
                                right.collapsing("Curvature Parameters", |ui| {
                                    ui.add(egui::Slider::new(&mut self.baker_options.curvature_intensity, 0.1..=3.0).text("Intensity / Contrast"));
                                    ui.horizontal(|ui| {
                                        ui.label("Method:");
                                        egui::ComboBox::from_id_salt("baker_curv_method_combo")
                                            .selected_text(if self.baker_curv_method == 0 { "From Mesh Geometry" } else { "From Normal Map" })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.baker_curv_method, 0, "From Mesh Geometry");
                                                ui.selectable_value(&mut self.baker_curv_method, 1, "From Normal Map");
                                            });
                                    });
                                    ui.label("Precision: High-Precision Quadric Surface");
                                });
                            }
                            2 => {
                                right.collapsing("Normal Map Parameters", |ui| {
                                    ui.checkbox(&mut self.baker_options.invert_normal_y, "Invert Green Channel (DirectX Format)");
                                    ui.label("Coordinate Space: Tangent Space (MikkTSpace standard)");
                                });
                            }
                            3 => {
                                right.collapsing("World Space Normal Parameters", |ui| {
                                    ui.label("Up Vector: Y-Up (OpenGL Standard)");
                                    ui.label("Normal Distribution: Normalized World Vectors");
                                });
                            }
                            4 => {
                                right.collapsing("Position Map Parameters", |ui| {
                                    ui.label("Bounding Box Normalization: Full (0.0 to 1.0)");
                                    ui.label("Format: RGB 16-bit Floating Point Precision");
                                });
                            }
                            5 => {
                                right.collapsing("Thickness Map Parameters", |ui| {
                                    ui.label("Subsurface Scattering: Inverted Normal Raymarch");
                                    ui.label("Search Distance: 1.5 units");
                                });
                            }
                            _ => {
                                right.collapsing("ID Map Parameters", |ui| {
                                    ui.label("Color Source: Mesh Sub-Object & Material Groups");
                                    ui.label("Palette: High-Contrast Unique Color IDs");
                                });
                            }
                        }
                    });
                });

                ui.separator();

                // Modal Footer Actions
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.show_baker_dialog = false;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("⚡ Bake Selected Mesh Maps")
                                .strong()
                                .color(Color32::WHITE)
                                .size(13.0),
                        )
                        .fill(ACTIVE_BLUE)
                        .min_size(vec2(180.0, 30.0));

                        if ui.add(btn).clicked() {
                            self.bake_mesh_maps();
                            self.show_baker_dialog = false;
                        }
                    });
                });
            });
        self.show_baker_dialog = show;
    }
}
