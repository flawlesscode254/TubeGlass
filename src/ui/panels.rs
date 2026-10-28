//! The side panel next to the player: comments, queue and related videos.

use crate::*;

impl App {
    /// The Queue tab: reorder by dragging, remove, save as a playlist.
    pub(crate) fn queue_body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        if self.queue.is_empty() {
            ui.add_space(24.0);
            ui.label(egui::RichText::new(tr("The queue is empty.")).size(15.0).color(muted()));
            ui.label(
                egui::RichText::new(tr("Use “Play all” on a playlist or channel, or right-click a video and choose “Add to queue”."))
                    .size(13.0)
                    .color(muted()),
            );
            return;
        }
        let mut jump: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let mut clear = false;
        let mut save_queue = false;
        let mut moved: Option<(usize, usize)> = None;
        ui.horizontal(|ui| {
            let count = self.queue.len().to_string();
            let summary = if self.queue.len() == 1 {
                trf("{count} item  •  drag to reorder", &[("count", &count)])
            } else {
                trf("{count} items  •  drag to reorder", &[("count", &count)])
            };
            ui.label(egui::RichText::new(summary).size(13.0).color(muted()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tr("Clear"))).clicked() {
                    clear = true;
                }
                if ui.add(egui::Button::new(tr("Save as playlist"))).clicked() {
                    save_queue = true;
                }
            });
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut rects: Vec<Rect> = vec![];
            for (i, item) in self.queue.iter().enumerate() {
                let cur = self.queue_idx == Some(i);
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click_and_drag());
                rects.push(r);
                let resp = resp.on_hover_cursor(if self.q_drag.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::PointingHand });
                if resp.drag_started() {
                    self.q_drag = Some(i);
                }
                if cur {
                    ui.painter().rect_filled(r, CornerRadius::same(12), accent().gamma_multiply(0.22));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(12), ov(16));
                }
                if cur {
                    icons::draw(ui.painter(), Pos2::new(r.min.x + 16.0, r.center().y), "play", 13.0, accent());
                } else {
                    ui.painter().text(Pos2::new(r.min.x + 16.0, r.center().y), Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(13.0), muted());
                }
                let title = truncate(ui, &item.title(), FontId::proportional(14.0), r.width() - 96.0, 1);
                let sub = truncate(ui, &item.subtitle(), FontId::proportional(12.0), r.width() - 96.0, 1);
                ui.painter().text(Pos2::new(r.min.x + 36.0, r.center().y - 9.0), Align2::LEFT_CENTER, title, FontId::proportional(14.0), ink());
                ui.painter().text(Pos2::new(r.min.x + 36.0, r.center().y + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), muted());
                let xr = Rect::from_center_size(Pos2::new(r.max.x - 18.0, r.center().y), vec2(26.0, 26.0));
                let over_x = ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p)) && resp.hovered();
                if over_x {
                    ui.painter().circle_filled(xr.center(), 13.0, ov(30));
                }
                if resp.hovered() {
                    icons::draw(ui.painter(), xr.center(), "close", 12.0, if over_x { ink() } else { muted() });
                }
                if resp.clicked() {
                    if over_x {
                        remove = Some(i);
                    } else {
                        jump = Some(i);
                    }
                }
            }
            if let Some(from) = self.q_drag {
                if let Some(p) = ui.ctx().pointer_hover_pos() {
                    let to = rects.iter().position(|r| p.y < r.max.y).unwrap_or(rects.len().saturating_sub(1));
                    if let Some(r) = rects.get(to) {
                        let y = if to > from { r.max.y } else { r.min.y };
                        ui.painter().line_segment([Pos2::new(r.min.x + 6.0, y), Pos2::new(r.max.x - 6.0, y)], Stroke::new(2.5, accent()));
                    }
                    if !ui.ctx().input(|i| i.pointer.any_down()) {
                        if to != from {
                            moved = Some((from, to));
                        }
                        self.q_drag = None;
                    }
                } else {
                    self.q_drag = None;
                }
            }
        });
        if let Some((from, to)) = moved {
            let item = self.queue.remove(from);
            self.queue.insert(to.min(self.queue.len()), item);
            self.queue_idx = self.queue_idx.map(|c| {
                if c == from {
                    to
                } else if from < c && to >= c {
                    c - 1
                } else if from > c && to <= c {
                    c + 1
                } else {
                    c
                }
            });
            self.gapless_next = None;
        }
        if save_queue {
            let videos: Vec<Video> = self
                .queue
                .iter()
                .filter_map(|q| match q {
                    QItem::Yt(v) if !v.id.starts_with("local:") => Some(v.clone()),
                    _ => None,
                })
                .collect();
            if videos.is_empty() {
                self.toast(tr("Nothing in the queue can be saved as a playlist"));
            } else {
                let id = format!("l-{}", now_secs());
                let count = videos.len().to_string();
                let name = if videos.len() == 1 {
                    trf("Saved queue ({count} item)", &[("count", &count)])
                } else {
                    trf("Saved queue ({count} items)", &[("count", &count)])
                };
                self.lists.push(LocalList { id: id.clone(), name: name.clone(), videos, offline: false });
                save_lists(&self.lists);
                self.toast(trf("Saved “{name}” — find it under Playlists", &[("name", &name)]));
            }
        }
        if clear {
            self.queue.clear();
            self.queue_idx = None;
        } else if let Some(i) = remove {
            self.queue.remove(i);
            self.queue_idx = match self.queue_idx {
                Some(c) if c == i => None,
                Some(c) if c > i => Some(c - 1),
                other => other,
            };
        } else if let Some(i) = jump {
            self.play_queue_at(i);
        }
    }

    /// The Related tab: videos YouTube suggests next.
    pub(crate) fn related_body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let Some(w) = self.watching.clone().filter(|w| !non_yt(&w.id)) else {
            ui.label(egui::RichText::new(tr("Related videos are only available for YouTube videos.")).size(14.0).color(muted()));
            return;
        };
        if self.related.as_ref().is_none_or(|r| r.0 != w.id) {
            self.related = Some((w.id.clone(), None));
            let (tx, ctx, vid) = (self.tx.clone(), self.ctx.clone(), w.id.clone());
            std::thread::spawn(move || {
                let r = yt::related(&vid);
                let _ = tx.send(Msg::Related(vid, r));
                ctx.request_repaint();
            });
        }
        let mut pick: Option<Video> = None;
        match self.related.as_ref().and_then(|r| r.1.as_ref()) {
            None => {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(26.0).color(accent()));
                });
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(muted()));
            }
            Some(Ok(list)) => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for v in list {
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 78.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(12), ov(16));
                        }
                        let th = Rect::from_min_size(r.min + vec2(4.0, 5.0), vec2(116.0, 68.0));
                        ui.painter().rect_filled(th, CornerRadius::same(8), card_hi());
                        let uri = img_uri(ui.ctx(), &v.thumb);
                        if !uri.is_empty() {
                            egui::Image::new(uri)
                                .fit_to_exact_size(th.size())
                                .corner_radius(CornerRadius::same(8))
                                .paint_at(ui, th);
                        }
                        if !v.duration.is_empty() {
                            let g = ui.fonts_mut(|f| f.layout_no_wrap(v.duration.clone(), FontId::proportional(11.0), Color32::WHITE));
                            let b = Rect::from_min_size(th.right_bottom() - vec2(g.size().x + 12.0, 20.0), vec2(g.size().x + 8.0, 16.0));
                            ui.painter().rect_filled(b, CornerRadius::same(4), Color32::from_black_alpha(190));
                            ui.painter().text(b.center(), Align2::CENTER_CENTER, &v.duration, FontId::proportional(11.0), Color32::WHITE);
                        }
                        let tx = th.max.x + 10.0;
                        let title = truncate(ui, &v.title, FontId::proportional(13.5), r.max.x - tx - 6.0, 2);
                        ui.painter().text(Pos2::new(tx, r.min.y + 8.0), Align2::LEFT_TOP, title, FontId::proportional(13.5), ink());
                        let sub = [v.channel_name.as_str(), v.meta.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" • ");
                        let sub = truncate(ui, &sub, FontId::proportional(12.0), r.max.x - tx - 6.0, 1);
                        ui.painter().text(Pos2::new(tx, r.max.y - 8.0), Align2::LEFT_BOTTOM, sub, FontId::proportional(12.0), muted());
                        if resp.clicked() {
                            pick = Some(v.clone());
                        }
                    }
                });
            }
        }
        if let Some(v) = pick {
            self.queue.clear();
            self.queue_idx = None;
            self.play(v);
        }
    }

    /// The side panel: tab strip and the comments, lyrics, queue, transcript, notes or clip tab.
    pub(crate) fn comments_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            let notes_n = self.watching.as_ref().and_then(|w| self.bookmarks.get(&w.id)).map_or(0, |b| b.len());
            let tabs = [
                (SideTab::Comments, tr("Comments")),
                (SideTab::Lyrics, tr("Lyrics")),
                (SideTab::Queue, if self.queue.is_empty() { tr("Queue") } else { trf("Queue ({count})", &[("count", &self.queue.len().to_string())]) }),
                (SideTab::Related, tr("Related")),
                (SideTab::Transcript, tr("Transcript")),
                (SideTab::Notes, if notes_n == 0 { tr("Notes") } else { trf("Notes ({count})", &[("count", &notes_n.to_string())]) }),
                (SideTab::Clip, tr("Clip")),
            ];
            for (t, label) in tabs {
                let on = self.side_tab == t;
                let g = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), FontId::proportional(14.0), ink()));
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 30.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if on {
                    ui.painter().rect_filled(r, CornerRadius::same(15), accent());
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(15), ov(18));
                }
                ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), if on { ON_ACCENT } else { ink().gamma_multiply(0.8) });
                if resp.clicked() {
                    self.side_tab = t;
                }
            }
            {
                let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                ui.painter().rect_filled(r, CornerRadius::same(9), ov((10.0 + 25.0 * hv) as u8));
                icons::draw(ui.painter(), r.center(), "close", 14.0, ink());
                if resp.clicked() {
                    self.comments_open = false;
                }
            }
        });
        match self.side_tab {
            SideTab::Transcript => {
                self.transcript_body(ui);
                return;
            }
            SideTab::Notes => {
                self.notes_body(ui);
                return;
            }
            SideTab::Clip => {
                self.clip_body(ui);
                return;
            }
            SideTab::Queue => {
                self.queue_body(ui);
                return;
            }
            SideTab::Related => {
                self.related_body(ui);
                return;
            }
            SideTab::Lyrics => {
                self.lyrics_body(ui);
                return;
            }
            SideTab::Comments => {}
        }
        ui.add_space(6.0);
        let mut more = false;
        let mut sort_pick: Option<String> = None;
        let mut toggle_reply: Option<(String, String)> = None; // (comment id, reply token)
        let mut more_reply: Option<(String, String)> = None;
        match &self.comments {
            None => {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(26.0).color(accent()));
                });
            }
            Some((_, Err(e))) => {
                ui.colored_label(muted(), e);
            }
            Some((_, Ok(list))) => {
                if !self.comment_sorts.is_empty() {
                    ui.horizontal(|ui| {
                        for o in &self.comment_sorts {
                            let b = egui::Button::new(egui::RichText::new(&o.title).color(if o.selected { ON_ACCENT } else { ink() }))
                                .fill(if o.selected { accent() } else { card() })
                                .corner_radius(CornerRadius::same(14));
                            if ui.add(b).clicked() && !o.selected {
                                sort_pick = Some(o.token.clone());
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for c in list {
                        let st = self.replies.get(&c.id);
                        let open = st.is_some_and(|s| s.open);
                        if comment_row(ui, c, false, open) {
                            if let Some(tok) = &c.reply_token {
                                toggle_reply = Some((c.id.clone(), tok.clone()));
                            }
                        }
                        if let Some(st) = st.filter(|s| s.open) {
                            ui.horizontal_top(|ui| {
                                ui.add_space(30.0);
                                // thin guide line like on YouTube
                                let line_x = ui.cursor().min.x - 12.0;
                                let top = ui.cursor().min.y;
                                ui.vertical(|ui| {
                                    for r in &st.items {
                                        comment_row(ui, r, true, false);
                                    }
                                    if st.loading {
                                        ui.add(egui::Spinner::new().size(18.0).color(accent()));
                                    }
                                    if let Some(e) = &st.err {
                                        ui.colored_label(LIVE, egui::RichText::new(e).size(12.0));
                                    }
                                    if let (Some(next), false) = (&st.next, st.loading) {
                                        let (r, resp) = ui.allocate_exact_size(vec2(170.0, 26.0), Sense::click());
                                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                                        ui.painter().rect_filled(r, CornerRadius::same(13), accent().gamma_multiply(0.15 + 0.2 * hv));
                                        icon_label(ui, r.center(), "chevron_down", &tr("Show more replies"), accent2(), false);
                                        if resp.clicked() {
                                            more_reply = Some((c.id.clone(), next.clone()));
                                        }
                                    }
                                });
                                let bottom = ui.min_rect().max.y;
                                ui.painter().line_segment(
                                    [Pos2::new(line_x, top), Pos2::new(line_x, bottom)],
                                    Stroke::new(1.5, ov(28)),
                                );
                            });
                        }
                        ui.add_space(8.0);
                    }
                    if self.comments_next.is_some() {
                        if self.comments_busy {
                            ui.add(egui::Spinner::new());
                        } else if ui.add(egui::Button::new(tr("Load more comments")).fill(card())).clicked() {
                            more = true;
                        }
                    }
                });
            }
        }
        if let Some((id, tok)) = toggle_reply {
            match self.replies.get_mut(&id) {
                Some(st) if !st.items.is_empty() || st.loading => st.open = !st.open,
                _ => self.fetch_replies(&id, tok, false),
            }
        }
        if let Some((id, tok)) = more_reply {
            self.fetch_replies(&id, tok, true);
        }
        if let Some(t) = sort_pick {
            self.fetch_comments(false, Some(t));
        }
        if more {
            self.load_comments(true);
        }
    }
}
