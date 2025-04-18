#![windows_subsystem = "windows"]
mod tools;
mod app;

use eframe::{run_native, NativeOptions};

fn main() -> eframe::Result<()> {
    let icon_data = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("Failed to load icon")
        .to_rgba8();
    let (icon_width, icon_height) = icon_data.dimensions();

    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_min_inner_size([800.0, 600.0])
            .with_inner_size([1024.0, 768.0])
            .with_decorations(true)
            .with_drag_and_drop(true)
            .with_resizable(true)
            .with_title("Pavlov Replay Toolbox")
            .with_icon(egui::IconData {
                rgba: icon_data.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        centered: true,
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 2,
        ..Default::default()
    };
    
    run_native(
        "Pavlov Replay Toolbox",
        native_options,
        Box::new(|cc| Ok(Box::new(app::ReplayApp::new(cc)))),
    )
}