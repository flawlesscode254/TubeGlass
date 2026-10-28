//! Side-panel tools next to the player: searchable transcript, notes / bookmarks, and
//! clipping a section of the video.

use crate::clip::{self, Format};
use crate::*;

/// A note or bookmark at a moment of a video.
#[derive(Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub t: f64,
    pub text: String,
}

/// Where bookmarks.json lives.
fn bookmarks_path() -> Option<std::path::PathBuf> {
    small_list_path("bookmarks.json")
}

/// Reads the notes and bookmarks.
pub fn load_bookmarks() -> HashMap<String, Vec<Bookmark>> {
    bookmarks_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Writes the notes and bookmarks.
fn save_bookmarks(b: &HashMap<String, Vec<Bookmark>>) {
    if let Some(p) = bookmarks_path() {
        let _ = std::fs::write(p, serde_json::to_vec(b).unwrap_or_default());
    }
}

/// A clip that has been saved: where it came from and where the file is.
#[derive(Clone, Serialize, Deserialize)]
pub struct ClipEntry {
    pub video: Video,
    pub start: f64,
    pub end: f64,
    /// 0 video, 1 audio, 2 gif
    pub format: u8,
    pub file: String,
    pub made: u64,
}

impl ClipEntry {
    /// "Video", "Audio" or "GIF".
    pub fn kind(&self) -> &'static str {
        ["Video", "Audio", "GIF"][self.format.min(2) as usize]
    }
}

/// Where clips.json lives.
fn clips_path() -> Option<std::path::PathBuf> {
    small_list_path("clips.json")
}

