use eframe::egui::{self, Context};

use super::super::ReplayApp;

pub fn render_update_dialog(app: &mut ReplayApp, ctx: &Context) {
    if let Some(update_info) = &app.update_info {
        let release_url = update_info.release_url.clone();
        let current_version = update_info.current_version.clone();
        let latest_version = update_info.latest_version.clone();
        let release_name = update_info.release_name.clone();
        let release_date = update_info.release_date.clone();
        let release_notes = update_info.release_notes.clone();

        let mut should_close = false;
        let mut error_message = None;

        egui::Window::new("Update Available")
            .collapsible(true)
            .resizable(true)
            .default_size([400.0, 300.0])
            .show(ctx, |ui| {
                ui.heading("New Version Available!");
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Current Version:");
                    ui.strong(current_version);
                });

                ui.horizontal(|ui| {
                    ui.label("Latest Version:");
                    ui.strong(latest_version);
                });

                ui.add_space(8.0);
                ui.label(format!("Release: {}", release_name));
                ui.label(format!("Released on: {}", release_date));

                ui.add_space(8.0);
                ui.label("Release Notes:");
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .show(ui, |ui| {
                        ui.label(&release_notes);
                    });

                ui.add_space(8.0);

                if ui.button("Download Update").clicked() {
                    if let Err(err) = open::that(&release_url) {
                        error_message = Some(format!("Failed to open browser: {}", err));
                    }
                }

                if ui.button("Remind Me Later").clicked() {
                    should_close = true;
                }
            });

        if let Some(err) = error_message {
            app.show_error(err);
        }

        if should_close {
            app.update_info = None;
        }
    }
}

pub fn render_completion_dialog(app: &mut ReplayApp, ctx: &Context) {
    if app.show_completion_dialog {
        egui::Window::new("Processing Complete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                if let Ok(status) = app.status.lock() {
                    ui.label(status.as_str());
                }
                if ui.button("OK").clicked() {
                    app.reset_state();
                }
            });
    }
}
