//! Offline downloads: queueing, retrying and keeping playlists saved on this computer.

use crate::*;

impl App {
    /// Queue a whole list for offline use. Songs are saved as audio, videos at the default quality.
    pub(crate) fn download_all(&mut self, vids: &[Video], group: &str) {
        // a manual request tries everything again, including what gave up before
        self.dl_gaveup.clear();
        let n = self.download_missing(vids, group);
        self.toast(if n == 0 { tr("Everything is already downloaded") } else if n == 1 { trf("Downloading {count} item", &[("count", &n.to_string())]) } else { trf("Downloading {count} items", &[("count", &n.to_string())]) });
    }

    /// Queue everything in `vids` that is not saved yet; returns how many were queued.
    pub(crate) fn download_missing(&mut self, vids: &[Video], group: &str) -> usize {
        let q = self.settings.default_quality;
        let mut n = 0;
        for v in vids {
            if v.kind == Kind::Live || v.kind == Kind::Upcoming || v.short || self.dl_gaveup.contains(&v.id) {
                continue;
            }
            if !self.is_downloaded(&v.id, v.music) {
                self.start_download_in(v, v.music, q, group);
                n += 1;
            }
        }
        n
    }

    /// Playlists marked "keep offline": fetch whatever is missing.
    pub(crate) fn sync_offline_lists(&mut self) {
        if Instant::now() < self.next_offline_sync || !self.online {
            return;
        }
        self.next_offline_sync = Instant::now() + Duration::from_secs(90);
        if !self.jobs.is_empty() || !self.dl_pending.is_empty() {
            return;
        }
        for l in self.lists.clone().into_iter().filter(|l| l.offline) {
            self.download_missing(&l.videos, &l.name);
        }
    }

    /// How many of `vids` are already saved offline.
    pub(crate) fn offline_count(&self, vids: &[Video]) -> usize {
        vids.iter().filter(|v| self.is_downloaded(&v.id, v.music)).count()
    }

    /// Start waiting downloads whenever a slot is free.
    pub(crate) fn pump_downloads(&mut self) {
        while self.jobs.len() < 2 {
            let Some((v, a, q, g)) = self.dl_pending.pop_front() else { break };
            self.start_download_in(&v, a, q, &g);
        }
    }

    /// True when this video (or its audio, when `audio` is set) is already saved offline.
    pub(crate) fn is_downloaded(&self, id: &str, audio: bool) -> bool {
        self.library.iter().any(|d| d.id == id && d.audio_only == audio)
    }

    /// Progress (0–1) of the running download for this video, if there is one.
    pub(crate) fn job_progress(&self, id: &str) -> Option<f32> {
        self.jobs.iter().filter(|j| j.vid == id).map(|j| j.progress).reduce(f32::max)
    }

    /// Queues a download with the default group (none).
    pub(crate) fn start_download(&mut self, v: &Video, audio_only: bool, quality: u32) {
        self.start_download_in(v, audio_only, quality, "");
    }

    /// Starts or queues a download; `group` names the album / playlist it belongs to. At most two run at once.
    pub(crate) fn start_download_in(&mut self, v: &Video, audio_only: bool, quality: u32, group: &str) {
        let key = format!("{}{}", v.id, if audio_only { "#a" } else { "" });
        if self.is_downloaded(&v.id, audio_only) || self.jobs.iter().any(|j| j.id == key) || v.kind == Kind::Live {
            return;
        }
        // at most two transfers at a time, the rest wait their turn
        if self.jobs.len() >= 2 {
            if !self.dl_pending.iter().any(|(p, a, _, _)| p.id == v.id && *a == audio_only) {
                self.dl_pending.push_back((v.clone(), audio_only, quality, group.to_string()));
            }
            return;
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pause = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.jobs.push(DlJob {
            id: key.clone(),
            vid: v.id.clone(),
            pause: pause.clone(),
            bytes: 0,
            started: Instant::now(),
            idle: Duration::ZERO,
            paused_at: None,
            title: if audio_only { format!("{} (audio)", v.title) } else { v.title.clone() },
            thumb: v.thumb.clone(),
            progress: 0.0,
            cancel: cancel.clone(),
            retry: (v.clone(), audio_only, quality, group.to_string()),
        });
        let pod_ep = if v.id.starts_with("pod:") { self.pod_episode(&v.id).map(|(_, e)| e.clone()) } else { None };
        let mut extras_tr = pod_ep.as_ref().map(|e| e.transcript.clone()).unwrap_or_default();
        let cached = self.cache.get(&v.id).map(|(_, s)| s.clone());
        let (tx, ctx, v) = (self.tx.clone(), self.ctx.clone(), v.clone());
        let group = group.to_string();
        let tr_on = self.settings.dl_extras;
        let extras = download::Extras {
            captions: self.settings.dl_extras,
            lyrics: self.settings.dl_extras,
            lang: self.settings.caption_lang.clone(),
            transcript: vec![],
        };
        if !tr_on {
            extras_tr.clear();
        }
        std::thread::spawn(move || {
            let streams = match (pod_ep, cached) {
                // a podcast episode: the file's address is in the feed (its length is looked up here)
                (Some(e), _) => Ok(podcasts::build_streams(&e)),
                (None, Some(s)) => Ok(s),
                (None, None) => yt::streams(&v.id),
            };
            let mut extras = extras;
            if !extras_tr.is_empty() {
                extras.transcript = podcasts::fetch_transcript(&extras_tr).unwrap_or_default();
            }
            let res = streams.and_then(|st| {
                let (tx2, ctx2, key2) = (tx.clone(), ctx.clone(), key.clone());
                let last = std::cell::Cell::new(-1i32);
                download::run(
                    &v.id,
                    &v.title,
                    &v.channel_name,
                    &v.thumb,
                    &st,
                    quality,
                    &cancel,
                    &pause,
                    audio_only,
                    v.music,
                    &group,
                    &extras,
                    &|p, bytes| {
                        let pct = (p * 100.0) as i32;
                        if pct != last.get() {
                            last.set(pct);
                            let _ = tx2.send(Msg::DlProgress(key2.clone(), p, bytes));
                            ctx2.request_repaint();
                        }
                    },
                )
            });
            let _ = tx.send(Msg::DlDone(key, res));
            ctx.request_repaint();
        });
    }

    /// Measures what the app stores on disk, in the background, for the Storage settings.
    pub(crate) fn refresh_storage(&mut self) {
        let (tx, ctx, n) = (self.tx.clone(), self.ctx.clone(), self.library.len());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Storage(storage_info(n)));
            ctx.request_repaint();
        });
    }
}
