use eframe::egui::{self, Context};
use crate::app::{ReplayApp, Page};
use crate::core::DownloadedReplayInfo;

fn delete_all_replays(app: &mut ReplayApp, downloaded_replays: &[DownloadedReplayInfo]) {
    let mut deleted_count = 0;
    let mut failed_count = 0;
    let total_count = downloaded_replays.len();
    
    for replay in downloaded_replays {
        match app.delete_replay_file(replay) {
            Ok(()) => {
                deleted_count += 1;
            }
            Err(_) => {
                failed_count += 1;
            }
        }
    }
    
    if failed_count == 0 {
        app.show_success(format!("Successfully deleted all {} replays", deleted_count));
    } else if deleted_count == 0 {
        app.show_error(format!("Failed to delete all {} replays", total_count));
    } else {
        app.show_info(format!("Deleted {} replays, {} failed", deleted_count, failed_count));
    }
}

pub fn render_manage_page(app: &mut ReplayApp, ui: &mut egui::Ui, ctx: &Context) {
    ui.heading("Manage Downloaded Replays");
    ui.add_space(8.0);
    
    ui.horizontal(|ui| {
        ui.label("Download Directory:");
        ui.monospace(app.settings.download_dir.display().to_string());
    });
    ui.add_space(12.0);
    
    // Scan for downloaded replays
    let downloaded_replays = app.get_downloaded_replays();
    
    if downloaded_replays.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.heading("No downloaded replays found");
            ui.add_space(8.0);
            ui.label("Download some replays first from the main page");
            ui.add_space(16.0);
            if ui.button("Go to Replays Page").clicked() {
                app.current_page = Page::Main;
            }
        });
        return;
    }
    
    // Display total count and total size
    let total_size: u64 = downloaded_replays.iter().map(|r| r.file_size).sum();
    let total_size_mb = total_size as f64 / (1024.0 * 1024.0);
    
    ui.horizontal(|ui| {
        ui.label(format!("Total: {} replays", downloaded_replays.len()));
        ui.separator();
        ui.label(format!("Total size: {:.1} MB", total_size_mb));
    });
    ui.add_space(8.0);
    
    // Action buttons
    ui.horizontal(|ui| {
        if ui.button("Open Download Folder").clicked() {
            if let Err(e) = open::that(&app.settings.download_dir) {
                app.show_error(format!("Failed to open folder: {}", e));
            }
        }
        
        ui.separator();
        
        if ui.button("Refresh List").clicked() {
            app.mark_downloaded_replay_cache_dirty();
            app.show_info("Replay list refreshed");
            ctx.request_repaint();
        }
        
        ui.separator();
        
        // Delete all button with confirmation
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(180, 40, 40);
            ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::from_rgb(200, 50, 50);
            
            if ui.button("Delete All").clicked() {
                // Store the confirmation state in egui's memory
                ui.memory_mut(|mem| {
                    mem.data.insert_temp(egui::Id::new("show_delete_all_dialog"), true);
                });
            }
            
            // Reset colors
            ui.visuals_mut().widgets.inactive.bg_fill = ui.style().visuals.widgets.inactive.bg_fill;
            ui.visuals_mut().widgets.hovered.bg_fill = ui.style().visuals.widgets.hovered.bg_fill;
        });
    });
    
    // Delete all confirmation dialog
    let show_delete_all_confirmation = ui.memory(|mem| {
        mem.data.get_temp::<bool>(egui::Id::new("show_delete_all_dialog")).unwrap_or(false)
    });
    
    if show_delete_all_confirmation {
        egui::Window::new("Confirm Delete All")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("⚠ Warning").size(18.0).color(egui::Color32::from_rgb(255, 200, 0)));
                    ui.add_space(8.0);
                    ui.label(format!("Are you sure you want to delete all {} replays?", downloaded_replays.len()));
                    ui.label(format!("Total size: {:.1} MB", total_size_mb));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("This action cannot be undone!").color(egui::Color32::from_rgb(255, 100, 100)));
                    ui.add_space(16.0);
                    
                    ui.horizontal(|ui| {
                        // Cancel button
                        if ui.button("Cancel").clicked() {
                            ui.memory_mut(|mem| {
                                mem.data.remove::<bool>(egui::Id::new("show_delete_all_dialog"));
                            });
                        }
                        
                        ui.add_space(20.0);
                        
                        // Confirm delete button
                        ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(180, 40, 40);
                        ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::from_rgb(200, 50, 50);
                        
                        if ui.button("Delete All").clicked() {
                            delete_all_replays(app, &downloaded_replays);
                            ui.memory_mut(|mem| {
                                mem.data.remove::<bool>(egui::Id::new("show_delete_all_dialog"));
                            });
                            ctx.request_repaint();
                        }
                        
                        // Reset colors
                        ui.visuals_mut().widgets.inactive.bg_fill = ui.style().visuals.widgets.inactive.bg_fill;
                        ui.visuals_mut().widgets.hovered.bg_fill = ui.style().visuals.widgets.hovered.bg_fill;
                    });
                    ui.add_space(10.0);
                });
            });
    }
    
    ui.add_space(12.0);
    
    // Replay list
    let horizontal_margin = 8.0;
    let full_width = ui.available_width();
    
    let frame_vertical_margin = 9.0;
    let content_height = 48.0;
    let row_spacing = 2.0;
    let replay_item_height = frame_vertical_margin + content_height + row_spacing;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show_rows(ui, replay_item_height, downloaded_replays.len(), |ui, row_range| {
            let mut to_delete: Option<usize> = None;
            let mut show_info_for: Option<usize> = None;
            
            for row in row_range {
                let replay = &downloaded_replays[row];
                let (rect, _response) = ui.allocate_exact_size(
                    egui::vec2(full_width - 2.0 * horizontal_margin, replay_item_height - row_spacing),
                    egui::Sense::hover(),
                );
                let rect = rect.translate(egui::vec2(horizontal_margin, 0.0));
                ui.allocate_new_ui(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                    |ui| {
                        render_replay_row(app, ui, ctx, replay, row, rect.width(), &mut to_delete, &mut show_info_for);
                    },
                );
                ui.add_space(row_spacing);
            }
            
            if let Some(index) = to_delete {
                if let Some(replay_to_delete) = downloaded_replays.get(index) {
                    match app.delete_replay_file(replay_to_delete) {
                        Ok(()) => {
                            // Success notification is handled in delete_replay_file
                        }
                        Err(e) => {
                            app.show_error(format!("Failed to delete replay: {}", e));
                        }
                    }
                    ctx.request_repaint();
                }
            }
            
            // Handle info display
            if let Some(index) = show_info_for {
                if let Some(replay_info) = downloaded_replays.get(index) {
                    app.show_info(format!(
                        "File: {}\nPath: {}\nSize: {} bytes", 
                        replay_info.filename,
                        replay_info.full_path.display(),
                        replay_info.file_size
                    ));
                }
            }
        });
}

