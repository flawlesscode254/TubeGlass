//! The player page: video stage, audio visualizers, controls, mini bar and picture-in-picture.

use crate::*;

impl App {
    /// Draws the player page: the picture or audio visualizer, captions and the control bars.
    pub(crate) fn player_view(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let full = ui.available_rect_before_wrap();
        if !self.amb_active {
            ui.painter().rect_filled(full, 0.0, Color32::BLACK);
        }
        let resp = ui.allocate_rect(full, Sense::click());

        // activity tracking for auto-hiding controls
        // a menu or dialog opened from the controls sits on top of the picture, so the picture
        // itself is no longer "hovered"; mouse activity anywhere over the player still counts
        let popup_open = egui::Popup::is_any_open(&ctx) || self.new_list.is_some() || self.new_group.is_some() || self.show_help;
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.any_down())
            && (resp.hovered() || ctx.pointer_hover_pos().is_some_and(|p| full.contains(p)))
        {
            self.last_move = Instant::now();
        }
        if popup_open {
            self.last_move = Instant::now();
        }
        let state = self.player.state();
        // Like YouTube: the title bar and the controls show while the pointer moves over the
        // picture (or while paused / loading) and fade away when it rests or leaves.
        let idle = self.last_move.elapsed();
        let inside = ctx.pointer_hover_pos().is_some_and(|p| full.contains(p));
        let recent = idle < Duration::from_millis(2500) && (inside || idle < Duration::from_millis(700));
        let show_ui = recent || popup_open || self.player.paused || !matches!(state, State::Playing | State::Buffering);
        let t = ctx.animate_bool_with_time(uid("ctl"), show_ui, 0.2);
        if !show_ui && inside && t < 0.05 {
            // the pointer hides along with the controls
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        if !show_ui || t < 1.0 {
            // keep frames coming so the fade finishes even when nothing else changes
            ctx.request_repaint_after(Duration::from_millis(40));
        }

        // video
        // the glow behind a white interface would wash it out, so it is a dark-theme feature
        let ambient = self.settings.ambient && !LIGHT.load(std::sync::atomic::Ordering::Relaxed) && !self.player.audio_only() && self.player.texture().is_some();
        self.player.ambient_on = self.settings.ambient;
        let aspect = self.player.aspect();
        self.amb_active = ambient;
        let stage = full;
        let vr = if stage.aspect_ratio() > aspect {
            Rect::from_center_size(stage.center(), vec2(stage.height() * aspect, stage.height()))
        } else {
            Rect::from_center_size(stage.center(), vec2(stage.width(), stage.width() / aspect))
        };
        if ambient {
            self.paint_ambient(ui, vr);
        }
        if self.player.audio_only() {
            // levels follow the audio that is really being played; they fall away when paused
            let (samples, rate) = self.player.viz_samples(1024);
            let live_audio = state == State::Playing;
            let target = if live_audio { spectrum(&samples, rate, self.viz_levels.len()) } else { vec![0.0; self.viz_levels.len()] };
            for (lvl, tg) in self.viz_levels.iter_mut().zip(target) {
                *lvl = if tg > *lvl { *lvl * 0.45 + tg * 0.55 } else { *lvl * 0.86 };
            }
            if self.viz_levels.iter().any(|l| *l > 0.003) {
                ui.ctx().request_repaint();
            }

            let is_pod = self.watching.as_ref().is_some_and(|w| w.id.starts_with("pod:"));
            if is_pod {
                // spoken word gets its own look: a round cover ringed by a rotating halo
                self.pod_visual(ui, full);
            } else {
            let w = (full.width() - 120.0).clamp(240.0, 900.0);
            // with synced lyrics available the artwork and bars move up and shrink to leave a
            // lyrics strip below them
            self.ensure_lyrics();
            let lyr_lines = self.current_lyric_window();
            let compact = lyr_lines.is_some() && full.height() >= 460.0;
            // everything scales with the room above the controls
            let usable = (full.height() - 110.0).max(300.0);
            let sc = (usable / 560.0).clamp(1.0, 1.7);
            // lyrics block geometry first, so the art and bars can be spaced evenly above it
            let lyr_spec = |k: usize, role: u8| -> (f32, Color32) {
                match role {
                    0 => (11.5 * sc, ink().gamma_multiply(0.45)),
                    1 => (18.0 * sc, accent()),
                    _ => (12.5 * sc, ink().gamma_multiply((0.72 - 0.12 * (k as f32 - 2.0).max(0.0)).max(0.18))),
                }
            };
            let lyr_gap = 9.0 * sc;
            let lyr_total: f32 = lyr_lines
                .as_ref()
                .map(|l| l.iter().map(|(k, _, r)| lyr_spec(*k, *r).0 + lyr_gap).sum::<f32>() - lyr_gap)
                .unwrap_or(0.0);
            let lyr_top = full.max.y - 100.0 - 30.0 * sc - lyr_total;
            // art, bars and lyrics sit one above the other with equal gaps (also above the art)
            let art_h = 90.0 * sc;
            let bars_h = (usable * 0.26).clamp(150.0, 300.0);
            let visible_bars = bars_h - 14.0;
            let y_start = full.min.y + 62.0;
            let even = ((lyr_top - y_start - art_h - visible_bars) / 3.0).max(12.0);
            let art_cy = y_start + even + art_h / 2.0;
            let bars_top = y_start + even + art_h + even;
            let area = if compact {
                Rect::from_min_size(Pos2::new(full.center().x - w / 2.0, bars_top - 10.0), vec2(w, bars_h))
            } else {
                Rect::from_center_size(full.center() + vec2(0.0, 70.0), vec2(w, 230.0))
            };
            let energy = self.viz_levels.iter().sum::<f32>() / self.viz_levels.len() as f32;
            let n = self.viz_levels.len();
            let slot = area.width() / n as f32;
            let base = area.bottom() - 40.0;
            let max_h = area.height() - 50.0;
            for (i, lvl) in self.viz_levels.iter().enumerate() {
                let f = i as f32 / (n - 1) as f32;
                let col = Color32::from_rgb(
                    (accent().r() as f32 + (accent2().r() as f32 - accent().r() as f32) * f) as u8,
                    (accent().g() as f32 + (accent2().g() as f32 - accent().g() as f32) * f) as u8,
                    (accent().b() as f32 + (accent2().b() as f32 - accent().b() as f32) * f) as u8,
                );
                let h = (lvl.powf(1.1) * max_h).max(3.0);
                let x = area.left() + i as f32 * slot + slot * 0.15;
                let bar = Rect::from_min_size(Pos2::new(x, base - h), vec2(slot * 0.7, h));
                ui.painter().rect_filled(bar, CornerRadius::same(3), col);
                // reflection
                let rh = (h * 0.35).min(36.0);
                let refl = Rect::from_min_size(Pos2::new(x, base + 3.0), vec2(slot * 0.7, rh));
                vgradient(ui.painter(), refl, col.gamma_multiply(0.30), Color32::TRANSPARENT);
            }
            // cover art when there is one; otherwise the app icon, gently pulsing with the music
            let art_uri = self
                .watching
                .as_ref()
                .map(|w| {
                    if crate::platform::is_local_path(&w.thumb) {
                        crate::platform::file_uri(&w.thumb)
                    } else if w.thumb.is_empty() {
                        String::new()
                    } else {
                        img_uri(ui.ctx(), &w.thumb)
                    }
                })
                .unwrap_or_default();
            let art_center = Pos2::new(full.center().x, if compact { art_cy } else { area.top() - 74.0 });
            if !art_uri.is_empty() {
                let art = Rect::from_center_size(art_center, if compact { vec2(160.0, 90.0) * sc } else { vec2(224.0, 126.0) });
                ui.painter().rect_filled(art, CornerRadius::same(16), card());
                egui::Image::new(art_uri)
                    .fit_to_exact_size(art.size())
                    .corner_radius(CornerRadius::same(16))
                    .paint_at(ui, art);
            } else {
                let size = (if compact { 88.0 * sc } else { 124.0 }) * (1.0 + 0.07 * energy.min(0.7));
                let icon = Rect::from_center_size(art_center, vec2(size, size));
                ui.painter().image(
                    self.logo.id(),
                    icon,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            if compact {
                // previous / current / next lines, centred and anchored just above the controls
                if let Some(lines) = lyr_lines {
                    let max_w = w.min(full.width() - 60.0);
                    let mut y = lyr_top;
                    for (k, text, role) in lines {
                        let (size, col) = lyr_spec(k, role);
                        let shown = truncate(ui, &text, FontId::proportional(size), max_w, 1);
                        ui.painter().text(Pos2::new(full.center().x, y), Align2::CENTER_TOP, shown, FontId::proportional(size), col);
                        y += size + lyr_gap;
                    }
                }
            } else {
                icon_label(ui, area.center_bottom() + vec2(0.0, 6.0), "note", "Audio", muted(), false);
            }
            }
        }
        if let Some(tex) = self.player.texture() {
            ui.painter().image(
                tex.id(),
                vr,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // captions
        if self.cc_track.is_some() && !self.cues.is_empty() {
            let pos = self.player.position();
            let idx = self.cues.partition_point(|c| c.start <= pos);
            if idx > 0 {
                let cue = &self.cues[idx - 1];
                if pos < cue.end {
                    let size = (vr.width() / 38.0).clamp(15.0, 30.0);
                    let galley = ui.fonts_mut(|f| {
                        f.layout(cue.text.clone(), FontId::proportional(size), Color32::WHITE, vr.width() * 0.72)
                    });
                    let bottom = full.max.y - if t > 0.05 { 124.0 } else { 40.0 };
                    let r = Rect::from_center_size(
                        Pos2::new(vr.center().x, bottom - galley.size().y / 2.0 - 8.0),
                        galley.size() + vec2(24.0, 14.0),
                    );
                    ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_black_alpha(185));
                    ui.painter().galley(r.min + vec2(12.0, 7.0), galley, Color32::WHITE);
                }
            }
        }

        // status overlay
        let center = full.center();
        match &state {
            State::Loading | State::Buffering | State::Idle => {
                ui.painter().rect_filled(Rect::from_center_size(center, vec2(150.0, 58.0)), CornerRadius::same(16), Color32::from_black_alpha(150));
                let sp = Rect::from_center_size(center - vec2(38.0, 0.0), vec2(24.0, 24.0));
                ui.put(sp, egui::Spinner::new().size(24.0).color(accent()));
                ui.painter().text(center + vec2(14.0, 0.0), Align2::LEFT_CENTER,
                    if state == State::Buffering { "Buffering" } else { "Loading" },
                    FontId::proportional(16.0), ink());
            }
            State::Error(e) => {
                let r = Rect::from_center_size(center, vec2(460.0_f32.min(full.width() - 40.0), 150.0));
                ui.painter().rect_filled(r, CornerRadius::same(16), Color32::from_black_alpha(200));
                ui.painter().text(r.center_top() + vec2(0.0, 28.0), Align2::CENTER_CENTER, "Can't play this video", FontId::proportional(20.0), LIVE);
                let shown = if e.contains("opening input") || e.contains("Input/output error") {
                    let d = proxy::last_error();
                    format!("YouTube did not hand over this video{}.", if d.is_empty() { String::new() } else { format!(" ({d})") })
                } else {
                    e.clone()
                };
                let msg = truncate(ui, &shown, FontId::proportional(14.0), r.width() - 40.0, 3);
                ui.painter().text(r.center() + vec2(0.0, 6.0), Align2::CENTER_CENTER, msg, FontId::proportional(14.0), muted());
                // a button to try again through the other clients
                let br = Rect::from_center_size(r.center() + vec2(0.0, 52.0), vec2(120.0, 32.0));
                let bresp = ui.interact(br, uid("err-retry"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(br, CornerRadius::same(16), if bresp.hovered() { accent2() } else { accent() });
                ui.painter().text(br.center(), Align2::CENTER_CENTER, "Try again", FontId::proportional(14.0), ON_ACCENT);
                if bresp.clicked() {
                    self.want_retry = true;
                }
            }
            State::Ended => {
                icons::draw(ui.painter(), center + vec2(-56.0, 0.0), "refresh", 30.0, ink());
                ui.painter().text(center + vec2(-34.0, 0.0), Align2::LEFT_CENTER, "Replay", FontId::proportional(28.0), ink());
            }
            _ => {}
        }

        // click handling on the video surface
        // Click the picture (or the artwork / bars for audio) to pause or resume, like Space.
        // A double click goes fullscreen; its two clicks cancel each other out.
        let mut toggle = false;
        if resp.clicked() {
            toggle = true;
        }
        if resp.double_clicked() {
            self.toggle_fullscreen();
        }

        // overlay controls
        let mut close = false;
        let mut toggle_cm = false;
        let mut toggle_lyrics = false;
        let mut toggle_transcript = false;
        let mut toggle_ambient = false;
        let mut open_channel: Option<Video> = None;
        let mut set_cc: Option<Option<usize>> = None;
        let mut translate: Option<(&'static str, &'static str)> = None;
        let mut open_clip = false;
        let mut skip_prev = false;
        let mut skip_next = false;
        let mut open_queue = false;
        let mut new_speed: Option<f32> = None;
        let mut new_repeat: Option<Repeat> = None;
        let mut toggle_shuffle = false;
        let mut set_sleep: Option<Option<SleepMode>> = None;
        let mut show_help = false;
        let mut toggle_audio = false;
        let mut start_dl: Option<(Video, bool, u32)> = None;
        let mut list_toggle: Option<(String, Video)> = None;
        let mut new_list_for: Option<Video> = None;
        if t > 0.01 {
            let a = (t * 255.0) as u8;
            let _ = a;
            // plain dark scrim under the title, and a flat full-width control bar
            let top = Rect::from_min_size(full.min, vec2(full.width(), 84.0));
            vgradient(ui.painter(), top, Color32::from_black_alpha((t * 170.0) as u8), Color32::TRANSPARENT);
            let bar = Rect::from_min_max(Pos2::new(full.min.x, full.max.y - 100.0), full.max);
            let bar_a = if ambient { 150.0 } else { 242.0 };
            ui.painter().rect_filled(bar, 0.0, Color32::from_rgba_unmultiplied(13, 20, 23, (t * bar_a) as u8));
            ui.painter().line_segment(
                [bar.left_top(), bar.right_top()],
                Stroke::new(1.0, ov((t * 20.0) as u8)),
            );
            // top bar
            let back = Rect::from_min_size(full.min + vec2(18.0, 18.0), vec2(40.0, 40.0));
            let br = ui.interact(back, uid("back"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().circle_filled(back.center(), 20.0, if br.hovered() { ov((60.0 * t) as u8) } else { Color32::from_black_alpha((120.0 * t) as u8) });
            icons::draw(ui.painter(), back.center(), "arrow_left", 21.0, ink().gamma_multiply(t));
            close |= br.clicked();
            if let Some(w) = &self.watching {
                let title = truncate(ui, &self.player.streams.as_ref().map_or(w.title.clone(), |s| s.title.clone()), FontId::proportional(18.0), full.width() - 220.0, 1);
                ui.painter().text(back.right_center() + vec2(14.0, -9.0), Align2::LEFT_CENTER, title, FontId::proportional(18.0), ink().gamma_multiply(t));
                let cur_ch = self.player.streams.as_ref().and_then(|st| {
                    let p = self.player.position();
                    st.chapters.iter().rev().find(|c| c.start <= p).map(|c| c.title.clone())
                });
                let sub = match cur_ch {
                    Some(c) => format!("{}  •  {}", w.channel_name, c),
                    None => w.channel_name.clone(),
                };
                let sub = truncate(ui, &sub, FontId::proportional(13.0), full.width() - 220.0, 1);
                let sub_pos = back.right_center() + vec2(14.0, 11.0);
                let sub_rect = ui.painter().text(sub_pos, Align2::LEFT_CENTER, sub, FontId::proportional(13.0), muted().gamma_multiply(t));
                // the channel name part (before the chapter) is a link to the channel page
                if !w.channel_name.is_empty() && (!non_yt(&w.id) || w.id.starts_with("pod:")) {
                    let name_w = ui.fonts_mut(|f| {
                        f.layout_no_wrap(w.channel_name.clone(), FontId::proportional(13.0), ink()).size().x
                    });
                    let hit = Rect::from_min_size(
                        Pos2::new(sub_rect.min.x - 3.0, sub_rect.min.y - 3.0),
                        vec2(name_w.min(sub_rect.width()) + 6.0, sub_rect.height() + 6.0),
                    );
                    let resp = ui
                        .interact(hit, uid("open-channel"), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.hovered() {
                        ui.painter().rect_filled(hit, CornerRadius::same(6), ov((28.0 * t) as u8));
                        ui.painter().text(sub_pos, Align2::LEFT_CENTER, &w.channel_name, FontId::proportional(13.0), accent().gamma_multiply(t));
                    }
                    if resp.clicked() {
                        open_channel = Some(w.clone());
                    }
                }
            }

            // bottom: seek bar
            let live = self.player.is_live();
            let pos = self.player.position();
            let dur = self.player.duration();
            let seek = Rect::from_min_size(Pos2::new(full.min.x + 24.0, full.max.y - 88.0), vec2(full.width() - 48.0, 16.0));
            if live {
                // red "LIVE" while in step with the broadcast, grey "N s behind" once playback has fallen back
                let behind = self.player.live_behind();
                let late = behind >= 4.0;
                let label = if late { {
                    let b = behind.round() as u64;
                    if b < 60 { format!("● {b}s behind") } else { format!("● {}m {:02}s behind", b / 60, b % 60) }
                } } else { "● LIVE".to_string() };
                let bw = if late { 116.0 } else { 58.0 };
                let c = Rect::from_min_size(seek.left_center() - vec2(0.0, 11.0), vec2(bw, 22.0));
                let fill = if late { ov(60) } else { LIVE };
                ui.painter().rect_filled(c, CornerRadius::same(6), fill.gamma_multiply(t));
                ui.painter().text(c.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.0), Color32::WHITE.gamma_multiply(t));
                let mut x = c.right() + 10.0;
                if late {
                    let b = Rect::from_min_size(Pos2::new(x, c.min.y), vec2(76.0, 22.0));
                    let r = ui.interact(b, uid("golive"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Jump to the live broadcast");
                    ui.painter().rect_filled(b, CornerRadius::same(11), if r.hovered() { accent2() } else { accent() }.gamma_multiply(t));
                    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Go live", FontId::proportional(12.5), ON_ACCENT.gamma_multiply(t));
                    if r.clicked() {
                        self.player.go_live();
                    }
                    x = b.right() + 12.0;
                }
                ui.painter().text(Pos2::new(x, c.center().y), Align2::LEFT_CENTER, format!("watching for {}", fmt_time(pos)), FontId::proportional(13.0), muted().gamma_multiply(t));
            } else {
                let id = uid("seek");
                let sr = ui
                    .interact(seek.expand2(vec2(0.0, 10.0)), id, Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let hov = ctx.animate_bool_with_time(id.with("h"), sr.hovered() || sr.dragged(), 0.12);
                let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
                let (chapters, storyboard) = self
                    .player
                    .streams
                    .as_ref()
                    .map(|st| (st.chapters.clone(), st.storyboard.clone()))
                    .unwrap_or_default();
                let hover_t: Option<f64> = sr
                    .hover_pos()
                    .map(|p| ((p.x - seek.min.x) / seek.width()).clamp(0.0, 1.0) as f64 * dur);
                // chapter boundaries: one segment per chapter, or a single bar without any
                let bounds: Vec<(f64, f64)> = if chapters.is_empty() || dur <= 0.0 {
                    vec![(0.0, dur.max(1.0))]
                } else {
                    chapters
                        .iter()
                        .enumerate()
                        .map(|(i, c)| (c.start, chapters.get(i + 1).map_or(dur, |n| n.start)))
                        .collect()
                };
                let seg_hover = hover_t.and_then(|ht| bounds.iter().position(|(a, b)| ht >= *a && ht < *b));
                let total = dur.max(1.0);
                let cy = seek.center().y;
                let played_x = seek.min.x + seek.width() * frac;
                let base_h = 4.0 + 2.0 * hov;
                for (i, (a, b)) in bounds.iter().enumerate() {
                    let gap = if bounds.len() > 1 { 1.5 } else { 0.0 };
                    let x0 = seek.min.x + seek.width() * (*a / total) as f32 + if i > 0 { gap } else { 0.0 };
                    let x1 = seek.min.x + seek.width() * (*b / total) as f32 - if i + 1 < bounds.len() { gap } else { 0.0 };
                    let h = if seg_hover == Some(i) && bounds.len() > 1 { base_h + 3.0 } else { base_h };
                    let seg = Rect::from_min_max(Pos2::new(x0, cy - h / 2.0), Pos2::new(x1.max(x0 + 1.0), cy + h / 2.0));
                    ui.painter().rect_filled(seg, CornerRadius::same(3), ov((70.0 * t) as u8));
                    if played_x > x0 {
                        let fx = played_x.min(x1);
                        let fill = Rect::from_min_max(seg.min, Pos2::new(fx.max(x0 + 1.0), seg.max.y));
                        let clip = ui.painter().with_clip_rect(fill);
                        hgradient(&clip, Rect::from_min_max(Pos2::new(seek.min.x, seg.min.y), Pos2::new(seek.max.x, seg.max.y)), accent(), accent2());
                    }
                }
                // SponsorBlock segments
                for sg in &self.sponsor {
                    if dur <= 0.0 {
                        break;
                    }
                    let x0 = seek.min.x + seek.width() * (sg.start / total) as f32;
                    let x1 = seek.min.x + seek.width() * (sg.end / total) as f32;
                    let col = match sg.category.as_str() {
                        "sponsor" => Color32::from_rgb(0, 212, 0),
                        "selfpromo" => Color32::from_rgb(255, 255, 0),
                        "interaction" => Color32::from_rgb(204, 0, 255),
                        "intro" => Color32::from_rgb(0, 255, 255),
                        _ => Color32::from_rgb(2, 66, 138),
                    };
                    let r = Rect::from_min_max(Pos2::new(x0, cy - base_h / 2.0), Pos2::new(x1.max(x0 + 2.0), cy + base_h / 2.0));
                    ui.painter().rect_filled(r, CornerRadius::same(2), col.gamma_multiply(0.85 * t));
                }
                // sections that have been saved as clips
                let src_id = self.watching.as_ref().map(|w| w.id.clone()).unwrap_or_default();
                for c in self.clips.iter().filter(|c| c.video.id == src_id) {
                    let x0 = seek.min.x + seek.width() * (c.start / total) as f32;
                    let x1 = seek.min.x + seek.width() * (c.end / total) as f32;
                    ui.painter().rect_filled(
                        Rect::from_min_max(Pos2::new(x0, cy - 12.0), Pos2::new(x1.max(x0 + 3.0), cy - 8.0)),
                        CornerRadius::same(2),
                        TEAL.gamma_multiply(t),
                    );
                }
                // notes / bookmarks
                if let Some(list) = self.watching.as_ref().and_then(|w| self.bookmarks.get(&w.id)) {
                    for b in list {
                        let x = seek.min.x + seek.width() * (b.t / total) as f32;
                        ui.painter().circle_filled(Pos2::new(x, cy - 9.0), 3.2 * t, accent());
                    }
                }
                // range being clipped
                if self.side_tab == SideTab::Clip && self.comments_open && self.clip.vid == self.watching.as_ref().map_or("", |w| w.id.as_str()) {
                    let x0 = seek.min.x + seek.width() * (self.clip.start / total) as f32;
                    let x1 = seek.min.x + seek.width() * (self.clip.end / total) as f32;
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(x0, cy - 6.0), Pos2::new(x1.max(x0 + 2.0), cy + 6.0)), CornerRadius::same(3), accent().gamma_multiply(0.35 * t));
                }
                ui.painter().circle_filled(Pos2::new(played_x, cy), 6.5 * (0.45 + 0.55 * hov) * t, Color32::WHITE);

                // hover card: preview frame + time + chapter label
                if let (Some(hp), Some(ht)) = (sr.hover_pos(), hover_t) {
                    let chapter = chapters
                        .iter()
                        .rev()
                        .find(|c| c.start <= ht + 0.001)
                        .map(|c| c.title.clone());
                    let frame = storyboard.as_ref().and_then(|sb| sb.frame_at(ht, dur));
                    let thumb_w = 200.0;
                    let thumb_h = storyboard
                        .as_ref()
                        .map(|sb| thumb_w * sb.height as f32 / sb.width.max(1) as f32)
                        .unwrap_or(0.0)
                        .clamp(0.0, 130.0);
                    let in_clip = self.clips.iter().find(|c| {
                        self.watching.as_ref().is_some_and(|w| w.id == c.video.id) && ht >= c.start && ht <= c.end
                    });
                    let chapter = match (chapter, in_clip) {
                        (Some(ch), Some(c)) => Some(format!("{ch}  •  clip {}–{}", fmt_time(c.start), fmt_time(c.end))),
                        (None, Some(c)) => Some(format!("Clip {}–{}", fmt_time(c.start), fmt_time(c.end))),
                        (ch, None) => ch,
                    };
                    let label = chapter
                        .as_ref()
                        .map(|c| truncate(ui, c, FontId::proportional(13.5), thumb_w + 8.0, 2));
                    let label_h = label.as_ref().map_or(0.0, |l| 18.0 * (l.lines().count().max(1) as f32));
                    let card_w = thumb_w + 20.0;
                    let card_h = 12.0
                        + if thumb_h > 0.0 { thumb_h + 8.0 } else { 0.0 }
                        + if label.is_some() { label_h + 4.0 } else { 0.0 }
                        + 20.0;
                    let cx = hp.x.clamp(seek.min.x + card_w / 2.0, seek.max.x - card_w / 2.0);
                    let card = Rect::from_min_size(Pos2::new(cx - card_w / 2.0, seek.min.y - 18.0 - card_h), vec2(card_w, card_h));
                    ui.painter().add(surface::panel(card, 14.0, 1.0, None, false));
                    let mut y0 = card.min.y + 10.0;
                    if thumb_h > 0.0 {
                        let tr = Rect::from_min_size(Pos2::new(card.min.x + 10.0, y0), vec2(thumb_w, thumb_h));
                        ui.painter().rect_filled(tr, CornerRadius::same(8), ov(14));
                        if let Some((url, uv)) = &frame {
                            let uri = img_uri(&ctx, url);
                            if !uri.is_empty() {
                                egui::Image::new(uri)
                                    .uv(Rect::from_min_max(Pos2::new(uv[0], uv[1]), Pos2::new(uv[2], uv[3])))
                                    .fit_to_exact_size(tr.size())
                                    .corner_radius(CornerRadius::same(8))
                                    .paint_at(ui, tr);
                            }
                        }
                        y0 += thumb_h + 8.0;
                    }
                    if let Some(l) = &label {
                        ui.painter().text(Pos2::new(card.min.x + 12.0, y0), Align2::LEFT_TOP, l, FontId::proportional(13.5), ink());
                        y0 += label_h + 4.0;
                    }
                    ui.painter().text(Pos2::new(card.min.x + 12.0, y0), Align2::LEFT_TOP, fmt_time(ht), FontId::proportional(13.0), accent());
                    // marker on the bar
                    ui.painter().line_segment(
                        [Pos2::new(hp.x.clamp(seek.min.x, seek.max.x), cy - 7.0), Pos2::new(hp.x.clamp(seek.min.x, seek.max.x), cy + 7.0)],
                        Stroke::new(1.5, ov(170)),
                    );
                }
                if sr.clicked() || sr.drag_stopped() {
                    if let Some(p) = sr.interact_pointer_pos() {
                        let f = ((p.x - seek.min.x) / seek.width()).clamp(0.0, 1.0);
                        self.player.seek(f as f64 * dur);
                    }
                }
            }

            // bottom: buttons
            let y = full.max.y - 36.0;
            let pp = Rect::from_center_size(Pos2::new(full.min.x + 44.0, y), vec2(40.0, 40.0));
            let pr = ui.interact(pp, uid("pp"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let col = if pr.hovered() { accent2() } else { Color32::WHITE }.gamma_multiply(t);
            let c = pp.center();
            if self.player.paused || state == State::Ended {
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![c + vec2(-7.0, -10.0), c + vec2(-7.0, 10.0), c + vec2(11.0, 0.0)], col, Stroke::NONE));
            } else {
                for dx in [-7.0, 2.0] {
                    ui.painter().rect_filled(Rect::from_min_size(c + vec2(dx, -10.0), vec2(5.0, 20.0)), CornerRadius::same(2), col);
                }
            }
            if pr.clicked() {
                toggle = true;
            }

            let mut x = pp.right() + 6.0;
            if self.queue.len() > 1 {
                for (icon, id, prev) in [("skip_prev", "qprev", true), ("skip_next", "qnext", false)] {
                    let br = Rect::from_center_size(Pos2::new(x + 16.0, y), vec2(32.0, 32.0));
                    let bresp = ui.interact(br, uid(id), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    let can = prev || self.queue_idx.is_some_and(|i| i + 1 < self.queue.len()) || self.settings.repeat == Repeat::All;
                    let col = if !can { muted().gamma_multiply(0.5) } else if bresp.hovered() { accent2() } else { ink() };
                    icons::draw(ui.painter(), br.center(), icon, 18.0, col.gamma_multiply(t));
                    if bresp.clicked() && can {
                        if prev { skip_prev = true } else { skip_next = true }
                    }
                    x += 34.0;
                }
            }
            x += 8.0;
            let ts = if live { String::new() } else { format!("{} / {}", fmt_time(pos), fmt_time(dur)) };
            let g = ui.painter().text(Pos2::new(x, y), Align2::LEFT_CENTER, ts, FontId::proportional(14.0), ink().gamma_multiply(t));
            x = g.right() + 24.0;

            // volume
            let spk = Rect::from_center_size(Pos2::new(x + 12.0, y), vec2(28.0, 28.0));
            let sr = ui.interact(spk, uid("mute"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            icons::draw(
                ui.painter(),
                spk.center(),
                if self.player.muted || self.player.volume == 0.0 { "speaker_mute" } else { "speaker" },
                21.0,
                ink().gamma_multiply(t),
            );
            if sr.clicked() {
                self.player.toggle_mute();
            }
            let vs = Rect::from_min_size(Pos2::new(spk.right() + 8.0, y - 8.0), vec2(90.0, 16.0));
            // Pills that would run into the time / volume controls are tucked away.
            let left_limit = vs.right() + 18.0;
            let vr2 = ui.interact(vs, uid("vol"), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if (vr2.dragged() || vr2.clicked()) && vr2.interact_pointer_pos().is_some() {
                let f = ((vr2.interact_pointer_pos().unwrap().x - vs.min.x) / vs.width()).clamp(0.0, 1.0);
                self.player.muted = false;
                self.player.set_volume(f);
            }
            let vb = Rect::from_center_size(vs.center(), vec2(vs.width(), 4.0));
            ui.painter().rect_filled(vb, CornerRadius::same(2), ov((70.0 * t) as u8));
            let vf = if self.player.muted { 0.0 } else { self.player.volume };
            ui.painter().rect_filled(Rect::from_min_size(vb.min, vec2(vb.width() * vf, 4.0)), CornerRadius::same(2), Color32::WHITE.gamma_multiply(t));
            ui.painter().circle_filled(Pos2::new(vb.min.x + vb.width() * vf, vb.center().y), 5.0, Color32::WHITE.gamma_multiply(t));

            // right: fullscreen, then pills laid out right-to-left with equal gaps
            let fs = Rect::from_center_size(Pos2::new(full.max.x - 44.0, y), vec2(40.0, 40.0));
            let fr = ui.interact(fs, uid("fs"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            icons::draw(
                ui.painter(),
                fs.center(),
                if self.fullscreen { "fullscreen_exit" } else { "fullscreen" },
                22.0,
                if fr.hovered() { accent2() } else { Color32::WHITE }.gamma_multiply(t),
            );
            if fr.clicked() {
                self.toggle_fullscreen();
            }

            let is_local = self.watching.as_ref().is_some_and(|w| non_yt(&w.id));
            // Which pills exist, in priority order (most important first). They are laid
            // out right-to-left; if everything does not fit, the pills turn icon-only, and
            // whatever still does not fit is dropped from the end of this list.
            let is_pod = self.watching.as_ref().is_some_and(|w| w.id.starts_with("pod:"));
            // YouTube videos and podcast episodes both can be saved to a playlist and downloaded
            let has_yt = self.watching.as_ref().is_some_and(|w| !non_yt(&w.id) || w.id.starts_with("pod:"));
            let ctracks_n = self.player.streams.as_ref().map_or(0, |s| s.captions.len());
            // (name, full width, compact width)
            let mut wanted: Vec<(&str, f32, f32)> = vec![];
            if !self.player.qualities().is_empty() {
                wanted.push(("quality", 70.0, 62.0));
            }
            wanted.push(("audio", 88.0, 88.0));
            if has_yt && !live {
                wanted.push(("download", 116.0, 38.0));
            }
            if has_yt {
                wanted.push(("playlist", 112.0, 38.0));
            }
            if !live {
                wanted.push(("speed", 66.0, 58.0));
            }
            // a podcast has no comments or lyrics; its transcript shows up as captions (CC)
            if !is_local {
                wanted.push(("comments", 104.0, 38.0));
            }
            if self.queue.len() > 1 {
                wanted.push(("queue", 96.0, 46.0));
            }
            if ctracks_n > 0 && (!is_local || is_pod) {
                wanted.push(("cc", 52.0, 38.0));
            }
            wanted.push(("more", 46.0, 38.0));
            let avail = (fs.min.x - 6.0) - left_limit;
            let total = |compact: bool| -> f32 {
                wanted.iter().map(|(_, f, c)| if compact { *c } else { *f } + 10.0).sum::<f32>()
            };
            let compact = total(false) > avail;
            let mut slots: std::collections::HashMap<&str, Rect> = std::collections::HashMap::new();
            {
                // lay out from the right in priority-reversed order so the most important
                // pills end up closest to the fullscreen button, dropping the least important
                let mut x = fs.min.x - 6.0;
                let mut used = 0.0;
                for (name, full_w, comp_w) in wanted.iter() {
                    let w = if compact { *comp_w } else { *full_w };
                    if used + w > avail {
                        continue;
                    }
                    used += w + 10.0;
                    slots.insert(*name, Rect::from_center_size(Pos2::new(x - w / 2.0, y), vec2(w, 30.0)));
                    x -= w + 10.0;
                }
            }
            let slot = |name: &str| -> Rect {
                slots
                    .get(name)
                    .copied()
                    .unwrap_or_else(|| Rect::from_min_size(Pos2::new(-10000.0, -10000.0), vec2(1.0, 1.0)))
            };
            let pill = |ui: &mut egui::Ui, r: Rect, id: &str, icon: &str, label: &str, chevron: bool, fill: Color32, t: f32| {
                let resp = ui
                    .interact(r, uid(id), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let base = if resp.hovered() { fill.gamma_multiply(1.5) } else { fill };
                ui.painter().rect_filled(r, CornerRadius::same(8), base.gamma_multiply(t));
                let txt = if fill == accent() { ON_ACCENT } else { ink() };
                if compact && !icon.is_empty() && label != "Audio" {
                    icon_label(ui, r.center(), icon, "", txt.gamma_multiply(t), false);
                    resp.on_hover_text(label)
                } else {
                    icon_label(ui, r.center(), icon, label, txt.gamma_multiply(t), chevron);
                    resp
                }
            };
            let idle = ov(25);

            // quality
            let qs = self.player.qualities();
            if !qs.is_empty() {
                let cur = self.player.streams.as_ref().and_then(|s| {
                    s.video.iter().filter(|v| v.height <= self.player.quality).map(|v| v.height).max()
                        .or_else(|| s.video.iter().map(|v| v.height).min())
                }).unwrap_or(0);
                let qr = slot("quality");
                let qresp = pill(ui, qr, "q", "", &format!("{cur}p"), false, idle, t);
                egui::Popup::menu(&qresp).show(|ui| {
                    ui.set_min_width(80.0);
                    for q in qs.iter().rev() {
                        if ui.selectable_label(*q == cur, format!("{q}p")).clicked() {
                            self.player.set_quality(*q);
                        }
                    }
                });
            }
            // speed
            if !live {
                let spr = slot("speed");
                let sp = self.player.speed;
                let label = if (sp - sp.round()).abs() < 0.01 { format!("{}×", sp.round() as i32) } else { format!("{sp}×") };
                let spresp = pill(ui, spr, "speed", "", &label, false, if (sp - 1.0).abs() > 0.01 { accent() } else { idle }, t);
                egui::Popup::menu(&spresp).show(|ui| {
                    ui.set_min_width(110.0);
                    for v in [0.5f32, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0] {
                        let name = if v == 1.0 { "Normal".to_string() } else { format!("{v}×") };
                        if ui.selectable_label((sp - v).abs() < 0.01, name).clicked() {
                            new_speed = Some(v);
                            ui.close();
                        }
                    }
                });
            }
            // audio-only: kept near the right edge so it survives on narrow windows
            {
                let ar = slot("audio");
                let on = self.player.audio_mode;
                let aresp = pill(ui, ar, "audiomode", "note", "Audio", false, if on { accent() } else { idle }, t)
                    .on_hover_text("Play audio only (A)");
                if aresp.clicked() {
                    toggle_audio = true;
                }
            }
            // captions
            let ctracks: Vec<yt::CaptionTrack> = self.player.streams.as_ref().map(|s| s.captions.clone()).unwrap_or_default();
            if !ctracks.is_empty() && (!is_local || is_pod) {
                let ccr = slot("cc");
                let on = self.cc_track.is_some();
                let ccresp = pill(ui, ccr, "ccpill", "cc", "", false, if on { accent() } else { idle }, t);
                egui::Popup::menu(&ccresp).show(|ui| {
                    ui.set_min_width(200.0);
                    if ui.selectable_label(self.cc_track.is_none(), "Off").clicked() {
                        set_cc = Some(None);
                        ui.close();
                    }
                    for (i, tr) in ctracks.iter().enumerate() {
                        if ui.selectable_label(self.cc_track == Some(i), &tr.name).clicked() {
                            set_cc = Some(Some(i));
                            ui.close();
                        }
                    }
                    if is_pod {
                        ui.separator();
                        if ui.button(tr("Open the full transcript")).clicked() {
                            toggle_transcript = true;
                            ui.close();
                        }
                    } else if !is_local_file(&ctracks) {
                        ui.separator();
                        ui.menu_button("Translate to…", |ui| {
                            for (code, name) in TRANSLATE_TO {
                                if ui.selectable_label(self.cc_translated.as_deref() == Some(*code), *name).clicked() {
                                    translate = Some((*code, *name));
                                    ui.close();
                                }
                            }
                        });
                    }
                });
            }
            // more: repeat, shuffle, sleep timer, shortcuts
            {
                let mr = slot("more");
                let mresp = pill(ui, mr, "more", "more", "", false, idle, t);
                egui::Popup::menu(&mresp).show(|ui| {
                    ui.set_min_width(220.0);
                    if let Some(w) = self.watching.clone().filter(|w| w.music) {
                        let liked = self.lists.iter().any(|l| l.id == "liked" && l.videos.iter().any(|x| x.id == w.id));
                        if ui.selectable_label(liked, "Like this song").clicked() {
                            list_toggle = Some(("liked".to_string(), w));
                            ui.close();
                        }
                    }
                    if let Some(w) = self.watching.clone().filter(|w| !w.id.starts_with("local:")) {
                        let in_wl = self.lists.iter().any(|l| l.id == "watchlater" && l.videos.iter().any(|x| x.id == w.id));
                        if ui.selectable_label(in_wl, "Watch later  (W)").clicked() {
                            list_toggle = Some(("watchlater".to_string(), w));
                            ui.close();
                        }
                    }
                    if !self.player.is_live() && ui.button(tr("Clip this moment…")).clicked() {
                        open_clip = true;
                        ui.close();
                    }
                    if ui.selectable_label(self.settings.ambient, "Ambient mode  (B)").clicked() {
                        toggle_ambient = true;
                        ui.close();
                    }
                    if ui.selectable_label(self.player.audio_mode, "Audio only  (A)").clicked() {
                        toggle_audio = true;
                        ui.close();
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("REPEAT").size(11.0).color(muted()).strong());
                    for (r, l) in [(Repeat::Off, "Off"), (Repeat::One, "Repeat this video"), (Repeat::All, "Repeat the queue")] {
                        if ui.selectable_label(self.settings.repeat == r, l).clicked() {
                            new_repeat = Some(r);
                        }
                    }
                    ui.separator();
                    if ui.selectable_label(self.settings.shuffle, "Shuffle the queue").clicked() {
                        toggle_shuffle = true;
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("SLEEP TIMER").size(11.0).color(muted()).strong());
                    let active = match &self.sleep {
                        Some(SleepMode::At(t)) => format!("Stops in {}", fmt_time(t.saturating_duration_since(Instant::now()).as_secs_f64())),
                        Some(SleepMode::EndOfVideo) => "Stops after this video".to_string(),
                        None => "Off".to_string(),
                    };
                    ui.label(egui::RichText::new(active).size(12.0).color(muted()));
                    for (label, mins) in [("15 minutes", 15u64), ("30 minutes", 30), ("45 minutes", 45), ("1 hour", 60)] {
                        if ui.button(label).clicked() {
                            set_sleep = Some(Some(SleepMode::At(Instant::now() + Duration::from_secs(mins * 60))));
                            ui.close();
                        }
                    }
                    if ui.button(tr("End of this video")).clicked() {
                        set_sleep = Some(Some(SleepMode::EndOfVideo));
                        ui.close();
                    }
                    if self.sleep.is_some() && ui.button(tr("Turn off")).clicked() {
                        set_sleep = Some(None);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr("Keyboard shortcuts")).clicked() {
                        show_help = true;
                        ui.close();
                    }
                });
            }
            // side panel (comments; lyrics for songs, the transcript for podcasts)
            if !is_local {
                let cr = slot("comments");
                let music = self.watching.as_ref().is_some_and(|w| w.music);
                let (lbl, tab) = if music {
                    ("Lyrics", SideTab::Lyrics)
                } else {
                    ("Comments", SideTab::Comments)
                };
                let cresp = pill(
                    ui,
                    cr,
                    "cm",
                    if music { "note" } else { "comment" },
                    lbl,
                    false,
                    if self.comments_open && self.side_tab == tab { accent() } else { idle },
                    t,
                );
                if cresp.clicked() {
                    if music {
                        toggle_lyrics = true;
                    } else {
                        toggle_cm = true;
                    }
                }
            }
            // queue
            if self.queue.len() > 1 {
                let qr = slot("queue");
                let on = self.comments_open && self.side_tab == SideTab::Queue;
                let qresp = pill(ui, qr, "queuepill", "queue", &format!("Queue {}", self.queue.len()), false, if on { accent() } else { idle }, t);
                if qresp.clicked() {
                    open_queue = true;
                }
            }
            // download + playlist
            if let Some(w) = self.watching.clone().filter(|w| !w.id.starts_with("local:")) {
                if !live {
                    let dr = slot("download");
                    let prog = self.job_progress(&w.id);
                    let dv = self.is_downloaded(&w.id, false);
                    let da = self.is_downloaded(&w.id, true);
                    let (dicon, label, chev) = match (prog, dv, da) {
                        (Some(p), _, _) => ("download", format!("{}%", (p * 100.0) as u32), false),
                        (None, true, _) => ("check", "Saved".to_string(), false),
                        (None, false, true) => ("check", if is_pod { "Saved" } else { "Audio saved" }.to_string(), false),
                        _ => ("download", "Download".to_string(), !is_pod),
                    };
                    let fill = if dv || da { TEAL } else { idle };
                    let dresp = pill(ui, dr, "dlbtn", dicon, &label, chev, fill, t);
                    if let Some(p) = prog {
                        let f = Rect::from_min_size(dr.min, vec2(dr.width() * p, dr.height()));
                        ui.painter().rect_filled(f, CornerRadius::same(9), accent().gamma_multiply(0.45 * t));
                    }
                    if is_pod {
                        // one file per episode: no format or quality to choose
                        if dresp.clicked() && !da && prog.is_none() {
                            start_dl = Some((w.clone(), true, 0));
                        }
                    } else {
                    egui::Popup::menu(&dresp).show(|ui| {
                        ui.set_min_width(200.0);
                        if dv {
                            ui.add_enabled(false, egui::Button::new("Video downloaded"));
                        } else {
                            ui.label(egui::RichText::new("VIDEO  (mp4)").size(11.0).color(muted()).strong());
                            let mut qs = self.player.qualities();
                            qs.sort_unstable_by(|a, b| b.cmp(a));
                            for q in qs {
                                if ui.button(format!("{q}p")).clicked() {
                                    start_dl = Some((w.clone(), false, q));
                                    ui.close();
                                }
                            }
                        }
                        ui.separator();
                        if da {
                            ui.add_enabled(false, egui::Button::new("Audio downloaded"));
                        } else if ui.button(tr("Audio only  (m4a)")).clicked() {
                            start_dl = Some((w.clone(), true, 0));
                            ui.close();
                        }
                    });
                    }
                }
                let sr = slot("playlist");
                let in_any = self.lists.iter().any(|l| l.videos.iter().any(|x| x.id == w.id));
                let sresp = pill(
                    ui,
                    sr,
                    "plbtn",
                    if in_any { "heart_fill" } else { "playlist_add" },
                    if in_any { "Saved" } else { "Playlist" },
                    true,
                    if in_any { accent() } else { idle },
                    t,
                );
                egui::Popup::menu(&sresp).show(|ui| {
                    ui.set_min_width(190.0);
                    for l in &self.lists {
                        let has = l.videos.iter().any(|x| x.id == w.id);
                        if ui.selectable_label(has, l.name.clone()).clicked() {
                            list_toggle = Some((l.id.clone(), w.clone()));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(tr("+  New playlist…")).clicked() {
                        new_list_for = Some(w.clone());
                        ui.close();
                    }
                });
            }
        }

        if toggle {
            self.player.toggle_pause();
        }
        if toggle_audio {
            let on = !self.player.audio_mode;
            self.player.set_audio_mode(on);
        }
        if open_clip {
            self.open_clip_tab();
        }
        if let Some((code, name)) = translate {
            // translate the track being shown, or the first one (usually the original language)
            let tracks: Vec<yt::CaptionTrack> = self.player.streams.as_ref().map(|s| s.captions.clone()).unwrap_or_default();
            let base = self.cc_track.filter(|i| *i < tracks.len()).unwrap_or(0);
            if let (Some(tr), Some(w)) = (tracks.get(base), self.watching.as_ref()) {
                let url = format!("{}&tlang={code}", tr.url);
                self.cc_track = Some(usize::MAX);
                self.cc_translated = Some(code.to_string());
                self.cues.clear();
                let (tx, ctx, vid) = (self.tx.clone(), self.ctx.clone(), w.id.clone());
                std::thread::spawn(move || {
                    let r = yt::caption_cues(&url);
                    let _ = tx.send(Msg::Cues(vid, usize::MAX, r));
                    ctx.request_repaint();
                });
                self.toast(format!("Translating captions to {name}"));
            }
        }
        if let Some(c) = set_cc {
            self.cc_translated = None;
            match c {
                Some(i) => self.load_cues(i),
                None => {
                    self.cc_track = None;
                    self.cues.clear();
                }
            }
        }
        if toggle_ambient {
            self.settings.ambient = !self.settings.ambient;
            self.save_settings();
        }
        if let Some(v) = open_channel {
            self.open_channel_of(&v);
        }
        if skip_next {
            self.queue_next();
        }
        if skip_prev {
            self.queue_prev();
        }
        if open_queue {
            if self.comments_open && self.side_tab == SideTab::Queue {
                self.comments_open = false;
            } else {
                self.side_tab = SideTab::Queue;
                self.comments_open = true;
            }
        }
        if let Some(v) = new_speed {
            self.player.set_speed(v);
            self.settings.speed = v;
            if let Some(cid) = self.watching.as_ref().map(|w| w.channel_id.clone()).filter(|c| !c.is_empty()) {
                self.settings.chan_prefs.entry(cid).or_default().speed = v;
            }
            self.save_settings();
        }
        if let Some(r) = new_repeat {
            self.settings.repeat = r;
            self.save_settings();
        }
        if toggle_shuffle {
            self.settings.shuffle = !self.settings.shuffle;
            self.save_settings();
        }
        if let Some(m) = set_sleep {
            self.sleep = m;
            self.toast(if self.sleep.is_some() { "Sleep timer set" } else { "Sleep timer off" });
        }
        if show_help {
            self.show_help = true;
        }
        if toggle_cm {
            self.toggle_comments();
        }
        if toggle_transcript {
            if self.comments_open && self.side_tab == SideTab::Transcript {
                self.comments_open = false;
            } else {
                self.side_tab = SideTab::Transcript;
                self.comments_open = true;
            }
        }
        if toggle_lyrics {
            if self.comments_open && self.side_tab == SideTab::Lyrics {
                self.comments_open = false;
            } else {
                self.side_tab = SideTab::Lyrics;
                self.comments_open = true;
            }
        }
        if let Some((v, audio, q)) = start_dl {
            self.start_download(&v, audio, q);
        }
        if let Some((id, v)) = list_toggle {
            self.toggle_in_list(&id, &v);
        }
        if let Some(v) = new_list_for {
            self.new_list = Some((String::new(), Some(v)));
        }
        if close {
            self.back_from_player();
        }
    }

    /// Slim bar shown while audio plays and you browse elsewhere.
    pub(crate) fn now_playing_bar(&mut self, ui: &mut egui::Ui) {
        let Some(w) = self.watching.clone() else { return };
        let full = ui.max_rect();
        let (pos, dur) = (self.player.position(), self.player.duration());
        // progress line along the top edge, clickable
        let line = Rect::from_min_size(full.min, vec2(full.width(), 5.0));
        let lr = ui.interact(line.expand2(vec2(0.0, 5.0)), uid("np-seek"), Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().rect_filled(line, 0.0, ov(28));
        if dur > 0.0 {
            let f = (pos / dur).clamp(0.0, 1.0) as f32;
            hgradient(ui.painter(), Rect::from_min_size(line.min, vec2(line.width() * f, 5.0)), accent(), accent2());
        }
        if lr.clicked() || lr.drag_stopped() {
            if let (Some(p), true) = (lr.interact_pointer_pos(), dur > 0.0) {
                self.player.seek(((p.x - full.min.x) / full.width()).clamp(0.0, 1.0) as f64 * dur);
            }
        }
        let cy = full.center().y + 2.0;
        // cover + title: click to open the full player
        let info = Rect::from_min_max(Pos2::new(full.min.x + 8.0, full.min.y + 8.0), Pos2::new(full.max.x - 330.0, full.max.y - 6.0));
        let ir = ui.interact(info, uid("np-open"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let cover = Rect::from_min_size(Pos2::new(info.min.x + 4.0, cy - 25.0), vec2(50.0, 50.0));
        ui.painter().rect_filled(cover, CornerRadius::same(8), card_hi());
        let uri = if crate::platform::is_local_path(&w.thumb) { crate::platform::file_uri(&w.thumb) } else { img_uri(ui.ctx(), &w.thumb) };
        if !uri.is_empty() {
            egui::Image::new(uri).fit_to_exact_size(cover.size()).corner_radius(CornerRadius::same(8)).paint_at(ui, cover);
        } else {
            icons::draw(ui.painter(), cover.center(), "note", 22.0, muted());
        }
        let tw = (info.max.x - cover.max.x - 20.0).max(80.0);
        let title = truncate(ui, &w.title, FontId::proportional(15.0), tw, 1);
        let up_next = self.queue_idx.and_then(|i| self.queue.get(i + 1)).map(|q| q.title());
        let sub_full = match up_next {
            Some(n) => format!("{}   •   Up next: {n}", w.channel_name),
            None => w.channel_name.clone(),
        };
        let sub = truncate(ui, &sub_full, FontId::proportional(12.5), tw, 1);
        ui.painter().text(Pos2::new(cover.max.x + 12.0, cy - 10.0), Align2::LEFT_CENTER, title, FontId::proportional(15.0), if ir.hovered() { accent() } else { ink() });
        ui.painter().text(Pos2::new(cover.max.x + 12.0, cy + 11.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.5), muted());
        if ir.clicked() {
            self.pip = false;
            self.set_mini(false);
        }
        // transport on the right
        let mut x = full.max.x - 26.0;
        let mut click = |ui: &mut egui::Ui, id: &str, icon: &str, size: f32, big: bool| -> bool {
            let r = Rect::from_center_size(Pos2::new(x, cy), vec2(40.0, 40.0));
            x -= 44.0;
            let resp = ui.interact(r, uid(id), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if big {
                ui.painter().circle_filled(r.center(), 20.0, if resp.hovered() { accent2() } else { accent() });
                icons::draw(ui.painter(), r.center(), icon, size, ON_ACCENT);
            } else {
                if resp.hovered() {
                    ui.painter().circle_filled(r.center(), 18.0, ov(22));
                }
                icons::draw(ui.painter(), r.center(), icon, size, ink());
            }
            resp.clicked()
        };
        if click(ui, "np-close", "close", 15.0, false) {
            self.close_player();
            self.set_mini(false);
            return;
        }
        let mini_icon = if self.mini { "pip_expand" } else { "collapse" };
        if click(ui, "np-mini", mini_icon, 15.0, false) {
            let on = !self.mini;
            self.set_mini(on);
            return;
        }
        let can_next = self.queue_idx.is_some_and(|i| i + 1 < self.queue.len()) || self.settings.repeat == Repeat::All;
        if click(ui, "np-next", "skip_next", 18.0, false) && can_next {
            self.queue_next();
        }
        let playing = !self.player.paused;
        if click(ui, "np-pp", if playing { "pause" } else { "play" }, 17.0, true) {
            self.player.toggle_pause();
        }
        if click(ui, "np-prev", "skip_prev", 18.0, false) {
            self.queue_prev();
        }
        ui.painter().text(
            Pos2::new(x - 4.0, cy),
            Align2::RIGHT_CENTER,
            format!("{} / {}", fmt_time(pos), fmt_time(dur)),
            FontId::proportional(12.5),
            muted(),
        );
    }

    /// Small always-on-top borderless window that keeps playing while the app is used.
    pub(crate) fn pip_window(&mut self, ctx: &egui::Context) {
        let aspect = self.player.aspect().clamp(0.5, 3.0);
        let w = 480.0;
        let builder = egui::ViewportBuilder::default()
            .with_title("TubeGlass — Picture in Picture")
            .with_inner_size([w, w / aspect])
            .with_min_inner_size([240.0, 140.0])
            .with_always_on_top()
            .with_decorations(false);
        let mut action: Option<u8> = None; // 1 = expand, 2 = close, 3 = toggle pause
        let mut drawn = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("tubeglass-pip"), builder, |ui, _class| {
            drawn = true;
            let pctx = ui.ctx().clone();
            let full = ui.max_rect();
            ui.painter().rect_filled(full, 0.0, Color32::BLACK);
            if let Some(tex) = self.player.texture() {
                let a = self.player.aspect();
                let vr = if full.aspect_ratio() > a {
                    Rect::from_center_size(full.center(), vec2(full.height() * a, full.height()))
                } else {
                    Rect::from_center_size(full.center(), vec2(full.width(), full.width() / a))
                };
                ui.painter().image(tex.id(), vr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            } else if self.player.audio_only() {
                icons::draw(ui.painter(), full.center(), "note", 56.0, muted());
            } else {
                ui.put(Rect::from_center_size(full.center(), vec2(28.0, 28.0)), egui::Spinner::new().color(accent()));
            }

            let resp = ui.allocate_rect(full, Sense::click_and_drag());
            if resp.drag_started() {
                pctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            let hov = resp.hovered() || pctx.input(|i| i.pointer.hover_pos().is_some());
            let t = pctx.animate_bool_with_time(uid("pip-ui"), hov, 0.15);
            if t > 0.01 {
                let a = t;
                let top = Rect::from_min_size(full.min, vec2(full.width(), 44.0));
                vgradient(ui.painter(), top, Color32::from_black_alpha((170.0 * a) as u8), Color32::TRANSPARENT);
                let btn = |ui: &mut egui::Ui, c: Pos2, label: &str, id: &str| -> bool {
                    let r = Rect::from_center_size(c, vec2(30.0, 30.0));
                    let rs = ui
                        .interact(r, uid(id), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if rs.hovered() {
                        ui.painter().circle_filled(c, 15.0, ov((70.0 * a) as u8));
                    }
                    icons::draw(ui.painter(), c, label, 16.0, Color32::WHITE.gamma_multiply(a));
                    rs.clicked()
                };
                if btn(ui, full.right_top() + vec2(-22.0, 22.0), "close", "pip-close") {
                    action = Some(2);
                }
                if btn(ui, full.right_top() + vec2(-58.0, 22.0), "pip_expand", "pip-expand") {
                    action = Some(1);
                }
                // centre play/pause button
                let c = full.center();
                let br = Rect::from_center_size(c, vec2(56.0, 56.0));
                let bresp = ui
                    .interact(br, uid("pip-pp"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let fill = if bresp.hovered() { 190.0 } else { 140.0 };
                ui.painter().circle_filled(c, 26.0, Color32::from_black_alpha((fill * a) as u8));
                let col = Color32::WHITE.gamma_multiply(a);
                if self.player.paused {
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![c + vec2(-7.0, -11.0), c + vec2(-7.0, 11.0), c + vec2(12.0, 0.0)],
                        col,
                        Stroke::NONE,
                    ));
                } else {
                    for dx in [-8.0, 2.0] {
                        ui.painter().rect_filled(Rect::from_min_size(c + vec2(dx, -10.0), vec2(6.0, 20.0)), CornerRadius::same(2), col);
                    }
                }
                if bresp.clicked() {
                    action = Some(3);
                }
            }
            // thin progress line
            let dur = self.player.duration();
            if dur > 0.0 {
                let f = (self.player.position() / dur).clamp(0.0, 1.0) as f32;
                let bar = Rect::from_min_size(Pos2::new(full.min.x, full.max.y - 3.0), vec2(full.width() * f, 3.0));
                hgradient(ui.painter(), bar, accent(), accent2());
            }
            if pctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
                action = Some(2);
            }
            if pctx.input(|i| i.key_pressed(egui::Key::Space)) {
                action = Some(3);
            }
        });
        // The window manager can close the window without our callback seeing a request.
        // If it was not drawn this frame the window is gone, so stop the media.
        if !drawn && action.is_none() {
            action = Some(2);
        }
        match action {
            Some(1) => self.pip = false,
            Some(2) => self.close_player(),
            Some(3) => self.player.toggle_pause(),
            _ => {}
        }
    }
}
