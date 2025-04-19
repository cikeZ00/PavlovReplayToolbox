use eframe::egui::{self, Context};

use crate::app::ReplayApp;
use crate::tools::replay_processor::ReplayItem;

pub fn render_main_page(app: &mut ReplayApp, ui: &mut egui::Ui, ctx: &Context) {
    ui.horizontal(|ui| {
        ui.heading("Replay Downloader");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.styled_button(ui, "Refresh").clicked() {
                app.refresh_replays();
            }
        });
    });
    ui.separator();

    ui.group(|ui| {
        ui.horizontal(|ui| {
            let total_width = ui.available_width() - 8.0;
            let field_count = 5.0;
            let spacing = ui.spacing().item_spacing.x * (field_count - 1.0);
            let field_width = (total_width - spacing) / field_count;
            let field_height = 24.0;

            // Game Mode filter
            ui.vertical(|ui| {
                ui.label("Game Mode:");
                ui.add_sized([field_width, field_height],
                    egui::TextEdit::singleline(&mut app.replay_list.filters.game_mode)
                        .hint_text("Filter"));
            });

            // Map filter
            ui.vertical(|ui| {
                ui.label("Map:");
                ui.add_sized([field_width, field_height],
                    egui::TextEdit::singleline(&mut app.replay_list.filters.map_name)
                        .hint_text("Filter"));
            });

            // Workshop Mods filter
            ui.vertical(|ui| {
                ui.label("Workshop Mods:");
                ui.add_sized([field_width, field_height],
                    egui::TextEdit::singleline(&mut app.replay_list.filters.workshop_mods)
                        .hint_text("Filter"));
            });

            // User ID filter
            ui.vertical(|ui| {
                ui.label("User ID:");
                ui.add_sized([field_width, field_height],
                    egui::TextEdit::singleline(&mut app.replay_list.filters.user_id)
                        .hint_text("Filter"));
            });

            // Platform filter
            ui.vertical(|ui| {
                ui.label("Platform:");
                let old_platform = app.replay_list.filters.platform;
                
                egui::ComboBox::new(egui::Id::new("platform_filter"), "")
                    .width(field_width)
                    .selected_text(match app.replay_list.filters.platform {
                        crate::app::PlatformFilter::All => "All",
                        crate::app::PlatformFilter::Quest => "Quest",
                        crate::app::PlatformFilter::PC => "PC",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.replay_list.filters.platform, crate::app::PlatformFilter::All, "All");
                        ui.selectable_value(&mut app.replay_list.filters.platform, crate::app::PlatformFilter::Quest, "Quest");
                        ui.selectable_value(&mut app.replay_list.filters.platform, crate::app::PlatformFilter::PC, "PC");
                    });
                
                if app.replay_list.filters.platform != old_platform {
                    app.replay_list.current_page = 0;
                    app.refresh_replays();
                }
            });
        });
    });

    let filtered_replays = app.get_filtered_replays();

    let replay_item_height = 200.0;
    let horizontal_margin = 8.0;
    let full_width = ui.available_width();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show_rows(ui, replay_item_height, filtered_replays.len(), |ui, row_range| {
            if filtered_replays.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No replays found...");
                });
            } else {
                for row in row_range {
                    let replay = &filtered_replays[row];
                    let (rect, _response) = ui.allocate_exact_size(
                        egui::vec2(full_width - 2.0 * horizontal_margin, replay_item_height),
                        egui::Sense::hover(),
                    );
                    let rect = rect.translate(egui::vec2(horizontal_margin, 0.0));
                    ui.allocate_new_ui(
                        egui::UiBuilder::new()
                            .max_rect(rect)  
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                        |ui| {
                            render_replay_item_with_width(app, ui, ctx, replay, rect.width());
                        },
                    );
                    ui.add_space(4.0);
                }
            }
        });

    if app.replay_list.total_pages > 0 {
        egui::Area::new(egui::Id::new("pagination_controls"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(ctx.style().visuals.window_fill)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 8,
                        spread: 0,
                        color: ctx.style().visuals.window_shadow.color,
                    })
                    .corner_radius(5.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.add_enabled(
                                app.replay_list.current_page > 0,
                                egui::Button::new("<")
                                    .min_size(egui::vec2(32.0, 32.0))
                            ).clicked() {
                                app.replay_list.current_page -= 1;
                                app.refresh_replays();
                                ctx.memory_mut(|mem| {
                                    mem.data.clear();
                                });
                            }
                            
                            ui.label(format!("Page {} of {}",
                                app.replay_list.current_page + 1,
                                app.replay_list.total_pages.max(1)
                            ));
                            
                            if ui.add_enabled(
                                app.replay_list.current_page < app.replay_list.total_pages - 1,
                                egui::Button::new(">")
                                    .min_size(egui::vec2(32.0, 32.0))
                            ).clicked() {
                                app.replay_list.current_page += 1;
                                app.refresh_replays();
                                ctx.memory_mut(|mem| {
                                    mem.data.clear();
                                });
                            }
                        });
                    });
            });
    }
}