fn render_replay_row(
    app: &mut ReplayApp,
    ui: &mut egui::Ui,
    ctx: &Context,
    replay: &DownloadedReplayInfo,
    index: usize,
    width: f32,
    to_delete: &mut Option<usize>,
    show_info_for: &mut Option<usize>,
) {
    ui.push_id(format!("replay_row_{}", index), |ui| {
        egui::Frame::new()
            .fill(if index % 2 == 0 { ui.style().visuals.faint_bg_color } else { egui::Color32::TRANSPARENT })
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(width - 24.0);
                ui.horizontal(|ui| {
                    // Map name
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Map").size(12.0).weak());
                        ui.label(egui::RichText::new(replay.map_name.as_deref().unwrap_or("Unknown")).strong());
                    });
                    
                    ui.separator();
                    
                    // Game mode
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Mode").size(12.0).weak());
                        ui.label(replay.game_mode.as_deref().unwrap_or("Unknown"));
                    });
                    
                    ui.separator();
                    
                    // Date
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Date").size(12.0).weak());
                        let date_text = if let Some(date) = &replay.date {
                            date.clone()
                        } else if let Some(modified) = replay.modified_time {
                            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                                let datetime = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0);
                                if let Some(dt) = datetime {
                                    dt.format("%Y/%m/%d").to_string()
                                } else {
                                    "Unknown".to_string()
                                }
                            } else {
                                "Unknown".to_string()
                            }
                        } else {
                            "Unknown".to_string()
                        };
                        ui.label(date_text);
                    });
                    
                    ui.separator();
                    
                    // File size
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Size").size(12.0).weak());
                        let size_text = if replay.file_size > 1024 * 1024 {
                            format!("{:.1} MB", replay.file_size as f64 / (1024.0 * 1024.0))
                        } else if replay.file_size > 1024 {
                            format!("{:.1} KB", replay.file_size as f64 / 1024.0)
                        } else {
                            format!("{} B", replay.file_size)
                        };
                        ui.label(size_text);
                    });
                    
                    ui.separator();
                    
                    // Replay ID
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("ID").size(12.0).weak());
                        
                        let available_width = ui.available_width();
                        let full_id_galley = ui.fonts(|f| {
                            f.layout_no_wrap(replay.id.clone(), egui::FontId::default(), egui::Color32::WHITE)
                        });
                        
                        // Only truncate if the full ID doesn't fit
                        let display_id = if full_id_galley.rect.width() > available_width - 30.0 {
                            let mut truncated = if replay.id.len() > 12 {
                                replay.id[..12].to_string()
                            } else {
                                replay.id.clone()
                            };
                            
                            loop {
                                let test_text = format!("{}...", truncated);
                                let test_galley = ui.fonts(|f| {
                                    f.layout_no_wrap(test_text.clone(), egui::FontId::default(), egui::Color32::WHITE)
                                });
                                
                                if test_galley.rect.width() <= available_width - 30.0 || truncated.len() <= 4 {
                                    break format!("{}...", truncated);
                                }
                                
                                truncated.pop();
                            }
                        } else {
                            replay.id.clone()
                        };
                        
                        let id_response = ui.button(display_id);
                        if id_response.clicked() {
                            ctx.copy_text(replay.id.clone());
                            app.show_success("Replay ID copied to clipboard");
                        }
                        if id_response.hovered() {
                            id_response.on_hover_text(&replay.id);
                        }
                    });
                    
                    // Action buttons on the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Info button
                        if ui.button("Info").clicked() {
                            *show_info_for = Some(index);
                        }
                        
                        // Delete button with warning color
                        ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(180, 40, 40);
                        ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::from_rgb(200, 50, 50);
                        
                        if ui.button("Delete").clicked() {
                            *to_delete = Some(index);
                        }
                        
                        // Reset colors for future widgets
                        ui.visuals_mut().widgets.inactive.bg_fill = ui.style().visuals.widgets.inactive.bg_fill;
                        ui.visuals_mut().widgets.hovered.bg_fill = ui.style().visuals.widgets.hovered.bg_fill;
                    });
                });
            });
    });
}
