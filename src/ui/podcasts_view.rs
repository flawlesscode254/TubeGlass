//! The Podcasts pages: subscribed shows, episode lists in the three layouts, and the audio stage for an episode.

use crate::*;
use crate::podcasts::*;
use crate::podcasts::Saved;

/// Right-click menu shared by the row and card layouts.
pub(crate) fn ep_menu(resp: &egui::Response, saved: bool, prog: Option<f32>, is_new: bool) -> Option<EpAct> {
    let mut act = None;
    resp.context_menu(|ui| {
        if ui.button(tr("Play next")).clicked() {
            act = Some(EpAct::Next);
            ui.close();
        }
        if ui.button(tr("Add to queue")).clicked() {
            act = Some(EpAct::Queue);
            ui.close();
        }
        if !saved && prog.is_none() && ui.button(tr("Download")).clicked() {
            act = Some(EpAct::Download);
            ui.close();
        }
        if is_new && ui.button(tr("Mark as played")).clicked() {
            act = Some(EpAct::Seen);
            ui.close();
        }
    });
    act
}

/// What the user asked for on an episode.
#[derive(Clone, Copy, PartialEq)]
pub enum EpAct {
    Play,
    Next,
    Queue,
    Download,
    Seen,
    None,
}

/// Short alias used by the show page.
pub(crate) type A = EpAct;

