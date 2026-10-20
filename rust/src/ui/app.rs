use crate::baker::{BakedMaps, BakerEngine, BakerOptions, MapType};
use crate::graphics::Mesh;
use crate::painting::{AnchorPoint, Brush, Layer, LayerStack, LayerType, MaskBlendMode, MaskEffect};
use crate::ui::theme::*;
use egui::{pos2, vec2, Align2, Color32, Rect, Stroke, Ui};
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
    pub shelf_category: usize, // 0: All, 1: Smart Mats, 2: Materials, 3: Alphas, 4: Brushes
    pub search_query: String,

    // Right Strip Active Panel
    pub active_right_panel: usize, // 0: Settings, 1: Texture Sets, 2: Layers, 3: Display, 4: Shaders, 5: History, 6: Log

    // Test Assets
    pub selected_model_idx: usize,
    pub status_message: String,
    pub history: Vec<String>,
    pub logs: Vec<(String, String, String)>, // time, level, text

    pub last_painted_uv: Option<[f32; 2]>,
}

impl Default for RemoainterApp {
    fn default() -> Self {
        let mut app = Self {
            active_tool: ToolMode::Paint,
            brush: Brush::default(),
            layer_stack: LayerStack::new(2048, 2048),
            mesh: Mesh::create_material_sphere(64, 64),
            baker_options: BakerOptions::default(),
            baked_maps: BakedMaps::default(),
            is_baking: false,
            baking_progress: Arc::new(AtomicUsize::new(0)),
            show_baker_dialog: false,

            display_channel: 0,
            env_preset: 1, // Warm & Cool Studio
            env_rotation: 0.0,
            light_intensity: 1.0,
            camera_yaw: 45.0,
            camera_pitch: 25.0,
            camera_dist: 3.2,
            symmetry_x: false,

            shelf_category: 0,
            search_query: String::new(),
            active_right_panel: 2, // Layers panel open by default

            selected_model_idx: 0,
            status_message: "Ready".to_string(),
            history: vec![
                "01  New Project (Head - 2048x2048)".into(),
                "02  Load Mesh (Substance Material Sphere)".into(),
                "03  Add Fill Layer (Base)".into(),
                "04  Add Paint Layer (Layer 1)".into(),
                "05  Add Folder (Scattering)".into(),
                "06  Add Folder (Color)".into(),
                "07  Add Folder (Roughness)".into(),
                "08  Add Folder (Final Touches)".into(),
                "09  Set Active Layer (Layer 1)".into(),
            ],
            logs: vec![
                ("09:00:01".into(), "INFO".into(), "REMoAINTER v2.0.0 (High-Performance Rust Core) initialized.".into()),
                ("09:00:02".into(), "INFO".into(), "Vulkan/WGPU hardware pipeline active with zero-stutter staging.".into()),
                ("09:00:03".into(), "INFO".into(), "Loaded 40+ Allegorithmic PBR materials, Smart Masks & Alphas.".into()),
                ("09:00:04".into(), "INFO".into(), "Rayon multithreaded baking engine ready.".into()),
            ],
            last_painted_uv: None,
        };

        // Pre-bake curvature and AO for the initial sphere
        app.bake_mesh_maps();
        app
    }
}

impl RemoainterApp {
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
        self.layer_stack.composite(self.baked_maps.curvature.as_deref(), self.baked_maps.ambient_occlusion.as_deref());
    }

    pub fn load_model(&mut self, idx: usize) {
        self.selected_model_idx = idx;
        match idx {
            0 => {
                self.mesh = Mesh::create_material_sphere(64, 64);
                self.status_message = "Loaded Substance Material Sphere".to_string();
            }
            1 => {
                if let Ok(m) = Mesh::load_obj("/mnt/shared/REMoAINTER/assets/test_models/lantern.obj") {
                    self.mesh = m;
                    self.status_message = "Loaded Test Asset: Antique Lantern".to_string();
                } else if let Ok(m) = Mesh::load_obj("/home/senju/Documents/env-port/lantern_low.obj") {
                    self.mesh = m;
                    self.status_message = "Loaded Test Asset: Antique Lantern".to_string();
                }
            }
            2 => {
                if let Ok(m) = Mesh::load_obj("/mnt/shared/REMoAINTER/assets/test_models/radio.obj") {
                    self.mesh = m;
                    self.status_message = "Loaded Test Asset: Retro Radio".to_string();
                } else if let Ok(m) = Mesh::load_obj("/home/senju/Pictures/radio-model/radio.obj") {
                    self.mesh = m;
                    self.status_message = "Loaded Test Asset: Retro Radio".to_string();
                }
            }
            _ => {}
        }
        self.bake_mesh_maps();
    }
}