/// Reads the clip library and picks up clip files that are not in it yet.
pub fn load_clips() -> Vec<ClipEntry> {
    let v: Vec<ClipEntry> = clips_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut v: Vec<ClipEntry> = v.into_iter().filter(|c| std::path::Path::new(&c.file).exists()).collect();
    // clips saved before the library existed (or by hand) are picked up from the folder
    if let Ok(rd) = std::fs::read_dir(clip::dir()) {
        for e in rd.flatten() {
            let path = e.path();
            let file = path.display().to_string();
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            let format = match ext.as_str() {
                "mp4" => 0u8,
                "m4a" => 1,
                "gif" => 2,
                _ => continue,
            };
            if v.iter().any(|c| c.file == file) {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            // "Title [1.51-10.38]" -> range 1:51 – 10:38
            let (title, range) = match stem.rsplit_once(" [") {
                Some((t, r)) => (t.to_string(), r.trim_end_matches(']').to_string()),
                None => (stem.clone(), String::new()),
            };
            let (start, end) = range
                .split_once('-')
                .and_then(|(a, b)| Some((clip::parse_time(&a.replace('.', ":"))?, clip::parse_time(&b.replace('.', ":"))?)))
                .unwrap_or((0.0, 0.0));
            let made = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            v.push(ClipEntry {
                video: Video {
                    id: String::new(),
                    title,
                    thumb: String::new(),
                    duration: String::new(),
                    meta: String::new(),
                    kind: Kind::Video,
                    channel_id: String::new(),
                    channel_name: String::new(),
                    rank: 0,
                    short: false,
                    music: false,
                },
                start,
                end,
                format,
                file,
                made,
            });
        }
    }
    v.sort_by_key(|c| std::cmp::Reverse(c.made));
    v
}

/// Writes the clip library.
pub fn save_clips(c: &[ClipEntry]) {
    if let Some(p) = clips_path() {
        let _ = std::fs::write(p, serde_json::to_vec(c).unwrap_or_default());
    }
}

/// The state of the Clip tab (range, format, progress).
pub struct ClipState {
    pub vid: String,
    pub start: f64,
    pub end: f64,
    pub start_txt: String,
    pub end_txt: String,
    pub format: u8,
    pub busy: Option<f32>,
    pub last: Option<std::path::PathBuf>,
    pub err: Option<String>,
    /// what is being cut right now, recorded in the library when it finishes
    pub pending: Option<(Video, f64, f64, u8)>,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// progress as drawn (eased towards `busy`) and when the cut started
    pub shown: f32,
    pub since: Option<Instant>,
    pub started: bool,
}

impl Default for ClipState {
    /// An empty Clip tab.
    fn default() -> Self {
        Self { vid: String::new(), start: 0.0, end: 0.0, start_txt: String::new(), end_txt: String::new(), format: 0, busy: None, last: None, err: None, pending: None, cancel: Default::default(), shown: 0.0, since: None, started: false }
    }
}

/// A pill-shaped button; true when clicked.
fn pill_button(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let b = egui::Button::new(egui::RichText::new(label).color(if on { ON_ACCENT } else { ink() }))
        .fill(if on { accent() } else { card_hi() })
        .corner_radius(CornerRadius::same(16));
    ui.add(b).clicked()
}

impl App {
    // ------------------------------------------------------------------ transcript
    /// The Transcript tab: searchable captions that follow the speaker.
    pub fn transcript_body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let Some(w) = self.watching.clone() else { return };
        let tracks: Vec<yt::CaptionTrack> = self.player.streams.as_ref().map(|s| s.captions.clone()).unwrap_or_default();
        let is_pod = w.id.starts_with("pod:");
        if is_pod {
            // podcasts bring their own transcript (when the feed has one)
            self.ensure_pod_transcript();
        } else if tracks.is_empty() {
            ui.label(egui::RichText::new(tr("This video has no captions, so there is no transcript.")).size(14.0).color(muted()));
            return;
        }
        if !is_pod && self.tr.as_ref().is_none_or(|t| t.0 != w.id) {
            let lang = self.settings.caption_lang.to_lowercase();
            let pick = |auto: bool| tracks.iter().find(|c| c.auto == auto && c.lang.to_lowercase().starts_with(&lang));
            let tr = pick(false).or_else(|| pick(true)).or_else(|| tracks.iter().find(|c| !c.auto)).unwrap_or(&tracks[0]).clone();
            self.tr = Some((w.id.clone(), None));
            self.tr_idx = usize::MAX;
            let (tx, ctx, vid) = (self.tx.clone(), self.ctx.clone(), w.id.clone());
            std::thread::spawn(move || {
                let r = yt::caption_cues(&tr.url);
                let _ = tx.send(Msg::Transcript(vid, r));
                ctx.request_repaint();
            });
        }
        let cues = match self.tr.as_ref().and_then(|t| t.1.as_ref()) {
            None => {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(26.0).color(accent()));
                });
                return;
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(trf("Could not load the transcript: {error}", &[("error", e)])).color(muted()));
                return;
            }
            Some(Ok(c)) => c.clone(),
        };
        let w_field = ui.available_width();
        text_field(ui, &mut self.tr_filter, &tr("Search the transcript"), w_field, 36.0, "search", None, false);
        ui.add_space(6.0);
        let needle = fold(self.tr_filter.trim());
        let rows: Vec<usize> = (0..cues.len()).filter(|i| needle.is_empty() || fold(&cues[*i].text).contains(&needle)).collect();
        let pos = self.player.position();
        let cur = cues.iter().rposition(|c| c.start <= pos + 0.1);
        let row_h = 46.0;
        let mut seek: Option<f64> = None;
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        // follow the speaker, unless the user is searching
        if needle.is_empty() {
            if let Some(c) = cur.filter(|c| *c != self.tr_idx) {
                self.tr_idx = c;
                area = area.vertical_scroll_offset((c as f32 * row_h - 120.0).max(0.0));
            }
        }
        area.show_rows(ui, row_h, rows.len(), |ui, range| {
            for k in range {
                let i = rows[k];
                let c = &cues[i];
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h - 4.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let is_cur = Some(i) == cur;
                if is_cur {
                    ui.painter().rect_filled(r, CornerRadius::same(10), accent().gamma_multiply(0.2));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(10), ov(16));
                }
                ui.painter().text(Pos2::new(r.min.x + 8.0, r.min.y + 6.0), Align2::LEFT_TOP, fmt_time(c.start), FontId::monospace(12.0), if is_cur { accent() } else { muted() });
                let text = truncate(ui, &c.text, FontId::proportional(13.5), r.width() - 70.0, 2);
                ui.painter().text(Pos2::new(r.min.x + 62.0, r.min.y + 5.0), Align2::LEFT_TOP, text, FontId::proportional(13.5), ink());
                if resp.clicked() {
                    seek = Some(c.start);
                }
            }
        });
        if let Some(t) = seek {
            self.player.seek(t);
        }
    }

    // ------------------------------------------------------------------ notes
    /// The Notes tab: timestamped notes, shown as dots on the seek bar.
    pub fn notes_body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let Some(w) = self.watching.clone() else { return };
        let pos = self.player.position();
        let mut add = false;
        ui.horizontal(|ui| {
            let fw = ui.available_width() - 130.0;
            let f = text_field(ui, &mut self.note_input, &tr("Add a note at this moment"), fw, 38.0, "search", None, false);
            add |= f.submit;
            if ui.add(egui::Button::new(egui::RichText::new(trf("Add  {time}", &[("time", &fmt_time(pos))])).color(ON_ACCENT).strong()).fill(accent())).clicked() {
                add = true;
            }
        });
        if add {
            let text = self.note_input.trim().to_string();
            let list = self.bookmarks.entry(w.id.clone()).or_default();
            list.push(Bookmark { t: pos, text: if text.is_empty() { tr("Bookmark") } else { text } });
            list.sort_by(|a, b| a.t.total_cmp(&b.t));
            self.note_input.clear();
            save_bookmarks(&self.bookmarks);
        }
        ui.add_space(8.0);
        let list = self.bookmarks.get(&w.id).cloned().unwrap_or_default();
        if list.is_empty() {
            ui.label(egui::RichText::new(tr("Notes and bookmarks you add show up here and as dots on the seek bar.")).size(13.5).color(muted()));
            return;
        }
        let mut seek: Option<f64> = None;
        let mut remove: Option<usize> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, b) in list.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(10), ov(16));
                }
                ui.painter().text(Pos2::new(r.min.x + 8.0, r.center().y), Align2::LEFT_CENTER, fmt_time(b.t), FontId::monospace(13.0), accent());
                let text = truncate(ui, &b.text, FontId::proportional(14.0), r.width() - 110.0, 2);
                ui.painter().text(Pos2::new(r.min.x + 70.0, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(14.0), ink());
                let xr = Rect::from_center_size(Pos2::new(r.max.x - 18.0, r.center().y), vec2(26.0, 26.0));
                let over_x = ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p)) && resp.hovered();
                if resp.hovered() {
                    icons::draw(ui.painter(), xr.center(), "close", 12.0, if over_x { ink() } else { muted() });
                }
                if resp.clicked() {
                    if over_x {
                        remove = Some(i);
                    } else {
                        seek = Some(b.t);
                    }
                }
            }
        });
        if let Some(t) = seek {
            self.player.seek(t);
        }
        if let Some(i) = remove {
            if let Some(l) = self.bookmarks.get_mut(&w.id) {
                if i < l.len() {
                    l.remove(i);
                }
                if l.is_empty() {
                    self.bookmarks.remove(&w.id);
                }
            }
            save_bookmarks(&self.bookmarks);
        }
    }

    // ------------------------------------------------------------------ clipping
    /// Open the Clip tab with a 30 second range starting at the playhead.
    pub fn open_clip_tab(&mut self) {
        if let Some(w) = &self.watching {
            let (pos, dur) = (self.player.position(), self.player.duration());
            let mut end = if dur > 0.0 { (pos + 30.0).min(dur) } else { pos + 30.0 };
            let mut pos = pos;
            // at the very end there is nothing after the playhead: take the last stretch instead
            if end - pos < 2.0 {
                end = if dur > 0.0 { dur } else { pos };
                pos = (end - 15.0).max(0.0);
            }
            self.clip = ClipState { vid: w.id.clone(), start: pos, end, start_txt: fmt_time(pos), end_txt: fmt_time(end), ..Default::default() };
        }
        self.side_tab = SideTab::Clip;
        self.comments_open = true;
    }

    /// The Clip tab: choose a range and format, save and play clips.
    pub fn clip_body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let Some(w) = self.watching.clone() else { return };
        if self.player.is_live() {
            ui.label(egui::RichText::new(tr("Live streams can't be clipped.")).color(muted()));
            return;
        }
        if self.clip.vid != w.id {
            self.open_clip_tab();
        }
        let (pos, dur) = (self.player.position(), self.player.duration().max(1.0));
        ui.label(egui::RichText::new(tr("Cut a piece out of this video and save it as a file.")).size(13.5).color(muted()));
        ui.add_space(10.0);

        // chapters: pick a segment instead of finding its start and end by hand
        let chapters = self.player.streams.as_ref().map(|st| st.chapters.clone()).unwrap_or_default();
        if !chapters.is_empty() {
            ui.label(egui::RichText::new(tr("SEGMENTS")).size(11.0).color(muted()).strong());
            ui.add_space(4.0);
            let mut pick: Option<(f64, f64, bool)> = None;
            egui::ScrollArea::vertical().id_salt("clip_segments").max_height(190.0).auto_shrink([false, true]).show(ui, |ui| {
                for (i, c) in chapters.iter().enumerate() {
                    let end = chapters.get(i + 1).map_or(dur, |n| n.start).min(dur);
                    if end - c.start < 0.5 {
                        continue;
                    }
                    let on = (self.clip.start - c.start).abs() < 0.6 && (self.clip.end - end).abs() < 0.6;
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let bg = if on { accent().gamma_multiply(0.22) } else if resp.hovered() { ov(26) } else { ov(12) };
                    ui.painter().rect_filled(r, CornerRadius::same(9), bg);
                    if on {
                        ui.painter().rect_stroke(r, CornerRadius::same(9), Stroke::new(1.5, accent()), StrokeKind::Inside);
                    }
                    let btn = Rect::from_center_size(Pos2::new(r.max.x - 34.0, r.center().y), vec2(60.0, 24.0));
                    let bresp = ui.interact(btn, resp.id.with("save"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.painter().rect_filled(btn, CornerRadius::same(12), if bresp.hovered() { accent2() } else { accent() });
                    ui.painter().text(btn.center(), Align2::CENTER_CENTER, tr("Clip"), FontId::proportional(12.5), ON_ACCENT);
                    let time = format!("{} – {}", fmt_time(c.start), fmt_time(end));
                    ui.painter().text(Pos2::new(r.min.x + 10.0, r.center().y), Align2::LEFT_CENTER, &time, FontId::monospace(11.5), muted());
                    let tx = r.min.x + 10.0 + 11.0 * time.chars().count() as f32 * 0.62 + 12.0;
                    let title = truncate(ui, &c.title, FontId::proportional(13.5), (btn.min.x - tx - 8.0).max(40.0), 1);
                    ui.painter().text(Pos2::new(tx, r.center().y), Align2::LEFT_CENTER, title, FontId::proportional(13.5), ink());
                    if bresp.clicked() {
                        pick = Some((c.start, end, true));
                    } else if resp.clicked() {
                        pick = Some((c.start, end, false));
                    }
                }
            });
            if let Some((a, b, save)) = pick {
                self.clip.start = a;
                self.clip.end = b;
                self.clip.start_txt = fmt_time(a);
                self.clip.end_txt = fmt_time(b);
                if save && self.clip.busy.is_none() {
                    self.start_clip();
                }
            }
            ui.add_space(10.0);
        }

        // range bar with two handles
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
        let (x0, x1) = (bar.min.x + 8.0, bar.max.x - 8.0);
        let to_x = |t: f64| x0 + (x1 - x0) * (t / dur) as f32;
        let to_t = |x: f32| (((x - x0) / (x1 - x0)).clamp(0.0, 1.0) as f64) * dur;
        let cy = bar.center().y;
        ui.painter().rect_filled(Rect::from_min_max(Pos2::new(x0, cy - 3.0), Pos2::new(x1, cy + 3.0)), CornerRadius::same(3), ov(40));
        let (sx, ex) = (to_x(self.clip.start), to_x(self.clip.end));
        ui.painter().rect_filled(Rect::from_min_max(Pos2::new(sx, cy - 4.0), Pos2::new(ex.max(sx + 1.0), cy + 4.0)), CornerRadius::same(3), accent());
        ui.painter().circle_filled(Pos2::new(to_x(pos), cy), 2.5, ink());
        let mut moved = false;
        for (which, x) in [(0, sx), (1, ex)] {
            let hr = Rect::from_center_size(Pos2::new(x, cy), vec2(22.0, 30.0));
            let resp = ui.interact(hr, uid(("cliph", which)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if let (true, Some(p)) = (resp.dragged(), resp.interact_pointer_pos()) {
                let t = to_t(p.x);
                if which == 0 {
                    self.clip.start = t.min(self.clip.end - 0.5).max(0.0);
                } else {
                    self.clip.end = t.max(self.clip.start + 0.5).min(dur);
                }
                moved = true;
            }
            ui.painter().rect_filled(Rect::from_center_size(Pos2::new(x, cy), vec2(8.0, 22.0)), CornerRadius::same(4), Color32::WHITE);
        }
        if moved {
            self.clip.start_txt = fmt_time(self.clip.start);
            self.clip.end_txt = fmt_time(self.clip.end);
        }
        ui.add_space(6.0);

        for which in 0..2 {
            ui.horizontal(|ui| {
                ui.add_sized([46.0, 30.0], egui::Label::new(egui::RichText::new(tr(if which == 0 { "Start" } else { "End" })).size(14.0).color(muted())));
                let txt = if which == 0 { &mut self.clip.start_txt } else { &mut self.clip.end_txt };
                let r = ui.add(egui::TextEdit::singleline(txt).desired_width(86.0));
                if r.lost_focus() {
                    if let Some(t) = clip::parse_time(txt) {
                        let t = t.clamp(0.0, dur);
                        if which == 0 {
                            self.clip.start = t.min(self.clip.end - 0.5).max(0.0);
                        } else {
                            self.clip.end = t.max(self.clip.start + 0.5).min(dur);
                        }
                    }
                    self.clip.start_txt = fmt_time(self.clip.start);
                    self.clip.end_txt = fmt_time(self.clip.end);
                }
                if ui.button(trf("Use {time}", &[("time", &fmt_time(pos))])).clicked() {
                    if which == 0 {
                        self.clip.start = pos.min(self.clip.end - 0.5).max(0.0);
                    } else {
                        self.clip.end = pos.max(self.clip.start + 0.5).min(dur);
                    }
                    self.clip.start_txt = fmt_time(self.clip.start);
                    self.clip.end_txt = fmt_time(self.clip.end);
                }
            });
        }
        let len = self.clip.end - self.clip.start;
        ui.label(egui::RichText::new(trf("Length {time}", &[("time", &fmt_time(len))])).size(13.0).color(muted()));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (label, v) in [(tr("Video"), 0u8), (tr("Audio"), 1), (tr("GIF"), 2)] {
                if pill_button(ui, &label, self.clip.format == v) {
                    self.clip.format = v;
                }
            }
        });
        if self.clip.format == 2 && len > 20.0 {
            ui.label(egui::RichText::new(tr("GIFs get big fast; 20 seconds or less works best.")).size(12.5).color(muted()));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(tr("Preview")).clicked() {
                self.player.seek(self.clip.start);
                if self.player.paused {
                    self.player.toggle_pause();
                }
                self.clip_preview = true;
            }
            if self.clip.busy.is_some() {
                // progress is drawn below, on its own row
            } else if ui.add(egui::Button::new(egui::RichText::new(tr("Save clip")).color(ON_ACCENT).strong()).fill(accent())).clicked() {
                self.start_clip();
            }
        });
        if let Some(target) = self.clip.busy {
            ui.add_space(10.0);
            // ease towards the real value so the bar glides instead of jumping
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            let goal = if self.clip.started { target } else { 0.0 };
            self.clip.shown += (goal - self.clip.shown) * (1.0 - (-8.0 * dt).exp());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(6), ov(34));
            if self.clip.started {
                let f = Rect::from_min_size(r.min, vec2((r.width() * self.clip.shown.clamp(0.02, 1.0)).max(12.0), r.height()));
                hgradient(&ui.painter().with_clip_rect(f), r, accent(), accent2());
            } else {
                // still opening the video: a block sweeping back and forth
                let t = ui.input(|i| i.time) as f32;
                let w = r.width() * 0.28;
                let x = r.min.x + (r.width() - w) * (0.5 + 0.5 * (t * 2.2).sin());
                ui.painter().rect_filled(Rect::from_min_size(Pos2::new(x, r.min.y), vec2(w, r.height())), CornerRadius::same(6), accent().gamma_multiply(0.8));
            }
            let secs = self.clip.since.map_or(0.0, |t| t.elapsed().as_secs_f64());
            let label = if self.clip.started && target > 0.03 {
                let left = secs * (1.0 - target as f64) / target as f64;
                trf("Cutting…  {percent}%   •   about {time} left", &[("percent", &((self.clip.shown * 100.0).round() as u32).to_string()), ("time", &fmt_time(left.max(1.0)))])
            } else {
                tr("Opening the video…")
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).size(13.0).color(muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(egui::RichText::new(tr("Cancel")).color(Color32::WHITE)).fill(LIVE)).clicked() {
                        self.clip.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            });
            ui.ctx().request_repaint();
        }
        if let Some(e) = &self.clip.err {
            ui.add_space(6.0);
            ui.colored_label(LIVE, e);
        }
        let mine: Vec<ClipEntry> = self.clips.iter().filter(|c| c.video.id == w.id).cloned().collect();
        if !mine.is_empty() {
            ui.add_space(14.0);
            ui.label(egui::RichText::new(tr("CLIPS OF THIS VIDEO")).size(11.0).color(muted()).strong());
            let mut play: Option<String> = None;
            let mut del: Option<String> = None;
            for c in &mine {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Play this clip"));
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(10), ov(16));
                }
                ui.painter().text(
                    Pos2::new(r.min.x + 8.0, r.center().y),
                    Align2::LEFT_CENTER,
                    trf("{start} – {end}   {kind}", &[("start", &fmt_time(c.start)), ("end", &fmt_time(c.end)), ("kind", &tr(c.kind()))]),
                    FontId::proportional(13.5),
                    ink(),
                );
                let xr = Rect::from_center_size(Pos2::new(r.max.x - 18.0, r.center().y), vec2(26.0, 26.0));
                let over_x = ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p)) && resp.hovered();
                icons::draw(ui.painter(), xr.center(), "close", 12.0, if over_x { ink() } else { muted() });
                if resp.clicked() {
                    if over_x { del = Some(c.file.clone()) } else { play = Some(c.file.clone()) }
                }
            }
            if let Some(f) = play {
                self.open_local(f);
            }
            if let Some(f) = del {
                self.delete_clip(&f);
            }
        }
        if let Some(p) = self.clip.last.clone() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(trf("Saved {name}", &[("name", &p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())])).size(13.0).color(muted()));
            let mut play_here = false;
            ui.horizontal(|ui| {
                if ui.button(tr("Play")).clicked() {
                    play_here = true;
                }
                if ui.button(tr("Show folder")).clicked() {
                    crate::platform::open_path(&clip::dir());
                }
            });
            if play_here {
                self.open_local(p.display().to_string());
            }
        }
    }

    /// Starts cutting the chosen range in the background.
    fn start_clip(&mut self) {
        let Some(w) = self.watching.clone() else { return };
        let Some(streams) = self.player.streams.clone() else { return };
        let (start, end) = (self.clip.start, self.clip.end);
        let format = [Format::Video, Format::Audio, Format::Gif][self.clip.format.min(2) as usize];
        let quality = self.settings.default_quality;
        self.clip.busy = Some(0.0);
        self.clip.err = None;
        self.clip.cancel = Default::default();
        self.clip.shown = 0.0;
        self.clip.started = false;
        self.clip.since = Some(Instant::now());
        let cancel = self.clip.cancel.clone();
        self.clip.pending = Some((w.clone(), start, end, self.clip.format.min(2)));
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(-1i32);
            let r = clip::run(&streams, quality, start, end, format, &w.title, &cancel, &|p| {
                let pct = (p * 100.0) as i32;
                if pct != last.get() {
                    last.set(pct);
                    let _ = tx.send(Msg::ClipProgress(p));
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::ClipDone(r));
            ctx.request_repaint();
        });
    }

    /// Stop the preview at the end of the range.
    pub fn clip_tick(&mut self) {
        if self.clip_preview {
            if !(self.side_tab == SideTab::Clip && self.comments_open) || self.player.position() >= self.clip.end {
                self.clip_preview = false;
                if self.player.position() >= self.clip.end && !self.player.paused {
                    self.player.toggle_pause();
                }
            }
        }
    }
}