impl App {
    /// One episode as a card (grid and compact grid views).
    pub(crate) fn episode_card(&self, ui: &mut egui::Ui, rect: Rect, m: &views::Metrics, show: &Show, e: &Episode, with_show: bool) -> Option<EpAct> {
        let vid = video_id(e);
        let resp = ui.interact(rect, ui.scope_id().with(("epcard", &vid)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        let art = if e.image.is_empty() { &show.image } else { &e.image };
        let uri = if ui.is_rect_visible(rect) { img_uri(ui.ctx(), art) } else { String::new() };
        let mut bits: Vec<String> = vec![];
        if with_show {
            bits.push(show.title.clone());
        }
        if e.published > 0 {
            bits.push(ago(e.published));
        }
        let hist = self.history.iter().find(|h| h.video.id == vid);
        let total = if e.duration > 0.0 { e.duration } else { hist.map_or(0.0, |h| h.duration) };
        let img = views::media_card(ui, rect, m, &uri, &e.title, &bits.join("  •  "), hv);
        if total > 0.0 {
            views::pill_label(ui, img.right_bottom() - vec2(0.0, 0.0), &fmt_time(total));
        }
        let is_new = !self.seen.contains(&vid);
        if is_new {
            let b = Rect::from_min_size(img.min + vec2(7.0, 7.0), vec2(38.0, 17.0));
            ui.painter().rect_filled(b, CornerRadius::same(5), accent());
            ui.painter().text(b.center(), Align2::CENTER_CENTER, tr("NEW"), FontId::proportional(10.5), ON_ACCENT);
        }
        if let Some(h) = hist.filter(|h| h.duration > 0.0 && h.position > 5.0) {
            let f = (h.position / h.duration).clamp(0.0, 1.0) as f32;
            let bar = Rect::from_min_size(Pos2::new(img.min.x, img.max.y - 4.0), vec2(img.width(), 4.0));
            ui.painter().rect_filled(bar, CornerRadius::ZERO, Color32::from_black_alpha(150));
            ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f, 4.0)), CornerRadius::ZERO, accent());
        }
        // download state in the picture's corner
        let saved = self.is_downloaded(&vid, true);
        let prog = self.job_progress(&vid);
        let dr = Rect::from_center_size(img.right_top() + vec2(-20.0, 20.0), vec2(32.0, 32.0));
        let over_dl = ui.ctx().pointer_hover_pos().is_some_and(|p| dr.contains(p)) && resp.hovered();
        if saved || prog.is_some() || hv > 0.05 {
            ui.painter().circle_filled(dr.center(), 15.0, if over_dl && !saved { accent() } else { Color32::from_black_alpha(165) });
            match (saved, prog) {
                (true, _) => icons::draw(ui.painter(), dr.center(), "check", 15.0, TEAL),
                (false, Some(p)) => {
                    let n = 24;
                    let pts: Vec<Pos2> = (0..=((n as f32 * p.max(0.05)) as usize))
                        .map(|i| {
                            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
                            dr.center() + vec2(a.cos(), a.sin()) * 11.0
                        })
                        .collect();
                    ui.painter().add(egui::Shape::line(pts, Stroke::new(2.4, accent())));
                    ui.ctx().request_repaint_after(Duration::from_millis(300));
                }
                _ => icons::draw(ui.painter(), dr.center(), "download", 15.0, if over_dl { ON_ACCENT } else { Color32::WHITE }),
            }
        }
        let mut act = ep_menu(&resp, saved, prog, is_new);
        if resp.clicked() && act.is_none() {
            let on_dl = resp.interact_pointer_pos().is_some_and(|p| dr.contains(p));
            act = Some(if on_dl {
                if saved || prog.is_some() { EpAct::None } else { EpAct::Download }
            } else {
                EpAct::Play
            });
        }
        act
    }

    /// A list of episodes in whichever layout is chosen. `eps` is (feed, show, episode).
    pub(crate) fn episode_list(&self, ui: &mut egui::Ui, eps: &[(&str, &Show, &Episode)], w: f32, with_show: bool) -> Option<(String, String, EpAct)> {
        let mut out = None;
        if self.settings.view_mode == 2 {
            for (feed, show, e) in eps {
                if let Some(a) = self.episode_row(ui, show, e, w, with_show) {
                    out = Some((feed.to_string(), e.id.clone(), a));
                }
                ui.add_space(8.0);
            }
            return out;
        }
        let m = views::metrics(&self.settings, w, true);
        for line in eps.chunks(m.cols) {
            let (area, _) = ui.allocate_exact_size(vec2(w, m.ch), Sense::hover());
            for (k, (feed, show, e)) in line.iter().enumerate() {
                let rect = Rect::from_min_size(area.min + vec2(k as f32 * (m.cw + m.gap), 0.0), vec2(m.cw, m.ch));
                if let Some(a) = self.episode_card(ui, rect, &m, show, e, with_show) {
                    out = Some((feed.to_string(), e.id.clone(), a));
                }
            }
            ui.add_space(m.gap);
        }
        out
    }

    /// One episode row. Returns what the user asked for.
    pub(crate) fn episode_row(&self, ui: &mut egui::Ui, show: &Show, e: &Episode, w: f32, with_show: bool) -> Option<EpAct> {
        let vid = video_id(e);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 84.0), Sense::click());
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
        let th_r = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(72.0, 72.0));
        ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
        let art = if e.image.is_empty() { &show.image } else { &e.image };
        // pictures are only fetched and decoded for rows that are on screen
        let uri = if ui.is_rect_visible(rect) { img_uri(ui.ctx(), art) } else { String::new() };
        if !uri.is_empty() {
            egui::Image::new(uri).fit_to_exact_size(th_r.size()).corner_radius(CornerRadius::same(10)).paint_at(ui, th_r);
        }
        let is_new = !self.seen.contains(&vid);
        let tx = th_r.max.x + 14.0;
        let right = 150.0;
        if is_new {
            let b = Rect::from_min_size(Pos2::new(th_r.min.x + 4.0, th_r.min.y + 4.0), vec2(38.0, 17.0));
            ui.painter().rect_filled(b, CornerRadius::same(5), accent());
            ui.painter().text(b.center(), Align2::CENTER_CENTER, tr("NEW"), FontId::proportional(10.5), ON_ACCENT);
        }
        let title = truncate(ui, &e.title, FontId::proportional(15.5), w - (tx - rect.min.x) - right, 2);
        ui.painter().text(Pos2::new(tx, rect.min.y + 10.0), Align2::LEFT_TOP, title, FontId::proportional(15.5), ink());
        let mut bits: Vec<String> = vec![];
        if with_show {
            bits.push(show.title.clone());
        }
        if e.published > 0 {
            bits.push(ago(e.published));
        }
        let hist = self.history.iter().find(|h| h.video.id == vid);
        let total = if e.duration > 0.0 { e.duration } else { hist.map_or(0.0, |h| h.duration) };
        if total > 0.0 {
            bits.push(fmt_time(total));
        }
        let sub = truncate(ui, &bits.join("  •  "), FontId::proportional(12.5), w - (tx - rect.min.x) - right, 1);
        ui.painter().text(Pos2::new(tx, rect.max.y - 12.0), Align2::LEFT_BOTTOM, sub, FontId::proportional(12.5), muted());
        // how far along
        if let Some(h) = hist.filter(|h| h.duration > 0.0 && h.position > 5.0) {
            let f = (h.position / h.duration).clamp(0.0, 1.0) as f32;
            let bar = Rect::from_min_size(Pos2::new(tx, rect.max.y - 6.0), vec2((w - (tx - rect.min.x) - right).max(40.0), 3.0));
            ui.painter().rect_filled(bar, CornerRadius::same(2), ov(30));
            ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f, 3.0)), CornerRadius::same(2), accent());
        }
        // download button
        let dr = Rect::from_center_size(Pos2::new(rect.max.x - 34.0, rect.center().y), vec2(40.0, 40.0));
        let saved = self.is_downloaded(&vid, true);
        let prog = self.job_progress(&vid);
        let over_dl = ui.ctx().pointer_hover_pos().is_some_and(|p| dr.contains(p)) && resp.hovered();
        if over_dl && !saved {
            ui.painter().circle_filled(dr.center(), 19.0, ov(30));
        }
        match (saved, prog) {
            (true, _) => icons::draw(ui.painter(), dr.center(), "check", 18.0, TEAL),
            (false, Some(p)) => {
                ui.painter().circle_stroke(dr.center(), 14.0, Stroke::new(2.5, ov(40)));
                let n = 28;
                let pts: Vec<Pos2> = (0..=((n as f32 * p.max(0.04)) as usize))
                    .map(|i| {
                        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
                        dr.center() + vec2(a.cos(), a.sin()) * 14.0
                    })
                    .collect();
                ui.painter().add(egui::Shape::line(pts, Stroke::new(2.5, accent())));
                ui.ctx().request_repaint_after(Duration::from_millis(300));
            }
            (false, None) => icons::draw(ui.painter(), dr.center(), "download", 18.0, if over_dl { accent() } else { muted() }),
        }
        let mut act = ep_menu(&resp, saved, prog, is_new);
        let resp = if e.summary.is_empty() { resp } else { resp.on_hover_text(e.summary.chars().take(240).collect::<String>()) };
        if resp.clicked() && act.is_none() {
            let on_dl = resp.interact_pointer_pos().is_some_and(|p| dr.contains(p));
            act = Some(if on_dl {
                if saved || prog.is_some() { EpAct::None } else { EpAct::Download }
            } else {
                EpAct::Play
            });
        }
        act
    }

    /// Carries out an episode action (play, queue, download, mark played).
    pub(crate) fn do_ep_act(&mut self, feed: &str, ep_id: &str, a: EpAct) {
        let Some(p) = self.pods.iter().find(|p| p.show.feed == feed) else { return };
        let Some(e) = p.episodes.iter().find(|e| e.id == ep_id) else { return };
        let v = podcasts::episode_video(&p.show, e);
        match a {
            EpAct::Play => {
                // playing from a list makes the rest of the shown episodes the queue
                self.play(v);
            }
            EpAct::Next => self.queue_add(QItem::Yt(v), true),
            EpAct::Queue => self.queue_add(QItem::Yt(v), false),
            EpAct::Download => self.start_download(&v, true, 0),
            EpAct::Seen => {
                self.seen.insert(v.id);
                self.seen_dirty = true;
            }
            EpAct::None => {}
        }
    }

    /// The search bar every top-level page starts with (Home searches YouTube, Music searches
    /// songs, this one searches podcasts). Returns true when it was used to leave a show page.
    pub(crate) fn pod_top_bar(&mut self, ui: &mut egui::Ui) -> bool {
        ui.add_space(6.0);
        let mut back = false;
        let mut go = false;
        let mut cleared = false;
        ui.horizontal(|ui| {
            if self.can_go_back() {
                let (r, resp) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Back  (Alt+←)"));
                let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                ui.painter().add(surface::capsule(r, 1.0, if hv > 0.05 { Some(accent()) } else { None }, false));
                icons::draw(ui.painter(), r.center(), "arrow_left", 19.0, if hv > 0.5 { accent() } else { ink() });
                back = resp.clicked();
                ui.add_space(4.0);
            }
            let w = ui.available_width();
            let f = text_field(ui, &mut self.pod_input, &tr("Search podcasts, or paste a feed or Apple Podcasts link"), w, 46.0, "search", Some(&tr("Search")), false);
            go = f.submit || f.action;
            cleared = f.cleared;
        });
        if cleared {
            self.pod_hits = None;
            self.pod_err = None;
        }
        if go {
            self.start_pod_input();
            self.sel = Sel::Podcasts;
        }
        ui.add_space(8.0);
        if back {
            self.go_back();
        }
        back
    }

    /// The audio-only stage for a podcast: round cover art inside a halo of spokes that follow
    /// the voice. The spectrum is mirrored left/right and the halo turns slowly.
    pub fn pod_visual(&mut self, ui: &mut egui::Ui, full: Rect) {
        let top = full.min.y + 70.0;
        let bottom = full.max.y - 120.0;
        // leave room under the halo for the label, or for two lines of the transcript
        let avail_h = (bottom - top - 40.0).max(200.0);
        let big = (avail_h.min(full.width() - 80.0) / 2.0).clamp(100.0, 330.0);
        let c = Pos2::new(full.center().x, top + avail_h / 2.0);
        let cover_r = big * 0.5;
        let inner = cover_r + big * 0.07;
        let reach = big - inner - 4.0;
        let n = self.viz_levels.len();
        let energy = self.viz_levels.iter().sum::<f32>() / n.max(1) as f32;
        let t = ui.input(|i| i.time) as f32;
        let spokes = 96usize;
        let rot = t * 0.12 - std::f32::consts::FRAC_PI_2;
        let p = ui.painter();
        // soft glow behind everything, breathing with the voice
        p.circle_filled(c, inner + reach * (0.55 + 0.35 * energy.min(0.8)), accent().gamma_multiply(0.05 + 0.10 * energy.min(0.8)));
        for k in 0..spokes {
            let f = k as f32 / spokes as f32;
            // low voices at the top, high ones towards the bottom, the same on both sides
            let m = if f < 0.5 { f * 2.0 } else { (1.0 - f) * 2.0 };
            let x = m * (n as f32 - 1.0);
            let (i0, fr) = (x.floor() as usize, x.fract());
            let l = self.viz_levels[i0.min(n - 1)] * (1.0 - fr) + self.viz_levels[(i0 + 1).min(n - 1)] * fr;
            let len = 3.0 + l.powf(1.05) * reach;
            let a = rot + f * std::f32::consts::TAU;
            let dir = vec2(a.cos(), a.sin());
            let col = {
                let (c1, c2) = (accent(), accent2());
                let g = m;
                Color32::from_rgb(
                    (c1.r() as f32 + (c2.r() as f32 - c1.r() as f32) * g) as u8,
                    (c1.g() as f32 + (c2.g() as f32 - c1.g() as f32) * g) as u8,
                    (c1.b() as f32 + (c2.b() as f32 - c1.b() as f32) * g) as u8,
                )
            };
            let (p0, p1) = (c + dir * inner, c + dir * (inner + len));
            p.line_segment([p0, p1], Stroke::new(big * 0.016 + 1.6, col));
            p.circle_filled(p1, (big * 0.016 + 1.6) / 2.0, col);
        }
        p.circle_stroke(c, cover_r + big * 0.03, Stroke::new(1.5, accent().gamma_multiply(0.55)));
        // the cover, cropped round (the app icon when the show has none)
        let side = cover_r * 2.0 * (1.0 + 0.025 * energy.min(0.8));
        let art = Rect::from_center_size(c, vec2(side, side));
        let uri = self.watching.as_ref().map(|w| if w.thumb.is_empty() { String::new() } else { img_uri(ui.ctx(), &w.thumb) }).unwrap_or_default();
        ui.painter().circle_filled(c, side / 2.0, card());
        if uri.is_empty() {
            ui.painter().image(self.logo.id(), art.shrink(side * 0.12), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        } else {
            egui::Image::new(uri)
                .fit_to_exact_size(art.size())
                .corner_radius(CornerRadius::same((side / 2.0) as u8))
                .paint_at(ui, art);
        }
            icon_label(ui, Pos2::new(c.x, c.y + big + 20.0), "mic", &tr("Podcast"), muted(), false);
        ui.ctx().request_repaint();
    }

    /// The Podcasts page: search, your shows and the latest episodes.
    pub fn podcasts_view(&mut self, ui: &mut egui::Ui) {
        if self.pod_top_bar(ui) {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(tr("Podcasts")).size(26.0).strong());
            if !self.pod_busy.is_empty() {
                ui.add(egui::Spinner::new().size(18.0).color(accent()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true), |ui| {
                if !self.pods.is_empty() {
                    self.view_toggle(ui);
                    if icon_btn(ui, "refresh",&tr("Refresh"), None, ink(), false, true).clicked() {
                        self.refresh_all_pods();
                    }
                }
            });
        });
        if let Some(e) = &self.pod_err {
            ui.add_space(4.0);
            ui.colored_label(LIVE, e);
        }
        ui.add_space(10.0);

        let mut subscribe: Option<String> = None;
        let mut open: Option<String> = None;
        let mut act: Option<(String, String, EpAct)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width() - 6.0;
            // search results
            if let Some((q, res)) = &self.pod_hits {
                ui.label(egui::RichText::new(trf("Results for “{query}”", &[("query", &q.to_uppercase())])).size(11.0).color(muted()).strong());
                match res {
                    None => {
                        ui.add_space(8.0);
                        ui.add(egui::Spinner::new().size(22.0).color(accent()));
                    }
                    Some(Err(e)) => {
                        ui.colored_label(LIVE, trf("Search failed: {error}", &[("error", e)]));
                    }
                    Some(Ok(hits)) if hits.is_empty() => {
                        ui.label(egui::RichText::new(tr("No podcasts found.")).color(muted()));
                    }
                    Some(Ok(hits)) => {
                        for h in hits {
                            let (rect, resp) = ui.allocate_exact_size(vec2(w, 76.0), Sense::click());
                            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                            let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                            ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, None, true));
                            let th_r = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(60.0, 60.0));
                            ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
                            let uri = img_uri(ui.ctx(), &h.image);
                            if !uri.is_empty() {
                                egui::Image::new(uri).fit_to_exact_size(th_r.size()).corner_radius(CornerRadius::same(10)).paint_at(ui, th_r);
                            }
                            let tx = th_r.max.x + 14.0;
                            let t = truncate(ui, &h.title, FontId::proportional(16.0), w - 270.0, 1);
                            let a = truncate(ui, &h.author, FontId::proportional(13.0), w - 270.0, 1);
                            ui.painter().text(Pos2::new(tx, rect.center().y - 10.0), Align2::LEFT_CENTER, t, FontId::proportional(16.0), ink());
                            ui.painter().text(Pos2::new(tx, rect.center().y + 11.0), Align2::LEFT_CENTER, a, FontId::proportional(13.0), muted());
                            let have = self.pods.iter().any(|p| p.show.feed == h.feed);
                            let br = Rect::from_center_size(Pos2::new(rect.max.x - 70.0, rect.center().y), vec2(112.0, 34.0));
                            let bresp = ui.interact(br, resp.id.with("sub"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                            ui.painter().rect_filled(br, CornerRadius::same(17), if have { card_hi() } else if bresp.hovered() { accent2() } else { accent() });
                            icon_label(ui, br.center(), if have { "check" } else { "plus" }, &tr(if have { "Subscribed" } else { "Subscribe" }), if have { ink() } else { ON_ACCENT }, false);
                            if bresp.clicked() && !have {
                                subscribe = Some(h.feed.clone());
                            } else if resp.clicked() && have {
                                open = Some(h.feed.clone());
                            }
                            ui.add_space(8.0);
                        }
                    }
                }
                ui.add_space(14.0);
            }
            if self.pods.is_empty() && self.pod_hits.is_none() {
                ui.add_space(50.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(tr("No podcasts yet.")).size(18.0).color(muted()));
                    ui.label(egui::RichText::new(tr("Search above, or paste a podcast's feed address (RSS) or its Apple Podcasts link.")).color(muted()));
                });
                return;
            }
            // your shows
            if !self.pods.is_empty() {
                ui.label(egui::RichText::new(tr("YOUR SHOWS")).size(11.0).color(muted()).strong());
                if self.settings.view_mode == 2 {
                    // list: one row per show
                    for p in &self.pods {
                        let (rect, resp) = ui.allocate_exact_size(vec2(w, 76.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
                        let th_r = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(60.0, 60.0));
                        ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
                        let uri = img_uri(ui.ctx(), &p.show.image);
                        if !uri.is_empty() {
                            egui::Image::new(uri).fit_to_exact_size(th_r.size()).corner_radius(CornerRadius::same(10)).paint_at(ui, th_r);
                        }
                        let tx = th_r.max.x + 14.0;
                        let t = truncate(ui, &p.show.title, FontId::proportional(16.0), w - 230.0, 1);
                        let count = p.episodes.len().to_string();
                        let summary = if p.episodes.len() == 1 {
                            trf("{author}  •  {count} episode", &[("author", &p.show.author), ("count", &count)])
                        } else {
                            trf("{author}  •  {count} episodes", &[("author", &p.show.author), ("count", &count)])
                        };
                        let a = truncate(ui, &summary, FontId::proportional(13.0), w - 230.0, 1);
                        ui.painter().text(Pos2::new(tx, rect.center().y - 10.0), Align2::LEFT_CENTER, t, FontId::proportional(16.0), ink());
                        ui.painter().text(Pos2::new(tx, rect.center().y + 11.0), Align2::LEFT_CENTER, a, FontId::proportional(13.0), muted());
                        let n = self.pod_new_count(p);
                        if n > 0 {
                            let b = Rect::from_center_size(Pos2::new(rect.max.x - 60.0, rect.center().y), vec2(78.0, 24.0));
                            ui.painter().rect_filled(b, CornerRadius::same(12), accent());
                            ui.painter().text(b.center(), Align2::CENTER_CENTER, trf("{count} new", &[("count", &n.to_string())]), FontId::proportional(12.5), ON_ACCENT);
                        }
                        if resp.clicked() {
                            open = Some(p.show.feed.clone());
                        }
                        ui.add_space(8.0);
                    }
                } else {
                    let card = if self.settings.view_mode == 1 { 110.0 } else { 150.0 };
                    let gap = 14.0;
                    let cols = (((w + gap) / (card + gap)).floor() as usize).max(1);
                    let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
                    for chunk in self.pods.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for p in chunk {
                                let (rect, resp) = ui.allocate_exact_size(vec2(cw, cw + 40.0), Sense::click());
                                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                                let img = Rect::from_min_size(rect.min, vec2(cw, cw));
                                ui.painter().rect_filled(img, CornerRadius::same(14), card_hi());
                                let uri = img_uri(ui.ctx(), &p.show.image);
                                if !uri.is_empty() {
                                    egui::Image::new(uri).fit_to_exact_size(img.size()).corner_radius(CornerRadius::same(14)).paint_at(ui, img);
                                }
                                if hv > 0.05 {
                                    ui.painter().rect_stroke(img, CornerRadius::same(14), Stroke::new(2.0, accent().gamma_multiply(hv)), StrokeKind::Inside);
                                }
                                let n = self.pod_new_count(p);
                                if n > 0 {
                                    let label = if n > 9 { "9+".to_string() } else { n.to_string() };
                                    let b = Rect::from_center_size(img.right_top() + vec2(-18.0, 16.0), vec2(26.0, 20.0));
                                    ui.painter().rect_filled(b, CornerRadius::same(10), accent());
                                    ui.painter().text(b.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.0), ON_ACCENT);
                                }
                                let t = truncate(ui, &p.show.title, FontId::proportional(14.0), cw, 1);
                                ui.painter().text(Pos2::new(rect.min.x, img.max.y + 6.0), Align2::LEFT_TOP, t, FontId::proportional(14.0), ink());
                                let a = truncate(ui, &p.show.author, FontId::proportional(12.0), cw, 1);
                                ui.painter().text(Pos2::new(rect.min.x, img.max.y + 24.0), Align2::LEFT_TOP, a, FontId::proportional(12.0), muted());
                                if resp.clicked() {
                                    open = Some(p.show.feed.clone());
                                }
                            }
                        });
                        ui.add_space(8.0);
                    }
                }
                // newest episodes of every show
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr("LATEST EPISODES")).size(11.0).color(muted()).strong());
                let mut all: Vec<(&Saved, &Episode)> = self.pods.iter().flat_map(|p| p.episodes.iter().map(move |e| (p, e))).collect();
                all.sort_by(|a, b| b.1.published.cmp(&a.1.published));
                let list: Vec<(&str, &Show, &Episode)> = all.into_iter().take(25).map(|(p, e)| (p.show.feed.as_str(), &p.show, e)).collect();
                if let Some(a) = self.episode_list(ui, &list, w, true) {
                    act = Some(a);
                }
            }
        });
        if let Some(f) = subscribe {
            self.pod_err = None;
            self.refresh_pod(&f, true);
        }
        if let Some(f) = open {
            self.sel = Sel::Podcast(f);
        }
        if let Some((f, id, a)) = act {
            self.do_ep_act(&f, &id, a);
        }
    }

    /// One show's page with all its episodes.
    pub fn podcast_view(&mut self, ui: &mut egui::Ui, feed: &str) {
        if self.pod_top_bar(ui) || self.sel != Sel::Podcast(feed.to_string()) {
            return;
        }
        let Some(p) = self.pods.iter().find(|p| p.show.feed == feed).cloned() else {
            ui.add_space(40.0);
            ui.label(egui::RichText::new(tr("You are not subscribed to this show.")).color(muted()));
            return;
        };
        ui.add_space(6.0);
        let busy = self.pod_busy.contains(feed);
        let mut unsub = false;
        let mut refresh = false;
        let mut play_latest = false;
        let mut mark_all = false;
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(110.0, 110.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(16), card_hi());
            let uri = img_uri(ui.ctx(), &p.show.image);
            if !uri.is_empty() {
                egui::Image::new(uri).fit_to_exact_size(r.size()).corner_radius(CornerRadius::same(16)).paint_at(ui, r);
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&p.show.title).size(26.0).strong());
                    if busy {
                        ui.add(egui::Spinner::new().size(18.0).color(accent()));
                    }
                });
                let count = p.episodes.len().to_string();
                let summary = if p.episodes.len() == 1 {
                    trf("{author}  •  {count} episode", &[("author", &p.show.author), ("count", &count)])
                } else {
                    trf("{author}  •  {count} episodes", &[("author", &p.show.author), ("count", &count)])
                };
                ui.label(egui::RichText::new(summary).size(13.5).color(muted()));
                let d = truncate(ui, &p.show.description, FontId::proportional(13.5), (ui.available_width() - 20.0).max(240.0), 2);
                ui.label(egui::RichText::new(d).size(13.5).color(muted()));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    self.view_toggle(ui);
                    if icon_btn(ui, "play",&tr("Play latest"), Some(accent()), ON_ACCENT, false, true).clicked() {
                        play_latest = true;
                    }
                    if icon_btn(ui, "refresh",&tr("Refresh"), None, ink(), false, true).clicked() {
                        refresh = true;
                    }
                    if self.pod_new_count(&p) > 0 && icon_btn(ui, "check",&tr("Mark all as played"), None, ink(), false, true).clicked() {
                        mark_all = true;
                    }
                    let label = tr(if self.pod_unsub_confirm { "Confirm unsubscribe" } else { "Unsubscribe" });
                    let b = icon_btn(ui, "", &label, if self.pod_unsub_confirm { Some(LIVE) } else { None }, if self.pod_unsub_confirm { Color32::WHITE } else { ink() }, false, true);
                    if b.clicked() {
                        if self.pod_unsub_confirm {
                            unsub = true;
                        } else {
                            self.pod_unsub_confirm = true;
                        }
                    }
                });
            });
        });
        ui.add_space(10.0);
        let mut act: Option<(String, A)> = None;
        // a show can have hundreds of episodes: show a page at a time and add the next one
        // when the scroll gets near the end
        const PAGE: usize = 30;
        if self.pod_shown.0 != feed {
            self.pod_shown = (feed.to_string(), PAGE);
        }
        let shown = self.pod_shown.1.min(p.episodes.len());
        let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width() - 6.0;
            let list: Vec<(&str, &Show, &Episode)> = p.episodes.iter().take(shown).map(|e| (p.show.feed.as_str(), &p.show, e)).collect();
            if let Some((_, id, a)) = self.episode_list(ui, &list, w, false) {
                act = Some((id, a));
            }
            if shown < p.episodes.len() {
                ui.add_space(6.0);
                ui.vertical_centered(|ui| ui.label(egui::RichText::new(trf("Showing {shown} of {total} episodes", &[("shown", &shown.to_string()), ("total", &p.episodes.len().to_string())])).size(12.5).color(muted())));
                ui.add_space(20.0);
            }
        });
        let near_end = out.state.offset().y + out.inner_rect.height() > out.content_size.y - 500.0;
        if near_end && shown < p.episodes.len() {
            self.pod_shown.1 = shown + PAGE;
            ui.ctx().request_repaint();
        }
        if let Some((id, a)) = act {
            self.do_ep_act(feed, &id, a);
        }
        if refresh {
            self.refresh_pod(feed, false);
        }
        if play_latest {
            if let Some(e) = p.episodes.first() {
                self.do_ep_act(feed, &e.id.clone(), EpAct::Play);
            }
        }
        if mark_all {
            for e in &p.episodes {
                self.seen.insert(video_id(e));
            }
            self.seen_dirty = true;
        }
        if unsub {
            self.pod_unsub_confirm = false;
            self.unsubscribe_pod(feed);
        }
    }
}
