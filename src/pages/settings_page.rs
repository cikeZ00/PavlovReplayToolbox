use eframe::egui::{self, Align, Layout};
use crate::app::ReplayApp;

pub fn render_settings_page(app: &mut ReplayApp, ui: &mut egui::Ui) {
    ui.add_space(10.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            section(ui, "Storage", |ui| {
                settings_row(ui, "Download directory", "Where downloaded replays will be saved", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);

                        if app.styled_button(ui, "Change").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                app.settings.download_dir = path;
                                app.mark_downloaded_replay_cache_dirty();

                                if let Err(err) = app.save_settings() {
                                    app.show_error(format!("Error saving settings: {}", err));
                                } else {
                                    app.show_success("Settings saved successfully");
                                }
                            }
                        }
                    });
                });

                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(app.settings.download_dir.display().to_string())
                            .monospace()
                            .size(14.0),
                    );
                });
            });

            section(ui, "Downloading", |ui| {
                settings_row(ui, "Disk cache", "Allow resumable downloads using disk cache", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.checkbox(&mut app.settings.download_use_disk_cache, "");
                    });
                });

                let max_threads = std::thread::available_parallelism()
                    .map(|count| count.get())
                    .unwrap_or(8)
                    .max(1);

                app.settings.download_thread_count =
                    app.settings.download_thread_count.clamp(1, max_threads);

                settings_row(ui, "Download threads", "How many chunks to download in parallel", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.add(
                            egui::Slider::new(&mut app.settings.download_thread_count, 1..=max_threads)
                                .clamping(egui::SliderClamping::Always)
                                .show_value(true),
                        );
                    });
                });
            });

            section(ui, "Automation", |ui| {
                settings_row(ui, "Auto refresh", "Refresh the replay list automatically", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.checkbox(&mut app.settings.auto_refresh_enabled, "");
                    });
                });

                settings_row(ui, "Refresh interval", "Minutes between refreshes", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.add_enabled(
                            app.settings.auto_refresh_enabled,
                            egui::Slider::new(&mut app.settings.auto_refresh_interval_mins, 1..=60)
                                .text("")
                                .clamping(egui::SliderClamping::Always)
                                .show_value(true),
                        );
                    });
                });

                settings_row(ui, "Auto download", "Automatically download matching replays", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.checkbox(&mut app.settings.auto_download_enabled, "");
                    });
                });

                settings_row(ui, "Trigger user ID", "Auto-download when this user appears", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.add_enabled(
                            app.settings.auto_download_enabled,
                            egui::TextEdit::singleline(&mut app.settings.auto_download_trigger_user_id)
                                .hint_text("Enter user ID")
                                .desired_width(220.0),
                        );
                    });
                });
            });

            section(ui, "API Integration", |ui| {
                settings_row(ui, "mod.io API URL", "Endpoint used to fetch mod details", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings.modio_api_url)
                                .desired_width(380.0),
                        );
                    });
                });

                settings_row(ui, "mod.io API token", "Private token used for API access", |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings.modio_api_token)
                                .password(true)
                                .desired_width(380.0),
                        );
                    });
                });

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label("Configure your mod.io credentials to see mod details.");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.hyperlink_to("Get an API key from mod.io", "https://mod.io/apikey");
                });
            });

            ui.add_space(16.0);

            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(12.0);

                    if app.styled_button(ui, "Apply").clicked() {
                        if let Err(err) = app.save_settings() {
                            app.show_error(format!("Error saving settings: {}", err));
                        } else {
                            app.show_success("Settings saved successfully");
                        }
                    }
                });
            });
        });
}

fn section<F>(ui: &mut egui::Ui, title: &str, add_contents: F)
where
    F: FnOnce(&mut egui::Ui),
{
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new(title)
            .strong()
            .size(19.0),
    );

    ui.add_space(6.0);

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical(|ui| {
            add_contents(ui);
        });
    });

    ui.add_space(12.0);
}

fn settings_row<F>(ui: &mut egui::Ui, title: &str, subtitle: &str, control: F)
where
    F: FnOnce(&mut egui::Ui),
{
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(title)
                    .strong()
                    .size(15.5),
            );

            ui.add_space(3.0);

            ui.label(
                egui::RichText::new(subtitle)
                    .size(13.5)
                    .weak(),
            );
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            control(ui);
        });
    });

    ui.add_space(10.0);
}