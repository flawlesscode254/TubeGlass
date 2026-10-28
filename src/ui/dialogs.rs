//! Pop-up dialogs (new playlist, new group) and the toast / shortcut overlays.

use crate::*;

impl App {
    /// The "New group" pop-up.
    pub(crate) fn new_group_dialog(&mut self, ctx: &egui::Context) {
        let Some((name, _)) = self.new_group.as_mut() else { return };
        let (mut create, mut cancel) = (false, false);
        egui::Window::new("New group")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(300.0);
                let f = text_field(ui, name, "Group name, e.g. Music", 300.0, 42.0, "folder", None, false);
                f.resp.request_focus();
                if f.submit {
                    create = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Create").strong()).fill(accent())).clicked() {
                        create = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if create {
            if let Some((name, first)) = self.new_group.take() {
                let name = name.trim().to_string();
                if name.is_empty() {
                    self.new_group = Some((name, first));
                } else {
                    let id = format!("g-{}", now_secs());
                    self.groups.push(Group { id: id.clone(), name, channels: first.into_iter().collect() });
                    save_groups(&self.groups);
                    self.sel = Sel::Group(id);
                }
            }
        } else if cancel {
            self.new_group = None;
        }
    }

    /// The "New playlist" pop-up (it can add a video straight away).
    pub(crate) fn new_list_dialog(&mut self, ctx: &egui::Context) {
        let Some((name, pending)) = self.new_list.as_mut() else { return };
        let mut create = false;
        let mut cancel = false;
        egui::Window::new("New playlist")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(300.0);
                let f = text_field(ui, name, "Playlist name", 300.0, 42.0, "list", None, false);
                f.resp.request_focus();
                if f.submit {
                    create = true;
                }
                if let Some(v) = pending.as_ref() {
                    ui.label(egui::RichText::new(format!("“{}” will be added to it.", v.title)).size(12.0).color(muted()));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Create").strong()).fill(accent())).clicked() {
                        create = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if create {
            if let Some((name, pending)) = self.new_list.take() {
                let name = name.trim().to_string();
                if !name.is_empty() {
                    let id = format!(
                        "pl-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    );
                    self.lists.push(LocalList { id: id.clone(), name, videos: vec![], offline: false });
                    if let Some(v) = pending {
                        self.toggle_in_list(&id, &v);
                    }
                    save_lists(&self.lists);
                } else {
                    self.new_list = Some((name, pending));
                }
            }
        } else if cancel {
            self.new_list = None;
        }
    }

    /// Toast notification and the keyboard-shortcut sheet.
    pub(crate) fn overlays(&mut self, ctx: &egui::Context) {
        if let Some((msg, at)) = &self.toast {
            let age = at.elapsed().as_secs_f32();
            if age > 3.0 {
                self.toast = None;
            } else {
                let a = (1.0 - ((age - 2.4) / 0.6).clamp(0.0, 1.0)).clamp(0.0, 1.0);
                let rect = ctx.content_rect();
                let font = FontId::proportional(15.0);
                let w = ctx.fonts_mut(|f| f.layout_no_wrap(msg.clone(), font.clone(), ink()).size().x) + 44.0;
                let r = Rect::from_center_size(Pos2::new(rect.center().x, rect.max.y - 130.0), vec2(w, 42.0));
                let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, uid("toast")));
                p.rect_filled(r, CornerRadius::same(21), panel().gamma_multiply(a));
                p.rect_stroke(r, CornerRadius::same(21), Stroke::new(1.0, accent().gamma_multiply(0.8 * a)), StrokeKind::Inside);
                p.text(r.center(), Align2::CENTER_CENTER, msg, font, ink().gamma_multiply(a));
                ctx.request_repaint_after(Duration::from_millis(30));
            }
        }
        if self.show_help {
            let mut open = true;
            egui::Window::new("Keyboard shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_width(430.0);
                    let rows = [
                        ("Space / K", "Play or pause"),
                        ("← →", "Seek 5 seconds"),
                        ("J / L", "Seek 10 seconds"),
                        ("↑ ↓", "Volume"),
                        ("M", "Mute"),
                        ("[  /  ]", "Slower / faster"),
                        ("N / P", "Next / previous in queue"),
                        ("A", "Audio only on or off"),
                        ("I", "Mini player on or off"),
                        ("B", "Ambient mode on or off"),
                        ("W", "Watch later on or off"),
                        ("C", "Comments"),
                        ("Y", "Lyrics (music)"),
                        ("F", "Fullscreen"),
                        ("Esc", "Back (keeps playing in PiP)"),
                        ("Ctrl+O", "Open a file"),
                        ("Ctrl+B", "Collapse the sidebar"),
                        ("?", "This sheet"),
                    ];
                    for (k, d) in rows {
                        ui.horizontal(|ui| {
                            ui.add_sized([130.0, 24.0], egui::Label::new(egui::RichText::new(k).strong().color(accent())));
                            ui.label(d);
                        });
                    }
                });
            if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_help = false;
            }
        }
    }
}

impl App {
    /// The "Import from YouTube" pop-up: paste playlist links, get editable local playlists.
    pub(crate) fn import_list_dialog(&mut self, ctx: &egui::Context) {
        let Some((text, loading)) = self.import_list.as_mut() else { return };
        let mut go = false;
        let mut cancel = false;
        let ids = playlist_ids(text);
        let busy = *loading > 0;
        egui::Window::new("Import playlists from YouTube")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(egui::RichText::new("Paste one or more playlist links (or ids). Each becomes a playlist you can edit, download and keep offline.").size(12.5).color(muted()));
                ui.add_space(6.0);
                let f = text_field(ui, text, "https://www.youtube.com/playlist?list=…", 420.0, 42.0, "list", None, false);
                if !busy {
                    f.resp.request_focus();
                }
                if f.submit && !ids.is_empty() && !busy {
                    go = true;
                }
                ui.add_space(4.0);
                let note = if busy {
                    format!("Importing… {loading} left")
                } else if text.trim().is_empty() {
                    String::new()
                } else if ids.is_empty() {
                    "No playlist link found yet. Private playlists cannot be imported.".to_string()
                } else {
                    format!("{} playlist{} found", ids.len(), if ids.len() == 1 { "" } else { "s" })
                };
                ui.label(egui::RichText::new(note).size(12.0).color(muted()));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let b = egui::Button::new(egui::RichText::new("Import").strong()).fill(accent());
                    if ui.add_enabled(!ids.is_empty() && !busy, b).clicked() {
                        go = true;
                    }
                    if ui.button(if busy { "Close" } else { "Cancel" }).clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if go {
            if let Some((_, loading)) = self.import_list.as_mut() {
                *loading = ids.len();
            }
            for id in ids {
                let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
                std::thread::spawn(move || {
                    let r = yt::playlist_videos(&id);
                    let _ = tx.send(Msg::ListImported(id, r));
                    ctx.request_repaint();
                });
            }
        }
        if cancel {
            self.import_list = None;
        }
    }
}
