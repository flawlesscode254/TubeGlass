//! The Settings page.

use crate::*;

impl App {
    /// Draws the Settings page.
    pub(crate) fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr("Settings")).size(26.0).strong());
        ui.add_space(8.0);
        let mut changed = false;
        let mut pick_dir = false;
        let mut clear_cache = false;
        let mut clear_history = false;
        let mut import_subs = false;
        let mut export_subs = false;
        let mut backup = false;
        let mut restore = false;
        let mut diagnose = false;
        let mut check_update = false;
        let mut free_finished = false;
        let mut free_older: Option<u64> = None;
        if !self.storage_page {
            self.storage_page = true;
            self.refresh_storage();
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let section = |ui: &mut egui::Ui, title: &str, f: &mut dyn FnMut(&mut egui::Ui)| {
                ui.label(egui::RichText::new(tr(title)).size(12.0).color(muted()).strong());
                egui::Frame::new()
                    .fill(card())
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        f(ui);
                    });
                ui.add_space(14.0);
            };
            let row = |ui: &mut egui::Ui, label: &str, hint: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                let texts = |ui: &mut egui::Ui| {
                    ui.label(egui::RichText::new(tr(label)).size(15.0));
                    if !hint.is_empty() {
                        ui.label(egui::RichText::new(tr(hint)).size(12.0).color(muted()));
                    }
                };
                if ui.available_width() >= 640.0 {
                    // label on the left (it wraps inside its half), controls on the right
                    ui.horizontal(|ui| {
                        let half = (ui.available_width() * 0.5).max(260.0);
                        ui.vertical(|ui| {
                            ui.set_max_width(half);
                            texts(ui);
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true), |ui| add(ui));
                    });
                } else {
                    // narrow: the controls go under the text instead of on top of it
                    ui.vertical(|ui| texts(ui));
                    ui.add_space(2.0);
                    let w = ui.available_width();
                    ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true), |ui| add(ui));
                }
                ui.add_space(6.0);
            };

            section(ui, "PLAYBACK", &mut |ui| {
                row(ui, "Default quality", "Used whenever a video starts", &mut |ui| {
                    for q in [1080u32, 720, 480, 360] {
                        let on = self.settings.default_quality == q;
                        let b = egui::Button::new(egui::RichText::new(format!("{q}p")).color(if on { ON_ACCENT } else { ink() }))
                            .fill(if on { accent() } else { card_hi() });
                        if ui.add(b).clicked() {
                            self.settings.default_quality = q;
                            self.player.quality = q;
                            changed = true;
                        }
                    }
                });
                row(ui, "Resume where you left off", "Videos you have watched reopen at the same place", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.resume);
                });
                row(ui, "Ambient mode", "A soft glow of the video's colours around the picture", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.ambient);
                });
                row(ui, "Gapless playback", "Songs in a queue run into each other with no pause", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.gapless);
                });
                row(ui, "Crossfade", "Blend the end of a song into the next (needs gapless)", &mut |ui| {
                    for (label, secs) in [(tr("6 s"), 6u32), (tr("3 s"), 3), (tr("Off"), 0)] {
                        let on = self.settings.crossfade == secs;
                        let b = egui::Button::new(egui::RichText::new(label).color(if on { ON_ACCENT } else { ink() }))
                            .fill(if on { accent() } else { card_hi() });
                        if ui.add(b).clicked() {
                            self.settings.crossfade = secs;
                            changed = true;
                        }
                    }
                });
                row(ui, "Autoplay next in queue", "Move on to the next item when one ends", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.autoplay);
                });
                row(ui, "Skip sponsor segments", "Uses the community SponsorBlock database", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.sponsorblock);
                });
            });
            section(ui, "AUDIO", &mut |ui| {
                row(ui, "Equalizer", "Presets or your own bass / mid / treble", &mut |ui| {
                    for (label, b, m, t) in [(tr("Vocal"), -2.0f32, 3.0, 1.0), (tr("Treble"), -1.0, 0.0, 5.0), (tr("Bass"), 6.0, 0.0, -1.0), (tr("Flat"), 0.0, 0.0, 0.0)] {
                        let on = (self.settings.eq_bass, self.settings.eq_mid, self.settings.eq_treble) == (b, m, t);
                        let btn = egui::Button::new(egui::RichText::new(label).color(if on { ON_ACCENT } else { ink() }))
                            .fill(if on { accent() } else { card_hi() });
                        if ui.add(btn).clicked() {
                            self.settings.eq_bass = b;
                            self.settings.eq_mid = m;
                            self.settings.eq_treble = t;
                            changed = true;
                        }
                    }
                });
                for (label, v) in [
                    ("Bass", &mut self.settings.eq_bass),
                    ("Mid", &mut self.settings.eq_mid),
                    ("Treble", &mut self.settings.eq_treble),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized([60.0, 20.0], egui::Label::new(egui::RichText::new(label).size(13.0).color(muted())));
                        if db_slider(ui, v, 360.0) {
                            changed = true;
                        }
                    });
                }
                ui.add_space(6.0);
                row(ui, "Even out volume", "Quiet parts are lifted so songs and videos feel equally loud", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.normalize);
                });
                row(ui, "Skip silence", "Cuts long pauses in spoken audio (not for music; the time shown runs a little ahead)", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.skip_silence);
                });
            });
            section(ui, "STORAGE", &mut |ui| {
                if let Some(st) = &self.storage {
                    let item_count = st.items.to_string();
                    let item_label = if st.items == 1 {
                        trf("{count} item", &[("count", &item_count)])
                    } else {
                        trf("{count} items", &[("count", &item_count)])
                    };
                    let rows = [
                        (tr("Downloads"), format!("{}  •  {item_label}", fmt_size(st.downloads))),
                        (tr("Clips"), fmt_size(st.clips)),
                        (tr("Thumbnails and avatars"), fmt_size(st.images)),
                        (tr("Saved feeds"), fmt_size(st.feeds)),
                        (tr("Free on this disk"), fmt_size(st.free)),
                    ];
                    for (k, v) in rows {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(k).size(14.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(v).size(14.0).color(muted()));
                            });
                        });
                    }
                    ui.add_space(8.0);
                } else {
                    ui.add(egui::Spinner::new().size(16.0).color(accent()));
                }
                let mut ask = |ui: &mut egui::Ui, id: u8, label: &str, hint: &str| {
                    let sure = self.storage_confirm == Some(id);
                    let mut go = false;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(tr(label)).size(15.0));
                            if !hint.is_empty() {
                                ui.label(egui::RichText::new(tr(hint)).size(12.0).color(muted()));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let b = egui::Button::new(egui::RichText::new(tr(if sure { "Confirm" } else { "Delete…" })).color(if sure { Color32::WHITE } else { ink() }))
                                .fill(if sure { LIVE } else { card_hi() });
                            if ui.add(b).clicked() {
                                go = sure;
                                self.storage_confirm = if sure { None } else { Some(id) };
                            }
                        });
                    });
                    ui.add_space(6.0);
                    go
                };
                if ask(ui, 1, &tr("Finished downloads"), &tr("Videos and songs you have already watched or listened to all the way")) {
                    free_finished = true;
                }
                if ask(ui, 2, &tr("Downloads older than 30 days"), &tr("Frees the most space when you download a lot")) {
                    free_older = Some(30);
                }
                if ask(ui, 3, &tr("Downloads older than 90 days"), "") {
                    free_older = Some(90);
                }
            });
            section(ui, "OFFLINE", &mut |ui| {
                row(ui, "Save captions and lyrics with downloads", "So subtitles and lyrics also work without a connection", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.dl_extras);
                });
            });
            section(ui, "CAPTIONS", &mut |ui| {
                row(ui, "Show captions by default", "When a video has them", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.captions_default);
                });
                row(ui, "Preferred language", "Language code, for example en, es, fr", &mut |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.settings.caption_lang).desired_width(70.0));
                    if r.lost_focus() {
                        changed = true;
                    }
                });
            });
            section(ui, "DOWNLOADS", &mut |ui| {
                row(ui, "Folder", &download::dir().display().to_string(), &mut |ui| {
                    if ui.button(tr("Choose…")).clicked() {
                        pick_dir = true;
                    }
                    if !self.settings.download_dir.is_empty() && ui.button(tr("Reset")).clicked() {
                        self.settings.download_dir.clear();
                        apply_settings(&self.settings);
                        self.library = download::list();
                        changed = true;
                    }
                });
            });
            section(ui, "APPEARANCE", &mut |ui| {
                row(ui, "Interface language", "Choose the language used for TubeGlass controls", &mut |ui| {
                    let options = language_options();
                    let selected = if self.settings.language == "system" {
                        tr("Use system language")
                    } else {
                        options.iter().find(|(code, _)| code == &self.settings.language)
                            .map(|(_, name)| name.clone()).unwrap_or_else(|| self.settings.language.clone())
                    };
                    egui::ComboBox::from_id_salt("ui-language")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if ui.selectable_value(&mut self.settings.language, "system".to_owned(), tr("Use system language")).changed() {
                                i18n::set_language(&self.settings.language);
                                changed = true;
                            }
                            for (code, label) in options {
                                if ui.selectable_value(&mut self.settings.language, code, label).changed() {
                                    i18n::set_language(&self.settings.language);
                                    changed = true;
                                }
                            }
                        });
                });
                row(ui, "Theme", "Dark, light, or follow the system (the video page stays dark)", &mut |ui| {
                    for (label, v) in [(tr("System"), 2u8), (tr("Light"), 1), (tr("Dark"), 0)] {
                        let on = self.settings.theme == v;
                        let b = egui::Button::new(egui::RichText::new(label).color(if on { ON_ACCENT } else { ink() }))
                            .fill(if on { accent() } else { card_hi() });
                        if ui.add(b).clicked() {
                            self.settings.theme = v;
                            changed = true;
                        }
                    }
                });
                row(ui, "Text size", "Scales the whole interface", &mut |ui| {
                    for (label, v) in [("130%", 1.3f32), ("115%", 1.15), ("100%", 1.0), ("90%", 0.9)] {
                        let on = (self.settings.font_scale - v).abs() < 0.01;
                        let b = egui::Button::new(egui::RichText::new(tr(label)).color(if on { ON_ACCENT } else { ink() }))
                            .fill(if on { accent() } else { card_hi() });
                        if ui.add(b).clicked() {
                            self.settings.font_scale = v;
                            changed = true;
                        }
                    }
                });
                row(ui, "Density", "How big and how far apart video cards are", &mut |ui| {
                    for (label, v) in [(tr("Spacious"), 2u8), (tr("Comfortable"), 1), (tr("Compact"), 0)] {
                        let on = self.settings.density == v;
                        let b = egui::Button::new(egui::RichText::new(label).color(if on { ON_ACCENT } else { ink() }))
                            .fill(if on { accent() } else { card_hi() });
                        if ui.add(b).clicked() {
                            self.settings.density = v;
                            changed = true;
                        }
                    }
                });
                row(ui, "Accent colour", "Pick a preset or mix your own", &mut |ui| {
                    // custom colour: opens egui's picker
                    let mut rgb = self.settings.accent_custom;
                    let on = self.settings.accent == ACCENTS.len();
                    ui.horizontal(|ui| {
                        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                            self.settings.accent_custom = rgb;
                            self.settings.accent = ACCENTS.len();
                            apply_settings(&self.settings);
                            changed = true;
                        }
                        if on {
                            ui.label(egui::RichText::new(tr("custom")).size(12.0).color(muted()));
                        }
                    });
                    for (i, (name, c, _)) in ACCENTS.iter().enumerate().rev() {
                        let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr(name));
                        ui.painter().circle_filled(r.center(), 11.0, *c);
                        if self.settings.accent == i {
                            ui.painter().circle_stroke(r.center(), 14.0, Stroke::new(2.0, ink()));
                        }
                        if resp.clicked() {
                            self.settings.accent = i;
                            apply_settings(&self.settings);
                            changed = true;
                        }
                    }
                });
            });
            section(ui, "SUBSCRIPTIONS", &mut |ui| {
                row(ui, "Import", "NewPipe export, YouTube Takeout CSV, OPML, or a list of links", &mut |ui| {
                    if ui.button(tr("Import…")).clicked() {
                        import_subs = true;
                    }
                });
                row(ui, "Export", "Saves a NewPipe-compatible subscriptions file", &mut |ui| {
                    if ui.button(tr("Export…")).clicked() {
                        export_subs = true;
                    }
                });
            });
            section(ui, "NOTIFICATIONS", &mut |ui| {
                row(ui, "Notify me", "Live streams by default. Right-click a channel to also get new uploads, or to mute it", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.notifications);
                });
            });
            section(ui, "DATA", &mut |ui| {
                row(ui, "Back up everything", "Channels, playlists, history, liked songs, groups and settings in one file", &mut |ui| {
                    if ui.button(tr("Back up…")).clicked() {
                        backup = true;
                    }
                });
                row(ui, "Restore a backup", "Replaces what is here with the file's contents, then restarts", &mut |ui| {
                    if ui.button(tr("Restore…")).clicked() {
                        restore = true;
                    }
                });
                row(ui, "Check for updates automatically", "Looks at the project's GitHub releases now and then (nothing else is sent)", &mut |ui| {
                    changed |= switch(ui, &mut self.settings.update_check);
                });
                let note = self.update_note.clone().unwrap_or_else(|| trf("You have version {version}", &[("version", update::CURRENT)]));
                row(ui, "Check for updates now", &note, &mut |ui| {
                    if self.update_checking {
                        ui.add(egui::Spinner::new().size(16.0).color(accent()));
                    } else if ui.button(tr("Check now")).clicked() {
                        check_update = true;
                    }
                });
                let ff = crate::platform::ffmpeg_in_use();
                let ffm = &mut self.ffmpeg;
                let mut redo = false;
                row(ui, "ffmpeg", &trf("Everything plays through it. In use: {ffmpeg}", &[("ffmpeg", &ff)]), &mut |ui| {
                    if matches!(ffm, FfmpegState::Downloading(_)) {
                        ui.add(egui::Spinner::new().size(16.0).color(accent()));
                    } else if ui.button(tr("Download a fresh copy")).clicked() {
                        redo = true;
                    }
                });
                if redo {
                    self.start_ffmpeg_setup();
                }
                row(ui, "Check YouTube access", "Tests search, playback, comments and music, to see what broke if something stops working", &mut |ui| {
                    if self.diag_running {
                        ui.add(egui::Spinner::new().size(16.0).color(accent()));
                    } else if ui.button(tr("Run check")).clicked() {
                        diagnose = true;
                    }
                });
                if let Some(d) = &self.diag {
                    for (name, ok, note) in d {
                        ui.horizontal(|ui| {
                            ui.colored_label(if *ok { Color32::from_rgb(110, 200, 120) } else { LIVE }, "●");
                            ui.label(egui::RichText::new(trf("{name}  —  {note}", &[("name", name), ("note", note)])).size(13.0).color(if *ok { muted() } else { ink() }));
                        });
                    }
                    ui.add_space(6.0);
                }
                row(ui, "Clear image cache", "Thumbnails and avatars are downloaded again when needed", &mut |ui| {
                    if ui.button(tr("Clear")).clicked() {
                        clear_cache = true;
                    }
                });
                row(ui, "Clear watch history", "Also forgets where you left each video", &mut |ui| {
                    if ui.button(tr("Clear")).clicked() {
                        clear_history = true;
                    }
                });
            });
        });
        if changed {
            self.save_settings();
        }
        if pick_dir {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let p = rfd::FileDialog::new()
                    .set_title(tr("Download folder"))
                    .pick_folder();
                let _ = tx.send(Msg::DirPicked(p.map(|p| p.display().to_string())));
                ctx.request_repaint();
            });
        }
        if import_subs {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Import subscriptions"))
                    .add_filter(tr("Subscriptions"), &["json", "csv", "opml", "xml", "txt"])
                    .add_filter(tr("All files"), &["*"])
                    .pick_file();
                if let Some(p) = picked {
                    if let Ok(text) = std::fs::read_to_string(p) {
                        let list = subs::parse(&text);
                        let _ = tx.send(Msg::Imported(list));
                        ctx.request_repaint();
                    }
                }
            });
        }
        if export_subs {
            let json = subs::export_newpipe(&self.channels);
            let n = self.channels.len();
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new()
                    .set_title(tr("Export subscriptions"))
                    .set_file_name("newpipe_subscriptions.json")
                    .add_filter(tr("JSON"), &["json"])
                    .save_file();
                if let Some(p) = picked {
                    let _ = std::fs::write(&p, json);
                    let _ = tx.send(Msg::ExportedSubs(n));
                    ctx.request_repaint();
                }
            });
        }
        if backup {
            let bundle = backup_bundle();
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title(tr("Back up TubeGlass"))
                    .set_file_name("tubeglass-backup.json")
                    .add_filter(tr("JSON"), &["json"])
                    .save_file()
                {
                    let msg = match std::fs::write(&p, bundle) {
                        Ok(_) => trf("Backup saved to {path}", &[("path", &p.display().to_string())]),
                        Err(e) => trf("Could not save the backup: {error}", &[("error", &e.to_string())]),
                    };
                    let _ = tx.send(Msg::Note(msg));
                    ctx.request_repaint();
                }
            });
        }
        if restore {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let Some(p) = rfd::FileDialog::new()
                    .set_title(tr("Restore a backup"))
                    .add_filter(tr("JSON"), &["json"])
                    .pick_file()
                else {
                    return;
                };
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                let sure = rfd::MessageDialog::new()
                    .set_title(tr("Restore backup"))
                    .set_description(tr("This replaces your current channels, playlists, history and settings with the backup, then restarts TubeGlass."))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::Yes;
                if !sure {
                    return;
                }
                match restore_bundle(&text) {
                    Ok(_) => {
                        let _ = tx.send(Msg::Restored);
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::Note(trf("Could not restore: {error}", &[("error", &e.to_string())])));
                    }
                }
                ctx.request_repaint();
            });
        }
        if free_finished || free_older.is_some() {
            let now = std::time::SystemTime::now();
            let finished: HashSet<String> = self
                .history
                .iter()
                .filter(|h| {
                    h.duration > 0.0 && (h.position == 0.0 || h.position >= h.duration - 10.0)
                })
                .map(|h| h.video.id.clone())
                .collect();
            let mut n = 0;
            let mut bytes = 0u64;
            for d in self.library.clone() {
                let old = free_older.is_some_and(|days| {
                    std::fs::metadata(&d.file)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| now.duration_since(t).ok())
                        .is_some_and(|age| age.as_secs() > days * 86_400)
                });
                if (free_finished && finished.contains(&d.id)) || old {
                    bytes += std::fs::metadata(&d.file).map(|m| m.len()).unwrap_or(0);
                    download::delete(&d);
                    n += 1;
                }
            }
            self.library = download::list();
            let count = n.to_string();
            let size = fmt_size(bytes);
            self.toast(if n == 1 {
                trf("Removed {count} download, freed {size}", &[("count", &count), ("size", &size)])
            } else {
                trf("Removed {count} downloads, freed {size}", &[("count", &count), ("size", &size)])
            });
            self.refresh_storage();
        }
        if check_update {
            // a skipped version is offered again when the user asks
            self.check_for_update(true);
        }
        if diagnose {
            self.diag_running = true;
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let r = run_diagnostics();
                let _ = tx.send(Msg::Diag(r));
                ctx.request_repaint();
            });
        }
        if clear_cache {
            if let Some(d) = cache_dir() {
                let _ = std::fs::remove_dir_all(d.join("img"));
                let _ = std::fs::create_dir_all(d.join("img"));
            }
            self.toast(tr("Image cache cleared (restart to refetch avatars)"));
        }
        if clear_history {
            self.history.clear();
            save_history(&self.history);
            self.toast(tr("Watch history cleared"));
        }
    }
}
