use egui::{Color32, Stroke, Style, Visuals};

pub const ACTIVE_BLUE: Color32 = Color32::from_rgb(26, 141, 255);
pub const ACTIVE_ROW_FILL: Color32 = Color32::from_rgb(26, 75, 117);
pub const SMART_MAT_ORANGE: Color32 = Color32::from_rgb(239, 126, 34);
pub const WINDOW_BG: Color32 = Color32::from_rgb(36, 36, 36);
pub const PANEL_BG: Color32 = Color32::from_rgb(33, 33, 33);
pub const INPUT_BG: Color32 = Color32::from_rgb(23, 23, 23);
pub const BORDER_COLOR: Color32 = Color32::from_rgb(56, 56, 56);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(215, 215, 215);
pub const TEXT_DIMMED: Color32 = Color32::from_rgb(140, 140, 140);

pub fn apply_substance_theme(ctx: &egui::Context) {
    let mut style = Style::default();
    let mut visuals = Visuals::dark();

    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = WINDOW_BG;
    visuals.extreme_bg_color = INPUT_BG;
    visuals.override_text_color = Some(TEXT_PRIMARY);

    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(45, 45, 45);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(55, 55, 58);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, ACTIVE_BLUE);

    visuals.widgets.active.bg_fill = ACTIVE_ROW_FILL;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, ACTIVE_BLUE);

    visuals.selection.bg_fill = ACTIVE_ROW_FILL;
    visuals.selection.stroke = Stroke::new(1.2_f32, ACTIVE_BLUE);

    visuals.window_stroke = Stroke::new(1.0_f32, BORDER_COLOR);
    visuals.window_rounding = 0.0_f32.into();

    style.visuals = visuals;
    ctx.set_style(style);
}