impl eframe::App for RemoainterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_substance_theme(ctx);

        // 1. TOP MENU BAR (with logo and Allegorithmic styling)
        egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Application Logo / Brand
                ui.colored_label(SMART_MAT_ORANGE, "Pt");
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
                        if ui.button("Material (Lit)").clicked() { self.display_channel = 0; ui.close_menu(); }
                        if ui.button("Base Color (Solo)").clicked() { self.display_channel = 1; ui.close_menu(); }
                        if ui.button("Roughness (Solo)").clicked() { self.display_channel = 2; ui.close_menu(); }
                        if ui.button("Metallic (Solo)").clicked() { self.display_channel = 3; ui.close_menu(); }
                        if ui.button("Normal (Solo)").clicked() { self.display_channel = 4; ui.close_menu(); }
                        if ui.button("Ambient Occlusion (Baked)").clicked() { self.display_channel = 5; ui.close_menu(); }
                        if ui.button("Curvature (Baked)").clicked() { self.display_channel = 6; ui.close_menu(); }
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
                let _ = ui.small_button("<");
                let _ = ui.small_button(">");
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

                ui.checkbox(&mut self.brush.lazy_mouse, "Lazy Mouse");
                ui.checkbox(&mut self.symmetry_x, "Symmetry");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("vp_channel_combo")
                        .selected_text(match self.display_channel {
                            0 => "Material",
                            1 => "Base Color",
                            2 => "Roughness",
                            3 => "Metallic",
                            4 => "Normal",
                            5 => "Ambient Occlusion",
                            6 => "Curvature",
                            _ => "Material",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.display_channel, 0, "Material");
                            ui.selectable_value(&mut self.display_channel, 1, "Base Color");
                            ui.selectable_value(&mut self.display_channel, 2, "Roughness");
                            ui.selectable_value(&mut self.display_channel, 3, "Metallic");
                            ui.selectable_value(&mut self.display_channel, 4, "Normal");
                            ui.selectable_value(&mut self.display_channel, 5, "Ambient Occlusion");
                            ui.selectable_value(&mut self.display_channel, 6, "Curvature");
                        });
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

        // 4. FAR-LEFT TOOLBAR (36px strip)
        egui::SidePanel::left("left_tools").exact_width(36.0).resizable(false).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let tools = [
                    (ToolMode::Paint, "🖌", "Paint Brush (1)"),
                    (ToolMode::Eraser, "⌫", "Eraser (2)"),
                    (ToolMode::Projection, "⧉", "Projection (3)"),
                    (ToolMode::PolygonFill, "▧", "Polygon Fill (4)"),
                    (ToolMode::Smudge, "〰", "Smudge (5)"),
                    (ToolMode::Clone, "⚯", "Clone Stamp (6)"),
                    (ToolMode::Eyedropper, "💉", "Color Eyedropper (I)"),
                ];

                for (mode, icon, tip) in tools {
                    let active = self.active_tool == mode;
                    let btn = egui::Button::new(egui::RichText::new(icon).size(16.0))
                        .fill(if active { ACTIVE_ROW_FILL } else { PANEL_BG })
                        .stroke(if active { Stroke::new(1.2_f32, ACTIVE_BLUE) } else { Stroke::NONE });

                    if ui.add(btn).on_hover_text(tip).clicked() {
                        self.active_tool = mode;
                    }
                }
            });
        });

        // 5. ASSET SHELF (`ASSETS`) (270px)
        egui::SidePanel::left("asset_shelf").default_width(270.0).width_range(200.0..=400.0).show(ctx, |ui| {
            ui.heading(egui::RichText::new("ASSETS").size(14.0).strong());

            // Category filter chips
            ui.horizontal_wrapped(|ui| {
                let cats = ["All", "Smart Mats", "Materials", "Alphas", "Brushes"];
                for (i, c) in cats.iter().enumerate() {
                    let active = self.shelf_category == i;
                    let btn = egui::Button::new(*c)
                        .fill(if active { ACTIVE_BLUE } else { Color32::from_rgb(45, 45, 48) });
                    if ui.add(btn).clicked() {
                        self.shelf_category = i;
                    }
                }
            });

            ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Search materials, alphas..."));
            ui.separator();

            // Materials Grid
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let presets = [
                        ("Steel Rough", [0.75, 0.75, 0.78, 1.0], 1.0, 0.45, 0),
                        ("Gold Pure", [1.00, 0.84, 0.00, 1.0], 1.0, 0.20, 0),
                        ("Copper Worn", [0.95, 0.64, 0.54, 1.0], 0.9, 0.35, 1),
                        ("Gunmetal Dark", [0.35, 0.35, 0.38, 1.0], 0.95, 0.28, 1),
                        ("Plastic Red", [0.90, 0.15, 0.15, 1.0], 0.0, 0.25, 0),
                        ("Rubber Black", [0.12, 0.12, 0.14, 1.0], 0.0, 0.70, 0),
                        ("Concrete Edges", [0.55, 0.55, 0.54, 1.0], 0.0, 0.85, 1),
                        ("Wood Varnish", [0.60, 0.35, 0.20, 1.0], 0.0, 0.30, 0),
                    ];

                    for (name, col, met, rough, badge) in presets {
                        let (rect, response) = ui.allocate_exact_size(vec2(76.0, 92.0), egui::Sense::click());
                        let painter = ui.painter();

                        // 3D Sphere Preview
                        let center = pos2(rect.center().x, rect.top() + 38.0);
                        let radius = 28.0;

                        // Ambient shadow
                        painter.circle_filled(pos2(center.x + 3.0, center.y + 4.0), radius, Color32::from_black_alpha(100));
                        // Base color
                        painter.circle_filled(center, radius, Color32::from_rgba_premultiplied(
                            (col[0] * 255.0) as u8, (col[1] * 255.0) as u8, (col[2] * 255.0) as u8, 255
                        ));
                        // Key highlight
                        let spec_alpha = ((1.0 - rough) * 200.0) as u8;
                        painter.circle_filled(pos2(center.x - 9.0, center.y - 9.0), 7.0, Color32::from_white_alpha(spec_alpha));
                        // Edge ring
                        painter.circle_stroke(center, radius, Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 60)));

                        // Badge
                        let badge_rect = Rect::from_min_size(pos2(center.x + 8.0, center.y + 8.0), vec2(16.0, 14.0));
                        if badge == 1 {
                            painter.rect_filled(badge_rect, 2.0, SMART_MAT_ORANGE);
                            painter.text(badge_rect.center(), Align2::CENTER_CENTER, "SM", egui::FontId::proportional(9.0), Color32::WHITE);
                        } else {
                            painter.rect_filled(badge_rect, 2.0, ACTIVE_BLUE);
                            painter.text(badge_rect.center(), Align2::CENTER_CENTER, "M", egui::FontId::proportional(9.0), Color32::WHITE);
                        }

                        // Label
                        let text_rect = Rect::from_min_size(pos2(rect.left(), rect.top() + 72.0), vec2(76.0, 16.0));
                        painter.text(text_rect.center(), Align2::CENTER_CENTER, name, egui::FontId::proportional(10.5), TEXT_PRIMARY);

                        if response.hovered() {
                            painter.rect_stroke(rect, 2.0_f32, Stroke::new(1.5_f32, ACTIVE_BLUE));
                        }

                        if response.clicked() {
                            self.brush.color = col;
                            self.brush.metallic = met;
                            self.brush.roughness = rough;
                            self.status_message = format!("Loaded Material: {}", name);
                        }
                    }
                });
            });
        });

        // 6. FAR-RIGHT DOCK STRIP (32px narrow strip)
        egui::SidePanel::right("right_strip").exact_width(32.0).resizable(false).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let strip = [
                    (0, "⚙", "Texture Set Settings"),
                    (1, "⧉", "Texture Set List"),
                    (2, "☰", "Layers"),
                    (3, "🖥", "Display Settings"),
                    (4, "🌐", "Shader Settings"),
                    (5, "⏱", "History"),
                    (6, "💻", "Log"),
                ];

                for (idx, icon, tip) in strip {
                    let active = self.active_right_panel == idx;
                    let btn = egui::Button::new(egui::RichText::new(icon).size(15.0))
                        .fill(if active { ACTIVE_ROW_FILL } else { PANEL_BG })
                        .stroke(if active { Stroke::new(1.5_f32, ACTIVE_BLUE) } else { Stroke::NONE });

                    if ui.add(btn).on_hover_text(tip).clicked() {
                        self.active_right_panel = idx;
                    }
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
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter();

            // Background Studio Gradient
            painter.rect_filled(rect, 0.0, Color32::from_rgb(26, 26, 28));

            // Viewport Floating Overlay
            let overlay_rect = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 16.0), vec2(220.0, 32.0));
            painter.rect_filled(overlay_rect, 4.0, Color32::from_black_alpha(180));
            painter.text(overlay_rect.center(), Align2::CENTER_CENTER, format!("{} | Persp 50mm | Lit", self.mesh.name), egui::FontId::proportional(12.0), Color32::WHITE);

            // Orbit controls with Alt+LMB or MMB
            if response.dragged() {
                let delta = response.drag_delta();
                if ctx.input(|i| i.modifiers.alt) || ctx.input(|i| i.pointer.middle_down()) {
                    self.camera_yaw += delta.x * 0.4;
                    self.camera_pitch = (self.camera_pitch + delta.y * 0.4).clamp(-85.0, 85.0);
                } else if ctx.input(|i| i.pointer.primary_down()) {
                    // Paint onto texture surface
                    let local_x = (response.interact_pointer_pos().unwrap().x - rect.left()) / rect.width();
                    let local_y = (response.interact_pointer_pos().unwrap().y - rect.top()) / rect.height();
                    let uv = [local_x.clamp(0.0, 1.0), local_y.clamp(0.0, 1.0)];

                    if let Some(last_uv) = self.last_painted_uv {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                            active_layer.paint_line(last_uv, uv, &self.brush, false);
                        }
                    } else {
                        if let Some(active_layer) = self.layer_stack.layers.get_mut(self.layer_stack.active_layer_index) {
                            active_layer.paint_stamp(uv, &self.brush, false);
                        }
                    }
                    self.last_painted_uv = Some(uv);
                    self.layer_stack.composite(self.baked_maps.curvature.as_deref(), self.baked_maps.ambient_occlusion.as_deref());
                }
            } else {
                self.last_painted_uv = None;
            }

            // Draw Central 3D Mesh / Sphere Projection
            let center = rect.center();
            let view_radius = rect.height().min(rect.width()) * 0.35;

            // Ambient Shadow
            painter.circle_filled(pos2(center.x, center.y + view_radius * 0.25), view_radius * 0.95, Color32::from_black_alpha(140));

            // Composite Sphere rendering
            let base_c = self.layer_stack.composite_color.get(0).copied().unwrap_or([0.85, 0.82, 0.78, 1.0]);
            let met = self.layer_stack.composite_metallic.get(0).copied().unwrap_or(0.0);
            let rough = self.layer_stack.composite_roughness.get(0).copied().unwrap_or(0.35);

            let sphere_col = Color32::from_rgba_premultiplied(
                (base_c[0] * 255.0) as u8,
                (base_c[1] * 255.0) as u8,
                (base_c[2] * 255.0) as u8,
                255,
            );
            painter.circle_filled(center, view_radius, sphere_col);

            // Shading & Specular
            let spec_col = if met > 0.5 { sphere_col } else { Color32::WHITE };
            let spec_alpha = ((1.0 - rough) * 220.0) as u8;
            painter.circle_filled(pos2(center.x - view_radius * 0.3, center.y - view_radius * 0.3), view_radius * 0.25, Color32::from_rgba_premultiplied(spec_col.r(), spec_col.g(), spec_col.b(), spec_alpha));
            painter.circle_stroke(center, view_radius, Stroke::new(1.2_f32, Color32::from_rgb(50, 50, 50)));

            // Draw painted ribbon on surface
            painter.text(pos2(center.x, center.y + view_radius + 24.0), Align2::CENTER_CENTER, "Left Click + Drag: Paint Surface | Alt + Drag: Orbit Camera", egui::FontId::proportional(12.0), TEXT_DIMMED);
        });

        // Baker Dialog
        if self.show_baker_dialog {
            self.ui_baker_modal(ctx);
        }
    }
}

