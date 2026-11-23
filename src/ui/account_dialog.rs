//! "Import from a YouTube account" and "copy settings from a backup": the pop-ups and what they do.

use crate::*;

/// State of the account import pop-up.
#[derive(Default)]
pub(crate) struct AcctDialog {
    pub input: String,
    pub loading: bool,
    pub err: Option<String>,
    pub snap: Option<account::Snapshot>,
    pub pick_ch: Vec<bool>,
    pub pick_pl: Vec<bool>,
    pub keep_sync: bool,
}

/// State of the "copy from a backup" pop-up.
pub(crate) struct ProfileDialog {
    pub bundle: serde_json::Value,
    pub groups: Vec<Group>,
    pub has_settings: bool,
    pub take_groups: bool,
    pub take_captions: bool,
    pub take_quality: bool,
}

impl App {
    /// Reads the public pages of the typed account in the background.
    fn look_up_account(&mut self) {
        let Some(d) = self.acct.as_mut() else { return };
        if d.loading || d.input.trim().is_empty() {
            return;
        }
        d.loading = true;
        d.err = None;
        d.snap = None;
        let input = d.input.clone();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::AcctLooked(account::snapshot(&input)));
            ctx.request_repaint();
        });
    }

    /// Adds the ticked channels and playlists.
    fn apply_account_import(&mut self) {
        let Some(d) = self.acct.take() else { return };
        let Some(snap) = d.snap else { return };
        let mut added = vec![];
        for (h, on) in snap.channels.iter().zip(&d.pick_ch) {
            // everything the account shows is "seen": a channel left out or removed later is not added by a sync
            self.settings.sync_seen.insert(format!("ch:{}", h.channel.id));
            if *on && !self.channels.iter().any(|c| c.id == h.channel.id) {
                self.channels.push(h.channel.clone());
                added.push(h.channel.clone());
            }
        }
        save(&self.channels);
        for c in &added {
            self.refresh(c);
        }
        let mut lists = 0;
        for (p, on) in snap.playlists.iter().zip(&d.pick_pl) {
            if *on {
                lists += 1;
                let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), p.id.clone());
                std::thread::spawn(move || {
                    let r = yt::playlist_videos(&id);
                    let _ = tx.send(Msg::ListImported(id, r));
                    ctx.request_repaint();
                });
            }
        }
        if d.keep_sync {
            if let Some(s) = &snap.source {
                self.settings.sync_source = if d.input.trim().is_empty() { s.id.clone() } else { d.input.trim().to_string() };
            }
            self.settings.sync_on = true;
            self.settings.sync_last = now_secs();
        }
        self.save_settings();
        self.toast(format!(
            "Imported {} channel{} and {lists} playlist{}",
            added.len(),
            if added.len() == 1 { "" } else { "s" },
            if lists == 1 { "" } else { "s" }
        ));
    }

    /// Re-checks the synced account (at most every half hour unless `force`) and adds what is new.
    pub(crate) fn sync_account(&mut self, force: bool) {
        if !self.settings.sync_on || self.settings.sync_source.trim().is_empty() || self.syncing || !self.online {
            return;
        }
        if !force && now_secs().saturating_sub(self.settings.sync_last) < 1800 {
            return;
        }
        self.syncing = true;
        self.settings.sync_last = now_secs();
        let src = self.settings.sync_source.clone();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::AcctSynced(account::snapshot(&src)));
            ctx.request_repaint();
        });
    }

    /// Adds what a sync found: new channels now, new playlist items once each list is fetched.
    pub(crate) fn apply_sync(&mut self, snap: account::Snapshot) {
        let plan = account::plan(&snap, &self.channels, &self.settings.sync_seen, &self.settings.imported_lists);
        for h in &snap.channels {
            self.settings.sync_seen.insert(format!("ch:{}", h.channel.id));
        }
        for c in &plan.new_channels {
            self.channels.push(c.clone());
        }
        if !plan.new_channels.is_empty() {
            save(&self.channels);
            for c in plan.new_channels.clone() {
                self.refresh(&c);
            }
        }
        for (pid, lid) in plan.check_lists {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let r = yt::playlist_videos(&pid);
                let _ = tx.send(Msg::SyncList(pid, lid, r));
                ctx.request_repaint();
            });
        }
        let mut note = format!("Checked just now: {} new channel{}", plan.new_channels.len(), if plan.new_channels.len() == 1 { "" } else { "s" });
        if plan.new_playlists > 0 {
            note += &format!(", {} playlist{} not imported yet", plan.new_playlists, if plan.new_playlists == 1 { "" } else { "s" });
        }
        self.sync_note = note;
        if !plan.new_channels.is_empty() {
            self.toast(format!("Synced {} new channel{}", plan.new_channels.len(), if plan.new_channels.len() == 1 { "" } else { "s" }));
        }
        self.save_settings();
    }

    /// Adds new items of a synced playlist to its local copy (nothing is removed).
    pub(crate) fn apply_sync_list(&mut self, pid: String, lid: String, r: Result<(String, Vec<Video>), String>) {
        let Ok((_, fetched)) = r else { return };
        let Some(list) = self.lists.iter_mut().find(|l| l.id == lid) else { return };
        let fresh = account::new_items(&pid, &fetched, &list.videos, &self.settings.sync_seen);
        let n = fresh.len();
        list.videos.extend(fresh.into_iter().map(|mut v| {
            if v.kind == Kind::Live {
                v.kind = Kind::Video;
            }
            v
        }));
        let name = list.name.clone();
        for v in &fetched {
            self.settings.sync_seen.insert(format!("pl:{pid}:{}", v.id));
        }
        if n > 0 {
            save_lists(&self.lists);
            self.toast(format!("Added {n} new video{} to “{name}”", if n == 1 { "" } else { "s" }));
        }
        self.save_settings();
    }

    /// Opens a backup file and shows what could be copied from it.
    pub(crate) fn open_profile_dialog(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            self.toast("That is not a TubeGlass backup");
            return;
        };
        if v["app"] != "tubeglass" {
            self.toast("That is not a TubeGlass backup");
            return;
        }
        let groups: Vec<Group> = serde_json::from_value(v["files"]["groups.json"].clone()).unwrap_or_default();
        let has_settings = v["files"]["settings.json"].is_object();
        self.profile = Some(ProfileDialog { bundle: v, take_groups: !groups.is_empty(), groups, has_settings, take_captions: has_settings, take_quality: has_settings });
    }

    /// Copies the ticked parts of the backup into this profile.
    fn apply_profile(&mut self) {
        let Some(p) = self.profile.take() else { return };
        let mut done: Vec<String> = vec![];
        if p.take_groups {
            let mut n = 0;
            for g in &p.groups {
                // a group of the same name gets the backup's channels added; otherwise a new group is made
                match self.groups.iter_mut().find(|x| x.name == g.name) {
                    Some(x) => {
                        for c in &g.channels {
                            if !x.channels.contains(c) {
                                x.channels.push(c.clone());
                            }
                        }
                    }
                    None => {
                        self.groups.push(Group { id: format!("g-{}-{n}", now_secs()), name: g.name.clone(), channels: g.channels.clone() });
                        n += 1;
                    }
                }
            }
            save_groups(&self.groups);
            done.push(format!("{} group{}", p.groups.len(), if p.groups.len() == 1 { "" } else { "s" }));
        }
        let theirs: serde_json::Value = p.bundle["files"]["settings.json"].clone();
        let take = |k: &str| theirs.get(k).cloned();
        let mut set = |k: &str, apply: &mut dyn FnMut(serde_json::Value)| {
            if let Some(v) = take(k) {
                apply(v);
            }
        };
        if p.take_captions {
            let s = &mut self.settings;
            set("captions_default", &mut |v| s.captions_default = v.as_bool().unwrap_or(s.captions_default));
            set("caption_lang", &mut |v| if let Some(x) = v.as_str() { s.caption_lang = x.to_string() });
            set("cc2_on", &mut |v| s.cc2_on = v.as_bool().unwrap_or(s.cc2_on));
            set("cc2_lang", &mut |v| if let Some(x) = v.as_str() { s.cc2_lang = x.to_string() });
            set("cc2_swap", &mut |v| s.cc2_swap = v.as_bool().unwrap_or(s.cc2_swap));
            for (k, dst) in [("cc1_size", &mut s.cc1_size), ("cc2_size", &mut s.cc2_size)] {
                if let Some(x) = theirs.get(k).and_then(|v| v.as_f64()) {
                    *dst = (x as f32).clamp(0.6, 1.8);
                }
            }
            for (k, dst) in [("cc1_color", &mut s.cc1_color), ("cc2_color", &mut s.cc2_color)] {
                if let Some(c) = theirs.get(k).and_then(|v| serde_json::from_value::<[u8; 3]>(v.clone()).ok()) {
                    *dst = c;
                }
            }
            for (k, dst) in [("cc1_alpha", &mut s.cc1_alpha), ("cc2_alpha", &mut s.cc2_alpha), ("cc_bg", &mut s.cc_bg)] {
                if let Some(x) = theirs.get(k).and_then(|v| v.as_u64()) {
                    *dst = x.min(255) as u8;
                }
            }
            done.push("caption settings".into());
        }
        if p.take_quality {
            let s = &mut self.settings;
            if let Some(q) = theirs.get("default_quality").and_then(|v| v.as_u64()) {
                s.default_quality = q as u32;
            }
            if let Some(x) = theirs.get("speed").and_then(|v| v.as_f64()) {
                s.speed = (x as f32).clamp(0.5, 2.0);
            }
            if let Some(b) = theirs.get("autoplay").and_then(|v| v.as_bool()) {
                s.autoplay = b;
            }
            self.player.quality = self.settings.default_quality;
            done.push("quality and playback".into());
        }
        self.save_settings();
        self.toast(if done.is_empty() { "Nothing was copied".to_string() } else { format!("Copied {}", done.join(", ")) });
    }

    /// The account import and backup-copy pop-ups.
    pub(crate) fn account_dialogs(&mut self, ctx: &egui::Context) {
        if self.acct.is_some() {
            self.account_dialog(ctx);
        }
        if self.profile.is_some() {
            self.profile_dialog(ctx);
        }
    }

    fn account_dialog(&mut self, ctx: &egui::Context) {
        let mut look = false;
        let mut import = false;
        let mut cancel = false;
        let have: Vec<String> = self.channels.iter().map(|c| c.id.clone()).collect();
        let imported = self.settings.imported_lists.clone();
        let Some(d) = self.acct.as_mut() else { return };
        egui::Window::new("Import from a YouTube account")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(egui::RichText::new("Paste the account's channel link or @handle. No sign-in is needed, so only what the account shows publicly can be read: its subscriptions (when it made them public, otherwise the channels it features) and its public playlists.").size(12.5).color(muted()));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let f = text_field(ui, &mut d.input, "@handle or https://www.youtube.com/@…", 410.0, 40.0, "search", None, false);
                    if d.snap.is_none() && !d.loading {
                        f.resp.request_focus();
                    }
                    if f.submit {
                        look = true;
                    }
                    let b = egui::Button::new(egui::RichText::new("Look up").strong()).fill(accent());
                    if ui.add_enabled(!d.loading && !d.input.trim().is_empty(), b).clicked() {
                        look = true;
                    }
                });
                if d.loading {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(16.0).color(accent()));
                        ui.label(egui::RichText::new("Reading the account…").color(muted()));
                    });
                }
                if let Some(e) = &d.err {
                    ui.add_space(6.0);
                    ui.colored_label(LIVE, e);
                }
                if let Some(snap) = &d.snap {
                    ui.add_space(8.0);
                    if snap.channels.is_empty() && snap.playlists.is_empty() {
                        ui.label(egui::RichText::new("This account shows no public subscriptions, featured channels or playlists.").color(muted()));
                    }
                    egui::ScrollArea::vertical().max_height(340.0).auto_shrink([false, true]).show(ui, |ui| {
                        if !snap.channels.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("CHANNELS  ({})", snap.channels.len())).size(11.0).color(muted()).strong());
                                if ui.small_button("All").clicked() {
                                    for (i, h) in snap.channels.iter().enumerate() {
                                        d.pick_ch[i] = !have.contains(&h.channel.id);
                                    }
                                }
                                if ui.small_button("None").clicked() {
                                    d.pick_ch.iter_mut().for_each(|p| *p = false);
                                }
                            });
                            for (i, h) in snap.channels.iter().enumerate() {
                                let exists = have.contains(&h.channel.id);
                                ui.horizontal(|ui| {
                                    ui.add_enabled(!exists, egui::Checkbox::new(&mut d.pick_ch[i], ""));
                                    ui.label(egui::RichText::new(&h.channel.name).size(14.0));
                                    let extra = [h.handle.as_str(), h.subs.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("  •  ");
                                    ui.label(egui::RichText::new(extra).size(12.0).color(muted()));
                                    if exists {
                                        ui.label(egui::RichText::new("already added").size(12.0).color(TEAL));
                                    }
                                });
                            }
                            ui.add_space(8.0);
                        }
                        if !snap.playlists.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("PLAYLISTS  ({})", snap.playlists.len())).size(11.0).color(muted()).strong());
                                if ui.small_button("All").clicked() {
                                    for (i, p) in snap.playlists.iter().enumerate() {
                                        d.pick_pl[i] = !imported.contains_key(&p.id);
                                    }
                                }
                                if ui.small_button("None").clicked() {
                                    d.pick_pl.iter_mut().for_each(|p| *p = false);
                                }
                            });
                            for (i, p) in snap.playlists.iter().enumerate() {
                                let exists = imported.contains_key(&p.id);
                                ui.horizontal(|ui| {
                                    ui.add_enabled(!exists, egui::Checkbox::new(&mut d.pick_pl[i], ""));
                                    ui.label(egui::RichText::new(&p.title).size(14.0));
                                    ui.label(egui::RichText::new(&p.count).size(12.0).color(muted()));
                                    if exists {
                                        ui.label(egui::RichText::new("already imported").size(12.0).color(TEAL));
                                    }
                                });
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.checkbox(&mut d.keep_sync, "Keep in sync: on refresh, add this account's new subscriptions and new playlist items. Nothing is ever deleted here.");
                    let n_ch = d.pick_ch.iter().filter(|p| **p).count();
                    let n_pl = d.pick_pl.iter().filter(|p| **p).count();
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(egui::RichText::new(format!("Import {n_ch} channel{} and {n_pl} playlist{}", if n_ch == 1 { "" } else { "s" }, if n_pl == 1 { "" } else { "s" })).strong()).fill(accent());
                        if ui.add_enabled(n_ch + n_pl > 0 || d.keep_sync, b).clicked() {
                            import = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                } else {
                    ui.add_space(6.0);
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if look {
            self.look_up_account();
        }
        if import {
            self.apply_account_import();
        } else if cancel {
            self.acct = None;
        }
    }

    fn profile_dialog(&mut self, ctx: &egui::Context) {
        let mut apply = false;
        let mut cancel = false;
        let Some(p) = self.profile.as_mut() else { return };
        egui::Window::new("Copy from a backup")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(egui::RichText::new("Choose what to copy from the backup. Your current data is kept: groups are merged, and settings are replaced only for what you tick.").size(12.5).color(muted()));
                ui.add_space(8.0);
                ui.add_enabled(!p.groups.is_empty(), egui::Checkbox::new(&mut p.take_groups, format!("Groups  ({})", p.groups.len())));
                ui.add_enabled(p.has_settings, egui::Checkbox::new(&mut p.take_captions, "Caption language, second language and caption style"));
                ui.add_enabled(p.has_settings, egui::Checkbox::new(&mut p.take_quality, "Default quality, playback speed and autoplay"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Copy").strong()).fill(accent())).clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if apply {
            self.apply_profile();
        } else if cancel {
            self.profile = None;
        }
    }
}
