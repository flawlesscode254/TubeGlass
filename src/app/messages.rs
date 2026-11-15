//! Receives the results of background work (network, ffmpeg, files) and applies them to the state.

use crate::*;

impl App {
    /// Applies every finished piece of background work (network, ffmpeg, files) to the app state.
    pub(crate) fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Resolved(r) => {
                    self.adding = false;
                    match r {
                        Ok(c) => {
                            self.input.clear();
                            if !self.channels.iter().any(|x| x.id == c.id) {
                                self.channels.push(c.clone());
                                save(&self.channels);
                            }
                            self.sel = Sel::Channel(c.id.clone());
                            self.refresh(&c);
                        }
                        Err(e) => self.add_err = Some(e),
                    }
                }
                Msg::Feed(id, r) => {
                    // a channel's very first load is the baseline, not "new" content
                    let first = !matches!(self.feeds.get(&id), Some(Ok(_)));
                    if !first && self.settings.notifications && self.settings.chan_prefs.get(&id).is_some_and(|p| p.notify == 1) {
                        if let (Some(Ok(old)), Ok(new)) = (self.feeds.get(&id), &r) {
                            let known: HashSet<&String> = old.iter().map(|v| &v.id).collect();
                            let name = self.channels.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_default();
                            for v in new.iter().filter(|v| v.kind == Kind::Video && !known.contains(&v.id)).take(3) {
                                if self.notified.insert(v.id.clone()) {
                                mpris::notify(&trf("New from {name}", &[("name", &name)]), &v.title);
                                }
                            }
                        }
                    }
                    if first {
                        if let Ok(v) = &r {
                            for x in v {
                                self.seen.insert(x.id.clone());
                            }
                            self.seen_dirty = true;
                        }
                    }
                    self.loading.remove(&id);
                    self.batch.remove(&id);
                    if self.batch.is_empty() {
                        self.batch_total = 0;
                    }
                    match r {
                        Ok(v) => {
                            self.fcache.feeds.insert(id.clone(), v.clone());
                            self.fcache.updated.insert(id.clone(), now_secs());
                            save_feed_cache(&self.fcache);
                            self.feeds.insert(id, Ok(v));
                        }
                        // keep the cached list if a refresh fails
                        Err(e) => {
                            if !matches!(self.feeds.get(&id), Some(Ok(_))) {
                                self.feeds.insert(id, Err(e));
                            }
                        }
                    }
                }
                Msg::SearchVideos(q, r) => {
                    if q == self.search_query {
                        self.svideos = Some(r);
                    }
                }
                Msg::SearchChannels(q, r) => {
                    if q == self.search_query {
                        self.schannels = Some(r);
                    }
                }
                Msg::SearchPlaylists(q, r) => {
                    if q == self.search_query {
                        self.splaylists = Some(r);
                    }
                }
                Msg::Suggestions(q, list) => {
                    if q == self.sugg_req {
                        self.sugg = list;
                    }
                }
                Msg::Playlist(id, r) => {
                    if self.playlist.as_ref().is_some_and(|p| p.0 == id) {
                        self.playlist = Some((id, Some(r)));
                    }
                }
                Msg::ListImported(id, r) => {
                    let mut left = 0;
                    if let Some((_, n)) = self.import_list.as_mut() {
                        *n = n.saturating_sub(1);
                        left = *n;
                    }
                    match r {
                        Ok((name, videos)) if !videos.is_empty() => {
                            let n = videos.len();
                            let single = self.import_list.as_ref().is_some_and(|(t, _)| playlist_ids(t).len() == 1);
                            if single {
                                self.save_playlist_locally(name, videos);
                            } else {
                                let mut name = name;
                                if self.lists.iter().any(|l| l.name == name) {
                                    name = format!("{name} (copy)");
                                }
                                let lid = format!("pl-{}-{}", now_secs(), self.lists.len());
                                self.lists.push(LocalList { id: lid, name: name.clone(), videos, offline: false });
                                save_lists(&self.lists);
                                self.toast(trf("Imported “{name}” with {count} videos", &[("name", &name), ("count", &n.to_string())]));
                            }
                        }
                        Ok(_) => self.toast(tr("That playlist has no videos (or it is private)")),
                        Err(e) => self.toast(trf("Could not import {id}: {error}", &[("id", &id), ("error", &e)])),
                    }
                    if left == 0 {
                        self.import_list = None;
                    }
                }
                Msg::ChanTab(id, tab, r) => {
                    let key = format!("{id}:{tab}");
                    self.tab_loading.remove(&key);
                    match r {
                        Ok((header, data, next)) => {
                            if !header.handle.is_empty() || !header.subs.is_empty() {
                                self.fcache.headers.insert(id.clone(), header);
                            }
                            match next {
                                Some(t) => self.fcache.tab_tokens.insert(key.clone(), t),
                                None => self.fcache.tab_tokens.remove(&key),
                            };
                            match data {
                                TabData::Videos(v) => {
                                    self.fcache.tab_videos.insert(key, v);
                                }
                                TabData::Playlists(p) => {
                                    self.fcache.tab_playlists.insert(id, p);
                                }
                                TabData::About(a) => {
                                    self.fcache.about.insert(id, a);
                                }
                            }
                            save_feed_cache(&self.fcache);
                        }
                        Err(e) => {
                            self.tab_err.insert(key, e);
                        }
                    }
                }
                Msg::DlProgress(id, p, bytes) => {
                    if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                        j.progress = p;
                        j.bytes = bytes;
                    }
                }
                Msg::Imported(list) => {
                    let mut added = 0;
                    let mut todo: Vec<String> = vec![];
                    for i in list {
                        let is_id = i.key.starts_with("UC") && i.key.len() == 24;
                        if is_id && self.channels.iter().any(|c| c.id == i.key) {
                            continue;
                        }
                        if is_id {
                            self.channels.push(Channel {
                                id: i.key.clone(),
                                name: if i.name.is_empty() { i.key.clone() } else { i.name.clone() },
                                avatar: String::new(),
                            });
                            added += 1;
                        }
                        todo.push(i.key);
                    }
                    save(&self.channels);
                    let count = added.to_string();
                    self.toast(if added == 1 { trf("Imported {count} channel", &[("count", &count)]) } else { trf("Imported {count} channels", &[("count", &count)]) });
                    // resolve names / avatars (and @handles) one by one, gently
                    let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
                    std::thread::spawn(move || {
                        for key in todo {
                            if let Ok(ch) = yt::resolve_channel(&key) {
                                let _ = tx.send(Msg::ChannelInfo(key, ch));
                                ctx.request_repaint();
                            }
                            std::thread::sleep(Duration::from_millis(250));
                        }
                    });
                }
                Msg::Cues(vid, idx, r) => {
                    if self.cc_for == vid && self.cc_track == Some(idx) {
                        match r {
                            Ok(c) => self.cues = c,
                            Err(e) => {
                                self.cc_track = None;
                                self.toast(trf("Captions unavailable: {error}", &[("error", &e)]));
                            }
                        }
                    }
                }
                Msg::Sponsor(vid, r) => {
                    if self.sponsor_for == vid {
                        if let Ok(v) = r {
                            self.sponsor = v;
                        }
                    }
                }
                Msg::Related(vid, r) => {
                    if self.related.as_ref().is_some_and(|x| x.0 == vid) {
                        self.related = Some((vid, Some(r)));
                    }
                }
                Msg::MusicHome(r) => self.music_home = Some(r),
                Msg::MusicSugg(q, list) => {
                    if q == self.music_sugg_req {
                        self.music_sugg = list;
                    }
                }
                Msg::Lyrics(vid, r) => {
                    if self.lyrics.as_ref().is_some_and(|l| l.0 == vid) {
                        self.lyrics = Some((vid, Some(r)));
                    }
                }
                Msg::ForYou(v) => self.for_you = Some(v),
                Msg::MusicSearch(q, k, r) => {
                    if self.music_active.as_ref().is_some_and(|a| a.0 == q && a.1 == k) {
                        self.music_results = Some((q, k, r));
                    }
                }
                Msg::MusicPage(id, r) => {
                    if self.music_page.as_ref().is_some_and(|p| p.0 == id) {
                        self.music_page = Some((id, Some(r)));
                    }
                }
                Msg::LiveCheck(list) => {
                    let first = !self.live_seeded;
                    self.live_seeded = true;
                    for (chan, v) in list {
                        if self.notified.insert(v.id.clone()) && !first && self.settings.notifications {
                            mpris::notify(&format!("{chan} is live"), &v.title);
                        }
                    }
                }
                Msg::OpenChannel(r) => match r {
                    Ok(c) => self.show_channel(c),
                    Err(e) => self.toast(trf("Could not open the channel: {error}", &[("error", &e)])),
                },
                Msg::ExportedSubs(n) => self.toast(trf("Exported {count} channels", &[("count", &n.to_string())])),
                Msg::Note(t) => self.toast(t),
                Msg::Podcast(feed, subscribe, r) => self.apply_podcast(feed, subscribe, r),
                Msg::PodSearch(q, r) => {
                    if self.pod_hits.as_ref().is_some_and(|h| h.0 == q) {
                        self.pod_hits = Some((q, Some(r)));
                    }
                }
                Msg::Storage(i) => self.storage = Some(i),
                Msg::Transcript(vid, r) => {
                    if self.tr.as_ref().is_some_and(|t| t.0 == vid) {
                        self.tr = Some((vid, Some(r)));
                    }
                }
                Msg::ClipProgress(p) => {
                    self.clip.busy = Some(p);
                    self.clip.started = true;
                }
                Msg::ClipDone(r) => {
                    self.clip.busy = None;
                    match r {
                        Ok(p) => {
                            self.toast(tr("Clip saved"));
                            self.clip_finished(p.clone());
                            self.clip.last = Some(p);
                        }
                        Err(e) if e == "cancelled" => {
                            self.clip.pending = None;
                            self.toast(tr("Clip cancelled"));
                        }
                        Err(e) => self.clip.err = Some(e),
                    }
                }
                Msg::Diag(r) => {
                    self.diag_running = false;
                    self.diag = Some(r);
                }
                Msg::Restored => {
                    // the new process loads the restored files; leaving without the usual
                    // save on exit keeps the old state from overwriting them
                    crate::platform::restart();
                }
                Msg::UpdateChecked(r, manual) => self.apply_update_check(r, manual),
                Msg::UpdateProgress(p) => {
                    if matches!(self.update_state, UpdateState::Downloading(_)) {
                        self.update_state = UpdateState::Downloading(p);
                    }
                }
                Msg::UpdateDownloaded(r) => self.finish_update(r),
                Msg::FfmpegChecked(ok) => self.ffmpeg_checked(ok),
                Msg::FfmpegProgress(p) => {
                    if matches!(self.ffmpeg, FfmpegState::Downloading(_)) {
                        self.ffmpeg = FfmpegState::Downloading(p);
                    }
                }
                Msg::FfmpegInstalled(r) => self.ffmpeg_installed(r),
                Msg::Online(up) => {
                    let was = self.online;
                    self.online = up;
                    if !self.online_known {
                        self.online_known = true;
                        // no connection at launch: open what works without one
                        if !up && self.sel == Sel::Home && !self.library.is_empty() && self.watching.is_none() {
                            self.sel = Sel::Downloads;
                        }
                    } else if !was && up {
                        self.toast(tr("Back online"));
                        if matches!(self.ffmpeg, FfmpegState::Failed(_)) && ffmpeg_setup::source_for_this_system().is_some() {
                            self.start_ffmpeg_setup();
                        }
                        self.refresh_all();
                    }
                }
                Msg::ChannelInfo(key, ch) => {
                    if let Some(c) = self.channels.iter_mut().find(|c| c.id == key) {
                        *c = ch.clone();
                    } else if !self.channels.iter().any(|c| c.id == ch.id) {
                        self.channels.push(ch.clone());
                    }
                    save(&self.channels);
                    if !self.feeds.contains_key(&ch.id) {
                        self.refresh(&ch);
                    }
                }
                Msg::DirPicked(p) => {
                    if let Some(p) = p {
                        self.settings.download_dir = p;
                        apply_settings(&self.settings);
                        self.save_settings();
                        self.library = download::list();
                        self.toast(tr("Download folder updated"));
                    }
                }
                Msg::Picked(p) => {
                    self.picking = false;
                    if let Some(p) = p {
                        self.open_local(p);
                    }
                }
                Msg::LocalProbe(path, r) => match r {
                    Ok((streams, title, artist)) => self.play_file(&path, streams, title, artist),
                    Err(e) => {
                        self.dl_err = Some(trf("Could not open {path}: {error}", &[("path", &path), ("error", &e)]));
                        self.sel = Sel::Downloads;
                    }
                },
                Msg::DlDone(id, r) => {
                    let job = self.jobs.iter().position(|j| j.id == id).map(|i| self.jobs.remove(i));
                    match r {
                        Ok(_) => {
                            self.dl_tries.remove(&id);
                            self.library = download::list();
                        }
                        Err(e) if e == "cancelled" => {}
                        Err(e) => {
                            // an expired link or a dropped connection: try again with fresh stream URLs
                            let tries = self.dl_tries.entry(id.clone()).or_insert(0);
                            *tries += 1;
                            match job {
                                Some(j) if *tries <= 2 => {
                                    self.cache.remove(&j.retry.0.id);
                                    let (v, a, q, g) = j.retry;
                                    self.dl_pending.push_back((v, a, q, g));
                                }
                                Some(j) => {
                                    self.dl_tries.remove(&id);
                                    self.dl_gaveup.insert(j.retry.0.id.clone());
                                    self.toast(trf("Could not download “{title}”", &[("title", &j.retry.0.title)]));
                                    self.dl_errs.push(format!("{}: {e}", j.retry.0.title));
                                }
                                None => self.dl_errs.push(e),
                            }
                        }
                    }
                }
                Msg::ChanSearch(chan, query, r) => {
                    if let Some(cs) = self.chan_search.as_mut() {
                        if cs.chan == chan && cs.query == query {
                            cs.res = Some(r);
                        }
                    }
                }
                Msg::ChanMore(id, tab, r) => {
                    let key = format!("{id}:{tab}");
                    self.more_loading.remove(&key);
                    match r {
                        Ok((data, next)) => {
                            match next {
                                Some(t) => self.fcache.tab_tokens.insert(key.clone(), t),
                                None => self.fcache.tab_tokens.remove(&key),
                            };
                            match data {
                                TabData::Videos(v) => {
                                    let list = self.fcache.tab_videos.entry(key).or_default();
                                    for x in v {
                                        if !list.iter().any(|y| y.id == x.id) {
                                            list.push(x);
                                        }
                                    }
                                }
                                TabData::Playlists(p) => {
                                    let list = self.fcache.tab_playlists.entry(id).or_default();
                                    for x in p {
                                        if !list.iter().any(|y| y.id == x.id) {
                                            list.push(x);
                                        }
                                    }
                                }
                                TabData::About(_) => {}
                            }
                            save_feed_cache(&self.fcache);
                        }
                        Err(e) => {
                            self.tab_err.insert(key, e);
                        }
                    }
                }
                Msg::Live(r) => {
                    self.live_loading = false;
                    match r {
                        Ok(v) => {
                            self.fcache.live = v.clone();
                            self.fcache.live_updated = now_secs();
                            save_feed_cache(&self.fcache);
                            self.live = Some(Ok(v));
                        }
                        Err(e) => {
                            if !matches!(self.live, Some(Ok(_))) {
                                self.live = Some(Err(e));
                            }
                        }
                    }
                }
                Msg::Replies(vid, parent, append, r) => {
                    if self.watching.as_ref().is_some_and(|w| w.id == vid) {
                        let st = self.replies.entry(parent).or_default();
                        st.loading = false;
                        match r {
                            Ok((list, next)) => {
                                if append {
                                    st.items.extend(list);
                                } else {
                                    st.items = list;
                                }
                                st.next = next;
                            }
                            Err(e) => st.err = Some(e),
                        }
                    }
                }
                Msg::Comments(vid, more, r) => {
                    self.comments_busy = false;
                    if self.watching.as_ref().is_some_and(|w| w.id == vid) {
                        match r {
                            Ok(page) => {
                                self.comments_next = page.next;
                                if !page.sorts.is_empty() {
                                    self.comment_sorts = page.sorts;
                                }
                                match (&mut self.comments, more) {
                                    (Some((_, Ok(old))), true) => old.extend(page.list),
                                    _ => self.comments = Some((vid, Ok(page.list))),
                                }
                            }
                            Err(e) => self.comments = Some((vid, Err(e))),
                        }
                    }
                }
                Msg::Streams(id, r) => {
                    self.inflight.remove(&id);
                    if let Ok(s) = &r {
                        self.cache.insert(id.clone(), (Instant::now(), s.clone()));
                    }
                    if let (Ok(s), Some(w)) = (&r, self.watching.as_mut()) {
                        if w.id == id && w.title == PLACEHOLDER_TITLE {
                            w.title = s.title.clone();
                            let t = s.title.clone();
                            if let Some(h) = self.history.iter_mut().find(|h| h.video.id == id) {
                                h.video.title = t;
                            }
                        }
                    }
                    let wants = self.watching.as_ref().is_some_and(|w| w.id == id)
                        && self.player.streams.is_none();
                    if wants {
                        match r {
                            Ok(s) => {
                                let start = self.resume_pos(&id);
                                self.player.load(s, start);
                            }
                            Err(e) => self.player.fail(e),
                        }
                    }
                }
            }
        }
    }
}