fn render_replay_item_with_width(
    app: &mut ReplayApp,
    ui: &mut egui::Ui,
    ctx: &Context,
    replay: &ReplayItem,
    width: f32,
) {
    ui.push_id(replay.id.as_str(), |ui| {
        egui::Frame::new()
            .outer_margin(egui::Margin::same(0)) 
            .show(ui, |ui| {
                egui::Frame::group(ui.style())
                    .fill(ui.style().visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::symmetric(8, 0)) 
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0); 
                        render_replay_item_contents(app, ui, ctx, replay);
                    });
            });
    });
}

fn render_replay_item_contents(
    app: &mut ReplayApp,
    ui: &mut egui::Ui,
    ctx: &Context,
    replay: &ReplayItem
) {
    ui.vertical(|ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(ui.style().visuals.extreme_bg_color)
                .stroke(egui::Stroke::new(1.0, ui.style().visuals.window_stroke().color))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin { top: 4, left: 8, right: 8, bottom: 4 })
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&replay.map_name)
                            .color(ui.style().visuals.text_color())
                            .size(24.0)
                            .strong()
                    );
                });
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_downloading = app.downloading_replay_id
                    .as_ref()
                    .map_or(false, |id| id == &replay.id);
                
                let is_downloaded = app.downloaded_replays.contains(&replay.id);

                if is_downloaded {
                    egui::Frame::new()
                        .inner_margin(egui::Margin { top: 8, left: 0, right: 0, bottom: 0 })
                        .show(ui, |ui| {
                            ui.add_enabled(
                                false, 
                                egui::Button::new("Downloaded")
                                    .min_size(egui::vec2(ui.available_width().min(120.0), 32.0))
                            );
                        });
                } else if !is_downloading {
                    egui::Frame::new()
                        .inner_margin(egui::Margin { top: 8, left: 0, right: 0, bottom: 0 })
                        .show(ui, |ui| {
                            if app.styled_button(ui, "Download & Process").clicked() {
                                app.process_online_replay(&replay.id);
                            }
                        });
                }
            });
        });

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label("Game Mode:");
            ui.label(&replay.game_mode);
            ui.separator();
            ui.label("Date:");
            ui.label(&replay.created_date);
        });

        ui.horizontal(|ui| {
            let mods_popup_id = egui::Id::new(format!("mods_popup_{}", replay.id));
            let button = ui.button("Mods");
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(format!("{} mod{}", replay.modcount, if replay.modcount == 1 { "" } else { "s" }));
            });

            if button.clicked() {
                ui.memory_mut(|mem| mem.open_popup(mods_popup_id));
            }

            egui::popup::popup_below_widget(
                ui,
                mods_popup_id,
                &button,
                egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                |ui: &mut egui::Ui| {
                    ui.set_max_width(400.0);
                    ui.set_max_height(400.0);

                    egui::Frame::new()
                        .show(ui, |ui| {
                            // Header
                            ui.label(egui::RichText::new("Workshop Mods").strong());
                            ui.separator();

                            let available_height = ui.available_height() - ui.spacing().item_spacing.y;
                            
                            if replay.workshop_mods.trim().is_empty() {
                                ui.label("No mods for this replay.");
                            } else {
                                let mod_ids = app.parse_mod_ids(&replay.workshop_mods);
                                
                                if mod_ids.is_empty() {
                                    ui.label("Could not parse mod IDs.");
                                    ui.label(&replay.workshop_mods);
                                } else {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false; 2])
                                        .max_height(available_height)
                                        .show(ui, |ui| {
                                            for mod_id in &mod_ids {
                                                // Trigger loading if not already cached
                                                if !app.mod_info_cache.contains_key(mod_id) {
                                                    app.load_mod_info(mod_id.clone());
                                                }

                                                ui.add_space(4.0);
                                                ui.push_id(mod_id, |ui| {
                                                    egui::Frame::group(ui.style())
                                                        .fill(ui.style().visuals.faint_bg_color)
                                                        .show(ui, |ui| {
                                                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                                                ui.add_space(4.0);
                                                                ui.horizontal(|ui| { ui.add_space(8.0); });
                                                                ui.vertical(|ui| {
                                                                    ui.set_min_width(ui.available_width() - 16.0);
                                                                    ui.set_max_width(ui.available_width() - 16.0);
                                                                    if let Some(mod_info) = app.mod_info_cache.get(mod_id) {
                                                                        if mod_info.is_loading {
                                                                            ui.horizontal(|ui| {
                                                                                ui.spinner();
                                                                                ui.label("Loading mod info...");
                                                                            });
                                                                        } else if mod_info.failed {
                                                                            ui.colored_label(
                                                                                ui.style().visuals.error_fg_color,
                                                                                format!("Error loading info for mod {}", mod_id)
                                                                            );
                                                                        } else {
                                                                            ui.heading(&mod_info.name);
                                                                            ui.add_space(4.0);
                                                                            egui::Frame::group(ui.style())
                                                                                .fill(ui.style().visuals.faint_bg_color)
                                                                                .corner_radius(egui::CornerRadius::same(4))
                                                                                .show(ui, |ui| {
                                                                                    egui::ScrollArea::vertical()
                                                                                        .max_width(310.0)
                                                                                        .max_height(100.0)
                                                                                        .id_salt(format!("desc_{}", mod_id))
                                                                                        .show(ui, |ui| {
                                                                                            ui.label(&mod_info.description);
                                                                                        });
                                                                                });

                                                                            if let Some(url) = &mod_info.thumbnail_url {
                                                                                ui.add_space(8.0);

                                                                                if let Some(texture) = app.mod_thumbnail_textures.get(mod_id) {
                                                                                    let thumbnail_size = egui::vec2(320.0, 180.0);
                                                                                    ui.add(egui::Image::new(texture)
                                                                                        .max_size(thumbnail_size)
                                                                                        .fit_to_exact_size(thumbnail_size));
                                                                                } else {
                                                                                    ui.horizontal(|ui| {
                                                                                        ui.spinner();
                                                                                        ui.label("Loading thumbnail...");
                                                                                    });

                                                                                    if !app.loading_thumbnails.contains(mod_id) {
                                                                                        app.load_mod_thumbnail(mod_id.clone(), url.clone());
                                                                                    }
                                                                                }

                                                                                ui.add_space(16.0);
                                                                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                                                                    if ui.add(egui::Button::new("View on mod.io")
                                                                                        .min_size(egui::vec2(120.0, 32.0)))
                                                                                        .clicked() {
                                                                                        ui.ctx().open_url(egui::OpenUrl::new_tab(
                                                                                            format!("https://mod.io/search/mods/{}", mod_id)
                                                                                        ));
                                                                                    }
                                                                                });
                                                                            }
                                                                        }
                                                                    } else {
                                                                        ui.horizontal(|ui| {
                                                                            ui.spinner();
                                                                            ui.label("Loading mod info...");
                                                                        });
                                                                    }
                                                                });
                                                                ui.horizontal(|ui| { ui.add_space(8.0); });
                                                            });
                                                        });
                                                });
                                            }
                                        });
                                }
                            }
                        }
                    );
                }
            );
        });

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label("Time Since:");
            ui.label(format!("{}s", replay.time_since));
        });

        ui.separator();

        let avatar_row_height = 72.0;
        let avatar_size = egui::vec2(64.0, 64.0);

        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), avatar_row_height),
            egui::Sense::hover(),
        );

        ui.allocate_new_ui(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                egui::ScrollArea::horizontal()
                    .max_height(avatar_row_height)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                            if replay.users.is_empty() {
                                let (avatar_rect, _) = ui.allocate_exact_size(avatar_size, egui::Sense::hover());
                                ui.painter().rect_filled(avatar_rect, 8.0, egui::Color32::DARK_GRAY);
                                ui.painter().text(
                                    avatar_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "No Users",
                                    egui::FontId::proportional(14.0),
                                    egui::Color32::WHITE,
                                );
                            } else {
                                for (idx, user) in replay.users.iter().enumerate() {
                                    ui.push_id(idx, |ui| {
                                        app.render_user_avatar(ui, ctx, user);
                                    });
                                }
                            }
                        });
                    });
            },
        );
    });
}