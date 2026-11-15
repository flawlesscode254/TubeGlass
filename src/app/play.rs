//! Playing media: starting videos, files and songs, the queue, gapless hand-over, resume and captions.

use crate::*;

impl App {
    /// Resolve stream URLs ahead of time (called while a card is hovered, and by play()).
    pub(crate) fn fetch_streams(&mut self, id: &str) {
        if id.starts_with("pod:") {
            return;
        }
        let fresh = self
            .cache
            .get(id)
            .is_some_and(|(t, _)| t.elapsed() < Duration::from_secs(20 * 60));
        if fresh || !self.inflight.insert(id.to_string()) {
            return;
        }
        let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), id.to_string());
        std::thread::spawn(move || {
            let r = yt::streams(&id);
            let _ = tx.send(Msg::Streams(id, r));
            ctx.request_repaint();
        });
    }

    /// Add (or bring to the front) a video in the watch history.
    pub(crate) fn record_view(&mut self, v: &Video) {
        let now = now_secs();
        if let Some(i) = self.history.iter().position(|h| h.video.id == v.id) {
            let mut e = self.history.remove(i);
            e.last_viewed = now;
            // keep a richer thumbnail / title if the new one is empty
            if !v.title.is_empty() {
                e.video.title = v.title.clone();
            }
            if !v.thumb.is_empty() {
                e.video.thumb = v.thumb.clone();
            }
            if !v.channel_name.is_empty() {
                e.video.channel_name = v.channel_name.clone();
            }
            self.history.insert(0, e);
        } else {
            self.history.insert(
                0,
                HistEntry { video: v.clone(), last_viewed: now, position: 0.0, duration: 0.0 },
            );
        }
        self.history.truncate(500);
        save_history(&self.history);
        self.hist_saved = Instant::now();
    }

    /// Where to start `id`: where it was left, unless that was the very start or the end.
    pub(crate) fn resume_pos(&self, id: &str) -> f64 {
        if !self.settings.resume {
            return 0.0;
        }
        self.history
            .iter()
            .find(|h| h.video.id == id)
            .filter(|h| h.position > 5.0 && (h.duration <= 0.0 || h.position < h.duration - 8.0))
            .map_or(0.0, |h| h.position)
    }

    /// Remember the playback position of what is open. Called every frame; it writes to
    /// disk at most every few seconds unless `force`d.
    pub(crate) fn record_progress(&mut self, force: bool) {
        let Some(w) = &self.watching else { return };
        if self.player.is_live() || self.player.streams.is_none() {
            return;
        }
        let state = self.player.state();
        if !matches!(state, State::Playing | State::Paused | State::Buffering | State::Ended) {
            return;
        }
        let dur = self.player.duration();
        let pos = if state == State::Ended { 0.0 } else { self.player.position() };
        let id = w.id.clone();
        if let Some(e) = self.history.iter_mut().find(|h| h.video.id == id) {
            e.position = pos;
            if dur > 0.0 {
                e.duration = dur;
            }
            if force || self.hist_saved.elapsed() > Duration::from_secs(3) {
                save_history(&self.history);
                self.hist_saved = Instant::now();
            }
        }
    }

    /// Starts a video or song: resolves its streams (or the downloaded file) and begins playback.
    pub(crate) fn play(&mut self, v: Video) {
        self.release_focus = true;
        self.record_progress(true);
        // files from the Local tab or Open file… have no YouTube stream to resolve
        if let Some(path) = v.id.strip_prefix("local:") {
            self.open_local(path.to_string());
            return;
        }
        // an episode that was downloaded plays from the file
        if v.id.starts_with("pod:") {
            if let Some(d) = self.library.iter().find(|d| d.id == v.id).cloned() {
                self.play_local(&d);
                return;
            }
        }
        // Opening the video that is already in the PiP window just brings it back full
        // size where it is, instead of restarting it.
        if self.pip && self.watching.as_ref().is_some_and(|w| w.id == v.id) {
            self.pip = false;
            return;
        }
        self.pip = false;
        self.player.audio_mode = v.music;
        let sp = self.settings.chan_prefs.get(&v.channel_id).map(|p| p.speed).filter(|s| *s > 0.0).unwrap_or(self.settings.speed);
        self.player.speed = sp.clamp(0.5, 2.0);
        self.player.set_loading();
        let id = v.id.clone();
        self.record_view(&v);
        if self.seen.insert(v.id.clone()) {
            self.seen_dirty = true;
        }
        self.watching = Some(v);
        self.comments = None;
        self.comment_sorts.clear();
        self.replies.clear();
        self.comments_next = None;
        if id.starts_with("pod:") {
            self.fetch_pod_streams(&id);
        } else {
            match self.cache.get(&id) {
                Some((t, s)) if t.elapsed() < Duration::from_secs(20 * 60) => {
                    let s = s.clone();
                    let start = self.resume_pos(&id);
                    self.player.load(s, start);
                }
                _ => self.fetch_streams(&id),
            }
        }
        if self.comments_open {
            self.load_comments(false);
        }
    }

    /// Fetches the next page of comments for the playing video.
    pub(crate) fn load_comments(&mut self, more: bool) {
        self.fetch_comments(more, None);
    }

    /// `sort_token` switches the order (Top / Newest) and starts the list over.
    pub(crate) fn fetch_comments(&mut self, more: bool, sort_token: Option<String>) {
        let Some(w) = &self.watching else { return };
        if non_yt(&w.id) {
            return;
        }
        if self.comments_busy && sort_token.is_none() {
            return;
        }
        let vid = w.id.clone();
        let token = match (&sort_token, more) {
            (Some(t), _) => Some(t.clone()),
            (None, true) => self.comments_next.clone(),
            (None, false) => None,
        };
        if more && token.is_none() {
            return;
        }
        self.comments_busy = true;
        if sort_token.is_some() {
            self.comments = None;
        }
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = yt::comments_page(&vid, token.as_deref());
            let _ = tx.send(Msg::Comments(vid, more, r));
            ctx.request_repaint();
        });
    }

    /// Fetches the replies of one comment (first page, or the next when `append`).
    pub(crate) fn fetch_replies(&mut self, parent: &str, token: String, append: bool) {
        let Some(w) = &self.watching else { return };
        let vid = w.id.clone();
        let st = self.replies.entry(parent.to_string()).or_default();
        st.open = true;
        st.loading = true;
        st.err = None;
        let (tx, ctx, parent) = (self.tx.clone(), self.ctx.clone(), parent.to_string());
        std::thread::spawn(move || {
            let r = yt::replies(&token);
            let _ = tx.send(Msg::Replies(vid, parent, append, r));
            ctx.request_repaint();
        });
    }

    /// Opens a media file from disk (probes it first, then plays it).
    pub(crate) fn open_local(&mut self, path: String) {
        let path = media::normalize_arg(&path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            self.last_dir = Some(parent.to_path_buf());
        }
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = media::probe(&path);
            let _ = tx.send(Msg::LocalProbe(path, r));
            ctx.request_repaint();
        });
    }

    /// Starts playing a local file whose details were already read.
    pub(crate) fn play_file(&mut self, path: &str, streams: Streams, title: String, artist: String) {
        self.release_focus = true;
        self.record_progress(true);
        self.pip = false;
        self.comments_open = false;
        self.player.set_loading();
        let folder = std::path::Path::new(path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| tr("Local file"));
        self.watching = Some(Video {
            id: format!("local:{path}"),
            title,
            thumb: String::new(),
            duration: String::new(),
            meta: String::new(),
            kind: Kind::Video,
            channel_id: String::new(),
            channel_name: if artist.is_empty() { folder } else { artist },
            rank: 0,
            short: false, music: false,
        });
        self.comments = None;
        self.comment_sorts.clear();
        self.replies.clear();
        self.comments_next = None;
        if let Some(w) = self.watching.clone() {
            self.record_view(&w);
        }
        let start = self.resume_pos(&format!("local:{path}"));
        self.player.load(streams, start);
    }

    /// Native "open file" dialog of the OS (xdg-desktop-portal on Linux, Cocoa / Win32 elsewhere).
    pub(crate) fn start_browser(&mut self) {
        if self.picking {
            return;
        }
        self.picking = true;
        let dir = self
            .last_dir
            .clone()
            .or_else(dirs::video_dir)
            .or_else(dirs::home_dir);
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let mut d = rfd::FileDialog::new()
                .set_title(tr("Open media file"))
                .add_filter(tr("Media files"), media::EXTENSIONS)
                .add_filter(tr("All files"), &["*"]);
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            let picked = d.pick_file().map(|p| p.display().to_string());
            let _ = tx.send(Msg::Picked(picked));
            ctx.request_repaint();
        });
    }

    /// Plays a downloaded item from its file, with its saved captions.
    pub(crate) fn play_local(&mut self, item: &DlItem) {
        self.release_focus = true;
        self.record_progress(true);
        self.pip = false;
        self.player.set_loading();
        self.watching = Some(Video {
            id: item.id.clone(),
            title: item.title.clone(),
            thumb: item.thumb.clone(),
            duration: String::new(),
            meta: String::new(),
            kind: Kind::Video,
            channel_id: String::new(),
            channel_name: item.channel.clone(),
            rank: 0,
            short: false, music: item.music,
        });
        self.comments = None;
        self.comment_sorts.clear();
        self.replies.clear();
        self.comments_next = None;
        let streams = Streams {
            captions: download::offline_captions(item),
            chapters: vec![],
            storyboard: None,
            title: item.title.clone(),
            is_live: false,
            duration: item.duration,
            video: vec![yt::VideoOpt {
                url: item.file.clone(),
                width: item.width,
                // height 0 marks an audio-only file for the player
                height: if item.audio_only { 0 } else { item.height },
                ua: String::new(),
                muxed: true,
                hls: true,
                audio_url: None,
            }],
            audio: None,
        };
        if let Some(w) = self.watching.clone() {
            self.record_view(&w);
        }
        let start = self.resume_pos(&item.id);
        self.player.load(streams, start);
    }

    /// Stops playback and leaves the player page.
    pub(crate) fn close_player(&mut self) {
        self.record_progress(true);
        self.pip = false;
        self.player.stop();
        self.watching = None;
        if self.fullscreen {
            self.fullscreen = false;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    /// Replace the queue; `start` is the item to play first. With shuffle on, everything
    /// after the start item is shuffled.
    pub(crate) fn set_queue(&mut self, mut items: Vec<QItem>, start: usize) {
        if items.is_empty() {
            self.queue.clear();
            self.queue_idx = None;
            return;
        }
        let start = start.min(items.len() - 1);
        if self.settings.shuffle {
            let first = items.remove(start);
            let mut x = now_secs() ^ 0x9E37_79B9_7F4A_7C15;
            for i in (1..items.len()).rev() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                items.swap(i, (x % (i as u64 + 1)) as usize);
            }
            items.insert(0, first);
            self.queue = items;
            self.queue_idx = Some(0);
        } else {
            self.queue = items;
            self.queue_idx = Some(start);
        }
    }

    /// Plays the queue entry at position `i`.
    pub(crate) fn play_queue_at(&mut self, i: usize) {
        let Some(item) = self.queue.get(i).cloned() else { return };
        self.queue_idx = Some(i);
        match item {
            QItem::Yt(v) => self.play(v),
            QItem::Dl(d) => self.play_local(&d),
        }
    }

    /// Moves to the next queue entry (wrapping when repeat-all is on). False when there is none.
    pub(crate) fn queue_next(&mut self) -> bool {
        let Some(i) = self.queue_idx else { return false };
        let next = if i + 1 < self.queue.len() {
            Some(i + 1)
        } else if self.settings.repeat == Repeat::All && !self.queue.is_empty() {
            Some(0)
        } else {
            None
        };
        match next {
            Some(n) => {
                self.play_queue_at(n);
                true
            }
            None => false,
        }
    }

    /// A stream that YouTube refuses at playback is fetched again through the other clients,
    /// once automatically; the error screen's button does the same on demand.
    pub(crate) fn recover_playback(&mut self) {
        let Some(w) = self.watching.clone() else { return };
        let State::Error(e) = self.player.state() else {
            self.want_retry = false;
            return;
        };
        let local = non_yt(&w.id) || self.player.streams.as_ref().is_some_and(|s| s.video.first().is_some_and(|v| crate::platform::is_local_path(&v.url)));
        if local {
            return;
        }
        let refused = e.contains("opening input") || e.contains("Input/output error") || e.contains("403") || e.contains("Server returned");
        let manual = std::mem::take(&mut self.want_retry);
        if !(manual || (refused && !self.err_retried.contains(&w.id))) {
            return;
        }
        if std::env::var_os("TG_DEBUG").is_some() {
            eprintln!("[recover] first attempt failed: {e}");
        }
        self.err_retried.insert(w.id.clone());
        self.cache.remove(&w.id);
        self.inflight.insert(w.id.clone());
        self.player.set_loading();
        self.toast(tr("Trying another way to play this video…"));
        let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), w.id.clone());
        std::thread::spawn(move || {
            let r = yt::streams_alt(&id);
            let _ = tx.send(Msg::Streams(id, r));
            ctx.request_repaint();
        });
    }

    /// Builds the ffmpeg audio filter chain (equalizer, loudness, silence) and applies it.
    pub(crate) fn sync_audio_filters(&mut self) {
        // dragging a slider would restart the decoder on every frame
        if self.sel == Sel::Settings && self.ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let s = &self.settings;
        let mut f: Vec<String> = vec![];
        for (hz, g) in [(80, s.eq_bass), (1000, s.eq_mid), (8000, s.eq_treble)] {
            if g.abs() > 0.05 {
                f.push(format!("equalizer=f={hz}:t=q:w=0.9:g={g:.1}"));
            }
        }
        if s.normalize {
            f.push("dynaudnorm=f=150:g=7:p=0.9".into());
        }
        let spoken = self.player.audio_mode && !self.watching.as_ref().is_some_and(|w| w.music);
        if s.skip_silence && spoken {
            f.push("silenceremove=stop_periods=-1:stop_duration=0.7:stop_threshold=-50dB".into());
        }
        self.player.set_afx(f.join(","));
    }

    /// Restarts the item when it has played a few seconds, otherwise goes to the previous one.
    pub(crate) fn queue_prev(&mut self) {
        // like most players: past the first seconds, "previous" restarts the item
        if self.player.position() > 5.0 || self.queue_idx.is_none_or(|i| i == 0) {
            self.player.seek(0.0);
            return;
        }
        if let Some(i) = self.queue_idx {
            self.play_queue_at(i - 1);
        }
    }

    /// Called every frame: moves on (or repeats) when the current item has ended.
    /// Gapless playback for audio-only queues: the next track is decoded while this one plays
    /// and the sound runs straight into it (with an optional crossfade).
    pub(crate) fn gapless_tick(&mut self) {
        if !self.settings.gapless || self.watching.is_none() {
            return;
        }
        // "stop after this one" must not run on into the next track
        if matches!(self.sleep, Some(SleepMode::EndOfVideo)) && !self.player.has_preloaded() {
            return;
        }
        if !self.player.audio_only() || self.player.speed != 1.0 || self.player.is_live() {
            self.gapless_next = None;
            return;
        }
        if self.player.poll_gapless().is_some() {
            self.advance_gapless();
            return;
        }
        if self.player.has_preloaded() {
            return;
        }
        // a preload that was lost (seek, quality change) is forgotten and made again
        self.gapless_next = None;
        if self.player.position() < 2.0 || self.player.duration() <= 0.0 || self.queue.len() < 2 {
            return;
        }
        let Some(i) = self.queue_idx else { return };
        if self.settings.repeat == Repeat::One {
            return;
        }
        let n = if i + 1 < self.queue.len() {
            i + 1
        } else if self.settings.repeat == Repeat::All {
            0
        } else {
            return;
        };
        let QItem::Yt(v) = self.queue[n].clone() else { return };
        if v.kind != Kind::Video {
            return;
        }
        let streams = match self.cache.get(&v.id) {
            Some((t, s)) if t.elapsed() < Duration::from_secs(20 * 60) => s.clone(),
            _ => {
                self.fetch_streams(&v.id);
                return;
            }
        };
        if streams.is_live {
            return;
        }
        if self.player.preload(&streams, self.settings.crossfade as f32) {
            self.gapless_next = Some((n, v));
        }
    }

    /// The sound has crossed into the prepared track: make the rest of the app follow.
    pub(crate) fn advance_gapless(&mut self) {
        let Some((n, v)) = self.gapless_next.take() else { return };
        if std::env::var_os("TG_DEBUG").is_some() {
            eprintln!("[gapless] switched to #{n} {} at position {:.2}s", v.title, self.player.position());
        }
        // the track that just ended counts as finished
        if let Some(prev) = self.watching.clone() {
            if let Some(h) = self.history.iter_mut().find(|h| h.video.id == prev.id) {
                h.position = 0.0;
            }
            if let Some(l) = self.lists.iter_mut().find(|l| l.id == "watchlater") {
                l.videos.retain(|x| x.id != prev.id);
                save_lists(&self.lists);
            }
        }
        self.queue_idx = Some(n);
        self.record_view(&v);
        if self.seen.insert(v.id.clone()) {
            self.seen_dirty = true;
        }
        self.watching = Some(v);
        self.comments = None;
        self.comment_sorts.clear();
        self.replies.clear();
        self.comments_next = None;
        self.ended_handled.clear();
        if self.comments_open && self.side_tab == SideTab::Comments {
            self.load_comments(false);
        }
    }

    /// When an item ends: repeat it, stop for the sleep timer, or move on in the queue.
    pub(crate) fn handle_end_of_playback(&mut self) {
        let Some(w) = &self.watching else { return };
        let id = w.id.clone();
        if self.player.state() != State::Ended {
            if self.ended_handled == id {
                // it started again (seek / replay): arm for the next end
                self.ended_handled.clear();
            }
            return;
        }
        if self.ended_handled == id {
            return;
        }
        self.ended_handled = id.clone();
        // a finished video leaves the Watch later list
        if let Some(l) = self.lists.iter_mut().find(|l| l.id == "watchlater") {
            if let Some(i) = l.videos.iter().position(|v| v.id == id) {
                l.videos.remove(i);
                save_lists(&self.lists);
            }
        }
        if self.settings.repeat == Repeat::One {
            self.player.seek(0.0);
            self.ended_handled.clear();
            return;
        }
        if matches!(self.sleep, Some(SleepMode::EndOfVideo)) {
            self.sleep = None;
            self.toast(tr("Sleep timer: stopped after this video"));
            return;
        }
        if self.settings.autoplay && self.queue_next() {
            return;
        }
        if self.settings.repeat == Repeat::All && self.queue.is_empty() {
            self.player.seek(0.0);
            self.ended_handled.clear();
        }
    }

    /// Adds an item to the queue, either right after the current one or at the end.
    pub(crate) fn queue_add(&mut self, item: QItem, next: bool) {
        // adding while something plays outside a queue: the playing item becomes item 0
        if self.queue.is_empty() {
            if let Some(w) = &self.watching {
                self.queue.push(QItem::Yt(w.clone()));
                self.queue_idx = Some(0);
            }
        }
        let at = if next { self.queue_idx.map_or(0, |i| i + 1) } else { self.queue.len() };
        self.queue.insert(at.min(self.queue.len()), item);
        if self.queue_idx.is_none() && self.watching.is_none() {
            self.queue_idx = Some(0);
        }
        self.toast(tr(if next { "Playing next" } else { "Added to queue" }));
    }

    /// Loads the caption track `idx` of the playing video so it can be drawn.
    pub(crate) fn load_cues(&mut self, idx: usize) {
        let Some(w) = &self.watching else { return };
        let Some(tr) = self.player.streams.as_ref().and_then(|s| s.captions.get(idx)).cloned() else { return };
        self.cc_track = Some(idx);
        self.cues.clear();
        let (tx, ctx, vid) = (self.tx.clone(), self.ctx.clone(), w.id.clone());
        std::thread::spawn(move || {
            let r = yt::caption_cues(&tr.url);
            let _ = tx.send(Msg::Cues(vid, idx, r));
            ctx.request_repaint();
        });
    }

    /// Per-video housekeeping that depends on the resolved streams: captions, SponsorBlock.
    pub(crate) fn per_video_setup(&mut self) {
        let Some(w) = self.watching.clone() else { return };
        if w.id.starts_with("pod:") && self.player.streams.is_some() && self.cc_for != w.id {
            self.cc_for = w.id.clone();
            self.cues.clear();
            self.cc_track = None;
            if self.settings.captions_default && self.player.streams.as_ref().is_some_and(|s| !s.captions.is_empty()) {
                self.load_cues(0);
            }
        }
        if non_yt(&w.id) || self.player.streams.is_none() || self.player.is_live() {
            return;
        }
        if self.cc_for != w.id {
            self.cc_for = w.id.clone();
            self.cues.clear();
            self.cc_track = None;
            if self.settings.captions_default {
                let lang = self.settings.caption_lang.to_lowercase();
                let pick = self.player.streams.as_ref().and_then(|s| {
                    let by = |auto: bool| s.captions.iter().position(|c| c.auto == auto && c.lang.to_lowercase().starts_with(&lang));
                    by(false).or_else(|| by(true))
                });
                if let Some(i) = pick {
                    self.load_cues(i);
                }
            }
        }
        if self.settings.sponsorblock && self.sponsor_for != w.id {
            self.sponsor_for = w.id.clone();
            self.sponsor.clear();
            self.last_skip = None;
            let (tx, ctx, vid) = (self.tx.clone(), self.ctx.clone(), w.id.clone());
            std::thread::spawn(move || {
                let r = yt::sponsor_segments(&vid);
                let _ = tx.send(Msg::Sponsor(vid, r));
                ctx.request_repaint();
            });
        }
        // jump over marked segments
        if self.settings.sponsorblock && !self.player.paused && !self.sponsor.is_empty() {
            let pos = self.player.position();
            if let Some(i) = self.sponsor.iter().position(|s| pos >= s.start && pos < s.end - 0.4) {
                if self.last_skip != Some(i) {
                    self.last_skip = Some(i);
                    let (end, cat) = (self.sponsor[i].end, self.sponsor[i].category.clone());
                    self.player.seek(end + 0.05);
                    self.toast(trf("Skipped {category}", &[("category", &cat)]));
                }
            } else if self.last_skip.is_some_and(|i| pos < self.sponsor.get(i).map_or(0.0, |s| s.start - 1.0)) {
                self.last_skip = None;
            }
        }
    }

    /// Play every video of the list that is on screen.
    pub(crate) fn play_all_visible(&mut self, shuffle: bool) {
        let items: Vec<QItem> = self
            .visible_videos()
            .into_iter()
            .filter(|v| v.kind != Kind::Upcoming)
            .map(QItem::Yt)
            .collect();
        if items.is_empty() {
            return;
        }
        let was = self.settings.shuffle;
        self.settings.shuffle = shuffle || was;
        let n = items.len();
        let start = if shuffle { (now_secs() as usize) % n } else { 0 };
        self.set_queue(items, start);
        self.settings.shuffle = was;
        self.play_queue_at(self.queue_idx.unwrap_or(0));
    }

    /// Leaving a video that is playing keeps it going in the picture-in-picture window;
    /// anything else (paused, ended, failed, still loading) is simply closed.
    pub(crate) fn back_from_player(&mut self) {
        if matches!(self.player.state(), State::Playing | State::Buffering) {
            self.enter_pip();
        } else {
            self.close_player();
        }
    }

    /// Keeps playback going as picture-in-picture (or the bottom bar) while another page opens.
    pub(crate) fn leave_player_for_navigation(&mut self) {
        if matches!(self.player.state(), State::Playing | State::Buffering | State::Paused) {
            self.enter_pip();
        } else {
            self.close_player();
        }
    }

    /// Moves the playing video into the picture-in-picture window.
    pub(crate) fn enter_pip(&mut self) {
        if self.watching.is_none() {
            return;
        }
        self.pip = true;
        self.comments_open = false;
        if self.fullscreen {
            self.fullscreen = false;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    /// Opens or closes the comments side panel (loading them the first time).
    pub(crate) fn toggle_comments(&mut self) {
        if self.comments_open && self.side_tab != SideTab::Comments {
            self.side_tab = SideTab::Comments;
            return;
        }
        self.side_tab = SideTab::Comments;
        self.comments_open = !self.comments_open;
        if self.comments_open && self.comments.is_none() {
            self.load_comments(false);
        }
    }

    /// A small always-on-top window with just the transport controls.
    pub(crate) fn set_mini(&mut self, on: bool) {
        if on == self.mini {
            return;
        }
        if on && self.fullscreen {
            self.toggle_fullscreen();
        }
        let c = self.ctx.clone();
        if on {
            self.pre_mini = Some((self.win.w, self.win.h));
            self.mini = true;
            self.pip = false;
            c.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(380.0, 70.0)));
            c.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(640.0, 74.0)));
            c.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        } else {
            self.mini = false;
            let (w, h) = self.pre_mini.take().unwrap_or((1280.0, 800.0));
            c.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
            c.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(820.0, 520.0)));
            c.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(w.max(820.0), h.max(520.0))));
        }
    }

    /// Switches fullscreen on or off.
    pub(crate) fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }
}