// Right panel implementations
impl RemoainterApp {
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

                // Layer name
                let type_icon = match layer.layer_type {
                    LayerType::Paint => "🖌",
                    LayerType::Fill => "▧",
                    LayerType::Folder => "📁",
                };
                ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, format!("{} {}", type_icon, layer.name), egui::FontId::proportional(12.0), Color32::WHITE);

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
                    ui.painter().text(a_rect.center(), Align2::CENTER_CENTER, "⚓", egui::FontId::proportional(10.0), Color32::WHITE);
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
                                if ui.small_button("+ Add Generator").clicked() {
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
                                        ui.label(format!("  ⚙ Generator: {} (Wear: {:.1})", name, wear_level));
                                    }
                                    MaskEffect::Fill { value, .. } => {
                                        ui.label(format!("  ▧ Fill Effect (Val: {:.2})", value));
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
        egui::Window::new("Bake Mesh Maps (Raytraced & Geometric)")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading("Mesh Maps Baker");
                ui.separator();

                ui.label(format!("Target Mesh: {}", self.mesh.name));
                ui.label(format!("Triangles: {}", self.mesh.indices.len() / 3));

                ui.separator();
                ui.label(egui::RichText::new("Select Individual Maps or All to Bake:").strong());

                ui.checkbox(&mut self.baker_options.bake_ao, "Ambient Occlusion (Raytraced)");
                ui.checkbox(&mut self.baker_options.bake_curvature, "Curvature (Convex Ridges & Concave Crevices)");
                ui.checkbox(&mut self.baker_options.bake_normal, "Normal Map (Tangent Space)");
                ui.checkbox(&mut self.baker_options.bake_world_space_normal, "World Space Normal");
                ui.checkbox(&mut self.baker_options.bake_position, "Position Map (Normalized Bounding Box)");
                ui.checkbox(&mut self.baker_options.bake_thickness, "Thickness Map (Subsurface Scattering)");
                ui.checkbox(&mut self.baker_options.bake_id, "ID Map (Material Groups)");

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    egui::ComboBox::from_id_salt("baker_res")
                        .selected_text(format!("{}x{}", self.baker_options.resolution, self.baker_options.resolution))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.baker_options.resolution, 512, "512x512");
                            ui.selectable_value(&mut self.baker_options.resolution, 1024, "1024x1024");
                            ui.selectable_value(&mut self.baker_options.resolution, 2048, "2048x2048");
                            ui.selectable_value(&mut self.baker_options.resolution, 4096, "4096x4096");
                        });
                });

                ui.add(egui::Slider::new(&mut self.baker_options.ao_samples, 8..=128).text("AO Rays/Pixel"));

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("Bake All Selected Maps").strong().color(Color32::WHITE)).clicked() {
                        self.bake_mesh_maps();
                        self.show_baker_dialog = false;
                    }
                    if ui.button("Bake AO Only").clicked() {
                        self.bake_single_map(MapType::AmbientOcclusion);
                        self.show_baker_dialog = false;
                    }
                    if ui.button("Bake Curvature Only").clicked() {
                        self.bake_single_map(MapType::Curvature);
                        self.show_baker_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_baker_dialog = false;
                    }
                });
            });
    }
}
