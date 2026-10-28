//! YouTube Music screens: home shelves, search, album / playlist / artist pages.

use crate::ytmusic::{self, Card, CardKind, SearchKind};
use crate::*;

/// Choices from a song's menu.
pub enum MAct {
    PlayNext,
    Queue,
    Toggle(String),
}

impl App {
    /// Recommendations built from what you play here (no account involved): your recent
    /// songs, plus YouTube Music's "radio" for the last few of them.
    pub fn music_refresh_for_you(&mut self) {
        let mut recent: Vec<Video> = vec![];
        for h in &self.history {
            if h.video.music && !recent.iter().any(|v| v.id == h.video.id) {
                recent.push(h.video.clone());
            }
            if recent.len() >= 12 {
                break;
            }
        }
        let liked: Vec<Video> = self
            .lists
            .iter()
            .find(|l| l.id == "liked")
            .map(|l| l.videos.clone())
            .unwrap_or_default();
        let artists = self.followed.clone();
        if recent.is_empty() && liked.is_empty() && artists.is_empty() {
            self.for_you = Some(vec![]);
            return;
        }
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let cards = |v: &[Video]| -> Vec<Card> { v.iter().map(ytmusic::song_card).collect() };
            let mut shelves: Vec<(String, Vec<Card>)> = vec![];
            if !liked.is_empty() {
                shelves.push((tr("Liked songs"), cards(&liked[..liked.len().min(20)])));
            }
            if !artists.is_empty() {
                let cs = artists
                    .iter()
                    .map(|a| Card { kind: CardKind::Artist, id: a.id.clone(), title: a.name.clone(), sub: tr("Following"), thumb: a.thumb.clone() })
                    .collect();
                shelves.push((tr("Your artists"), cs));
                // newest releases of the followed artists (YouTube lists them newest first)
                let mut fresh: Vec<(u32, Card)> = vec![];
                for a in artists.iter().take(10) {
                    if let Ok(page) = ytmusic::artist(&a.id) {
                        for (name, cs) in page.shelves {
                            if name == "Albums" || name.starts_with("Singles") {
                                for c in cs.into_iter().take(2) {
                                    let year = c.sub.split(" • ").last().and_then(|y| y.trim().parse::<u32>().ok()).unwrap_or(0);
                                    let mut c = c;
                                    c.sub = format!("{} • {}", a.name, c.sub);
                                    fresh.push((year, c));
                                }
                            }
                        }
                    }
                }
                fresh.sort_by(|a, b| b.0.cmp(&a.0));
                let cs: Vec<Card> = fresh.into_iter().map(|(_, c)| c).take(20).collect();
                if !cs.is_empty() {
                    shelves.push((tr("New from artists you follow"), cs));
                }
            }
            if !recent.is_empty() {
                shelves.push((tr("Jump back in"), cards(&recent)));
            }
            // radios: seeds are your liked songs first, then what you played lately
            let mut seeds: Vec<&Video> = liked.iter().take(2).collect();
            for v in &recent {
                if seeds.len() >= 3 {
                    break;
                }
                if !seeds.iter().any(|s| s.id == v.id) {
                    seeds.push(v);
                }
            }
            for seed in seeds {
                if let Ok(mut r) = ytmusic::radio(&seed.id) {
                    r.retain(|v| !recent.iter().any(|x| x.id == v.id) && !liked.iter().any(|x| x.id == v.id));
                    r.truncate(20);
                    if !r.is_empty() {
                        shelves.push((trf("Because you like {title}", &[("title", &seed.title)]), cards(&r)));
                    }
                }
            }
            let _ = tx.send(Msg::ForYou(shelves));
            ctx.request_repaint();
        });
    }

    /// Opens the Music home page (recommendations).
    pub fn music_open_home(&mut self) {
        self.sel = Sel::Music;
        if self.for_you.is_none() {
            self.for_you = Some(vec![]);
            self.music_refresh_for_you();
        }
        if self.music_home.is_none() {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Msg::MusicHome(ytmusic::home()));
                ctx.request_repaint();
            });
        }
    }

    /// Runs the Music search for the typed text and the chosen tab.
    pub fn music_search(&mut self) {
        let q = self.music_q.trim().to_string();
        if q.is_empty() {
            self.music_active = None;
            return;
        }
        self.sel = Sel::Music;
        let kind = self.music_kind;
        self.music_active = Some((q.clone(), kind));
        self.music_results = None;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = ytmusic::search(&q, kind);
            let _ = tx.send(Msg::MusicSearch(q, kind, r));
            ctx.request_repaint();
        });
    }

    /// Opens an album, artist or playlist page.
    pub fn music_open(&mut self, kind: &CardKind, id: &str) {
        let artist = *kind == CardKind::Artist;
        self.sel = if artist { Sel::MusicArtist(id.to_string()) } else { Sel::MusicAlbum(id.to_string()) };
        if self.music_page.as_ref().is_some_and(|p| p.0 == id && p.1.is_some()) {
            return;
        }
        self.music_page = Some((id.to_string(), None));
        let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), id.to_string());
        std::thread::spawn(move || {
            let r = if artist { ytmusic::artist(&id) } else { ytmusic::tracklist(&id) };
            let _ = tx.send(Msg::MusicPage(id, r));
            ctx.request_repaint();
        });
    }

    /// Queue `items` and start with `start`; songs open in audio mode.
    pub fn music_play(&mut self, items: Vec<Video>, start: usize, shuffle: bool) {
        if items.is_empty() {
            return;
        }
        let was = self.settings.shuffle;
        if shuffle {
            self.settings.shuffle = true;
        }
        let q: Vec<QItem> = items.into_iter().map(QItem::Yt).collect();
        self.set_queue(q, start);
        self.settings.shuffle = was;
        self.play_queue_at(self.queue_idx.unwrap_or(0));
    }

    /// The Music search box with its suggestions.
    fn music_search_bar(&mut self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        let f = text_field(ui, &mut self.music_q, &tr("Search songs, albums, artists"), w, 46.0, "search", Some(&tr("Search")), false);
        let mut go = f.submit || f.action;
        if f.cleared {
            self.music_active = None;
        }
        if f.resp.changed() {
            self.music_edit = Instant::now();
        }
        let focused = f.resp.has_focus();
        let q = self.music_q.trim().to_string();
        if focused && !q.is_empty() && q != self.music_sugg_req {
            if self.music_edit.elapsed() > Duration::from_millis(180) {
                self.music_sugg_req = q.clone();
                let (tx, ctx, q2) = (self.tx.clone(), self.ctx.clone(), q.clone());
                std::thread::spawn(move || {
                    let list = ytmusic::suggestions(&q2).unwrap_or_default();
                    let _ = tx.send(Msg::MusicSugg(q2, list));
                    ctx.request_repaint();
                });
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
        }
        let ctx = ui.ctx().clone();
        let over = self.music_sugg_rect.zip(ctx.pointer_hover_pos()).is_some_and(|(r, p)| r.contains(p));
        if (focused || over) && !self.music_sugg.is_empty() && !q.is_empty() {
            let width = f.rect.width();
            let rows: Vec<String> = self.music_sugg.iter().take(8).cloned().collect();
            let mut pick: Option<String> = None;
            let area = egui::Area::new(uid("music-sugg"))
                .order(egui::Order::Foreground)
                .fixed_pos(f.rect.left_bottom() + vec2(0.0, 4.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(panel())
                        .stroke(Stroke::new(1.0, ov(22)))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(width - 12.0);
                            for sg in &rows {
                                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                if resp.hovered() {
                                    ui.painter().rect_filled(rect, CornerRadius::same(8), card_hi());
                                }
                                icons::draw(ui.painter(), Pos2::new(rect.min.x + 18.0, rect.center().y), "search", 14.0, icons::muted_icon());
                                ui.painter().text(Pos2::new(rect.min.x + 38.0, rect.center().y), Align2::LEFT_CENTER, sg, FontId::proportional(15.0), ink());
                                if resp.clicked() {
                                    pick = Some(sg.clone());
                                }
                            }
                        });
                });
            self.music_sugg_rect = Some(area.response.rect);
            if let Some(p) = pick {
                self.music_q = p;
                go = true;
            }
        } else {
            self.music_sugg_rect = None;
        }
        if go {
            self.music_sugg.clear();
            self.music_search();
        }
        ui.add_space(8.0);
    }

    /// Start looking up lyrics for what is playing (once per song).
    pub fn ensure_lyrics(&mut self) {
        let Some(w) = self.watching.clone() else { return };
        if w.id.starts_with("local:") {
            return;
        }
        // lyrics are for songs; for anything else only when asked, or they would be a guess
        if !w.music && self.lyrics_forced != w.id {
            return;
        }
        if self.lyrics.as_ref().is_some_and(|l| l.0 == w.id) {
            // a lookup that failed only because the service was down is tried again
            let failed = matches!(&self.lyrics, Some((_, Some(Err(e)))) if e == ytmusic::RETRYABLE);
            if failed && self.lyric_tries < 4 && self.lyric_retry_at.is_some_and(|t| Instant::now() >= t) {
                self.lyric_tries += 1;
                self.lyric_retry_at = None;
            } else {
                if failed && self.lyric_retry_at.is_none() {
                    self.lyric_retry_at = Some(Instant::now() + Duration::from_secs(8));
                    self.ctx.request_repaint_after(Duration::from_secs(8));
                }
                return;
            }
        } else {
            self.lyric_tries = 0;
            self.lyric_retry_at = None;
        }
        self.lyrics = Some((w.id.clone(), None));
        self.lyric_idx = usize::MAX;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let (id, title, artist, dur) = (w.id.clone(), w.title.clone(), w.channel_name.clone(), self.player.duration());
        // lyrics saved with a download work without a connection
        let saved = self.library.iter().find(|d| d.id == w.id).and_then(download::offline_lyrics);
        std::thread::spawn(move || {
            let r = match saved {
                Some(l) => Ok(l),
                None => ytmusic::lyrics(&id, &title, &artist, dur),
            };
            let _ = tx.send(Msg::Lyrics(id, r));
            ctx.request_repaint();
        });
    }

    /// Up to five lines around the current position: (index in window, text, role) where role
    /// 0 = previous, 1 = current, 2 = upcoming. `None` unless timed lyrics are loaded.
    pub fn current_lyric_window(&self) -> Option<Vec<(usize, String, u8)>> {
        let w = self.watching.as_ref()?;
        let l = match self.lyrics.as_ref() {
            Some((id, Some(Ok(l)))) if *id == w.id && l.synced => l,
            _ => return None,
        };
        let pos = self.player.position();
        let lines: Vec<&(f64, String)> = l.lines.iter().filter(|(_, t)| !t.trim().is_empty()).collect();
        let cur = lines.iter().rposition(|(t, _)| *t <= pos + 0.15);
        let mut out = vec![];
        match cur {
            Some(c) => {
                if c > 0 {
                    out.push((0, lines[c - 1].1.clone(), 0));
                }
                out.push((1, lines[c].1.clone(), 1));
                for (k, ln) in lines.iter().skip(c + 1).take(3).enumerate() {
                    out.push((2 + k, ln.1.clone(), 2));
                }
            }
            None => {
                // before the first line: show what is coming
                for (k, ln) in lines.iter().take(4).enumerate() {
                    out.push((2 + k, ln.1.clone(), 2));
                }
            }
        }
        Some(out)
    }

    /// Draws the Lyrics side tab (timed lines follow the song).
    pub fn lyrics_body(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let Some(w) = self.watching.clone() else { return };
        if w.id.starts_with("local:") {
            ui.label(egui::RichText::new(tr("Lyrics are only looked up for YouTube songs.")).color(muted()));
            return;
        }
        if !w.music && self.lyrics_forced != w.id {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(tr("This doesn't look like a song, so lyrics are not looked up automatically.")).size(14.0).color(muted()));
            ui.add_space(8.0);
            if ui.button(tr("Look up lyrics anyway")).clicked() {
                self.lyrics_forced = w.id.clone();
            }
            return;
        }
        self.ensure_lyrics();
        let pos = self.player.position();
        let mut seek: Option<f64> = None;
        let mut new_idx = self.lyric_idx;
        match self.lyrics.as_ref().and_then(|l| l.1.as_ref()) {
            None => {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(26.0).color(accent()));
                });
            }
            Some(Err(e)) => {
                ui.add_space(20.0);
                ui.label(egui::RichText::new(e).size(14.0).color(muted()));
            }
            Some(Ok(l)) => {
                let cur = if l.synced {
                    match l.lines.partition_point(|(t, _)| *t <= pos + 0.15) {
                        0 => usize::MAX - 1, // before the first line
                        n => n - 1,
                    }
                } else {
                    usize::MAX
                };
                let moved = cur != self.lyric_idx;
                new_idx = cur;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(30.0);
                    for (i, (t, text)) in l.lines.iter().enumerate() {
                        if text.is_empty() {
                            ui.add_space(14.0);
                            continue;
                        }
                        let active = i == cur;
                        let size = if active { 22.0 } else { 18.0 };
                        let col = if !l.synced {
                            ink()
                        } else if active {
                            accent()
                        } else if i < cur {
                            muted().gamma_multiply(0.75)
                        } else {
                            ink().gamma_multiply(0.75)
                        };
                        let resp = ui.add(
                            egui::Label::new(egui::RichText::new(text).size(size).color(col).strong())
                                .wrap()
                                .sense(Sense::click()),
                        );
                        let resp = if l.synced { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
                        if active && moved {
                            ui.scroll_to_rect(resp.rect.expand2(vec2(0.0, 90.0)), Some(egui::Align::Center));
                        }
                        if resp.clicked() && l.synced {
                            seek = Some(*t);
                        }
                        ui.add_space(8.0);
                    }
                    ui.add_space(20.0);
                ui.label(egui::RichText::new(trf("Lyrics: {source}", &[("source", &l.source)])).size(11.0).color(muted()));
                });
                if l.synced {
                    ui.ctx().request_repaint_after(Duration::from_millis(120));
                }
            }
        }
        self.lyric_idx = new_idx;
        if let Some(t) = seek {
            self.player.seek(t);
        }
    }

    /// The Music page: search bar, then home, results or an album / artist page.
    pub fn music_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let mut back = false;
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
            self.music_search_bar(ui);
        });
        if back {
            self.go_back();
            return;
        }
        match self.sel.clone() {
            Sel::MusicAlbum(id) | Sel::MusicArtist(id) => self.music_page_view(ui, &id),
            _ => self.music_home_view(ui),
        }
    }

    /// Home shelves, or search results with their tabs.
    fn music_home_view(&mut self, ui: &mut egui::Ui) {
        let mut act: Option<(Video, MAct)> = None;
        let mut play: Option<(Vec<Video>, usize)> = None;
        let mut open: Option<(CardKind, String)> = None;
        if let Some((q, kind)) = self.music_active.clone() {
            // result tabs
            ui.horizontal(|ui| {
                for (k, l) in [
                    (SearchKind::Songs, tr("Songs")),
                    (SearchKind::Albums, tr("Albums")),
                    (SearchKind::Artists, tr("Artists")),
                    (SearchKind::Playlists, tr("Playlists")),
                ] {
                    let on = kind == k;
                    let b = egui::Button::new(egui::RichText::new(l).color(if on { ON_ACCENT } else { ink() }))
                        .fill(if on { accent() } else { card() })
                        .corner_radius(CornerRadius::same(16));
                    if ui.add(b).clicked() && !on {
                        self.music_kind = k;
                        self.music_search();
                    }
                }
            });
            ui.add_space(6.0);
            match &self.music_results {
                None => {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(26.0).color(accent()));
                    });
                }
                Some((rq, rk, Err(e))) if *rq == q && *rk == kind => {
                    ui.colored_label(LIVE, trf("Search failed: {error}", &[("error", e)]));
                }
                Some((rq, rk, Ok(res))) if *rq == q && *rk == kind => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        if kind == SearchKind::Songs {
                            song_rows(ui, &res.songs, &self.lists, &mut act, &mut play);
                        } else {
                            card_grid(ui, &res.cards, &mut open);
                        }
                        if res.songs.is_empty() && res.cards.is_empty() {
                            ui.label(egui::RichText::new(tr("Nothing found.")).color(muted()));
                        }
                    });
                }
                _ => {}
            }
        } else {
            match &self.music_home {
                None => {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(26.0).color(accent()));
                    });
                }
                Some(Err(e)) => {
                    ui.colored_label(LIVE, trf("Could not load YouTube Music: {error}", &[("error", e)]));
                }
                Some(Ok(page)) => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.label(egui::RichText::new(tr("YouTube Music")).size(26.0).strong());
                        ui.add_space(6.0);
                        for (title, cards) in self.for_you.iter().flatten() {
                            shelf(ui, title, cards, &mut open, &mut play);
                        }
                        if self.for_you.as_ref().is_some_and(|f| f.is_empty()) {
                            ui.label(
                                egui::RichText::new(tr("Play some songs and your own picks will appear here."))
                                    .size(13.0)
                                    .color(muted()),
                            );
                            ui.add_space(10.0);
                        }
                        for (title, cards) in &page.shelves {
                            shelf(ui, title, cards, &mut open, &mut play);
                        }
                    });
                }
            }
        }
        self.music_apply(act, play, open);
    }

    /// An album, playlist or artist page with Play, Shuffle, Follow and Download.
    fn music_page_view(&mut self, ui: &mut egui::Ui, id: &str) {
        let mut act: Option<(Video, MAct)> = None;
        let mut play: Option<(Vec<Video>, usize)> = None;
        let mut open: Option<(CardKind, String)> = None;
        let mut play_all: Option<bool> = None;
        let mut download_page = false;
        let mut follow: Option<(String, String)> = None;
        let mut download_tracks: Option<(String, Vec<Video>)> = None;
        let is_artist = matches!(self.sel, Sel::MusicArtist(_));
        match self.music_page.as_ref().filter(|p| p.0 == id).and_then(|p| p.1.as_ref()) {
            None => {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(26.0).color(accent()));
                });
            }
            Some(Err(e)) => {
                    ui.colored_label(LIVE, trf("Could not load: {error}", &[("error", e)]));
            }
            Some(Ok(page)) => {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let side = 168.0;
                        let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(if is_artist { 84 } else { 14 }), card_hi());
                        let uri = img_uri(ui.ctx(), &page.thumb);
                        if !uri.is_empty() {
                            egui::Image::new(uri)
                                .fit_to_exact_size(r.size())
                                .corner_radius(CornerRadius::same(if is_artist { 84 } else { 14 }))
                                .paint_at(ui, r);
                        }
                        ui.vertical(|ui| {
                            ui.add_space(14.0);
                            ui.label(egui::RichText::new(&page.title).size(30.0).strong());
                            ui.label(egui::RichText::new(&page.sub).size(14.0).color(muted()));
                            ui.add_space(10.0);
                            if !page.tracks.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    if icon_btn(ui, "play",&tr("Play"), Some(accent()), ON_ACCENT, false, true).clicked() {
                                        play_all = Some(false);
                                    }
                                    if icon_btn(ui, "shuffle",&tr("Shuffle"), None, ink(), false, true).clicked() {
                                        play_all = Some(true);
                                    }
                                    if is_artist {
                                        let following = self.followed.iter().any(|a| a.id == id);
                                        let follow_label = tr(if following { "Following" } else { "Follow" });
                                        let fb = icon_btn(
                                            ui,
                                            if following { "check" } else { "plus" },
                                            &follow_label,
                                            Some(if following { card_hi() } else { accent() }),
                                            if following { ink() } else { ON_ACCENT },
                                            false,
                                            true,
                                        );
                                        if fb.clicked() {
                                            follow = Some((page.title.clone(), page.thumb.clone()));
                                        }
                                    }
                                    let have = self.offline_count(&page.tracks);
                                    let all = have == page.tracks.len();
                                    let label = if all { tr("Downloaded") } else { trf("Download  ({downloaded}/{total})", &[("downloaded", &have.to_string()), ("total", &page.tracks.len().to_string())]) };
                                    if icon_btn(ui, if all { "check" } else { "download" }, &label, None, ink(), false, !all).clicked() {
                                        download_page = true;
                                    }
                                });
                            }
                        });
                    });
                    ui.add_space(14.0);
                    if !page.tracks.is_empty() {
                        if is_artist {
                            ui.label(egui::RichText::new(tr("Songs")).size(18.0).strong());
                            ui.add_space(4.0);
                        }
                        song_rows(ui, &page.tracks, &self.lists, &mut act, &mut play);
                    }
                    ui.add_space(8.0);
                    for (title, cards) in &page.shelves {
                        shelf(ui, title, cards, &mut open, &mut play);
                    }
                });
                if download_page {
                    download_tracks = Some((page.title.clone(), page.tracks.clone()));
                }
                if let Some(sh) = play_all {
                    let items = page.tracks.clone();
                    let start = if sh { (now_secs() as usize) % items.len().max(1) } else { 0 };
                    play = Some((items, start));
                    if sh {
                        self.music_shuffle_next = true;
                    }
                }
            }
        }
        if let Some((name, thumb)) = follow {
            match self.followed.iter().position(|a| a.id == id) {
                Some(i) => {
                    self.followed.remove(i);
                    self.toast(trf("Unfollowed {name}", &[("name", &name)]));
                }
                None => {
                    self.followed.push(FollowedArtist { id: id.to_string(), name: name.clone(), thumb });
                    self.toast(trf("Following {name}", &[("name", &name)]));
                }
            }
            save_followed(&self.followed);
            self.for_you = None;
        }
        if let Some(t) = download_tracks {
            self.download_all(&t.1, &t.0);
        }
        self.music_apply(act, play, open);
    }

    /// Carries out what the user picked on a Music page.
    fn music_apply(&mut self, act: Option<(Video, MAct)>, play: Option<(Vec<Video>, usize)>, open: Option<(CardKind, String)>) {
        if let Some((items, start)) = play {
            let sh = std::mem::take(&mut self.music_shuffle_next);
            self.music_play(items, start, sh);
        }
        if let Some((kind, id)) = open {
            self.music_open(&kind, &id);
        }
        if let Some((v, a)) = act {
            match a {
                MAct::PlayNext => self.queue_add(QItem::Yt(v), true),
                MAct::Queue => self.queue_add(QItem::Yt(v), false),
                MAct::Toggle(list) => self.toggle_in_list(&list, &v),
            }
        }
    }
}

