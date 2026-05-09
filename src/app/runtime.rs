use std::time::Duration;

use eframe::egui::{self, Context};
use eframe::App;

use super::{ui, Page, ReplayApp};

impl App for ReplayApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if let Ok(update_info) = self.update_rx.try_recv() {
            self.update_info = Some(update_info.clone());
            self.show_info(format!(
                "New version {} available! You are running {}",
                update_info.latest_version,
                update_info.current_version
            ));
        }

        ui::render_update_dialog(self, ctx);

        ui::update_notifications(self);

        ui::render_download_progress(self, ctx);

        while let Ok((user, color_image)) = self.profile_rx.try_recv() {
            let texture_handle = ctx.load_texture(
                &format!("avatar_{}", user),
                color_image,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    ..Default::default()
                },
            );
            self.profile_textures.insert(user.clone(), texture_handle);
            self.loading_profiles.remove(&user);
        }

        while let Ok(mod_info) = self.mod_info_rx.try_recv() {
            self.mod_info_cache.insert(mod_info.id.clone(), mod_info);
        }

        while let Ok((mod_id, color_image)) = self.mod_thumbnail_rx.try_recv() {
            let texture_handle = ctx.load_texture(
                &format!("mod_thumbnail_{}", mod_id),
                color_image,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    ..Default::default()
                },
            );
            self.mod_thumbnail_textures.insert(mod_id.clone(), texture_handle);
            self.loading_thumbnails.remove(&mod_id);
        }

        while let Ok(replay_id) = self.downloaded_rx.try_recv() {
            self.downloaded_replays.insert(replay_id.clone());
            self.mark_downloaded_replay_cache_dirty();
            self.show_success(format!("Replay {} downloaded successfully", replay_id));
        }

        ui::render_completion_dialog(self, ctx);

        ui::render_top_panel(self, ctx);
        ui::render_current_page(self, ctx);

        if self.is_processing_local {
            if let Ok(status) = self.status.lock() {
                if status.contains("complete") || status.contains("Error") {
                    self.show_completion_dialog = true;
                    self.is_processing_local = false;
                }
            }
        }

        if self.is_downloading && self.downloading_replay_id.is_none() {
            self.is_downloading = false;
        }

        if self.settings.auto_refresh_enabled
            && self.last_refresh_time.elapsed() > Duration::from_secs(self.settings.auto_refresh_interval_mins * 60)
            && self.current_page == Page::Main
            && !self.is_downloading
        {
            self.refresh_replays();
        } else if self.settings.auto_download_enabled
            && !self.is_downloading
            && self.current_page == Page::Main
        {
            self.check_auto_download_triggers();
        }

        ui::render_notifications(self, ctx);

        let has_loading_mods = self.mod_info_cache.values().any(|m| m.is_loading);
        let needs_repaint = self.is_processing_local
            || self.is_downloading
            || self.downloading_replay_id.is_some()
            || !self.loading_profiles.is_empty()
            || !self.loading_thumbnails.is_empty()
            || has_loading_mods
            || !self.notifications.is_empty();

        if needs_repaint {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if self.settings.auto_refresh_enabled
            && self.current_page == Page::Main
            && !self.is_downloading
        {
            let refresh_interval = Duration::from_secs(self.settings.auto_refresh_interval_mins * 60);
            let elapsed = self.last_refresh_time.elapsed();
            if elapsed < refresh_interval {
                ctx.request_repaint_after(refresh_interval - elapsed);
            } else {
                ctx.request_repaint();
            }
        }
    }
}
