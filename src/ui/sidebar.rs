//! The left sidebar: library, playlists, groups and channels.

use crate::*;

/// One row of the sidebar (icon or avatar, label, live dot).
pub(crate) fn sidebar_row(
    ui: &mut egui::Ui,
    label: &str,
    avatar: Option<&str>,
    icon: &str,
    selected: bool,
    live: bool,
    compact: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    let mut resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if compact {
        resp = resp.on_hover_text(label);
    }
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let row = rect.shrink2(vec2(0.0, 1.0));
    let p = ui.painter();
    if selected {
        p.rect_filled(row, CornerRadius::same(16), accent());
    } else if h > 0.0 {
        p.rect_filled(row, CornerRadius::same(14), ov((13.0 * h) as u8));
    }
    let tile = if compact {
        Rect::from_center_size(row.center(), vec2(36.0, 36.0))
    } else {
        Rect::from_min_size(
            Pos2::new(row.min.x + 6.0, row.center().y - 18.0),
            vec2(36.0, 36.0),
        )
    };
    match avatar {
        Some(url) if !url.is_empty() => {
            let c = tile.center();
            p.circle_filled(c, 16.0, card_hi());
            let uri = img_uri(ui.ctx(), url);
            if !uri.is_empty() {
                egui::Image::new(uri)
                    .corner_radius(CornerRadius::same(16))
                    .paint_at(ui, Rect::from_center_size(c, vec2(32.0, 32.0)));
            }
            if selected {
                ui.painter()
                    .circle_stroke(c, 18.0, Stroke::new(2.0, ON_ACCENT));
            }
        }
        _ => {
            let fill = if selected {
                Color32::from_black_alpha(34)
            } else {
                ov((12.0 + 14.0 * h) as u8)
            };
            p.rect_filled(tile, CornerRadius::same(11), fill);
            let col = if selected {
                ON_ACCENT
            } else if h > 0.4 {
                ink()
            } else {
                icons::muted_icon()
            };
            icons::draw(ui.painter(), tile.center(), icon, 20.0, col);
        }
    }
    if !compact {
        let max_w = row.width() - 100.0;
        let s = truncate(ui, label, FontId::proportional(15.0), max_w, 1);
        ui.painter().text(
            Pos2::new(tile.max.x + 12.0, row.center().y),
            Align2::LEFT_CENTER,
            s,
            FontId::proportional(15.0),
            if selected {
                ON_ACCENT
            } else {
                ink().gamma_multiply(0.86)
            },
        );
    }
    if live {
        let pulse = 0.6 + 0.4 * (ui.input(|i| i.time) * 3.0).sin().abs() as f32;
        let dot = if compact {
            Pos2::new(tile.max.x - 3.0, tile.min.y + 3.0)
        } else {
            Pos2::new(row.max.x - 16.0, row.center().y)
        };
        ui.painter()
            .circle_filled(dot, 4.5, LIVE.gamma_multiply(pulse));
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    resp
}

impl App {
    /// Draws the sidebar: pages, playlists, groups and channels.
    pub(crate) fn sidebar(&mut self, ui: &mut egui::Ui, compact: bool) {
        let live_ids: HashSet<String> = self
            .feeds
            .iter()
            .filter(|(_, r)| {
                r.as_ref()
                    .is_ok_and(|v| v.iter().any(|x| x.kind == Kind::Live))
            })
            .map(|(k, _)| k.clone())
            .collect();

        ui.add_space(14.0);
        let mut toggle = false;
        // header: logo (+ name) and the collapse button
        let logo = self.logo.clone();
        let draw_logo = |ui: &mut egui::Ui| {
            ui.add(egui::Image::new((logo.id(), vec2(38.0, 38.0))));
        };
        let collapse_btn = |ui: &mut egui::Ui, icon: &str| -> bool {
            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
            let resp = resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(tr(if compact {
                    "Expand sidebar  (Ctrl+B)"
                } else {
                    "Collapse sidebar  (Ctrl+B)"
                }));
            let hv = ui
                .ctx()
                .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
            ui.painter()
                .rect_filled(r, CornerRadius::same(10), ov((10.0 + 22.0 * hv) as u8));
            icons::draw(
                ui.painter(),
                r.center(),
                icon,
                17.0,
                if hv > 0.5 { ink() } else { icons::muted_icon() },
            );
            resp.clicked()
        };
        if compact {
            ui.vertical_centered(|ui| {
                draw_logo(ui);
                ui.add_space(8.0);
                toggle |= collapse_btn(ui, "expand");
            });
        } else {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                draw_logo(ui);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("TubeGlass").size(20.0).strong());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    toggle |= collapse_btn(ui, "collapse");
                });
            });
        }
        ui.add_space(12.0);

        if !compact {
            // add channel
            let f = text_field(
                ui,
                &mut self.input,
                &tr("@handle or link"),
                ui.available_width(),
                40.0,
                "plus",
                Some(&tr("Add")),
                self.adding,
            );
            let submit = f.submit || f.action;
            if submit {
                self.add_channel();
            }
            if let Some(e) = &self.add_err {
                ui.colored_label(LIVE, egui::RichText::new(e).size(12.0));
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(tr("LIBRARY"))
                    .size(11.0)
                    .color(muted())
                    .strong(),
            );
        }

        let sel = self.sel.clone();
        let mut new_sel = None;
        let mut remove = None;
        let mut open_file = false;
        let active = self.jobs.len();
        if sidebar_row(
            ui,
            &tr("Home"),
            None,
            "home",
            sel == Sel::Home,
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Home);
        }
        if sidebar_row(
            ui,
            &tr("Live now"),
            None,
            "live",
            sel == Sel::Live,
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Live);
        }
        if sidebar_row(
            ui,
            &tr("Music"),
            None,
            "note",
            matches!(sel, Sel::Music | Sel::MusicAlbum(_) | Sel::MusicArtist(_)),
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Music);
        }
        if sidebar_row(
            ui,
            &tr("Podcasts"),
            None,
            "mic",
            matches!(sel, Sel::Podcasts | Sel::Podcast(_)),
            self.pods.iter().any(|p| self.pod_new_count(p) > 0),
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Podcasts);
        }
        let dl_label = if active > 0 {
            let avg = self.jobs.iter().map(|j| j.progress).sum::<f32>() / active as f32;
            trf("Downloads  ({active} • {percent}%)", &[("active", &active.to_string()), ("percent", &((avg * 100.0) as u32).to_string())])
        } else {
            tr("Downloads")
        };
        if sidebar_row(
            ui,
            &tr("History"),
            None,
            "history",
            sel == Sel::History,
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::History);
        }
        if sidebar_row(
            ui,
            &tr("Insights"),
            None,
            "chart",
            sel == Sel::Stats,
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Stats);
        }
        if sidebar_row(
            ui,
            &dl_label,
            None,
            "download",
            sel == Sel::Downloads,
            active > 0,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Downloads);
        }
        if sidebar_row(
            ui,
            &tr("Clips"),
            None,
            "scissors",
            sel == Sel::Clips,
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Clips);
        }
        if sidebar_row(ui, &tr("Open file…"), None, "open", false, false, compact).clicked() {
            open_file = true;
        }
        if sidebar_row(
            ui,
            &tr("Settings"),
            None,
            "settings",
            sel == Sel::Settings,
            false,
            compact,
        )
        .clicked()
        {
            new_sel = Some(Sel::Settings);
        }
        ui.add_space(4.0);
        let mut new_playlist = false;
        let mut import_playlists = false;
        let mut new_group_clicked = false;
        let mut delete_group: Option<String> = None;
        let mut group_toggle: Option<(String, String)> = None;
        let mut chan_notify: Option<(String, u8)> = None;
        let mut new_group_for: Option<String> = None;
        let mut delete_list: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if compact {
                ui.separator();
            } else {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("PLAYLISTS")).size(11.0).color(muted()).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        let resp = resp
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(tr("New playlist"));
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        ui.painter().rect_filled(r, CornerRadius::same(8), ov((10.0 + 25.0 * hv) as u8));
                        icons::draw(ui.painter(), r.center(), "plus", 13.0, if hv > 0.5 { ink() } else { icons::muted_icon() });
                        new_playlist |= resp.clicked();
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        let resp = resp
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(tr("Import playlists from YouTube"));
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        ui.painter().rect_filled(r, CornerRadius::same(8), ov((10.0 + 25.0 * hv) as u8));
                        icons::draw(ui.painter(), r.center(), "download", 13.0, if hv > 0.5 { ink() } else { icons::muted_icon() });
                        import_playlists |= resp.clicked();
                    });
                });
            }
            for l in &self.lists {
                let is_sel = sel == Sel::Local(l.id.clone());
                let icon = match l.id.as_str() {
                    "favorites" | "liked" => "heart",
                    "watchlater" => "clock",
                    _ => "list",
                };
                let label = format!("{}  ({})", l.name, l.videos.len());
                let r = sidebar_row(ui, &label, None, icon, is_sel, false, compact);
                if r.clicked() {
                    new_sel = Some(Sel::Local(l.id.clone()));
                }
                if !is_builtin_list(&l.id) {
                    r.context_menu(|ui| {
                        if ui.button(tr("Delete playlist")).clicked() {
                            delete_list = Some(l.id.clone());
                            ui.close();
                        }
                    });
                }
            }
            ui.add_space(4.0);
            // groups of channels, each with its own feed
            if compact {
                if !self.groups.is_empty() {
                    ui.separator();
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("GROUPS")).size(11.0).color(muted()).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("New group"));
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        ui.painter().rect_filled(r, CornerRadius::same(8), ov((10.0 + 25.0 * hv) as u8));
                        icons::draw(ui.painter(), r.center(), "plus", 13.0, if hv > 0.5 { ink() } else { icons::muted_icon() });
                        new_group_clicked |= resp.clicked();
                    });
                });
                if self.groups.is_empty() {
                    ui.label(egui::RichText::new(tr("Group channels, e.g. Music or News. Right-click a channel to add it.")).size(12.0).color(muted()));
                }
            }
            for g in &self.groups {
                let is_sel = sel == Sel::Group(g.id.clone());
                let label = format!("{}  ({})", g.name, g.channels.len());
                let r = sidebar_row(ui, &label, None, "folder", is_sel, false, compact);
                if r.clicked() {
                    new_sel = Some(Sel::Group(g.id.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button(tr("Delete group")).clicked() {
                        delete_group = Some(g.id.clone());
                        ui.close();
                    }
                });
            }
            ui.add_space(4.0);
            if compact {
                ui.separator();
            } else {
                ui.label(egui::RichText::new(tr("CHANNELS")).size(11.0).color(muted()).strong());
                if !self.channels.is_empty() {
                    let w = ui.available_width();
                    text_field(ui, &mut self.chan_filter,&tr("Filter channels"), w, 36.0, "search", None, false);
                    ui.add_space(4.0);
                }
            }
            let needle = fold(self.chan_filter.trim());
            if !needle.is_empty() && !compact {
                let n = self.channels.iter().filter(|c| fold(&c.name).contains(&needle)).count();
                if n == 0 {
                    ui.label(egui::RichText::new(tr("No channels match.")).color(muted()).size(13.0));
                }
            }
            for c in self.channels.iter().filter(|c| needle.is_empty() || fold(&c.name).contains(&needle)) {
                let is_sel = sel == Sel::Channel(c.id.clone());
                let new_count = self
                    .feeds
                    .get(&c.id)
                    .and_then(|r| r.as_ref().ok())
                    .map_or(0, |v| v.iter().filter(|x| x.kind == Kind::Video && !self.seen.contains(&x.id)).count());
                let r = sidebar_row(ui, &c.name, Some(&c.avatar), "", is_sel, live_ids.contains(&c.id), compact);
                if new_count > 0 && !is_sel {
                    let label = if new_count > 9 { "9+".to_string() } else { new_count.to_string() };
                    let bx = if compact {
                        Rect::from_center_size(Pos2::new(r.rect.min.x + 10.0, r.rect.min.y + 10.0), vec2(18.0, 16.0))
                    } else {
                        Rect::from_center_size(Pos2::new(r.rect.max.x - 46.0, r.rect.center().y), vec2(24.0, 18.0))
                    };
                    ui.painter().rect_filled(bx, CornerRadius::same(9), accent());
                    ui.painter().text(bx.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), ON_ACCENT);
                }
                if self.loading.contains(&c.id) {
                    let at = if compact {
                        Pos2::new(r.rect.max.x - 10.0, r.rect.center().y + 12.0)
                    } else {
                        Pos2::new(r.rect.max.x - 38.0, r.rect.center().y)
                    };
                    ui.put(
                        Rect::from_center_size(at, vec2(14.0, 14.0)),
                        egui::Spinner::new().size(14.0).color(accent()),
                    );
                }
                if r.clicked() {
                    new_sel = Some(Sel::Channel(c.id.clone()));
                }
                r.context_menu(|ui| {
                    ui.menu_button(tr("Add to group"), |ui| {
                        for g in &self.groups {
                            let has = g.channels.contains(&c.id);
                            if ui.selectable_label(has, g.name.clone()).clicked() {
                                group_toggle = Some((g.id.clone(), c.id.clone()));
                                ui.close();
                            }
                        }
                        if !self.groups.is_empty() {
                            ui.separator();
                        }
                        if ui.button(tr("+  New group…")).clicked() {
                            new_group_for = Some(c.id.clone());
                            ui.close();
                        }
                    });
                    ui.menu_button(tr("Notifications"), |ui| {
                        let cur = self.settings.chan_prefs.get(&c.id).map_or(0, |p| p.notify);
                        for (v, label) in [(0u8, tr("Live streams only")), (1, tr("Live streams and new uploads")), (2, tr("Off for this channel"))] {
                            if ui.selectable_label(cur == v, label).clicked() {
                                chan_notify = Some((c.id.clone(), v));
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(tr("Remove channel")).clicked() {
                        remove = Some(c.id.clone());
                        ui.close();
                    }
                });
            }
            if self.channels.is_empty() && !compact {
                ui.label(egui::RichText::new(tr("Nothing here yet. Add a channel above to get started.")).color(muted()).size(13.0));
            }
        });
        if open_file {
            self.start_browser();
        }
        if new_playlist {
            self.new_list = Some((String::new(), None));
        }
        if import_playlists {
            self.import_list = Some((String::new(), 0));
        }
        if new_group_clicked {
            self.new_group = Some((String::new(), None));
        }
        if let Some(c) = new_group_for {
            self.new_group = Some((String::new(), Some(c)));
        }
        if let Some((cid, v)) = chan_notify {
            self.settings.chan_prefs.entry(cid).or_default().notify = v;
            self.save_settings();
            if !self.settings.notifications {
                self.toast(tr("Turn on notifications in Settings to receive them"));
            }
        }
        if let Some((gid, cid)) = group_toggle {
            if let Some(g) = self.groups.iter_mut().find(|g| g.id == gid) {
                if let Some(i) = g.channels.iter().position(|x| x == &cid) {
                    g.channels.remove(i);
                } else {
                    g.channels.push(cid);
                }
                save_groups(&self.groups);
            }
        }
        if let Some(id) = delete_group {
            self.groups.retain(|g| g.id != id);
            save_groups(&self.groups);
            if self.sel == Sel::Group(id) {
                self.sel = Sel::Home;
            }
        }
        if let Some(id) = delete_list {
            self.lists.retain(|l| l.id != id);
            if !self.lists.iter().any(|l| l.id == "favorites") {
                self.lists.insert(
                    0,
                    LocalList {
                        id: "favorites".into(),
                        name: "Favorites".into(),
                        videos: vec![],
                        offline: false,
                    },
                );
            }
            save_lists(&self.lists);
            if self.sel == Sel::Local(id) {
                self.sel = Sel::Home;
            }
        }
        if toggle {
            self.toggle_sidebar();
        }
        if let Some(s) = new_sel {
            // Navigating away from a playing video / song keeps it going in PiP.
            if self.watching.is_some() && !self.pip {
                self.leave_player_for_navigation();
            }
            if s == Sel::Music {
                self.music_active = None;
                self.music_q.clear();
                self.music_open_home();
            }
            if s == Sel::Live
                && (self.live.is_none()
                    || now_secs().saturating_sub(self.fcache.live_updated) > 180)
                && !self.live_loading
            {
                self.load_live();
            }
            if let Sel::Channel(cid) = &s {
                self.chan_tab = ChanTab::Videos;
                self.mark_channel_seen(cid);
            }
            self.sel = s;
            self.filter = Filter::All;
        }
        if let Some(id) = remove {
            self.remove_channel(&id);
        }
    }
}
