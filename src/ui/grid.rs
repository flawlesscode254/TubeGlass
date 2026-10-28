//! The video pages (Home, Search, History, playlists...): header, filters and the card grid.

use crate::*;

impl App {
    /// Draws a video page: header, filters, layout switch and the cards in the chosen layout.
    pub(crate) fn grid(&mut self, ui: &mut egui::Ui) {
        self.sync_lib_filter();
        let videos = self.visible_videos();
        let title = match &self.sel {
            Sel::Downloads => tr("Downloads"),
            Sel::Settings => tr("Settings"),
            Sel::Stats => tr("Insights"),
            Sel::Clips => tr("Clips"),
            Sel::Podcasts => tr("Podcasts"),
            Sel::Podcast(_) => tr("Podcast"),
            Sel::Music | Sel::MusicAlbum(_) | Sel::MusicArtist(_) => tr("Music"),
            Sel::Group(gid) => self.groups.iter().find(|g| &g.id == gid).map(|g| g.name.clone()).unwrap_or_default(),
            Sel::History => tr("History"),
            Sel::Local(id) => self
                .lists
                .iter()
                .find(|l| &l.id == id)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            Sel::Home => tr("Latest from your channels"),
            Sel::Search => trf("Results for “{query}”", &[("query", &self.search_query)]),
            Sel::Live => tr("Live now"),
            Sel::Playlist(_) => self
                .playlist
                .as_ref()
                .and_then(|p| p.1.as_ref())
                .and_then(|r| r.as_ref().ok())
                .map(|(t, _)| t.clone())
                .unwrap_or_else(|| tr("Playlist")),
            Sel::Channel(id) => self
                .channels
                .iter()
                .find(|c| &c.id == id)
                .map(|c| c.name.clone())
                .unwrap_or_default(),
        };
        let busy = match &self.sel {
            Sel::Downloads | Sel::Local(_) | Sel::History | Sel::Settings | Sel::Stats | Sel::Clips | Sel::Podcasts | Sel::Podcast(_) | Sel::Music | Sel::MusicAlbum(_) | Sel::MusicArtist(_) => false,
            Sel::Group(gid) => self
                .groups
                .iter()
                .find(|g| &g.id == gid)
                .is_some_and(|g| g.channels.iter().any(|c| self.loading.contains(c))),
            Sel::Home => !self.loading.is_empty(),
            Sel::Search => self.svideos.is_none() || self.schannels.is_none(),
            Sel::Live => self.live.is_none() || self.live_loading,
            Sel::Playlist(_) => self.playlist.as_ref().is_none_or(|p| p.1.is_none()),
            Sel::Channel(id) => {
                self.active_chan_search().is_some_and(|cs| cs.res.is_none())
                    || self.loading.contains(id)
                    || self.tab_loading.contains(&format!("{id}:{}", self.chan_tab.key()))
            }
        };
        if matches!(self.sel, Sel::Channel(_)) {
            self.ensure_tab(false);
        }

        ui.add_space(6.0);
        let mut do_search: bool;
        let mut save_as_local = false;
        let mut download_visible = false;
        let focused: bool;
        let mut go_back = false;
        let can_back = self.can_go_back();
        let fld = ui
            .horizontal(|ui| {
                if can_back {
                    let (r, resp) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::click());
                    let resp = resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(tr("Back  (Alt+←)"));
                    let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                    ui.painter().add(surface::capsule(r, 1.0, if hv > 0.05 { Some(accent()) } else { None }, false));
                    icons::draw(ui.painter(), r.center(), "arrow_left", 19.0, if hv > 0.5 { accent() } else { ink() });
                    if resp.clicked() {
                        go_back = true;
                    }
                    ui.add_space(4.0);
                }
                text_field(
                    ui,
                    &mut self.search_input, &tr("Search YouTube — videos and channels"),
                    ui.available_width(),
                    46.0,
                    "search",
                    Some(&tr("Search")),
                    false,
                )
            })
            .inner;
        if go_back {
            self.go_back();
        }
        focused = fld.resp.has_focus();
        if fld.resp.changed() {
            self.last_edit = Instant::now();
        }
        do_search = fld.submit || fld.action;
        let bar = fld.rect;
        {
            let ctxp = ui.ctx().clone();
            let q = self.search_input.trim().to_string();
            if focused && !q.is_empty() && q != self.sugg_req {
                if self.last_edit.elapsed() > Duration::from_millis(180) {
                    self.sugg_req = q.clone();
                    let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
                    std::thread::spawn(move || {
                        let list = yt::suggestions(&q).unwrap_or_default();
                        let _ = tx.send(Msg::Suggestions(q, list));
                        ctx.request_repaint();
                    });
                } else {
                    ctxp.request_repaint_after(Duration::from_millis(200));
                }
            }
            let over = self
                .sugg_rect
                .zip(ctxp.pointer_hover_pos())
                .is_some_and(|(r, p)| r.contains(p));
            let show_recent = self.search_input.trim().is_empty() && !self.recent.is_empty();
            let rows: Vec<String> = if show_recent {
                self.recent.iter().take(8).cloned().collect()
            } else {
                self.sugg.iter().take(8).cloned().collect()
            };
            if (focused || over) && !rows.is_empty() && (show_recent || !self.search_input.trim().is_empty()) {
                let pos = bar.left_bottom() + vec2(0.0, 4.0);
                let mut forget: Option<String> = None;
                let width = bar.width();
                let mut pick: Option<String> = None;
                let area = egui::Area::new(uid("sugg"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pos)
                    .show(&ctxp, |ui| {
                        egui::Frame::new()
                            .fill(panel())
                            .stroke(Stroke::new(1.0, ov(22)))
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(egui::Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(width - 12.0);
                                if show_recent {
                                    ui.label(egui::RichText::new(tr("RECENT SEARCHES")).size(11.0).color(muted()).strong());
                                }
                                for sg in &rows {
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(rect, CornerRadius::same(8), card_hi());
                                    }
                                    let icon_x = rect.min.x + 18.0;
                                    icons::draw(
                                        ui.painter(),
                                        Pos2::new(icon_x, rect.center().y),
                                        if show_recent { "history" } else { "search" },
                                        14.0,
                                        icons::muted_icon(),
                                    );
                                    ui.painter().text(Pos2::new(rect.min.x + 38.0, rect.center().y), Align2::LEFT_CENTER, sg, FontId::proportional(15.0), ink());
                                    let xr = Rect::from_center_size(Pos2::new(rect.max.x - 20.0, rect.center().y), vec2(24.0, 24.0));
                                    let on_x = show_recent
                                        && ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p))
                                        && resp.hovered();
                                    if show_recent && resp.hovered() {
                                        icons::draw(ui.painter(), xr.center(), "close", 12.0, if on_x { ink() } else { muted() });
                                    }
                                    if resp.clicked() {
                                        if on_x {
                                            forget = Some(sg.clone());
                                        } else {
                                            pick = Some(sg.clone());
                                        }
                                    }
                                }
                            });
                    });
                self.sugg_rect = Some(area.response.rect);
                if let Some(f) = forget {
                    self.recent.retain(|r| r != &f);
                    save_recent(&self.recent);
                }
                if let Some(p) = pick {
                    self.search_input = p;
                    do_search = true;
                }
            } else {
                self.sugg_rect = None;
            }
        }
        if do_search {
            self.start_search();
        }
        ui.add_space(8.0);
        if matches!(self.sel, Sel::Channel(_)) {
            self.channel_header(ui, busy);
        } else {
        // lists carry many buttons: the title gets a line of its own so nothing overlaps it
        let narrow = ui.available_width() < 980.0;
        let own_line = matches!(self.sel, Sel::Local(_) | Sel::Playlist(_) | Sel::History);
        if own_line {
            ui.label(egui::RichText::new(title.clone()).size(26.0).strong());
            ui.add_space(2.0);
        }
        ui.horizontal_wrapped(|ui| {
            if !own_line {
                ui.label(egui::RichText::new(title.clone()).size(26.0).strong());
            }
            if self.batch_total > 1 && matches!(self.sel, Sel::Home) {
                let done = self.batch_total.saturating_sub(self.batch.len());
                let frac = done as f32 / self.batch_total as f32;
                progress_pill(ui, frac, &trf("Refreshing  {done} / {total}", &[("done", &done.to_string()), ("total", &self.batch_total.to_string())]), 210.0);
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            } else if busy {
                ui.add(egui::Spinner::new().size(18.0));
            } else {
                let ts = match &self.sel {
                    Sel::Home => self.channels.iter().filter_map(|c| self.fcache.updated.get(&c.id)).min().copied(),
                    Sel::Channel(id) => self.fcache.updated.get(id).copied(),
                    Sel::Live => Some(self.fcache.live_updated).filter(|t| *t > 0),
                    _ => None,
                };
                if let Some(ts) = ts {
                    ui.label(egui::RichText::new(trf("Updated {time}", &[("time", &ago(ts))])).size(13.0).color(muted()));
                }
            }
            // on a narrow window the buttons drop to their own line, and wrap if they still do not fit
            if narrow {
                ui.end_row();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true), |ui| {
                if let Sel::Group(gid) = self.sel.clone() {
                    if icon_btn(ui, "",&tr("Delete group"), None, ink(), false, true).clicked() {
                        self.groups.retain(|g| g.id != gid);
                        save_groups(&self.groups);
                        self.sel = Sel::Home;
                    } else if icon_btn(ui, "refresh",&tr("Refresh"), None, ink(), false, true).clicked() {
                        let members: Vec<String> = self.groups.iter().find(|g| g.id == gid).map(|g| g.channels.clone()).unwrap_or_default();
                        for c in self.channels.clone().into_iter().filter(|c| members.contains(&c.id)) {
                            self.refresh(&c);
                        }
                    }
                }
                if matches!(self.sel, Sel::Playlist(_) | Sel::Local(_)) && !videos.is_empty() {
                    let sh = icon_btn(ui, "shuffle",&tr("Shuffle"), None, ink(), false, true);
                    if sh.clicked() {
                        self.play_all_visible(true);
                    }
                    if icon_btn(ui, "play",&tr("Play all"), Some(accent()), ON_ACCENT, false, true).clicked() {
                        self.play_all_visible(false);
                    }
                    let have = self.offline_count(&videos);
                    let label = if have == videos.len() { tr("Downloaded") } else { trf("Download all  ({downloaded}/{total})", &[("downloaded", &have.to_string()), ("total", &videos.len().to_string())]) };
                    if icon_btn(ui, if have < videos.len() { "download" } else { "check" }, &label, None, ink(), false, have < videos.len()).clicked() {
                        download_visible = true;
                    }
                    if let Sel::Local(id) = self.sel.clone() {
                        let on = self.lists.iter().any(|l| l.id == id && l.offline);
                        let label = tr(if on { "Kept offline" } else { "Keep offline" });
                        let b = icon_btn(ui, if on { "check" } else { "download" }, &label, Some(if on { accent() } else { card() }), if on { ON_ACCENT } else { ink() }, false, true);
                        if b.on_hover_text(tr("Keep every item of this playlist downloaded, including ones added later")).clicked() {
                            if let Some(l) = self.lists.iter_mut().find(|l| l.id == id) {
                                l.offline = !l.offline;
                                save_lists(&self.lists);
                            }
                            self.dl_gaveup.clear();
                            self.next_offline_sync = Instant::now();
                        }
                    }
                    // a YouTube playlist can be kept as an editable local one
                    if matches!(self.sel, Sel::Playlist(_)) && ui.add(egui::Button::new(tr("Save as playlist"))).clicked() {
                        save_as_local = true;
                    }
                }
                if self.sel == Sel::History {
                    if !self.history.is_empty() {
                        let label = tr(if self.hist_confirm { "Confirm clear" } else { "Clear history" });
                        let b = egui::Button::new(egui::RichText::new(label).color(if self.hist_confirm { Color32::WHITE } else { ink() }))
                            .fill(if self.hist_confirm { LIVE } else { card() });
                        if ui.add(b).clicked() {
                            if self.hist_confirm {
                                self.history.clear();
                                save_history(&self.history);
                                self.hist_confirm = false;
                            } else {
                                self.hist_confirm = true;
                            }
                        }
                    }
                } else if let Sel::Local(id) = self.sel.clone() {
                    if icon_btn(ui, "",&tr("Delete playlist"), None, ink(), false, true).clicked() {
                        self.lists.retain(|l| l.id != id);
                        if !self.lists.iter().any(|l| l.id == "favorites") {
                            self.lists.insert(0, LocalList { id: "favorites".into(), name: "Favorites".into(), videos: vec![], offline: false });
                        }
                        save_lists(&self.lists);
                        self.sel = Sel::Home;
                    }
                } else if self.sel == Sel::Home && {
                    let any_new = self
                        .feeds
                        .values()
                        .filter_map(|r| r.as_ref().ok())
                        .flatten()
                        .any(|v| !self.seen.contains(&v.id));
                    any_new && icon_btn(ui, "check",&tr("Mark all as seen"), None, ink(), false, true).clicked()
                } {
                    self.mark_all_seen();
                } else if !matches!(self.sel, Sel::Group(_)) && icon_btn(ui, "refresh",&tr("Refresh"), None, ink(), false, true).clicked() {
                    self.refresh_all();
                    if self.sel == Sel::Live {
                        self.load_live();
                    }
                }
                ui.add_space(8.0);
                for (f, label) in [(Filter::Videos, tr("Videos")), (Filter::Live, tr("Live")), (Filter::New, tr("New")), (Filter::All, tr("All"))] {
                    if own_line {
                        break;
                    }
                    let on = self.filter == f;
                    if icon_btn(ui, "", &label, Some(if on { accent() } else { card() }), if on { ON_ACCENT } else { muted() }, false, true).clicked() {
                        self.filter = f;
                    }
                }
            });
        });
        }
        ui.add_space(4.0);

        let has_items = match &self.sel {
            Sel::History => !self.history.is_empty(),
            Sel::Local(id) => self.lists.iter().any(|l| &l.id == id && !l.videos.is_empty()),
            _ => false,
        };
        if has_items {
            let w = ui.available_width().min(460.0);
            text_field(ui, &mut self.lib_filter,&tr("Search this list"), w, 38.0, "search", None, false);
            ui.add_space(6.0);
        }

        // how the videos are laid out (and, on Home, how they are ordered)
        let shows_videos = !videos.is_empty()
            && !matches!(self.sel, Sel::Downloads | Sel::Settings | Sel::Music | Sel::MusicAlbum(_) | Sel::MusicArtist(_))
            && !(matches!(self.sel, Sel::Channel(_)) && self.chan_tab == ChanTab::About);
        // sort and filter chips for a channel's videos and for playlists
        if self.has_list_tools() && (shows_videos || self.list_sort != 0 || self.list_date != 0 || self.list_len != 0) {
            let sorts = [tr("Default order"), tr("Newest"), tr("Oldest"), tr("Most viewed"), tr("Least viewed"), tr("Longest"), tr("Shortest"), tr("Title A–Z")];
            let dates = [tr("Any time"), tr("Today"), tr("This week"), tr("This month"), tr("This year")];
            let lens = [tr("Any length"), tr("Under 4 min"), tr("4–20 min"), tr("Over 20 min")];
            ui.horizontal_wrapped(|ui| {
                let chip = |ui: &mut egui::Ui, label: String, active: bool| -> egui::Response {
                    icon_btn(ui, "", &label, Some(if active { accent() } else { card() }), if active { ON_ACCENT } else { ink() }, true, true)
                };
                let r1 = chip(ui, trf("Sort: {value}", &[("value", &sorts[self.list_sort as usize % 8])]), self.list_sort != 0);
                egui::Popup::menu(&r1).show(|ui| {
                    for (i, n) in sorts.iter().enumerate() {
                        if ui.selectable_label(self.list_sort as usize == i, n).clicked() {
                            self.list_sort = i as u8;
                            ui.close();
                        }
                    }
                });
                let r2 = chip(ui, dates[self.list_date as usize % 5].clone(), self.list_date != 0);
                egui::Popup::menu(&r2).show(|ui| {
                    for (i, n) in dates.iter().enumerate() {
                        if ui.selectable_label(self.list_date as usize == i, n).clicked() {
                            self.list_date = i as u8;
                            ui.close();
                        }
                    }
                });
                let r3 = chip(ui, lens[self.list_len as usize % 4].clone(), self.list_len != 0);
                egui::Popup::menu(&r3).show(|ui| {
                    for (i, n) in lens.iter().enumerate() {
                        if ui.selectable_label(self.list_len as usize == i, n).clicked() {
                            self.list_len = i as u8;
                            ui.close();
                        }
                    }
                });
                if (self.list_sort != 0 || self.list_date != 0 || self.list_len != 0) && icon_btn(ui, "",&tr("Reset"), None, muted(), false, true).clicked() {
                    self.list_sort = 0;
                    self.list_date = 0;
                    self.list_len = 0;
                }
                if matches!(self.sel, Sel::Channel(_)) {
                    ui.label(egui::RichText::new(tr("applies to the videos loaded so far")).size(12.0).color(muted()));
                }
            });
            ui.add_space(4.0);
        }

        if shows_videos {
            let mut mode = self.settings.view_mode;
            let mut for_you = self.settings.home_for_you;
            ui.horizontal(|ui| {
                if self.sel == Sel::Home {
                    for (label, on_v) in [(tr("Latest"), false), (tr("For you"), true)] {
                        let on = for_you == on_v;
                        let b = icon_btn(ui, "", &label, Some(if on { accent() } else { card() }), if on { ON_ACCENT } else { ink() }, false, true);
                        if b.on_hover_text(tr(if on_v { "Ordered by what you watch and listen to" } else { "Newest first" })).clicked() {
                            for_you = on_v;
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    views::view_switch(ui, &mut mode);
                });
            });
            ui.add_space(4.0);
            if mode != self.settings.view_mode || for_you != self.settings.home_for_you {
                self.settings.view_mode = mode;
                self.settings.home_for_you = for_you;
                self.save_settings();
            }
        }

        if self.sel == Sel::Search {
            let mut changed = false;
            ui.horizontal(|ui| {
                let sort = [tr("Relevance"), tr("Rating"), tr("Upload date"), tr("View count")];
                let date = [(0u8, tr("Any time")), (2, tr("Today")), (3, tr("This week")), (4, tr("This month")), (5, tr("This year"))];
                let dur = [(0u8, tr("Any length")), (1, tr("Under 4 min")), (3, tr("4–20 min")), (2, tr("Over 20 min"))];
                let o = &mut self.search_opts;
                let chip = |ui: &mut egui::Ui, label: String, active: bool| -> egui::Response {
                    icon_btn(ui, "", &label, Some(if active { accent() } else { card() }), if active { ON_ACCENT } else { ink() }, true, true)
                };
                let r1 = chip(ui, trf("Sort: {value}", &[("value", &sort[o.sort as usize % 4])]), o.sort != 0);
                egui::Popup::menu(&r1).show(|ui| {
                    for (i, n) in sort.iter().enumerate() {
                        if ui.selectable_label(o.sort as usize == i, n).clicked() {
                            o.sort = i as u8;
                            changed = true;
                            ui.close();
                        }
                    }
                });
                let dl = date.iter().find(|d| d.0 == o.date).map_or_else(|| tr("Any time"), |d| d.1.clone());
                let r2 = chip(ui, dl, o.date != 0);
                egui::Popup::menu(&r2).show(|ui| {
                    for (v, n) in date {
                        if ui.selectable_label(o.date == v, n).clicked() {
                            o.date = v;
                            changed = true;
                            ui.close();
                        }
                    }
                });
                let ll = dur.iter().find(|d| d.0 == o.duration).map_or_else(|| tr("Any length"), |d| d.1.clone());
                let r3 = chip(ui, ll, o.duration != 0);
                egui::Popup::menu(&r3).show(|ui| {
                    for (v, n) in dur {
                        if ui.selectable_label(o.duration == v, n).clicked() {
                            o.duration = v;
                            changed = true;
                            ui.close();
                        }
                    }
                });
            });
            if changed {
                self.search_input = self.search_query.clone();
                self.start_search();
            }
            ui.add_space(4.0);
        }

        if let Sel::Channel(id) = &self.sel {
            if let Some(e) = self.tab_err.get(&format!("{id}:{}", self.chan_tab.key())) {
                ui.colored_label(LIVE, trf("Could not load this tab: {error}", &[("error", e)]));
            }
            if self.chan_tab == ChanTab::About {
                let about = self.fcache.about.get(id).cloned();
                self.about_ui(ui, about);
                return;
            }
        }
        if let Some((_, Some(Err(e)))) = &self.playlist {
            if matches!(self.sel, Sel::Playlist(_)) {
                ui.colored_label(LIVE, trf("Could not load playlist: {error}", &[("error", e)]));
            }
        }
        if let Some(Err(e)) = &self.live {
            if self.sel == Sel::Live {
                ui.colored_label(LIVE, trf("Could not load live streams: {error}", &[("error", e)]));
            }
        }
        if self.sel == Sel::Search {
            if let Some(Err(e)) = &self.svideos {
                ui.colored_label(LIVE, trf("Search failed: {error}", &[("error", e)]));
            }
        }
        let hits: Vec<ChannelHit> = if self.sel == Sel::Search && self.filter != Filter::Videos {
            self.schannels
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(|h| h.iter().take(5).cloned().collect())
                .unwrap_or_default()
        } else {
            vec![]
        };
        let chan_pl: Option<Vec<PlaylistHit>> = match &self.sel {
            Sel::Channel(_) if self.chan_tab == ChanTab::Playlists && self.active_chan_search().is_some() => Some(
                self.active_chan_search()
                    .and_then(|cs| cs.res.as_ref())
                    .and_then(|r| r.as_ref().ok())
                    .map(|(_, p)| p.clone())
                    .unwrap_or_default(),
            ),
            Sel::Channel(id) if self.chan_tab == ChanTab::Playlists => {
                Some(self.fcache.tab_playlists.get(id).cloned().unwrap_or_default())
            }
            _ => None,
        };
        let plhits: Vec<PlaylistHit> = if let Some(p) = chan_pl {
            p
        } else if self.sel == Sel::Search && self.filter != Filter::Videos {
            self.splaylists
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(|h| h.iter().take(4).cloned().collect())
                .unwrap_or_default()
        } else {
            vec![]
        };
        let videos = if self.sel_is_chan_playlists() { vec![] } else { videos };
        if videos.is_empty() && hits.is_empty() && plhits.is_empty() && !busy {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("◌").size(48.0).color(muted()));
                ui.label(egui::RichText::new(tr(if has_items && !self.lib_filter.trim().is_empty() {
                    "Nothing in this list matches your search."
                } else if self.sel == Sel::History {
                    "Nothing watched yet. Videos you open will show up here, and pick up where you left off."
                } else if matches!(self.sel, Sel::Local(_)) {
                    "This playlist is empty. Right-click any video and choose “Add to playlist”."
                } else if self.channels.is_empty() {
                    "Add your favourite channels to see their videos and live streams here."
                } else {
                    "No videos to show."
                })).size(16.0).color(muted()));
            });
            return;
        }

        let dens = [0.82f32, 1.0, 1.22][self.settings.density.min(2) as usize];
        let gap = 18.0 * dens;
        let avail = ui.available_width() - 6.0;
        let portrait = !videos.is_empty() && videos.iter().all(|v| v.short);
        let list_mode = self.settings.view_mode == 2 && !portrait;
        let base_card = match self.settings.view_mode {
            1 => 205.0,
            _ => 290.0,
        };
        let min_card = if portrait { 190.0 } else { base_card } * dens;
        let cols = if list_mode { 1 } else { ((avail + gap) / (min_card + gap)).floor().max(1.0) as usize };
        let cw = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        let th = if portrait { (cw * 16.0 / 9.0).min(420.0) } else { cw * 9.0 / 16.0 };
        let ch = if list_mode { 104.0 * dens.max(0.9) } else { th + 74.0 };
        let rows = videos.len().div_ceil(cols);
        let mut clicked = None;
        let more_key = match &self.sel {
            Sel::Channel(id) if self.chan_tab != ChanTab::About => Some(format!("{id}:{}", self.chan_tab.key())),
            _ => None,
        };
        let has_more = self.active_chan_search().is_none()
            && more_key.as_ref().is_some_and(|k| self.fcache.tab_tokens.contains_key(k));
        let more_busy = more_key.as_ref().is_some_and(|k| self.more_loading.contains(k));
        let more_failed = more_key.as_ref().is_some_and(|k| self.tab_err.contains_key(k));
        let mut want_more = false;
        let watched: HashMap<String, f32> = self
            .history
            .iter()
            .filter(|h| h.duration > 0.0 && h.position > 5.0 && h.position < h.duration - 8.0)
            .map(|h| (h.video.id.clone(), (h.position / h.duration) as f32))
            .collect();
        let in_history_view = self.sel == Sel::History;
        let job_prog: HashMap<String, f32> = self.jobs.iter().map(|j| (j.vid.clone(), j.progress)).collect();
        let known_v: HashSet<String> =
            self.library.iter().filter(|d| !d.audio_only).map(|d| d.id.clone()).collect();
        let known_a: HashSet<String> =
            self.library.iter().filter(|d| d.audio_only).map(|d| d.id.clone()).collect();
        let lists_snapshot: Vec<LocalList> = self
            .lists
            .iter()
            .map(|l| LocalList { id: l.id.clone(), name: l.name.clone(), videos: l.videos.iter().map(|v| Video { id: v.id.clone(), title: String::new(), thumb: String::new(), duration: String::new(), meta: String::new(), kind: Kind::Video, channel_id: String::new(), channel_name: String::new(), rank: 0, short: false, music: false }).collect(), offline: l.offline })
            .collect();
        let current_list: Option<String> = match &self.sel {
            Sel::Local(id) => Some(id.clone()),
            _ => None,
        };
        let mut card_act: Option<(Video, CardAction)> = None;
        let mut open_pl: Option<String> = None;
        let mut prefetch: Option<String> = None;
        let mut add_hit: Option<Channel> = None;
        let mut open_hit: Option<Channel> = None;
        let known: HashSet<String> = self.channels.iter().map(|c| c.id.clone()).collect();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, view| {
                let origin = ui.cursor().min.y;
                if !hits.is_empty() {
                    ui.label(egui::RichText::new(tr("CHANNELS")).size(11.0).color(muted()).strong());
                    for h in &hits {
                        let (rect, resp) = ui.allocate_exact_size(vec2(avail, 68.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        let p = ui.painter();
                        p.add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
                        let c = Pos2::new(rect.min.x + 14.0 + 22.0, rect.center().y);
                        p.circle_filled(c, 22.0, card_hi());
                        let uri = img_uri(ui.ctx(), &h.channel.avatar);
                        if !uri.is_empty() {
                            egui::Image::new(uri)
                                .corner_radius(CornerRadius::same(22))
                                .paint_at(ui, Rect::from_center_size(c, vec2(44.0, 44.0)));
                        }
                        let tx = rect.min.x + 14.0 + 44.0 + 14.0;
                        let sub = [h.handle.as_str(), h.subs.as_str()]
                            .iter()
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("  •  ");
                        let name = truncate(ui, &h.channel.name, FontId::proportional(16.0), avail - 240.0, 1);
                        let sub = truncate(ui, &sub, FontId::proportional(13.0), avail - 240.0, 1);
                        ui.painter().text(Pos2::new(tx, rect.center().y - 10.0), Align2::LEFT_CENTER, name, FontId::proportional(16.0), ink());
                        ui.painter().text(Pos2::new(tx, rect.center().y + 11.0), Align2::LEFT_CENTER, sub, FontId::proportional(13.0), muted());
                        let br = Rect::from_center_size(Pos2::new(rect.max.x - 62.0, rect.center().y), vec2(96.0, 34.0));
                        let added = known.contains(&h.channel.id);
                        let bresp = ui.interact(br, resp.id.with("add"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let fill = if added { card_hi() } else if bresp.hovered() { accent2() } else { accent() };
                        ui.painter().rect_filled(br, CornerRadius::same(17), fill);
                        icon_label(ui, br.center(), if added { "check" } else { "plus" }, &tr(if added { "Added" } else { "Add" }), if added { ink() } else { ON_ACCENT }, false);
                        if bresp.clicked() && !added {
                            add_hit = Some(h.channel.clone());
                        } else if resp.clicked() && !bresp.hovered() {
                            open_hit = Some(h.channel.clone());
                        }
                    }
                    ui.add_space(14.0);
                }
                if !plhits.is_empty() {
                    ui.label(egui::RichText::new(tr("PLAYLISTS")).size(11.0).color(muted()).strong());
                    for h in &plhits {
                        let (rect, resp) = ui.allocate_exact_size(vec2(avail, 76.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
                        let th_r = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(96.0, 60.0));
                        ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
                        let uri = img_uri(ui.ctx(), &h.thumb);
                        if !uri.is_empty() {
                            egui::Image::new(uri)
                                .fit_to_exact_size(th_r.size())
                                .corner_radius(CornerRadius::same(10))
                                .paint_at(ui, th_r);
                        }
                        let tx = th_r.max.x + 14.0;
                        let title = truncate(ui, &h.title, FontId::proportional(16.0), avail - 150.0, 1);
                        let sub = [h.owner.as_str(), h.count.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join("  •  ");
                        ui.painter().text(Pos2::new(tx, rect.center().y - 10.0), Align2::LEFT_CENTER, title, FontId::proportional(16.0), ink());
                        ui.painter().text(Pos2::new(tx, rect.center().y + 12.0), Align2::LEFT_CENTER, sub, FontId::proportional(13.0), muted());
                        if resp.clicked() {
                            open_pl = Some(h.id.clone());
                        }
                    }
                    ui.add_space(14.0);
                }
                if (!hits.is_empty() || !plhits.is_empty()) && !videos.is_empty() {
                    ui.label(egui::RichText::new(tr("VIDEOS")).size(11.0).color(muted()).strong());
                }
                let total = rows as f32 * (ch + gap);
                let (area, _) = ui.allocate_exact_size(vec2(avail, total), Sense::hover());
                let off = area.min.y - origin;
                for (i, v) in videos.iter().enumerate() {
                    let (row, col) = (i / cols, i % cols);
                    let y = row as f32 * (ch + gap);
                    if off + y + ch < view.min.y - 40.0 || off + y > view.max.y + 40.0 {
                        continue;
                    }
                    let rect = Rect::from_min_size(
                        area.min + vec2(col as f32 * (cw + gap), y),
                        vec2(cw, ch),
                    );
                    let info = CardInfo {
                        is_new: !v.short && !self.seen.contains(&v.id) && matches!(self.sel, Sel::Home | Sel::Channel(_) | Sel::Group(_)),
                        watched: watched.get(&v.id).copied(),
                        in_history_view,
                        progress: job_prog.get(&v.id).copied(),
                        video_saved: known_v.contains(&v.id),
                        audio_saved: known_a.contains(&v.id),
                        lists: &lists_snapshot,
                        current_list: current_list.as_deref(),
                        row: list_mode,
                    };
                    let (c, hov, act) = video_card(ui, rect, th, v, &info);
                    if c {
                        clicked = Some(v.clone());
                    }
                    if let Some(a) = act {
                        card_act = Some((v.clone(), a));
                    }
                    if hov {
                        prefetch = Some(v.id.clone());
                    }
                }
                if has_more {
                    let (r, _) = ui.allocate_exact_size(vec2(avail, 64.0), Sense::hover());
                    if more_busy {
                        ui.put(Rect::from_center_size(r.center(), vec2(28.0, 28.0)), egui::Spinner::new().color(accent()));
                    } else {
                        let b = egui::Button::new(egui::RichText::new(tr(if more_failed { "Retry" } else { "Load more" })).strong())
                            .fill(card_hi())
                            .corner_radius(CornerRadius::same(18))
                            .min_size(vec2(140.0, 36.0));
                        if ui.put(Rect::from_center_size(r.center(), vec2(140.0, 36.0)), b).clicked() {
                            want_more = true;
                        }
                        // infinite scroll: fetch as soon as the footer scrolls into view
                        if !more_failed && ui.is_rect_visible(r) {
                            want_more = true;
                        }
                    }
                }
            });
        if let Some((v, a)) = card_act {
            self.apply_card_action(v, a);
        }
        if want_more {
            if more_failed {
                if let Some(k) = &more_key {
                    self.tab_err.remove(k);
                }
            }
            self.load_more_tab();
        }
        if let Some(id) = prefetch {
            self.fetch_streams(&id);
        }
        if let Some(id) = open_pl {
            self.open_playlist(id);
        }
        if let Some(c) = add_hit {
            self.add_known_channel(c);
        }
        if let Some(c) = open_hit {
            self.chan_tab = ChanTab::Videos;
            self.sel = Sel::Channel(c.id.clone());
            self.add_known_channel(c);
        }
        if download_visible {
            let group = match &self.sel {
                Sel::Local(id) => self.lists.iter().find(|l| &l.id == id).map(|l| l.name.clone()).unwrap_or_default(),
                _ => title.clone(),
            };
            self.download_all(&videos, &group);
        }
        if save_as_local {
            let name = match &self.sel {
                Sel::Playlist(_) => self
                    .playlist
                    .as_ref()
                    .and_then(|p| p.1.as_ref())
                    .and_then(|r| r.as_ref().ok())
                    .map(|(t, _)| t.clone())
                    .unwrap_or_else(|| tr("Playlist")),
                _ => tr("Playlist"),
            };
            self.save_playlist_locally(name, videos.clone());
        }
        if let Some(v) = clicked {
            let list_ctx = matches!(self.sel, Sel::Playlist(_) | Sel::Local(_) | Sel::History)
                || (matches!(self.sel, Sel::Channel(_)) && self.chan_tab != ChanTab::About);
            if list_ctx {
                let items: Vec<QItem> = videos
                    .iter()
                    .filter(|x| x.kind != Kind::Upcoming)
                    .map(|x| QItem::Yt(x.clone()))
                    .collect();
                let start = items.iter().position(|q| q.key() == v.id).unwrap_or(0);
                self.set_queue(items, start);
                self.play_queue_at(self.queue_idx.unwrap_or(0));
            } else {
                self.queue.clear();
                self.queue_idx = None;
                self.play(v);
            }
        }
    }
}