impl App {
    /// Deletes a clip file and its library entry.
    pub fn delete_clip(&mut self, file: &str) {
        let _ = std::fs::remove_file(file);
        self.clips.retain(|c| c.file != file);
        save_clips(&self.clips);
    }

    /// Record a finished clip.
    pub fn clip_finished(&mut self, path: std::path::PathBuf) {
        if let Some((video, start, end, format)) = self.clip.pending.take() {
            self.clips.insert(0, ClipEntry { video, start, end, format, file: path.display().to_string(), made: now_secs() });
            save_clips(&self.clips);
        }
    }

    /// A clip without a known source (made before clips were tracked) is matched to the video
    /// being played by its title, and from then on behaves like any other clip.
    pub fn link_clips(&mut self) {
        let Some(w) = self.watching.clone() else { return };
        if w.id.starts_with("local:") || self.clip_linked_for == w.id {
            return;
        }
        self.clip_linked_for = w.id.clone();
        let name = download::sanitize(&w.title);
        let mut any = false;
        for c in self.clips.iter_mut().filter(|c| c.video.id.is_empty() && c.video.title.trim() == name) {
            c.video = w.clone();
            any = true;
        }
        if any {
            save_clips(&self.clips);
        }
    }

    /// Jump to the start of a clip once its source video is playing.
    pub fn clip_seek_tick(&mut self) {
        if let Some((vid, t)) = self.clip_open_at.clone() {
            if self.watching.as_ref().is_some_and(|w| w.id == vid) && self.player.state() == State::Playing {
                self.player.seek(t);
                self.clip_open_at = None;
            }
        }
    }

