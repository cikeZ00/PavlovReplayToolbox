use eframe::egui::{self, Layout, Align};
use crate::app::ReplayApp;

pub fn render_settings_page(app: &mut ReplayApp, ui: &mut egui::Ui) {
    ui.heading("Settings");
    ui.separator();
    
    ui.add_space(8.0);
    
    // Download directory settings
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("Download Directory");
            ui.horizontal(|ui| {
                let path_text = app.settings.download_dir.display().to_string();
                ui.label("Save replays to:");
                ui.add(egui::Label::new(path_text).wrap());
                
                if app.styled_button(ui, "Browse").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        app.settings.download_dir = path;
                        if let Err(err) = app.save_settings() {
                            app.show_error(format!("Error saving settings: {}", err));
                        } else {
                            app.show_success("Settings saved successfully");
                        }
                    }
                }
            });
            
            ui.add_space(4.0);
            ui.label("This is where downloaded replays will be saved");
        });
    });
    
    ui.add_space(16.0);
    
    // Auto refresh settings
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("Auto Refresh");
            
            ui.checkbox(&mut app.settings.auto_refresh_enabled, "Enable auto refresh");
            
            ui.add_enabled(
                app.settings.auto_refresh_enabled,
                egui::Slider::new(&mut app.settings.auto_refresh_interval_mins, 1..=60)
                    .text("Refresh interval (minutes)")
                    .clamping(egui::SliderClamping::Always)

            );
            
            ui.add_space(4.0);
            ui.label("Automatically refresh the replay list at the specified interval");
        });
    });

    ui.add_space(16.0);

    // Auto download settings
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("Auto Download");
            
            ui.checkbox(&mut app.settings.auto_download_enabled, "Enable auto download");
            
            ui.add_space(4.0);
            ui.label("User ID trigger:");
            ui.add_enabled(
                app.settings.auto_download_enabled,
                egui::TextEdit::singleline(&mut app.settings.auto_download_trigger_user_id)
                    .hint_text("Enter user ID to auto-download")
            );
            
            ui.add_space(4.0);
            ui.label("Automatically download replays containing the specified user ID");
        });
    });
    
    // Apply button
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
            if app.styled_button(ui, "Apply").clicked() {
                if let Err(err) = app.save_settings() {
                    app.show_error(format!("Error saving settings: {}", err));
                } else {
                    app.show_success("Settings saved successfully");
                }
            }
        });
    });
}