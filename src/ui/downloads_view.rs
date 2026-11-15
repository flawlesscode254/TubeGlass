//! The Downloads page: progress, library, groups, selection and sorting.

use crate::*;

impl App {
    /// Draws the Downloads page: running downloads and the library in the chosen layout.
    pub(crate) fn downloads_view(&mut self, ui: &mut egui::Ui) {
        self.sync_lib_filter();
        let needle = fold(self.lib_filter.trim());
        let mut shown: Vec<DlItem> = self
            .library
            .iter()
            .filter(|d| {
                needle.is_empty()
                    || fold(&d.title).contains(&needle)
                    || fold(&d.channel).contains(&needle)
                    || fold(&d.group).contains(&needle)
            })
            .cloned()
            .collect();
        let size_of = |d: &DlItem| std::fs::metadata(&d.file).map(|m| m.len()).unwrap_or(0);
        match self.dl_sort {
            1 => shown.sort_by_key(|d| std::cmp::Reverse(size_of(d))),
            2 => shown.reverse(),
            3 => shown.sort_by_key(|d| fold(&d.title)),
            _ => {}
        }
        ui.add_space(8.0);
        let narrow = ui.available_width() < 900.0;
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(tr("Downloads")).size(26.0).strong());
            ui.label(
                egui::RichText::new(download::dir().display().to_string())
                    .size(12.0)
                    .color(muted()),
            );
            if narrow {
                ui.end_row();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true), |ui| {
                if ui.add(egui::Button::new(tr("Open folder"))).clicked() {
                    let _ = std::fs::create_dir_all(download::dir());
                    crate::platform::open_path(&download::dir());
                }
                if !self.library.is_empty() {
                    if self.dl_sel.is_some() {
                        let n = self.dl_sel.as_ref().map_or(0, |s| s.len());
                        if ui.add(egui::Button::new(tr("Done"))).clicked() {
                            self.dl_sel = None;
                            self.dl_confirm = false;
                        }
                        let count = n.to_string();
                        let label = if self.dl_confirm {
                            trf("Confirm delete ({count})", &[("count", &count)])
                        } else {
                            trf("Delete ({count})", &[("count", &count)])
                        };
                        let b = egui::Button::new(egui::RichText::new(label).color(if n > 0 { Color32::WHITE } else { muted() }))
                            .fill(if n > 0 { LIVE } else { card() });
                        if ui.add_enabled(n > 0, b).clicked() {
                            if self.dl_confirm {
                                let chosen = self.dl_sel.take().unwrap_or_default();
                                for d in self.library.clone() {
                                    if chosen.contains(&d.file) {
                                        download::delete(&d);
                                    }
                                }
                                self.library = download::list();
                                self.dl_confirm = false;
                            } else {
                                self.dl_confirm = true;
                            }
                        }
                        let all = !shown.is_empty() && self.dl_sel.as_ref().is_some_and(|s| shown.iter().all(|d| s.contains(&d.file)));
                        if ui.add(egui::Button::new(tr(if all { "Select none" } else { "Select all" }))).clicked() {
                            self.dl_confirm = false;
                            if all {
                                self.dl_sel = Some(HashSet::new());
                            } else {
                                self.dl_sel = Some(shown.iter().map(|d| d.file.clone()).collect());
                            }
                        }
                    } else if ui.add(egui::Button::new(tr("Select"))).clicked() {
                        self.dl_sel = Some(HashSet::new());
                    }
                }
            });
        });
        if let Some(e) = &self.dl_err {
            ui.colored_label(LIVE, trf("Download failed: {error}", &[("error", e)]));
        }
        if !self.dl_errs.is_empty() {
            ui.horizontal(|ui| {
                let count = self.dl_errs.len().to_string();
                let summary = if self.dl_errs.len() == 1 {
                    trf("{count} download failed:", &[("count", &count)])
                } else {
                    trf("{count} downloads failed:", &[("count", &count)])
                };
                ui.colored_label(LIVE, summary);
                if ui.small_button(tr("Dismiss")).clicked() {
                    self.dl_errs.clear();
                }
            });
            for e in &self.dl_errs {
                ui.label(egui::RichText::new(e).size(13.0).color(muted()));
            }
        }
        ui.add_space(8.0);
        if !self.library.is_empty() {
            let w = ui.available_width();
            text_field(ui, &mut self.lib_filter, &tr("Search downloads"), w, 38.0, "search", None, false);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (label, v) in [(tr("Newest"), 0u8), (tr("Largest"), 1), (tr("Oldest"), 2), (tr("A–Z"), 3)] {
                    let on = self.dl_sort == v;
                    if icon_btn(ui, "", &label, Some(if on { accent() } else { card() }), if on { ON_ACCENT } else { ink() }, false, true).clicked() {
                        self.dl_sort = v;
                    }
                }
                let total: u64 = self.library.iter().map(|d| std::fs::metadata(&d.file).map(|m| m.len()).unwrap_or(0)).sum();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.view_toggle(ui);
                    ui.label(
                        egui::RichText::new(trf("{count} items  •  {used}  •  {free} free", &[("count", &self.library.len().to_string()), ("used", &fmt_size(total)), ("free", &fmt_size(free_space(&download::dir()))) ]))
                            .size(12.5)
                            .color(muted()),
                    );
                });
            });
            ui.add_space(8.0);
        }

        let mut play: Option<DlItem> = None;
        let mut del: Option<DlItem> = None;
        let mut cancel: Option<String> = None;
        let mut toggle_pause: Option<String> = None;
        let mut toggle_sel: Option<String> = None;
        let mut toggle_open: Option<String> = None;
        let mut play_group: Option<Vec<DlItem>> = None;
        let mut sel_group: Option<Vec<String>> = None;
        if self.jobs.is_empty() && self.library.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                icons::draw(ui.painter(), r.center(), "download", 48.0, muted());
                ui.label(
                    egui::RichText::new(tr("Nothing downloaded yet. Use the Download button in the player, or right-click any video."))
                        .size(16.0)
                        .color(muted()),
                );
            });
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width() - 6.0;
            // in progress
            for j in &self.jobs {
                let is_paused = j.pause.load(std::sync::atomic::Ordering::Relaxed);
                let (rect, _) = ui.allocate_exact_size(vec2(w, 84.0), Sense::hover());
                ui.painter().add(surface::panel(rect, 20.0, 0.9, None, true));
                let th_r = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(120.0, 72.0));
                ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
                let uri = img_uri(ui.ctx(), &j.thumb);
                if !uri.is_empty() {
                    egui::Image::new(uri)
                        .fit_to_exact_size(th_r.size())
                        .corner_radius(CornerRadius::same(10))
                        .paint_at(ui, th_r);
                }
                let tx = th_r.max.x + 14.0;
                let title = truncate(ui, &j.title, FontId::proportional(16.0), w - 320.0, 1);
                ui.painter().text(Pos2::new(tx, rect.min.y + 24.0), Align2::LEFT_CENTER, title, FontId::proportional(16.0), ink());
                let bar = Rect::from_min_size(Pos2::new(tx, rect.min.y + 44.0), vec2(w - (tx - rect.min.x) - 162.0, 8.0));
                ui.painter().rect_filled(bar, CornerRadius::same(4), ov(25));
                let fill = Rect::from_min_size(bar.min, vec2(bar.width() * j.progress.max(0.02), 8.0));
                if is_paused {
                    ui.painter().rect_filled(fill, CornerRadius::same(4), muted());
                } else {
                    hgradient(&ui.painter().with_clip_rect(fill), bar, accent(), accent2());
                }
                let secs = j.started.elapsed().saturating_sub(j.idle).as_secs_f64().max(0.5);
                let mb = j.bytes as f64 / 1_048_576.0;
                let speed = mb / secs;
                let eta = if j.progress > 0.02 { secs * (1.0 - j.progress as f64) / j.progress as f64 } else { 0.0 };
                let mut line = trf("{percent}%  •  {size} MB", &[("percent", &((j.progress * 100.0) as u32).to_string()), ("size", &format!("{mb:.1}"))]);
                if is_paused {
                    line += &trf("  •  {status}", &[("status", &tr("Paused"))]);
                } else {
                    if speed > 0.0 {
                        line += &trf("  •  {speed} MB/s", &[("speed", &format!("{speed:.1}"))]);
                    }
                    if eta > 0.0 {
                        line += &trf("  •  about {time} left", &[("time", &fmt_time(eta))]);
                    }
                }
                ui.painter().text(Pos2::new(tx, rect.min.y + 64.0), Align2::LEFT_CENTER, line, FontId::proportional(13.0), muted());
                ui.ctx().request_repaint_after(Duration::from_millis(500));
                let br = Rect::from_center_size(Pos2::new(rect.max.x - 56.0, rect.center().y), vec2(88.0, 32.0));
                let resp = ui.interact(br, uid(("dlc", &j.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(br, CornerRadius::same(16), if resp.hovered() { LIVE } else { card_hi() });
                ui.painter().text(br.center(), Align2::CENTER_CENTER, tr("Cancel"), FontId::proportional(14.0), Color32::WHITE);
                if resp.clicked() {
                    cancel = Some(j.id.clone());
                }
                // pause / resume
                let pr = Rect::from_center_size(Pos2::new(br.min.x - 28.0, rect.center().y), vec2(36.0, 32.0));
                let presp = ui.interact(pr, uid(("dlp", &j.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr(if is_paused { "Resume" } else { "Pause" }));
                ui.painter().rect_filled(pr, CornerRadius::same(16), if is_paused { accent() } else if presp.hovered() { card_hi() } else { card() });
                icons::draw(ui.painter(), pr.center(), if is_paused { "play" } else { "pause" }, 15.0, if is_paused { ON_ACCENT } else { ink() });
                if presp.clicked() {
                    toggle_pause = Some(j.id.clone());
                }
                ui.add_space(8.0);
            }
            // finished: albums / playlists collapse into one folder row
            enum Row<'a> {
                Head(String, Vec<&'a DlItem>, bool),
                Item(&'a DlItem, f32),
                /// a collapsed album / playlist shown as one card in the grid views
                GroupCard(String, Vec<&'a DlItem>),
            }
            let grid_mode = self.settings.view_mode != 2;
            let metrics = views::metrics(&self.settings, w, false);
            let mut cells: Vec<Row> = vec![];
            let mut rows: Vec<Row> = vec![];
            let mut seen_groups = HashSet::new();
            for d in &shown {
                if d.group.is_empty() {
                    rows.push(Row::Item(d, 0.0));
                } else if seen_groups.insert(d.group.clone()) {
                    // oldest first, which is the order the tracks were queued in
                    let mut members: Vec<&DlItem> = shown.iter().filter(|x| x.group == d.group).collect();
                    members.reverse();
                    let open = !needle.is_empty() || self.dl_open.contains(&d.group);
                    if open {
                        rows.push(Row::Head(d.group.clone(), members.clone(), true));
                        rows.extend(members.into_iter().map(|m| Row::Item(m, if grid_mode { 0.0 } else { 28.0 })));
                    } else if grid_mode {
                        rows.push(Row::GroupCard(d.group.clone(), members));
                    } else {
                        rows.push(Row::Head(d.group.clone(), members, false));
                    }
                }
            }
            if rows.is_empty() && !self.library.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(tr("No downloads match your search.")).size(16.0).color(muted()));
                });
            }
            macro_rules! flush_cards {
                () => {
                    for line in cells.chunks(metrics.cols) {
                        let m = &metrics;
                        let (area, _) = ui.allocate_exact_size(vec2(w, m.ch), Sense::hover());
                        for (k, cell) in line.iter().enumerate() {
                            let rect = Rect::from_min_size(area.min + vec2(k as f32 * (m.cw + m.gap), 0.0), vec2(m.cw, m.ch));
                            let selecting = self.dl_sel.is_some();
                            match cell {
                                Row::Item(d, _) => {
                                    let resp = ui.interact(rect, ui.scope_id().with(("dlcard", &d.file)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                                    let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                                    let uri = if std::path::Path::new(&d.thumb).exists() { crate::platform::file_uri(&d.thumb) } else { String::new() };
                                    let size = std::fs::metadata(&d.file).map(|x| x.len()).unwrap_or(0) as f64 / 1_048_576.0;
                                    let sub = if d.audio_only {
                                        trf("{channel}  •  {kind}  •  {size} MB", &[("channel", &d.channel), ("kind", &tr("Audio")), ("size", &format!("{size:.0}"))])
                                    } else {
                                        trf("{channel}  •  {quality}p  •  {size} MB", &[("channel", &d.channel), ("quality", &d.height.to_string()), ("size", &format!("{size:.0}"))])
                                    };
                                    let img = views::media_card(ui, rect, m, &uri, &d.title, &sub, hv);
                                    views::pill_label(ui, img.right_bottom(), &fmt_time(d.duration));
                                    let selected = self.dl_sel.as_ref().is_some_and(|s| s.contains(&d.file));
                                    let xr = Rect::from_center_size(img.right_top() + vec2(-20.0, 20.0), vec2(30.0, 30.0));
                                    if selecting {
                                        let cb = Rect::from_min_size(img.min + vec2(7.0, 7.0), vec2(22.0, 22.0));
                                        ui.painter().rect_filled(cb, CornerRadius::same(7), if selected { accent() } else { Color32::from_black_alpha(170) });
                                        ui.painter().rect_stroke(cb, CornerRadius::same(7), Stroke::new(1.5, if selected { accent() } else { ov(150) }), StrokeKind::Inside);
                                        if selected {
                                            icons::draw(ui.painter(), cb.center(), "check", 14.0, ON_ACCENT);
                                            ui.painter().rect_stroke(rect, CornerRadius::same(18), Stroke::new(1.5, accent()), StrokeKind::Inside);
                                        }
                                        if resp.clicked() {
                                            toggle_sel = Some(d.file.clone());
                                        }
                                    } else {
                                        let over_x = ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p)) && resp.hovered();
                                        if hv > 0.05 {
                                            let c = img.center();
                                            ui.painter().circle_filled(c, 22.0 * (0.7 + 0.3 * hv), Color32::from_black_alpha((150.0 * hv) as u8));
                                            ui.painter().add(egui::Shape::convex_polygon(
                                                vec![c + vec2(-5.0, -9.0), c + vec2(-5.0, 9.0), c + vec2(9.0, 0.0)],
                                                Color32::WHITE.gamma_multiply(hv),
                                                Stroke::NONE,
                                            ));
                                            ui.painter().circle_filled(xr.center(), 14.0, if over_x { LIVE.gamma_multiply(0.9) } else { Color32::from_black_alpha((150.0 * hv) as u8) });
                                            icons::draw(ui.painter(), xr.center(), "close", 13.0, Color32::WHITE.gamma_multiply(hv));
                                        }
                                        if resp.clicked() {
                                            if resp.interact_pointer_pos().is_some_and(|p| xr.contains(p)) {
                                                del = Some((*d).clone());
                                            } else {
                                                play = Some((*d).clone());
                                            }
                                        }
                                    }
                                }
                                Row::GroupCard(name, members) => {
                                    let resp = ui.interact(rect, ui.scope_id().with(("dlgroup", name)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                                    let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                                    let uri = members.iter().find(|x| std::path::Path::new(&x.thumb).exists()).map(|x| crate::platform::file_uri(&x.thumb)).unwrap_or_default();
                                    let mb: f64 = members.iter().map(|x| std::fs::metadata(&x.file).map(|f| f.len()).unwrap_or(0) as f64).sum::<f64>() / 1_048_576.0;
                                    let count = members.len().to_string();
                                    let sub = if members.len() == 1 {
                                        trf("{count} item  •  {size} MB", &[("count", &count), ("size", &format!("{mb:.0}"))])
                                    } else {
                                        trf("{count} items  •  {size} MB", &[("count", &count), ("size", &format!("{mb:.0}"))])
                                    };
                                    let img = views::media_card(ui, rect, m, &uri, name, &sub, hv);
                                    // a folder badge with the track count
                                    let b = Rect::from_min_size(img.min + vec2(7.0, 7.0), vec2(48.0, 22.0));
                                    ui.painter().rect_filled(b, CornerRadius::same(7), Color32::from_black_alpha(190));
                                    icons::draw(ui.painter(), b.min + vec2(14.0, 11.0), "folder", 13.0, Color32::WHITE);
                                    ui.painter().text(b.min + vec2(26.0, 11.0), Align2::LEFT_CENTER, members.len().to_string(), FontId::proportional(12.0), Color32::WHITE);
                                    let pr = Rect::from_center_size(img.left_bottom() + vec2(26.0, -26.0), vec2(34.0, 34.0));
                                    let over_p = ui.ctx().pointer_hover_pos().is_some_and(|p| pr.contains(p)) && resp.hovered();
                                    let all_sel = self.dl_sel.as_ref().is_some_and(|s| members.iter().all(|x| s.contains(&x.file)));
                                    if selecting {
                                        if all_sel {
                                            ui.painter().rect_stroke(rect, CornerRadius::same(18), Stroke::new(1.5, accent()), StrokeKind::Inside);
                                            icons::draw(ui.painter(), img.right_top() + vec2(-20.0, 20.0), "check", 16.0, accent());
                                        }
                                    } else {
                                        ui.painter().circle_filled(pr.center(), 17.0, if over_p { accent() } else { Color32::from_black_alpha(170) });
                                        icons::draw(ui.painter(), pr.center(), "play", 14.0, if over_p { ON_ACCENT } else { Color32::WHITE });
                                    }
                                    if resp.clicked() {
                                        let on_play = resp.interact_pointer_pos().is_some_and(|p| pr.contains(p));
                                        if selecting {
                                            sel_group = Some(members.iter().map(|x| x.file.clone()).collect());
                                        } else if on_play {
                                            play_group = Some(members.iter().map(|x| (*x).clone()).collect());
                                        } else {
                                            toggle_open = Some(name.clone());
                                        }
                                    }
                                }
                                Row::Head(..) => {}
                            }
                        }
                        ui.add_space(m.gap);
                    }
                    cells.clear();
                };
            }
            for row in rows {
                // grid views gather cards and draw them a line at a time; a header ends a run
                if grid_mode && !matches!(row, Row::Head(..)) {
                    cells.push(row);
                    continue;
                }
                if grid_mode {
                    flush_cards!();
                }
                let (d, indent) = match row {
                    Row::GroupCard(..) => continue,
                    Row::Head(name, members, open) => {
                        let (rect, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
                        let selecting = self.dl_sel.is_some();
                        let all_sel = self.dl_sel.as_ref().is_some_and(|s| members.iter().all(|m| s.contains(&m.file)));
                        let lead = Pos2::new(rect.min.x + 30.0, rect.center().y);
                        if selecting {
                            let cb = Rect::from_center_size(lead, vec2(22.0, 22.0));
                            ui.painter().rect_filled(cb, CornerRadius::same(7), if all_sel { accent() } else { Color32::from_black_alpha(170) });
                            ui.painter().rect_stroke(cb, CornerRadius::same(7), Stroke::new(1.5, if all_sel { accent() } else { ov(150) }), StrokeKind::Inside);
                            if all_sel {
                                icons::draw(ui.painter(), cb.center(), "check", 14.0, ON_ACCENT);
                            }
                        } else {
                            if open {
                                icons::draw(ui.painter(), lead, "chevron_down", 18.0, accent());
                            } else {
                                let st = Stroke::new(2.2, accent());
                                ui.painter().line_segment([lead + vec2(-3.0, -6.0), lead + vec2(3.0, 0.0)], st);
                                ui.painter().line_segment([lead + vec2(3.0, 0.0), lead + vec2(-3.0, 6.0)], st);
                            }
                        }
                        let mb: f64 = members.iter().map(|m| std::fs::metadata(&m.file).map(|x| x.len()).unwrap_or(0) as f64).sum::<f64>() / 1_048_576.0;
                        let name_t = truncate(ui, &name, FontId::proportional(17.0), w - 230.0, 1);
                        ui.painter().text(Pos2::new(rect.min.x + 58.0, rect.center().y - 10.0), Align2::LEFT_CENTER, name_t, FontId::proportional(17.0), ink());
                        let count = members.len().to_string();
                        let summary = if members.len() == 1 {
                            trf("{count} item  •  {size} MB", &[("count", &count), ("size", &format!("{mb:.0}"))])
                        } else {
                            trf("{count} items  •  {size} MB", &[("count", &count), ("size", &format!("{mb:.0}"))])
                        };
                        ui.painter().text(
                            Pos2::new(rect.min.x + 58.0, rect.center().y + 12.0),
                            Align2::LEFT_CENTER,
                            summary,
                            FontId::proportional(13.0),
                            muted(),
                        );
                        let pr = Rect::from_center_size(Pos2::new(rect.max.x - 36.0, rect.center().y), vec2(36.0, 36.0));
                        if !selecting {
                            let over = ui.ctx().pointer_hover_pos().is_some_and(|p| pr.contains(p)) && resp.hovered();
                            ui.painter().circle_filled(pr.center(), 18.0, if over { accent() } else { card_hi() });
                            icons::draw(ui.painter(), pr.center(), "play", 15.0, if over { ON_ACCENT } else { ink() });
                        }
                        if resp.clicked() {
                            let on_play = resp.interact_pointer_pos().is_some_and(|p| pr.contains(p));
                            if selecting {
                                sel_group = Some(members.iter().map(|m| m.file.clone()).collect());
                            } else if on_play {
                                play_group = Some(members.iter().map(|m| (*m).clone()).collect());
                            } else {
                                toggle_open = Some(name.clone());
                            }
                        }
                        ui.add_space(8.0);
                        continue;
                    }
                    Row::Item(d, indent) => (d, indent),
                };
                let (full, resp) = ui.allocate_exact_size(vec2(w, 84.0), Sense::click());
                let rect = Rect::from_min_max(full.min + vec2(indent, 0.0), full.max);
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
                let th_r = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(120.0, 72.0));
                ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
                if std::path::Path::new(&d.thumb).exists() {
                    egui::Image::new(crate::platform::file_uri(&d.thumb))
                        .fit_to_exact_size(th_r.size())
                        .corner_radius(CornerRadius::same(10))
                        .paint_at(ui, th_r);
                }
                let selecting = self.dl_sel.is_some();
                let selected = self.dl_sel.as_ref().is_some_and(|s| s.contains(&d.file));
                let c = th_r.center();
                if !selecting {
                    ui.painter().circle_filled(c, 16.0 * (0.6 + 0.4 * hv), Color32::from_black_alpha((150.0 * hv) as u8));
                }
                if hv > 0.0 && !selecting {
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![c + vec2(-4.0, -7.0), c + vec2(-4.0, 7.0), c + vec2(8.0, 0.0)],
                        Color32::WHITE.gamma_multiply(hv),
                        Stroke::NONE,
                    ));
                }
                let tx = th_r.max.x + 14.0;
                let title = truncate(ui, &d.title, FontId::proportional(16.0), w - 320.0, 1);
                let size = std::fs::metadata(&d.file).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0;
                let sub = if d.audio_only {
                    trf("{channel}  •  {kind}  •  {time}  •  {size} MB", &[("channel", &d.channel), ("kind", &tr("Audio")), ("time", &fmt_time(d.duration)), ("size", &format!("{size:.0}"))])
                } else {
                    trf("{channel}  •  {quality}p  •  {time}  •  {size} MB", &[("channel", &d.channel), ("quality", &d.height.to_string()), ("time", &fmt_time(d.duration)), ("size", &format!("{size:.0}"))])
                };
                ui.painter().text(Pos2::new(tx, rect.center().y - 11.0), Align2::LEFT_CENTER, title, FontId::proportional(16.0), ink());
                ui.painter().text(Pos2::new(tx, rect.center().y + 12.0), Align2::LEFT_CENTER, sub, FontId::proportional(13.0), muted());
                // One click target per row: the delete button is a region of the row, so a
                // click is decided by where the pointer was, never by overlapping widgets.
                let xr = Rect::from_center_size(Pos2::new(rect.max.x - 30.0, rect.center().y), vec2(34.0, 34.0));
                if selecting {
                    // checkbox on the thumbnail corner; the whole row toggles the selection
                    let cb = Rect::from_min_size(th_r.min + vec2(6.0, 6.0), vec2(22.0, 22.0));
                    ui.painter().rect_filled(cb, CornerRadius::same(7), if selected { accent() } else { Color32::from_black_alpha(170) });
                    ui.painter().rect_stroke(cb, CornerRadius::same(7), Stroke::new(1.5, if selected { accent() } else { ov(150) }), StrokeKind::Inside);
                    if selected {
                        icons::draw(ui.painter(), cb.center(), "check", 14.0, ON_ACCENT);
                        ui.painter().rect_stroke(rect, CornerRadius::same(20), Stroke::new(1.5, accent()), StrokeKind::Inside);
                    }
                    if resp.clicked() {
                        toggle_sel = Some(d.file.clone());
                    }
                } else {
                    let over_x = ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p)) && resp.hovered();
                    if over_x {
                        ui.painter().circle_filled(xr.center(), 17.0, LIVE.gamma_multiply(0.85));
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    icons::draw(ui.painter(), xr.center(), "close", 15.0, ink());
                    if resp.clicked() {
                        let on_x = resp.interact_pointer_pos().is_some_and(|p| xr.contains(p));
                        if on_x {
                            del = Some(d.clone());
                        } else {
                            play = Some(d.clone());
                        }
                    }
                }
                ui.add_space(8.0);
            }
            if grid_mode {
                flush_cards!();
            }
        });
        if let Some(id) = toggle_pause {
            if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                use std::sync::atomic::Ordering;
                let now_paused = !j.pause.load(Ordering::SeqCst);
                j.pause.store(now_paused, Ordering::SeqCst);
                if now_paused {
                    j.paused_at = Some(Instant::now());
                } else if let Some(t) = j.paused_at.take() {
                    j.idle += t.elapsed();
                }
            }
        }
        if let Some(g) = toggle_open {
            if !self.dl_open.remove(&g) {
                self.dl_open.insert(g);
            }
        }
        if let Some(files) = sel_group {
            self.dl_confirm = false;
            if let Some(sel) = self.dl_sel.as_mut() {
                if files.iter().all(|f| sel.contains(f)) {
                    for f in &files {
                        sel.remove(f);
                    }
                } else {
                    sel.extend(files);
                }
            }
        }
        if let Some(items) = play_group {
            let q: Vec<QItem> = items.into_iter().map(QItem::Dl).collect();
            self.set_queue(q, 0);
            self.play_queue_at(self.queue_idx.unwrap_or(0));
        }
        if let Some(f) = toggle_sel {
            self.dl_confirm = false;
            if let Some(sel) = self.dl_sel.as_mut() {
                if !sel.remove(&f) {
                    sel.insert(f);
                }
            }
        }
        if let Some(id) = cancel {
            if let Some(j) = self.jobs.iter().find(|j| j.id == id) {
                j.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                // a paused job has to be woken so it can notice the cancel
                j.pause.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            self.jobs.retain(|j| j.id != id);
        }
        if let Some(d) = del {
            download::delete(&d);
            self.library = download::list();
        }
        if let Some(d) = play {
            let items: Vec<QItem> = self.library.iter().cloned().map(QItem::Dl).collect();
            let start = self.library.iter().position(|x| x.file == d.file).unwrap_or(0);
            self.set_queue(items, start);
            self.play_queue_at(self.queue_idx.unwrap_or(0));
        }
    }
}
