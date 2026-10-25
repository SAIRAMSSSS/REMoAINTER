use remoainter::ui::{self, RemoainterApp};

fn main() -> eframe::Result<()> {
    println!("============================================================");
    println!(" REMoAINTER v0.2 (Rust Core)");
    println!(" 3D PBR Texture Painting Suite");
    println!("============================================================");

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1600.0, 960.0])
        .with_min_inner_size([1024.0, 700.0])
        .with_title("REMoAINTER v0.2 - [PBR Metal-Roughness]");

    let logo_bytes = include_bytes!("../../assets/logo_icon.png");
    if let Ok(img) = image::load_from_memory(logo_bytes) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        viewport = viewport.with_icon(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        });
    }

    // Prioritize Vulkan on Linux and Windows, while allowing primary hardware backends (Vulkan, DX12, Metal)
    let supported_backends =
        wgpu::util::backend_bits_from_env().unwrap_or(if cfg!(target_os = "linux") {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::VULKAN | wgpu::Backends::PRIMARY
        });

    let native_options = eframe::NativeOptions {
        viewport,
        depth_buffer: 24,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            supported_backends,
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "REMoAINTER",
        native_options,
        Box::new(|cc| {
            ui::configure_fonts(&cc.egui_ctx);
            ui::apply_theme(&cc.egui_ctx);
            Ok(Box::new(RemoainterApp::new(cc)))
        }),
    )
}
