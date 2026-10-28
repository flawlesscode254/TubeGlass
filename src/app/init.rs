//! Start-up: builds the application state from the files on disk and restores the last session.

use crate::*;

impl App {
    /// Builds the application from the files on disk, restores the last page and starts the background watchers.
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        std::thread::spawn(yt::warm);
        style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let saved = load_saved();
        SIDE_COLLAPSED.store(saved.collapsed, std::sync::atomic::Ordering::Relaxed);
        let mut app = Self {
            logo: cc.egui_ctx.load_texture(
                "logo",
                egui::ColorImage::from_rgba_unmultiplied([128, 128], &icons::render_app_icon(128)),
                egui::TextureOptions::LINEAR,
            ),
            chan_filter: String::new(),
            replies: HashMap::new(),
            mpris: None,
            notified: HashSet::new(),
            live_seeded: false,
            next_live_check: Instant::now() + Duration::from_secs(20),
            next_offline_sync: Instant::now() + Duration::from_secs(6),
            q_drag: None,
            pods: podcasts::load(),
            pod_busy: HashSet::new(),
            pod_input: String::new(),
            pod_hits: None,
            pod_err: None,
            pod_next_check: Instant::now() + Duration::from_secs(8),
            pod_unsub_confirm: false,
            pod_shown: (String::new(), 0),
            err_retried: HashSet::new(),
            want_retry: false,
            clip_linked_for: String::new(),
            lyrics_forced: String::new(),
            clips: side_tools::load_clips(),
            clip_open_at: None,
            tr: None,
            tr_filter: String::new(),
            tr_idx: usize::MAX,
            bookmarks: side_tools::load_bookmarks(),
            note_input: String::new(),
            clip: Default::default(),
            clip_preview: false,
            storage: None,
            storage_page: false,
            storage_confirm: None,
            dl_sort: 0,
            stats: insights::load(),
            stats_dirty: false,
            stats_confirm: false,
            stat_tick: Instant::now(),
            stat_saved: Instant::now(),
            look_sig: (false, 0, [0, 0, 0]),
            online: true,
            update: None,
            update_state: UpdateState::Idle,
            update_next_check: Instant::now() + Duration::from_secs(20),
            update_checking: false,
            update_note: None,
            ffmpeg: FfmpegState::Ready,
            ffmpeg_hidden: false,
            update_notes_open: false,
            online_known: false,
            mini: false,
            pre_mini: None,
            cc_translated: None,
            diag: None,
            diag_running: false,
            dl_gaveup: HashSet::new(),
            cues: vec![],
            cc_track: None,
            cc_for: String::new(),
            sponsor: vec![],
            sponsor_for: String::new(),
            last_skip: None,
            related: None,
            seen: load_seen(),
            seen_dirty: false,
            recent: load_recent(),
            search_opts: yt::SearchOpts::default(),
            queue: vec![],
            queue_idx: None,
            ended_handled: String::new(),
            side_tab: SideTab::Comments,
            music_q: String::new(),
            music_sugg: vec![],
            music_sugg_req: String::new(),
            music_edit: Instant::now(),
            music_sugg_rect: None,
            lyrics: None,
            lyric_idx: usize::MAX,
            lyric_tries: 0,
            lyric_retry_at: None,
            for_you: None,
            followed: load_followed(),
            music_kind: ytmusic::SearchKind::Songs,
            music_active: None,
            music_results: None,
            music_home: None,
            music_page: None,
            music_shuffle_next: false,
            amb_active: false,
            amb_vr: Rect::NOTHING,
            win: load_win(),
            win_dirty_since: None,
            groups: load_groups(),
            new_group: None,
            release_focus: false,
            amb_cur: vec![],
            amb_tex: None,
            nav_back: vec![],
            nav_cur: NavEntry {
                sel: Sel::Home,
                tab: ChanTab::Videos,
                query: String::new(),
            },
            nav_skip: false,
            settings: load_settings(),
            sleep: None,
            toast: None,
            show_help: false,
            history: load_history(),
            hist_saved: Instant::now(),
            hist_confirm: false,
            chan_q: String::new(),
            chan_q_for: String::new(),
            chan_search: None,
            dl_pending: std::collections::VecDeque::new(),
            lib_filter: String::new(),
            lib_filter_page: String::new(),
            dl_open: HashSet::new(),
            gapless_next: None,
            dl_sel: None,
            dl_confirm: false,
            picking: false,
            batch: HashSet::new(),
            batch_total: 0,
            last_dir: None,
            viz_levels: vec![0.0; 64],
            panics: 0,
            lists: load_lists(),
            new_list: None,
            import_list: None,
            list_sort: 0,
            list_date: 0,
            list_len: 0,
            confirm_unsub: false,
            jobs: vec![],
            library: download::list(),
            dl_err: None,
            dl_tries: HashMap::new(),
            dl_errs: vec![],
            channels: saved.channels,
            feeds: HashMap::new(),
            loading: HashSet::new(),
            sel: Sel::Home,
            filter: Filter::All,
            input: String::new(),
            search_input: String::new(),
            search_query: String::new(),
            svideos: None,
            schannels: None,
            splaylists: None,
            sugg: vec![],
            sugg_req: String::new(),
            sugg_rect: None,
            last_edit: Instant::now(),
            playlist: None,
            live: None,
            fcache: FeedCache::default(),
            chan_tab: ChanTab::Videos,
            tab_loading: HashSet::new(),
            more_loading: HashSet::new(),
            pip: false,
            tab_err: HashMap::new(),
            live_loading: false,
            cache: HashMap::new(),
            inflight: HashSet::new(),
            comments_open: false,
            comments: None,
            comments_next: None,
            comment_sorts: vec![],
            comments_busy: false,
            adding: false,
            add_err: None,
            tx,
            rx,
            player: Player::new(),
            watching: None,
            last_move: Instant::now(),
            last_refresh: Instant::now(),
            fullscreen: false,
            ctx: cc.egui_ctx.clone(),
        };
        i18n::set_language(&app.settings.language);
        // Show what was fetched last time right away; only channels we have never seen
        // are fetched. Everything else updates when the user presses Refresh.
        apply_settings(&app.settings);
        let saved_page = app.win.page.clone();
        if !saved_page.is_empty() {
            app.restore_page(&saved_page);
        }
        app.mpris = Some(mpris::start());
        // everything (playback, downloads, clips) runs through ffmpeg: say so early if it is missing
        {
            let (tx, ctx) = (app.tx.clone(), app.ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Msg::FfmpegChecked(crate::platform::ffmpeg_available()));
                ctx.request_repaint();
            });
        }
        // watch the connection so the app can fall back to what is saved on this computer
        {
            let (tx, ctx) = (app.tx.clone(), app.ctx.clone());
            std::thread::spawn(move || {
                use std::net::ToSocketAddrs;
                loop {
                    let up = "www.youtube.com:443"
                        .to_socket_addrs()
                        .ok()
                        .and_then(|mut a| a.next())
                        .is_some_and(|a| {
                            std::net::TcpStream::connect_timeout(&a, Duration::from_secs(4)).is_ok()
                        });
                    let _ = tx.send(Msg::Online(up));
                    ctx.request_repaint();
                    std::thread::sleep(Duration::from_secs(if up { 30 } else { 8 }));
                }
            });
        }
        app.player.quality = app.settings.default_quality;
        app.player.speed = app.settings.speed.clamp(0.5, 2.0);
        app.player.ambient_on = app.settings.ambient;
        if let Some(arg) = std::env::args()
            .skip(1)
            .find(|a| !a.starts_with('-') && !a.trim().is_empty())
        {
            app.open_local(arg);
        }
        let fc = load_feed_cache();
        for (id, v) in &fc.feeds {
            if app.channels.iter().any(|c| &c.id == id) {
                app.feeds.insert(id.clone(), Ok(v.clone()));
                if app.seen.is_empty() {
                    for x in v {
                        app.seen.insert(x.id.clone());
                    }
                    app.seen_dirty = true;
                }
            }
        }
        if !fc.live.is_empty() {
            app.live = Some(Ok(fc.live.clone()));
        }
        app.fcache = fc;
        for c in app.channels.clone() {
            if !app.feeds.contains_key(&c.id) {
                app.refresh(&c);
            }
        }
        app
    }
}
