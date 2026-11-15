//! The per-frame entry point called by eframe: lays out the panels and pages.

use crate::*;

impl eframe::App for App {
    /// Saves the window state, statistics and watch position when the app closes.
    fn on_exit(&mut self) {
        save_win(&self.win);
        insights::save(&self.stats);
        self.record_progress(true);
        save_history(&self.history);
    }

    /// Runs once per frame. A bug in one page must not close the app, so a panic while drawing is
    /// written to the crash log and the next frame simply tries again.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.draw(ui))).is_err() {
            self.toast(tr("Something went wrong on this page (details are in crash.log)"));
            // a page that panics every frame would spin: go back to a safe page after a repeat
            self.panics += 1;
            if self.panics >= 3 {
                self.panics = 0;
                self.sel = Sel::Home;
            }
        }
    }
}

impl App {
    /// Handles messages and keys, then lays out the sidebar, player and the current page.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        self.sync_look(ctx);
        if self.sel != Sel::Settings {
            self.storage_page = false;
            self.storage_confirm = None;
        }
        self.pump();
        self.keys(ctx);

        if self.seen_dirty {
            self.seen_dirty = false;
            save_seen(&self.seen);
        }
        self.desktop_integration();
        self.pump_downloads();
        self.sync_offline_lists();
        self.track_stats();
        self.pod_auto_refresh();
        self.update_tick();
        self.track_navigation();
        self.track_window(ctx);
        if self.release_focus {
            self.release_focus = false;
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }
        {
            let back_pressed = ctx.input(|i| {
                (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft))
                    || i.pointer.button_pressed(egui::PointerButton::Extra1)
            });
            let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape))
                && !ctx.egui_wants_keyboard_input()
                && self.new_list.is_none()
                && self.import_list.is_none()
                && !self.show_help
                && (self.watching.is_none() || self.pip);
            if (back_pressed || esc) && self.can_go_back() {
                self.go_back();
            }
        }
        let watching = self.watching.is_some();
        if let Some(SleepMode::At(t)) = &self.sleep {
            if Instant::now() >= *t {
                self.sleep = None;
                if watching && !self.player.paused {
                    self.player.toggle_pause();
                }
                self.toast(tr("Sleep timer: playback paused"));
            } else {
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }
        self.sync_audio_filters();
        self.clip_tick();
        self.clip_seek_tick();
        self.link_clips();
        if watching {
            self.recover_playback();
            self.per_video_setup();
            self.gapless_tick();
            self.handle_end_of_playback();
            self.record_progress(false);
            self.player.tick(ctx);
            if matches!(self.player.state(), State::Playing | State::Buffering | State::Loading) {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }

        if self.mini {
            if !watching {
                self.set_mini(false);
            } else {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(panel()))
                    .show(ui, |ui| self.now_playing_bar(ui));
                ctx.request_repaint_after(Duration::from_millis(250));
                return;
            }
        }
        {
            let bgp = ctx.layer_painter(egui::LayerId::background());
            surface::background(&bgp, ctx.content_rect());
        }
        let in_player = watching && !self.pip;
        if !in_player {
            self.amb_active = false;
        }
        if in_player && self.amb_active {
            self.draw_ambient(ctx);
        }
        let amb_ui = in_player && self.amb_active;
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::B)) {
            self.toggle_sidebar();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.start_browser();
        }
        let dropped: Vec<std::path::PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().next() {
            self.open_local(p.display().to_string());
        }
        if !(in_player && self.fullscreen) {
            let want = if SIDE_COLLAPSED.load(std::sync::atomic::Ordering::Relaxed) { 76.0 } else { 272.0 };
            let width = ctx.animate_value_with_time(uid("side-w"), want, 0.18);
            let compact = width < 170.0;
            egui::Panel::left("side")
                .exact_size(width)
                .resizable(false)
                .frame(egui::Frame::new()
                    .fill(if amb_ui { Color32::from_rgba_unmultiplied(20, 29, 32, 150) } else { panel() })
                    .inner_margin(egui::Margin::symmetric(if compact { 8 } else { 12 }, 0)))
                .show(ui, |ui| self.sidebar(ui, compact));
        }

        if in_player && self.comments_open && !self.fullscreen {
            egui::Panel::right("comments")
                .exact_size(390.0)
                .resizable(false)
                .frame(egui::Frame::new().fill(panel()).inner_margin(egui::Margin::same(14)))
                .show(ui, |ui| self.comments_panel(ui));
        }

        // a newer release was found on GitHub
        if !in_player {
            self.ffmpeg_bar(ui);
            self.update_bar(ui);
        }

        if !self.online && !in_player {
            egui::Panel::top("offline")
                .exact_size(36.0)
                .resizable(false)
                .frame(egui::Frame::new().fill(Color32::from_rgb(70, 48, 18)).inner_margin(egui::Margin::symmetric(24, 6)))
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(egui::RichText::new(tr("You're offline — downloads, history and saved playlists still work.")).size(13.5).color(Color32::from_rgb(255, 214, 160)));
                        if self.sel != Sel::Downloads && ui.add(egui::Button::new(tr("Open downloads")).small()).clicked() {
                            self.sel = Sel::Downloads;
                        }
                    });
                });
        }

        // music and other audio keep playing in a slim bar instead of a floating window
        if watching && self.pip && self.player.audio_only() {
            egui::Panel::bottom("nowplaying")
                .exact_size(70.0)
                .resizable(false)
                .frame(egui::Frame::new().fill(panel()))
                .show(ui, |ui| self.now_playing_bar(ui));
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(if amb_ui { Color32::TRANSPARENT } else { bg() }).inner_margin(if in_player { egui::Margin::ZERO } else { egui::Margin::symmetric(24, 12) }))
            .show(ui, |ui| {
                if in_player {
                    // the video page sits on black, so it keeps the dark palette in the light theme
                    let light = !dark_mode();
                    let saved = ui.visuals().clone();
                    if light {
                        FORCE_DARK.store(true, std::sync::atomic::Ordering::Relaxed);
                        *ui.visuals_mut() = build_visuals();
                    }
                    self.player_view(ui);
                    if light {
                        FORCE_DARK.store(false, std::sync::atomic::Ordering::Relaxed);
                        *ui.visuals_mut() = saved;
                    }
                } else if self.sel == Sel::Downloads {
                    self.downloads_view(ui);
                } else if self.sel == Sel::Stats {
                    self.insights_view(ui);
                } else if self.sel == Sel::Clips {
                    self.clips_view(ui);
                } else if self.sel == Sel::Podcasts {
                    self.podcasts_view(ui);
                } else if let Sel::Podcast(f) = self.sel.clone() {
                    self.podcast_view(ui, &f);
                } else if self.sel == Sel::Settings {
                    self.settings_view(ui);
                } else if matches!(self.sel, Sel::Music | Sel::MusicAlbum(_) | Sel::MusicArtist(_)) {
                    self.music_view(ui);
                } else {
                    self.grid(ui);
                }
            });

        self.new_list_dialog(ctx);
        self.import_list_dialog(ctx);
        self.new_group_dialog(ctx);
        self.overlays(ctx);

        if watching && self.pip && !self.player.audio_only() && !self.mini {
            FORCE_DARK.store(true, std::sync::atomic::Ordering::Relaxed);
            self.pip_window(ctx);
            FORCE_DARK.store(false, std::sync::atomic::Ordering::Relaxed);
            ctx.request_repaint();
        }
    }
}