    /// The Clips page.
    pub fn clips_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr("Clips")).size(26.0).strong());
            ui.label(egui::RichText::new(clip::dir().display().to_string()).size(12.0).color(muted()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Open folder")).clicked() {
                    let _ = std::fs::create_dir_all(clip::dir());
                    crate::platform::open_path(&clip::dir());
                }
                self.view_toggle(ui);
            });
        });
        ui.add_space(8.0);
        if self.clips.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(tr("No clips yet.")).size(18.0).color(muted()));
                ui.label(egui::RichText::new(tr("Open a video, choose the Clip tab (or “Clip this moment…” in the … menu) and save a section.")).color(muted()));
            });
            return;
        }
        let mut play: Option<String> = None;
        let mut source: Option<(Video, f64)> = None;
        let mut del: Option<String> = None;
        let grid_mode = self.settings.view_mode != 2;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width() - 6.0;
            if grid_mode {
                let m = views::metrics(&self.settings, w, false);
                for line in self.clips.chunks(m.cols) {
                    let (area, _) = ui.allocate_exact_size(vec2(w, m.ch), Sense::hover());
                    for (k, c) in line.iter().enumerate() {
                        let rect = Rect::from_min_size(area.min + vec2(k as f32 * (m.cw + m.gap), 0.0), vec2(m.cw, m.ch));
                        let resp = ui.interact(rect, ui.scope_id().with(("clipcard", &c.file)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                        let uri = img_uri(ui.ctx(), &c.video.thumb);
                        let size = std::fs::metadata(&c.file).map(|x| x.len()).unwrap_or(0);
                        let sub = if c.end > c.start {
                            format!("{} – {}  •  {}", fmt_time(c.start), fmt_time(c.end), fmt_size(size))
                        } else {
                            fmt_size(size)
                        };
                        let img = views::media_card(ui, rect, &m, &uri, &c.video.title, &sub, hv);
                        // kind badge, length, and the delete button that appears on hover
                        let badge = Rect::from_min_size(img.min + vec2(7.0, 7.0), vec2(46.0, 18.0));
                        ui.painter().rect_filled(badge, CornerRadius::same(6), Color32::from_black_alpha(190));
                        ui.painter().text(badge.center(), Align2::CENTER_CENTER, tr(c.kind()), FontId::proportional(11.0), Color32::WHITE);
                        if c.end > c.start {
                            views::pill_label(ui, img.right_bottom(), &fmt_time(c.end - c.start));
                        }
                        let xr = Rect::from_center_size(img.right_top() + vec2(-20.0, 20.0), vec2(30.0, 30.0));
                        let over_x = ui.ctx().pointer_hover_pos().is_some_and(|p| xr.contains(p)) && resp.hovered();
                        if hv > 0.05 {
                            let cc = img.center();
                            ui.painter().circle_filled(cc, 22.0 * (0.7 + 0.3 * hv), Color32::from_black_alpha((150.0 * hv) as u8));
                            ui.painter().add(egui::Shape::convex_polygon(
                                vec![cc + vec2(-5.0, -9.0), cc + vec2(-5.0, 9.0), cc + vec2(9.0, 0.0)],
                                Color32::WHITE.gamma_multiply(hv),
                                Stroke::NONE,
                            ));
                            ui.painter().circle_filled(xr.center(), 14.0, if over_x { LIVE.gamma_multiply(0.9) } else { Color32::from_black_alpha((150.0 * hv) as u8) });
                            icons::draw(ui.painter(), xr.center(), "close", 13.0, Color32::WHITE.gamma_multiply(hv));
                        }
                        resp.context_menu(|ui| {
                            if !c.video.id.is_empty() && ui.button(tr("Open the source video")).clicked() {
                                source = Some((c.video.clone(), c.start));
                                ui.close();
                            }
                            if ui.button(tr("Delete clip")).clicked() {
                                del = Some(c.file.clone());
                                ui.close();
                            }
                        });
                        if resp.clicked() {
                            if resp.interact_pointer_pos().is_some_and(|p| xr.contains(p)) {
                                del = Some(c.file.clone());
                            } else {
                                play = Some(c.file.clone());
                            }
                        }
                    }
                    ui.add_space(m.gap);
                }
                return;
            }
            for c in &self.clips {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 84.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                ui.painter().add(surface::panel(rect, 20.0, 0.75 + 0.25 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.25 * hv)) } else { None }, true));
                let th_r = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(120.0, 72.0));
                ui.painter().rect_filled(th_r, CornerRadius::same(10), card_hi());
                let uri = img_uri(ui.ctx(), &c.video.thumb);
                if !uri.is_empty() {
                    egui::Image::new(uri).fit_to_exact_size(th_r.size()).corner_radius(CornerRadius::same(10)).paint_at(ui, th_r);
                }
                let badge = Rect::from_min_size(th_r.min + vec2(6.0, 6.0), vec2(46.0, 18.0));
                ui.painter().rect_filled(badge, CornerRadius::same(6), Color32::from_black_alpha(190));
                        ui.painter().text(badge.center(), Align2::CENTER_CENTER, tr(c.kind()), FontId::proportional(11.0), Color32::WHITE);
                let tx = th_r.max.x + 14.0;
                let size = std::fs::metadata(&c.file).map(|m| m.len()).unwrap_or(0);
                let title = truncate(ui, &c.video.title, FontId::proportional(16.0), w - 400.0, 1);
                ui.painter().text(Pos2::new(tx, rect.center().y - 11.0), Align2::LEFT_CENTER, title, FontId::proportional(16.0), ink());
                ui.painter().text(
                    Pos2::new(tx, rect.center().y + 12.0),
                    Align2::LEFT_CENTER,
                    if c.end > c.start {
                        format!("{} – {}  •  {}  •  {}", fmt_time(c.start), fmt_time(c.end), fmt_time(c.end - c.start), fmt_size(size))
                    } else {
                        fmt_size(size)
                    },
                    FontId::proportional(13.0),
                    muted(),
                );
                // one click target per row; where the pointer was decides the action
                let xr = Rect::from_center_size(Pos2::new(rect.max.x - 30.0, rect.center().y), vec2(34.0, 34.0));
                let sr = Rect::from_center_size(Pos2::new(xr.min.x - 62.0, rect.center().y), vec2(110.0, 32.0));
                let ptr = ui.ctx().pointer_hover_pos().filter(|_| resp.hovered());
                let over_x = ptr.is_some_and(|p| xr.contains(p));
                let over_s = ptr.is_some_and(|p| sr.contains(p));
                if over_x {
                    ui.painter().circle_filled(xr.center(), 17.0, LIVE.gamma_multiply(0.85));
                }
                icons::draw(ui.painter(), xr.center(), "close", 15.0, ink());
                let has_source = !c.video.id.is_empty();
                if has_source {
                    ui.painter().rect_filled(sr, CornerRadius::same(16), if over_s { accent() } else { card_hi() });
                    ui.painter().text(sr.center(), Align2::CENTER_CENTER, tr("Source video"), FontId::proportional(13.0), if over_s { ON_ACCENT } else { ink() });
                }
                let over_s = over_s && has_source;
                if over_x || over_s {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    let p = resp.interact_pointer_pos();
                    if p.is_some_and(|p| xr.contains(p)) {
                        del = Some(c.file.clone());
                    } else if has_source && p.is_some_and(|p| sr.contains(p)) {
                        source = Some((c.video.clone(), c.start));
                    } else {
                        play = Some(c.file.clone());
                    }
                }
                ui.add_space(8.0);
            }
        });
        if let Some(f) = play {
            self.open_local(f);
        }
        if let Some((v, t)) = source {
            self.clip_open_at = Some((v.id.clone(), t));
            self.play(v);
        }
        if let Some(f) = del {
            self.delete_clip(&f);
        }
    }
}
