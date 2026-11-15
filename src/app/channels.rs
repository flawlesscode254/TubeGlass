//! Channels, search and playlists fetched from YouTube: refreshing feeds, tabs and in-channel search.

use crate::*;

impl App {
    /// Reloads the feed of every saved channel (and the live list), with a progress count.
    pub(crate) fn refresh_all(&mut self) {
        self.last_refresh = Instant::now();
        self.batch.clear();
        for c in self.channels.clone() {
            if self.refresh(&c) {
                self.batch.insert(c.id.clone());
            }
        }
        // channels that were already loading still count towards this refresh
        for id in self.loading.clone() {
            self.batch.insert(id);
        }
        self.batch_total = self.batch.len();
    }

    /// Returns true when a fetch was started.
    pub(crate) fn refresh(&mut self, c: &Channel) -> bool {
        if !self.loading.insert(c.id.clone()) {
            return false;
        }
        let (tx, ctx, c) = (self.tx.clone(), self.ctx.clone(), c.clone());
        std::thread::spawn(move || {
            let r = yt::channel_videos(&c);
            let _ = tx.send(Msg::Feed(c.id.clone(), r));
            ctx.request_repaint();
        });
        true
    }

    /// Resolves what is typed in the sidebar box (handle, link or id) and saves it as a channel.
    pub(crate) fn add_channel(&mut self) {
        let q = self.input.trim().to_string();
        if q.is_empty() || self.adding {
            return;
        }
        self.adding = true;
        self.add_err = None;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Resolved(yt::resolve_channel(&q)));
            ctx.request_repaint();
        });
    }

    /// Runs the search typed in the top bar, or opens it directly when it is a pasted video / playlist link.
    pub(crate) fn start_search(&mut self) {
        let q = self.search_input.trim().to_string();
        if q.is_empty() {
            return;
        }
        // a pasted link opens directly instead of being searched for
        match classify_link(&q) {
            Link::Playlist(id) => {
                self.search_input.clear();
                self.open_playlist(id);
                return;
            }
            Link::Video(id) => {
                self.search_input.clear();
                let v = Video {
                    id: id.clone(),
                    title: PLACEHOLDER_TITLE.into(),
                    thumb: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
                    duration: String::new(),
                    meta: String::new(),
                    kind: Kind::Video,
                    channel_id: String::new(),
                    channel_name: String::new(),
                    rank: 0,
                    short: false, music: false,
                };
                self.queue.clear();
                self.queue_idx = None;
                self.play(v);
                return;
            }
            Link::None => {}
        }
        self.search_query = q.clone();
        self.recent.retain(|r| !r.eq_ignore_ascii_case(&q));
        self.recent.insert(0, q.clone());
        self.recent.truncate(12);
        save_recent(&self.recent);
        let opts = self.search_opts;
        self.sel = Sel::Search;
        self.filter = Filter::All;
        self.svideos = None;
        self.schannels = None;
        self.splaylists = None;
        self.sugg.clear();
        self.sugg_req = q.clone();
        let (tx, ctx, q3) = (self.tx.clone(), self.ctx.clone(), q.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::SearchPlaylists(q3.clone(), yt::search_playlists(&q3)));
            ctx.request_repaint();
        });
        let (tx, ctx, q2) = (self.tx.clone(), self.ctx.clone(), q.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::SearchVideos(q2.clone(), yt::search_videos(&q2, opts)));
            ctx.request_repaint();
        });
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::SearchChannels(q.clone(), yt::search_channels(&q)));
            ctx.request_repaint();
        });
    }

    /// Saves a channel we already know the details of (from a search hit) and loads its feed.
    pub(crate) fn add_known_channel(&mut self, c: Channel) {
        if !self.channels.iter().any(|x| x.id == c.id) {
            self.channels.push(c.clone());
            save(&self.channels);
            self.refresh(&c);
        }
    }

    /// Forgets a channel together with its cached feed and group memberships.
    pub(crate) fn remove_channel(&mut self, id: &str) {
        for g in &mut self.groups {
            g.channels.retain(|c| c != id);
        }
        save_groups(&self.groups);
        self.channels.retain(|c| c.id != id);
        self.feeds.remove(id);
        if self.sel == Sel::Channel(id.to_string()) {
            self.sel = Sel::Home;
        }
        save(&self.channels);
    }

    /// Opens a YouTube playlist page and fetches its videos.
    pub(crate) fn open_playlist(&mut self, id: String) {
        self.sel = Sel::Playlist(id.clone());
        self.filter = Filter::All;
        self.playlist = Some((id.clone(), None));
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = yt::playlist_videos(&id);
            let _ = tx.send(Msg::Playlist(id, r));
            ctx.request_repaint();
        });
    }

    /// True when the given channel tab was already fetched (so it can be shown at once).
    pub(crate) fn tab_cached(&self, id: &str, tab: ChanTab) -> bool {
        let key = format!("{id}:{}", tab.key());
        match tab {
            ChanTab::Videos | ChanTab::Live | ChanTab::Shorts => self.fcache.tab_videos.contains_key(&key),
            ChanTab::Playlists => self.fcache.tab_playlists.contains_key(id),
            ChanTab::About => self.fcache.about.contains_key(id),
        }
    }

    /// Fetch the open channel tab if we have nothing cached for it (or when forced).
    pub(crate) fn ensure_tab(&mut self, force: bool) {
        let Sel::Channel(id) = self.sel.clone() else { return };
        let tab = self.chan_tab;
        let key = format!("{id}:{}", tab.key());
        if force {
            self.tab_err.remove(&key);
        }
        if (self.tab_cached(&id, tab) && !force) || self.tab_err.contains_key(&key) {
            return;
        }
        if !self.tab_loading.insert(key.clone()) {
            return;
        }
        let Some(ch) = self.channels.iter().find(|c| c.id == id).cloned() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = yt::channel_tab(&ch, tab.key());
            let _ = tx.send(Msg::ChanTab(ch.id, tab.key().to_string(), r));
            ctx.request_repaint();
        });
    }

    /// Starts a search inside the open channel for the text in its search box.
    pub(crate) fn start_chan_search(&mut self) {
        let Sel::Channel(id) = self.sel.clone() else { return };
        let q = self.chan_q.trim().to_string();
        if q.is_empty() {
            self.chan_search = None;
            return;
        }
        let Some(ch) = self.channels.iter().find(|c| c.id == id).cloned() else { return };
        self.chan_search = Some(ChanSearch { chan: id, query: q.clone(), res: None });
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = yt::channel_search(&ch, &q);
            let _ = tx.send(Msg::ChanSearch(ch.id, q, r));
            ctx.request_repaint();
        });
    }

    /// Results of the active in-channel search for the open channel, if any.
    pub(crate) fn active_chan_search(&self) -> Option<&ChanSearch> {
        match (&self.sel, &self.chan_search) {
            (Sel::Channel(id), Some(cs)) if &cs.chan == id => Some(cs),
            _ => None,
        }
    }

    /// Fetches the next page of the open channel tab (infinite scroll).
    pub(crate) fn load_more_tab(&mut self) {
        let Sel::Channel(id) = self.sel.clone() else { return };
        let tab = self.chan_tab;
        let key = format!("{id}:{}", tab.key());
        let Some(token) = self.fcache.tab_tokens.get(&key).cloned() else { return };
        if self.tab_err.contains_key(&key) || !self.more_loading.insert(key) {
            return;
        }
        let Some(ch) = self.channels.iter().find(|c| c.id == id).cloned() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = yt::channel_more(&ch, tab.key(), &token);
            let _ = tx.send(Msg::ChanMore(ch.id, tab.key().to_string(), r));
            ctx.request_repaint();
        });
    }

    /// Fetches the "Live now" list across the saved channels.
    pub(crate) fn load_live(&mut self) {
        self.live_loading = true;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Live(yt::live_now()));
            ctx.request_repaint();
        });
    }

    /// Marks everything in a channel's feed as seen (clears its NEW badges).
    pub(crate) fn mark_channel_seen(&mut self, id: &str) {
        if let Some(Ok(v)) = self.feeds.get(id) {
            for x in v {
                if self.seen.insert(x.id.clone()) {
                    self.seen_dirty = true;
                }
            }
        }
    }

    /// Marks every video in every feed as seen.
    pub(crate) fn mark_all_seen(&mut self) {
        let ids: Vec<String> = self
            .feeds
            .values()
            .filter_map(|r| r.as_ref().ok())
            .flat_map(|v| v.iter().map(|x| x.id.clone()))
            .collect();
        for i in ids {
            self.seen.insert(i);
        }
        self.seen_dirty = true;
    }

    /// Navigate to a channel page (adding it to the saved channels if needed).
    pub(crate) fn show_channel(&mut self, c: Channel) {
        if self.watching.is_some() && !self.pip {
            self.leave_player_for_navigation();
        }
        self.chan_tab = ChanTab::Videos;
        self.filter = Filter::All;
        self.sel = Sel::Channel(c.id.clone());
        self.add_known_channel(c.clone());
        self.mark_channel_seen(&c.id);
        // fill in the avatar / banner info if this channel was only known by name
        if c.avatar.is_empty() {
            let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), c.id.clone());
            std::thread::spawn(move || {
                if let Ok(full) = yt::resolve_channel(&id) {
                    let _ = tx.send(Msg::ChannelInfo(id, full));
                    ctx.request_repaint();
                }
            });
        }
    }

    /// Open the channel that owns `v`: by id when known, else by saved name, else by search.
    pub(crate) fn open_channel_of(&mut self, v: &Video) {
        // a podcast episode's "channel" is its show
        if v.id.starts_with("pod:") {
            if self.pods.iter().any(|p| p.show.feed == v.channel_id) {
                self.leave_player_for_navigation();
                self.sel = Sel::Podcast(v.channel_id.clone());
            }
            return;
        }
        if !v.channel_id.is_empty() {
            let known = self.channels.iter().find(|c| c.id == v.channel_id).cloned();
            self.show_channel(known.unwrap_or(Channel {
                id: v.channel_id.clone(),
                name: v.channel_name.clone(),
                avatar: String::new(),
            }));
            return;
        }
        if let Some(c) = self.channels.iter().find(|c| c.name == v.channel_name).cloned() {
            self.show_channel(c);
            return;
        }
        let (tx, ctx, name) = (self.tx.clone(), self.ctx.clone(), v.channel_name.clone());
        self.toast(trf("Opening {name}…", &[("name", &name)]));
        std::thread::spawn(move || {
            let r = yt::search_channels(&name).and_then(|hits| {
                hits.into_iter()
                    .find(|h| h.channel.name == name)
                    .map(|h| h.channel)
                    .ok_or_else(|| "channel not found".to_string())
            });
            let _ = tx.send(Msg::OpenChannel(r));
            ctx.request_repaint();
        });
    }
}