/// A list of songs with a heart and a right-click menu.
fn song_rows(
    ui: &mut egui::Ui,
    songs: &[Video],
    lists: &[LocalList],
    act: &mut Option<(Video, MAct)>,
    play: &mut Option<(Vec<Video>, usize)>,
) {
    for (i, v) in songs.iter().enumerate() {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 4.0, 60.0), Sense::click());
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        if hv > 0.0 {
            ui.painter().rect_filled(r, CornerRadius::same(12), ov((16.0 * hv) as u8));
        }
        let th = Rect::from_min_size(r.min + vec2(6.0, 6.0), vec2(48.0, 48.0));
        ui.painter().rect_filled(th, CornerRadius::same(8), card_hi());
        let uri = img_uri(ui.ctx(), &v.thumb);
        if !uri.is_empty() {
            egui::Image::new(uri).fit_to_exact_size(th.size()).corner_radius(CornerRadius::same(8)).paint_at(ui, th);
        }
        if hv > 0.0 {
            ui.painter().rect_filled(th, CornerRadius::same(8), Color32::from_black_alpha((120.0 * hv) as u8));
            icons::draw(ui.painter(), th.center(), "play", 18.0, Color32::WHITE.gamma_multiply(hv));
        }
        // heart: shows when hovered, stays lit once liked
        let liked = lists.iter().any(|l| l.id == "liked" && l.videos.iter().any(|x| x.id == v.id));
        let heart = Rect::from_center_size(Pos2::new(r.max.x - 84.0, r.center().y), vec2(30.0, 30.0));
        let over_heart = ui.ctx().pointer_hover_pos().is_some_and(|p| heart.contains(p)) && resp.hovered();
        if liked || hv > 0.0 {
            icons::draw(
                ui.painter(),
                heart.center(),
                if liked { "heart_fill" } else { "heart" },
                17.0,
                if liked { LIVE } else if over_heart { ink() } else { muted() },
            );
        }
        let right = r.max.x - 110.0;
        let tx = th.max.x + 14.0;
        let title = truncate(ui, &v.title, FontId::proportional(15.0), (right - tx).max(60.0), 1);
        let sub = [v.channel_name.as_str(), v.meta.as_str()]
            .iter()
            .filter(|s| !s.is_empty() && **s != v.title.as_str())
            .cloned()
            .collect::<Vec<_>>()
            .join("  •  ");
        let sub = truncate(ui, &sub, FontId::proportional(12.5), (right - tx).max(60.0), 1);
        ui.painter().text(Pos2::new(tx, r.center().y - 10.0), Align2::LEFT_CENTER, title, FontId::proportional(15.0), ink());
        ui.painter().text(Pos2::new(tx, r.center().y + 11.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.5), muted());
        ui.painter().text(Pos2::new(r.max.x - 14.0, r.center().y), Align2::RIGHT_CENTER, &v.duration, FontId::proportional(13.0), muted());
        if resp.clicked() {
            if resp.interact_pointer_pos().is_some_and(|p| heart.contains(p)) {
                *act = Some((v.clone(), MAct::Toggle("liked".into())));
            } else {
                *play = Some((songs.to_vec(), i));
            }
        }
        resp.context_menu(|ui| {
            if ui.button(tr("Play next")).clicked() {
                *act = Some((v.clone(), MAct::PlayNext));
                ui.close();
            }
            if ui.button(tr("Add to queue")).clicked() {
                *act = Some((v.clone(), MAct::Queue));
                ui.close();
            }
            ui.menu_button(tr("Add to playlist"), |ui| {
                for l in lists {
                    let has = l.videos.iter().any(|x| x.id == v.id);
                    if ui.selectable_label(has, l.name.clone()).clicked() {
                        *act = Some((v.clone(), MAct::Toggle(l.id.clone())));
                        ui.close();
                    }
                }
            });
        });
    }
}

