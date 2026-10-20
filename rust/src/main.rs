mod baker;
mod graphics;
mod painting;
mod ui;

use ui::RemoainterApp;

fn main() -> eframe::Result<()> {
    println!("============================================================");
    println!(" REMoAINTER v2.0.0 (Rust Edition)");
    println!(" High-Performance 3D PBR Texture Painting Suite");
    println!(" Architecture: Rayon Multithreaded Baker & Sub-Stack Engine");
    println!("============================================================");

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1600.0, 960.0])
        .with_min_inner_size([1024.0, 700.0])
        .with_title("REMoAINTER 2026.1 (v2.0.0) - [PBR Metal-Roughness]");

    if let Ok(img) = image::open("/home/senju/Downloads/REMoAINTER logo.png") {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        viewport = viewport.with_icon(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        });
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "REMoAINTER",
        native_options,
        Box::new(|cc| {
            ui::configure_substance_fonts(&cc.egui_ctx);
            ui::apply_substance_theme(&cc.egui_ctx);
            Ok(Box::new(RemoainterApp::default()))
        }),
    )
}
