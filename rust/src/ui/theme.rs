//! reference PBR suite UI theme.
//!
//! Tokens are lifted from the *actual* shipped AlgWidgets QML style files
//! (`AlgWidgets/Style.2/Alg{Style,BackgroundStyle,TextStyle,BorderStyle,
//! WidgetsStyle}.qml` — see `docs/ARCHITECTURE.md`):
//!
//! - `colors.gray(L)` is `Qt.hsla(0, 0, L/100)`, so gray(20) = #333333,
//!   gray(15) = #262626, gray(10) = #1A1A1A, gray(25) = #404040,
//!   gray(30) = #4D4D4D, gray(40) = #666666, gray(80) = #CCCCCC.
//! - Accent colors: blue `#1A8DFF`, carott `#EF7E22`, alizarin `#EF4E35`,
//!   sun_flower `#FFB91A`, jungle_green `#30B29C`.
//! - `borderRadius: 0` everywhere — the interface is deliberately sharp-cornered.
//! - `defaultSpacing: 10`.

use egui::{Color32, Stroke, Style, Visuals};

// ── Accent colors (AlgStyle.colors) ──────────────────────────────────────────
pub const ACTIVE_BLUE: Color32 = Color32::from_rgb(26, 141, 255); // blue
/// Legacy selection fill (dark-blue row highlight); kept for app.rs users.
pub const ACTIVE_ROW_FILL: Color32 = Color32::from_rgb(26, 75, 117);
pub const SMART_MAT_ORANGE: Color32 = Color32::from_rgb(239, 126, 34); // carott
pub const ALIZARIN_RED: Color32 = Color32::from_rgb(239, 78, 53); // error
pub const SUN_FLOWER: Color32 = Color32::from_rgb(255, 185, 26); // warning
pub const JUNGLE_GREEN: Color32 = Color32::from_rgb(48, 178, 156);

// ── Background grays (AlgStyle.background) ───────────────────────────────────
/// `mainWindow: colors.gray(20)`
pub const WINDOW_BG: Color32 = Color32::from_rgb(51, 51, 51);
/// Shared panel background (same as the window in reference).
pub const PANEL_BG: Color32 = WINDOW_BG;
/// Text/combobox inputs idle: `gray(15)`
pub const INPUT_BG: Color32 = Color32::from_rgb(38, 38, 38);
/// Input pressed/hovered: `gray(10)`
pub const INPUT_BG_ACTIVE: Color32 = Color32::from_rgb(26, 26, 26);
/// Button idle fill: `gray(30)`
pub const BUTTON_BG: Color32 = Color32::from_rgb(77, 77, 77);
/// Button pressed fill: `gray(15)`
pub const BUTTON_PRESSED_BG: Color32 = INPUT_BG;
/// Button hovered fill: `gray(25)`
pub const BUTTON_HOVER_BG: Color32 = Color32::from_rgb(64, 64, 64);
/// ComboBox border: `gray(30)`
pub const BORDER_COLOR: Color32 = Color32::from_rgb(77, 77, 77);
/// Default widget border: `gray(10)`
pub const BORDER_DARK: Color32 = INPUT_BG_ACTIVE;

// ── Text grays (AlgStyle.text) ───────────────────────────────────────────────
/// `text.color.normal: colors.gray(80)`
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(204, 204, 204);
/// `text.color.hovered: white`
pub const TEXT_HOVERED: Color32 = Color32::WHITE;
/// `text.color.disabled: colors.gray(40)`
pub const TEXT_DIMMED: Color32 = Color32::from_rgb(102, 102, 102);

/// Identity-magnitude reference font stack (OpenSans embedded below answers
/// for glyph coverage; sizes mirror `text.font.size: 11` scaled for egui).
pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Embed authentic OpenSans fonts into the binary (Standalone on Linux & Windows)
    fonts.font_data.insert(
        "OpenSans-Regular".to_owned(),
        egui::FontData::from_static(include_bytes!("../../../assets/fonts/OpenSans-Regular.ttf")),
    );
    fonts.font_data.insert(
        "OpenSans-SemiBold".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/OpenSans-SemiBold.ttf"
        )),
    );

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "OpenSans-Regular".to_owned());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.insert(0, "OpenSans-Regular".to_owned());
    }

    ctx.set_fonts(fonts);
}

/// Apply the reference-dark theme derived from the shipped QML tokens.
pub fn apply_theme(ctx: &egui::Context) {
    // Text sizes: reference uses an 11 px UI face on 96 DPI.
    let mut style = Style::default();
    style.text_styles = [
        (egui::TextStyle::Body, egui::FontId::proportional(11.5)),
        (egui::TextStyle::Button, egui::FontId::proportional(11.5)),
        (egui::TextStyle::Small, egui::FontId::proportional(9.5)),
        (egui::TextStyle::Heading, egui::FontId::proportional(14.0)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(11.0)),
    ]
    .into();
    // defaultSpacing: 10 in AlgStyle.
    style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    style.spacing.button_padding = egui::vec2(7.0, 6.0);
    style.spacing.indent = 16.0;

    let mut visuals = Visuals::dark();

    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = WINDOW_BG;
    visuals.extreme_bg_color = INPUT_BG_ACTIVE;
    visuals.override_text_color = Some(TEXT_PRIMARY);

    // Squared-off corners: `borderRadius: 0` across AlgWidgetProperties.
    visuals.window_rounding = 0.0_f32.into();
    visuals.menu_rounding = 0.0_f32.into();
    visuals.widgets.noninteractive.rounding = 0.0.into();
    visuals.widgets.inactive.rounding = 0.0.into();
    visuals.widgets.hovered.rounding = 0.0.into();
    visuals.widgets.active.rounding = 0.0.into();
    visuals.widgets.open.rounding = 0.0.into();

    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_DARK);

    visuals.widgets.inactive.bg_fill = BUTTON_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_DARK);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);

    visuals.widgets.hovered.bg_fill = BUTTON_HOVER_BG;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, TEXT_HOVERED);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_HOVERED);

    // Pressed: fill `gray(15)`, border/text highlight = blue (`highlight_color`).
    visuals.widgets.active.bg_fill = BUTTON_PRESSED_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACTIVE_BLUE);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, ACTIVE_BLUE);

    visuals.widgets.open.bg_fill = INPUT_BG;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);

    visuals.selection.bg_fill = ACTIVE_ROW_FILL;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACTIVE_BLUE);

    visuals.window_stroke = Stroke::new(1.0_f32, BORDER_DARK);
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.window_shadow = egui::epaint::Shadow::NONE;

    style.visuals = visuals;
    ctx.set_style(style);
}