/// One album / artist / playlist card.
fn card_ui(ui: &mut egui::Ui, c: &Card, side: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(side, side + 50.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let art = Rect::from_min_size(r.min, vec2(side, side));
    let round = if c.kind == CardKind::Artist { (side / 2.0) as u8 } else { 14 };
    ui.painter().rect_filled(art, CornerRadius::same(round), card_hi());
    let uri = img_uri(ui.ctx(), &c.thumb);
    if !uri.is_empty() {
        egui::Image::new(uri).fit_to_exact_size(art.size()).corner_radius(CornerRadius::same(round)).paint_at(ui, art);
    }
    if hv > 0.0 {
        ui.painter().rect_stroke(art, CornerRadius::same(round), Stroke::new(2.0, accent().gamma_multiply(hv)), StrokeKind::Inside);
        if matches!(c.kind, CardKind::Song) {
            icons::draw(ui.painter(), art.center(), "play", 26.0, Color32::WHITE.gamma_multiply(hv));
        }
    }
    let title = truncate(ui, &c.title, FontId::proportional(14.0), side, 1);
    let sub = truncate(ui, &c.sub, FontId::proportional(12.0), side, 1);
    let center = c.kind == CardKind::Artist;
    let (anchor, x) = if center { (Align2::CENTER_TOP, r.center().x) } else { (Align2::LEFT_TOP, r.min.x) };
    ui.painter().text(Pos2::new(x, art.max.y + 8.0), anchor, title, FontId::proportional(14.0), ink());
    ui.painter().text(Pos2::new(x, art.max.y + 28.0), anchor, sub, FontId::proportional(12.0), muted());
    resp
}

/// A titled row of cards.
fn shelf(
    ui: &mut egui::Ui,
    title: &str,
    cards: &[Card],
    open: &mut Option<(CardKind, String)>,
    play: &mut Option<(Vec<Video>, usize)>,
) {
    ui.label(egui::RichText::new(title).size(18.0).strong());
    ui.add_space(6.0);
    let songs: Vec<Video> = cards.iter().filter(|c| c.kind == CardKind::Song).map(ytmusic::card_to_video).collect();
    egui::ScrollArea::horizontal().id_salt(("shelf", title)).auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal(|ui| {
            for c in cards {
                let resp = card_ui(ui, c, 150.0);
                if resp.clicked() {
                    if c.kind == CardKind::Song {
                        let i = songs.iter().position(|s| s.id == c.id).unwrap_or(0);
                        *play = Some((songs.clone(), i));
                    } else {
                        *open = Some((c.kind.clone(), c.id.clone()));
                    }
                }
            }
        });
    });
    ui.add_space(14.0);
}

/// Cards laid out in a wrapping grid.
fn card_grid(ui: &mut egui::Ui, cards: &[Card], open: &mut Option<(CardKind, String)>) {
    let avail = ui.available_width();
    let side = 150.0;
    let gap = 18.0;
    let cols = (((avail + gap) / (side + gap)).floor() as usize).max(1);
    for row in cards.chunks(cols) {
        ui.horizontal(|ui| {
            for c in row {
                if card_ui(ui, c, side).clicked() {
                    *open = Some((c.kind.clone(), c.id.clone()));
                }
            }
        });
        ui.add_space(10.0);
    }
}

