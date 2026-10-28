//! Playback engine: ffmpeg decodes video to raw RGBA frames on stdout and PCM audio on fd 3
//! (Linux, macOS) or from a second ffmpeg process (Windows, or whenever the sound has its own
//! source or filters). Audio goes to the sound card through cpal and its sample counter is the
//! master clock that video frames are paced against.

use crate::proxy;
use crate::yt::{Streams, VideoOpt};
use eframe::egui;
use std::collections::VecDeque;
use std::io::Read;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Frame rate the picture is decoded at.
const FPS: u32 = 30;
/// How many decoded frames wait for their turn.
const VIDEO_QUEUE: usize = 24;

/// One decoded picture (RGBA) and its index in the stream.
struct Frame {
    idx: u64,
    rgba: Vec<u8>,
}

/// The queue of decoded sound samples the audio device drains.
#[derive(Default)]
struct Ring {
    epoch: u64,
    q: VecDeque<f32>,
}

/// Audio output shared between the device callback and the decoder threads.
struct Audio {
    /// Most recent mono samples that were sent to the sound card (for the visualizer).
    viz: Mutex<Vec<f32>>,
    ring: Mutex<Ring>,
    played: AtomicU64,
    paused: AtomicBool,
    volume: AtomicU32,
}

/// Next track's decoder, started ahead of time so playback can run straight into it.
struct NextAudio {
    reader: Box<dyn Read + Send>,
    child: Child,
    streams: Streams,
}

/// Lets the next track's decoder take over from the current one without a gap.
struct Handoff {
    next: Mutex<Option<NextAudio>>,
    /// stereo frames of the session timeline written to the ring so far
    pushed: AtomicU64,
    /// frame at which the next track starts (u64::MAX = none yet)
    boundary: AtomicU64,
    /// crossfade length in stereo frames
    xfade: AtomicU64,
    /// decoder of the track that took over, kept so it can be stopped with the session
    active: Mutex<Option<Child>>,
    next_streams: Mutex<Option<Streams>>,
}

impl Handoff {
    /// A fresh, empty hand-over.
    fn new() -> Arc<Self> {
        Arc::new(Self {
            next: Mutex::new(None),
            pushed: AtomicU64::new(0),
            boundary: AtomicU64::new(u64::MAX),
            xfade: AtomicU64::new(0),
            active: Mutex::new(None),
            next_streams: Mutex::new(None),
        })
    }
    /// Stops the decoders of the next track.
    fn kill_all(&self) {
        let mut kids: Vec<Child> = vec![];
        if let Some(n) = self.next.lock().unwrap().take() {
            kids.push(n.child);
        }
        if let Some(c) = self.active.lock().unwrap().take() {
            kids.push(c);
        }
        for mut c in kids {
            let _ = c.kill();
            std::thread::spawn(move || {
                let _ = c.wait();
            });
        }
    }
}

/// One running decode of one stream (its ffmpeg processes and threads).
struct Session {
    handoff: Arc<Handoff>,
    child: Option<Child>,
    /// Separate audio decoder when sound and picture come from different files.
    child2: Option<Child>,
    rx: Receiver<Frame>,
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    err: Arc<Mutex<String>>,
    start: f64,
    got_frame: bool,
    audio_only: bool,
}

impl Drop for Session {
    /// Stops the decoder processes when a session ends.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.handoff.kill_all();
        for mut child in [self.child.take(), self.child2.take()].into_iter().flatten() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

/// What the player is doing.
#[derive(Clone, PartialEq, Debug)]
pub enum State {
    Idle,
    Loading,
    Playing,
    Paused,
    Buffering,
    Ended,
    Error(String),
}

/// The media player: decodes with ffmpeg, plays sound with cpal, hands pictures to the interface.
pub struct Player {
    audio: Arc<Audio>,
    rate: u32,
    _stream: Option<cpal::Stream>,
    session: Option<Session>,
    epoch: u64,
    pub streams: Option<Streams>,
    pub quality: u32,
    pub paused: bool,
    /// Play only the audio stream (saves bandwidth, shows the visualizer).
    pub audio_mode: bool,
    /// Playback speed (0.5 – 2.0); audio keeps its pitch.
    pub speed: f32,
    /// extra ffmpeg audio filters (equalizer, loudness, silence) applied to everything played
    pub afx: String,
    pub volume: f32,
    pub muted: bool,
    tex: Option<egui::TextureHandle>,
    pending: Option<Frame>,
    shown: Option<u64>,
    size: [usize; 2],
    // wall-clock fallback when there is no audio device
    wall: f64,
    last_tick: Instant,
    state: State,
    dbg_t0: Option<Instant>,
    dbg_last: State,
    /// Ambient mode: sample the picture into a small colour grid.
    pub ambient_on: bool,
    ambient: Vec<[u8; 3]>,
    amb_n: u32,
    /// Seconds of a live stream spent paused or buffering since it was opened: how far playback
    /// has fallen behind the broadcast.
    live_behind: f64,
}

impl Player {
    /// A player with nothing loaded.
    pub fn new() -> Self {
        let audio = Arc::new(Audio {
            viz: Mutex::new(Vec::new()),
            ring: Mutex::new(Ring::default()),
            played: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            volume: AtomicU32::new(1.0f32.to_bits()),
        });
        let (stream, rate) = match open_output(audio.clone()) {
            Ok((s, r)) => (Some(s), r),
            Err(e) => {
                eprintln!("audio disabled: {e}");
                (None, 48000)
            }
        };
        Self {
            audio,
            rate,
            _stream: stream,
            session: None,
            epoch: 0,
            streams: None,
            quality: 720,
            paused: false,
            audio_mode: false,
            speed: 1.0,
            afx: String::new(),
            volume: 1.0,
            muted: false,
            tex: None,
            pending: None,
            shown: None,
            size: [16, 9],
            wall: 0.0,
            last_tick: Instant::now(),
            state: State::Idle,
            dbg_t0: None,
            dbg_last: State::Idle,
            ambient_on: false,
            ambient: vec![],
            amb_n: 0,
            live_behind: 0.0,
        }
    }

    /// Current state (playing, buffering, paused...).
    pub fn state(&self) -> State {
        self.state.clone()
    }

    /// How many seconds playback has fallen behind the live edge since the stream was opened
    /// (0 right after opening or jumping to live).
    pub fn live_behind(&self) -> f64 {
        self.live_behind
    }

    /// Jumps to the live edge by opening the stream again.
    pub fn go_live(&mut self) {
        if let Some(s) = self.streams.clone() {
            self.load(s, 0.0);
        }
    }

    /// True for live streams.
    pub fn is_live(&self) -> bool {
        self.streams.as_ref().is_some_and(|s| s.is_live)
    }

    /// Length of what is loaded, in seconds.
    pub fn duration(&self) -> f64 {
        self.streams.as_ref().map_or(0.0, |s| s.duration)
    }

    /// Last `n` mono samples that were played (zero padded), plus the sample rate.
    pub fn viz_samples(&self, n: usize) -> (Vec<f32>, u32) {
        let v = self.audio.viz.lock().unwrap();
        let mut out = vec![0.0; n];
        let take = v.len().min(n);
        out[n - take..].copy_from_slice(&v[v.len() - take..]);
        (out, self.rate)
    }

    /// True when only sound is being played.
    pub fn audio_only(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.audio_only)
    }

    /// The latest picture, ready to draw.
    pub fn texture(&self) -> Option<&egui::TextureHandle> {
        // a stale frame from before switching to audio-only must not cover the visualizer
        if self.audio_only() { None } else { self.tex.as_ref() }
    }

    /// Average colours of the current picture on an `AMBIENT_W` × `AMBIENT_H` grid.
    pub fn ambient(&self) -> &[[u8; 3]] {
        &self.ambient
    }

    /// Width / height of the picture.
    pub fn aspect(&self) -> f32 {
        self.size[0] as f32 / self.size[1].max(1) as f32
    }

    /// Heights available to choose from.
    pub fn qualities(&self) -> Vec<u32> {
        self.streams
            .as_ref()
            .map(|s| {
                let mut h: Vec<u32> = s.video.iter().map(|v| v.height).filter(|h| *h > 0).collect();
                h.sort_unstable();
                h.dedup();
                h
            })
            .unwrap_or_default()
    }

    /// Drop everything that is still queued for the speakers, so nothing keeps playing
    /// after the session has been closed.
    fn silence(&mut self) {
        self.epoch += 1;
        let mut r = self.audio.ring.lock().unwrap();
        r.epoch = self.epoch;
        r.q.clear();
        drop(r);
        self.audio.played.store(0, Ordering::SeqCst);
        if let Ok(mut v) = self.audio.viz.lock() {
            v.clear();
        }
    }

    /// Stops playback and forgets what was loaded.
    pub fn stop(&mut self) {
        self.session = None;
        self.silence();
        self.streams = None;
        self.pending = None;
        self.shown = None;
        self.tex = None;
        self.state = State::Idle;
    }

    /// Shows the loading state (used while streams are being found).
    pub fn set_loading(&mut self) {
        self.stop();
        self.state = State::Loading;
    }

    /// Puts the player into an error state with a message.
    pub fn fail(&mut self, msg: String) {
        self.session = None;
        self.silence();
        self.state = State::Error(msg);
    }

    /// Loads streams and starts playing from `start` seconds.
    pub fn load(&mut self, streams: Streams, start: f64) {
        self.streams = Some(streams);
        self.paused = false;
        self.start_session(start);
    }

    /// True while a following track is prepared (or has just taken over).
    pub fn has_preloaded(&self) -> bool {
        self.session.as_ref().is_some_and(|s| {
            s.handoff.next.lock().unwrap().is_some() || s.handoff.boundary.load(Ordering::SeqCst) != u64::MAX
        })
    }

    /// Start decoding `next` now so the current track can flow straight into it.
    /// Only audio-only playback at normal speed can do this.
    pub fn preload(&mut self, next: &Streams, xfade_secs: f32) -> bool {
        let rate = self.rate;
        let Some(s) = self.session.as_ref() else { return false };
        if !s.audio_only || self.speed != 1.0 || self.is_live() || self.has_preloaded() {
            return false;
        }
        match spawn_audio_only(next, rate, s.stop.clone(), &self.afx) {
            Ok((child, reader)) => {
                s.handoff.xfade.store((xfade_secs.max(0.0) * rate as f32) as u64, Ordering::Relaxed);
                *s.handoff.next.lock().unwrap() = Some(NextAudio { reader, child, streams: next.clone() });
                true
            }
            Err(_) => false,
        }
    }

    /// When playback has reached the start of the preloaded track: switch the bookkeeping
    /// over to it and return its streams.
    pub fn poll_gapless(&mut self) -> Option<Streams> {
        let s = self.session.as_mut()?;
        let b = s.handoff.boundary.load(Ordering::SeqCst);
        if b == u64::MAX || self.audio.played.load(Ordering::Relaxed) < b {
            return None;
        }
        let streams = s.handoff.next_streams.lock().unwrap().take()?;
        self.audio.played.fetch_sub(b, Ordering::SeqCst);
        s.handoff.pushed.fetch_sub(b, Ordering::SeqCst);
        s.handoff.boundary.store(u64::MAX, Ordering::SeqCst);
        s.start = 0.0;
        self.streams = Some(streams.clone());
        Some(streams)
    }

    /// Current position in seconds (the audio clock when sound is playing).
    pub fn position(&self) -> f64 {
        let Some(s) = &self.session else { return 0.0 };
        let out = if self._stream.is_some() {
            self.audio.played.load(Ordering::Relaxed) as f64 / self.rate as f64
        } else {
            self.wall
        };
        s.start + out * self.speed as f64
    }

    /// Change the audio filter chain; the running session restarts at the same spot.
    pub fn set_afx(&mut self, afx: String) {
        if self.afx == afx {
            return;
        }
        self.afx = afx;
        if self.streams.is_some() && self.session.is_some() {
            let p = if self.is_live() { 0.0 } else { self.position() };
            self.start_session(p);
        }
    }

    /// Changes the playback speed, restarting the decode at the same spot.
    pub fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(0.5, 2.0);
        if (self.speed - speed).abs() < 0.001 || self.is_live() {
            return;
        }
        let pos = self.position();
        self.speed = speed;
        if self.streams.is_some() {
            self.start_session(pos);
        }
    }

    /// Jumps to a position.
    pub fn seek(&mut self, to: f64) {
        if self.is_live() {
            return;
        }
        let max = (self.duration() - 0.5).max(0.0);
        self.start_session(to.clamp(0.0, max));
    }

    /// Switches the video height, keeping the position.
    pub fn set_quality(&mut self, h: u32) {
        if self.quality == h {
            return;
        }
        self.quality = h;
        if self.streams.is_some() {
            let p = if self.is_live() { 0.0 } else { self.position() };
            self.start_session(p);
        }
    }

    /// Switch between normal and audio-only playback, keeping the position.
    pub fn set_audio_mode(&mut self, on: bool) {
        if self.audio_mode == on {
            return;
        }
        self.audio_mode = on;
        if self.streams.is_some() {
            let p = if self.is_live() { 0.0 } else { self.position() };
            self.start_session(p);
        }
    }

    /// Pauses or resumes (replays when it has ended).
    pub fn toggle_pause(&mut self) {
        if self.state == State::Ended {
            self.seek(0.0);
            return;
        }
        self.paused = !self.paused;
        self.audio.paused.store(self.paused, Ordering::SeqCst);
    }

    /// Sets the volume (0–1).
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        self.apply_volume();
    }

    /// Mutes or unmutes.
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.apply_volume();
    }

    /// Pushes the volume to the audio callback.
    fn apply_volume(&self) {
        let v = if self.muted { 0.0 } else { self.volume * self.volume };
        self.audio.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    /// The video option matching the chosen quality.
    fn pick(&self) -> Option<VideoOpt> {
        let s = self.streams.as_ref()?;
        s.video
            .iter()
            .filter(|v| v.height <= self.quality)
            .max_by_key(|v| v.height)
            .or_else(|| s.video.iter().min_by_key(|v| v.height))
            .cloned()
    }

    /// Starts decoding at `start` seconds, replacing the running session.
    fn start_session(&mut self, start: f64) {
        self.dbg_t0 = Some(Instant::now());
        self.session = None;
        self.pending = None;
        self.shown = None;
        self.wall = 0.0;
        self.live_behind = 0.0;
        self.state = State::Loading;

        let (Some(opt), Some(streams)) = (self.pick(), self.streams.clone()) else {
            self.state = State::Error("no stream".into());
            return;
        };

        // New generation: stale audio from the previous session is discarded.
        self.epoch += 1;
        {
            let mut r = self.audio.ring.lock().unwrap();
            r.epoch = self.epoch;
            r.q.clear();
            self.audio.played.store(0, Ordering::SeqCst);
        }
        self.audio.paused.store(self.paused, Ordering::SeqCst);

        let w = (opt.width.clamp(2, 1920) & !1) as usize;
        let h = (opt.height.clamp(2, 1080) & !1) as usize;
        self.size = [w, h];

        match spawn(&opt, &streams, start, w, h, self.rate, self.audio.clone(), self.epoch, self.audio_mode, if self.is_live() { 1.0 } else { self.speed }, &self.afx) {
            Ok(s) => self.session = Some(s),
            Err(e) => self.state = State::Error(e),
        }
        self.last_tick = Instant::now();
    }

    /// Advance playback; uploads the newest due frame to the GPU.
    pub fn tick(&mut self, ctx: &egui::Context) {
        let dt = self.last_tick.elapsed().as_secs_f64();
        self.last_tick = Instant::now();
        let Some(sess) = self.session.as_mut() else { return };
        if matches!(self.state, State::Error(_)) {
            return;
        }
        // a live stream goes on while playback waits: every second paused or buffering is a second behind
        if self.streams.as_ref().is_some_and(|s| s.is_live) && matches!(self.state, State::Paused | State::Buffering) {
            self.live_behind += dt.min(1.0);
        }

        let has_audio = self._stream.is_some();
        // output-time clock: with speed != 1 the frames are retimed by the filter graph, so
        // a frame is due when its output index has been reached by the audio that played
        let clock = if has_audio {
            self.audio.played.load(Ordering::Relaxed) as f64 / self.rate as f64
        } else {
            self.wall
        };

        if sess.audio_only {
            sess.got_frame = true;
            let empty = self.audio.ring.lock().unwrap().q.is_empty();
            let ended = sess.ended.load(Ordering::SeqCst) && empty;
            self.state = if ended {
                State::Ended
            } else if self.paused {
                State::Paused
            } else if empty {
                State::Buffering
            } else {
                State::Playing
            };
            if !has_audio && self.state == State::Playing {
                self.wall += dt;
            }
            return;
        }

        let mut latest: Option<Frame> = None;
        let mut starved = false;
        if !self.paused || self.shown.is_none() {
            loop {
                let next = match self.pending.take() {
                    Some(f) => f,
                    None => match sess.rx.try_recv() {
                        Ok(f) => f,
                        Err(_) => {
                            starved = true;
                            break;
                        }
                    },
                };
                // The first frame is always shown immediately (also while paused).
                let due = next.idx as f64 / FPS as f64 <= clock + 0.001
                    || self.shown.is_none() && latest.is_none();
                if due {
                    latest = Some(next);
                } else {
                    self.pending = Some(next);
                    break;
                }
            }
        }

        if let Some(f) = latest {
            if !sess.got_frame && std::env::var_os("TG_DEBUG").is_some() {
                if let Some(t0) = self.dbg_t0 {
                    eprintln!("[seek] first frame {:.2}s after start_session(start={:.0}s)", t0.elapsed().as_secs_f32(), sess.start);
                }
            }
            sess.got_frame = true;
            self.shown = Some(f.idx);
            // about ten colour samples a second are plenty for a slow-moving glow
            self.amb_n = self.amb_n.wrapping_add(1);
            if self.ambient_on && (self.amb_n % 3 == 1 || self.ambient.is_empty()) {
                self.ambient = sample_grid(&f.rgba, self.size[0], self.size[1]);
            }
            let img = egui::ColorImage::from_rgba_unmultiplied(self.size, &f.rgba);
            match &mut self.tex {
                Some(t) if t.size() == self.size => t.set(img, egui::TextureOptions::LINEAR),
                _ => {
                    self.tex = Some(ctx.load_texture("video", img, egui::TextureOptions::LINEAR));
                }
            }
        }

        let ended = sess.ended.load(Ordering::SeqCst) && self.pending.is_none() && starved;
        if ended {
            if !sess.got_frame {
                let e = sess.err.lock().unwrap().trim().to_string();
                self.state = State::Error(if e.is_empty() { "stream ended unexpectedly".into() } else { e });
            } else {
                self.state = State::Ended;
            }
            return;
        }

        self.state = if !sess.got_frame {
            State::Loading
        } else if self.paused {
            State::Paused
        } else if starved && self.pending.is_none() {
            State::Buffering
        } else {
            State::Playing
        };

        if std::env::var_os("TG_DEBUG").is_some() && self.state != self.dbg_last {
            eprintln!(
                "[state] {:?} at {:.1}s (ring {} samples)",
                self.state,
                self.dbg_t0.map_or(0.0, |t| t.elapsed().as_secs_f32()),
                self.audio.ring.lock().map(|r| r.q.len()).unwrap_or(0)
            );
            self.dbg_last = self.state.clone();
        }
        if !has_audio && self.state == State::Playing {
            self.wall += dt;
        }
    }
}

/// Width of the colour grid used for the ambient glow.
pub const AMBIENT_W: usize = 24;
/// Height of the colour grid used for the ambient glow.
pub const AMBIENT_H: usize = 14;

/// Average colour of each grid cell, from a handful of sample points per cell.
fn sample_grid(rgba: &[u8], w: usize, h: usize) -> Vec<[u8; 3]> {
    let (cw, ch) = (w / AMBIENT_W, h / AMBIENT_H);
    let mut out = Vec::with_capacity(AMBIENT_W * AMBIENT_H);
    if cw == 0 || ch == 0 || rgba.len() < w * h * 4 {
        return vec![[0, 0, 0]; AMBIENT_W * AMBIENT_H];
    }
    /// Size in pixels of the blocks averaged for the ambient grid.
    const K: usize = 4;
    for gy in 0..AMBIENT_H {
        for gx in 0..AMBIENT_W {
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for sy in 0..K {
                for sx in 0..K {
                    let x = gx * cw + (sx * 2 + 1) * cw / (K * 2);
                    let y = gy * ch + (sy * 2 + 1) * ch / (K * 2);
                    let i = (y * w + x) * 4;
                    r += rgba[i] as u32;
                    g += rgba[i + 1] as u32;
                    b += rgba[i + 2] as u32;
                }
            }
            let n = (K * K) as u32;
            out.push([(r / n) as u8, (g / n) as u8, (b / n) as u8]);
        }
    }
    out
}

/// Decoder for the next track: raw stereo PCM on stdout, audio only.
fn spawn_audio_only(streams: &Streams, rate: u32, stop: Arc<AtomicBool>, afx: &str) -> Result<(Child, Box<dyn Read + Send>), String> {
    let (url, ua) = match (&streams.audio, streams.video.iter().max_by_key(|v| v.height)) {
        (Some(a), _) => (proxy::serve(&a.url, &a.ua, stop)?, a.ua.clone()),
        (None, Some(v)) if crate::platform::is_local_path(&v.url) => (v.url.clone(), v.ua.clone()),
        (None, Some(v)) if v.hls => {
            let u = v.audio_url.as_deref().unwrap_or(&v.url);
            (if crate::platform::is_local_path(u) { u.to_string() } else { proxy::serve_hls(u, &v.ua, stop)? }, v.ua.clone())
        }
        (None, Some(v)) => (proxy::serve(&v.url, &v.ua, stop)?, v.ua.clone()),
        _ => return Err("no stream".into()),
    };
    let mut cmd = crate::platform::command("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
    if !crate::platform::is_local_path(&url) {
        cmd.args(["-user_agent", &ua, "-probesize", "1500000", "-analyzeduration", "1000000"]);
    }
    cmd.args(["-i", &url, "-map", "0:a:0?", "-vn"]);
    if !afx.is_empty() {
        cmd.args(["-af", afx]);
    }
    cmd.args(["-f", "f32le", "-ar"]);
    cmd.arg(rate.to_string());
    cmd.args(["-ac", "2", "pipe:1"]);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("could not start ffmpeg ({e})"))?;
    let out = child.stdout.take().ok_or("no pipe")?;
    if let Some(mut e) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut sink = String::new();
            let _ = e.read_to_string(&mut sink);
        });
    }
    Ok((child, Box::new(out)))
}

/// Starts ffmpeg (one or two processes) and the threads that feed pictures and sound to the player.
fn spawn(
    opt: &VideoOpt,
    streams: &Streams,
    start: f64,
    w: usize,
    h: usize,
    rate: u32,
    audio: Arc<Audio>,
    epoch: u64,
    force_audio: bool,
    speed: f32,
    afx: &str,
) -> Result<Session, String> {
    let handoff = Handoff::new();
    let t0 = Instant::now();
    let dbg = std::env::var_os("TG_DEBUG").is_some();
    let (pr, pw) = std::io::pipe().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let wfd = pw.as_raw_fd();

    let stop = Arc::new(AtomicBool::new(false));
    let seekable = !streams.is_live;
    let via_proxy = |url: &str, ua: &str, hls: bool| -> Result<String, String> {
        if crate::platform::is_local_path(url) {
            // a file on disk (downloads, clips, opened files) is read by ffmpeg directly
            Ok(url.to_string())
        } else if hls {
            proxy::serve_hls(url, ua, stop.clone())
        } else {
            proxy::serve(url, ua, stop.clone())
        }
    };
    let audio_only = force_audio || opt.height == 0;
    // Audio-only: read just the dedicated audio stream when there is one, so the video
    // is never downloaded at all.
    let (vurl, vua) = match (&streams.audio, audio_only) {
        (Some(a), true) if !opt.muxed => (via_proxy(&a.url, &a.ua, false)?, a.ua.clone()),
        _ => match (&opt.audio_url, audio_only) {
            (Some(au), true) => (via_proxy(au, &opt.ua, true)?, opt.ua.clone()),
            _ => (via_proxy(&opt.url, &opt.ua, opt.hls)?, opt.ua.clone()),
        },
    };
    let aurl = match (&streams.audio, opt.muxed, audio_only) {
        (Some(a), false, false) => Some(via_proxy(&a.url, &a.ua, false)?),
        // HLS: the variant is video-only and its sound is a separate playlist
        (None, false, false) => match &opt.audio_url {
            Some(a) => Some(via_proxy(a, &opt.ua, true)?),
            None => None,
        },
        _ => None,
    };

    let mut cmd = crate::platform::command("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
    let input = |cmd: &mut Command, url: &str, ua: &str, seek: bool| {
        // network-only options would be rejected for local files (offline playback)
        if !crate::platform::is_local_path(&url) {
            cmd.args(["-user_agent", ua]);
            cmd.args(["-probesize", "1500000", "-analyzeduration", "1000000"]);
            cmd.args(["-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "5"]);
        }
        if seek && start > 0.5 {
            cmd.args(["-ss", &format!("{start:.3}")]);
        }
        cmd.args(["-i", url]);
    };
    if streams.is_live && opt.hls && !crate::platform::is_local_path(&opt.url) {
        // start at the second-to-last segment instead of the third-to-last: closer to real time
        cmd.args(["-live_start_index", "-2"]);
    }
    input(&mut cmd, &vurl, &vua, seekable);
    // Picture and sound in different files: decode them in two processes so both inputs
    // open and seek at the same time (about 0.2–0.6s faster per seek than one after another).
    // Filters such as volume levelling hold back seconds of sound before they output any.
    // In one process that starves the sound while the picture waits for the clock (a
    // deadlock), so filtered sound always gets its own process.
    // Without file descriptor passing (Windows) the sound always comes from its own process.
    let split = !audio_only && (aurl.is_some() || !afx.is_empty() || !cfg!(unix));
    let mut acmd = crate::platform::command("ffmpeg");
    let audio_map = if split {
        acmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        match &aurl {
            Some(a) => {
                input(&mut acmd, a, "TubeGlass", seekable);
                "0:a:0"
            }
            // same file as the picture: open it a second time for the sound
            None => {
                input(&mut acmd, &vurl, &vua, seekable);
                "0:a:0?"
            }
        }
    } else if let Some(a) = &aurl {
        input(&mut cmd, a, "TubeGlass", seekable);
        "1:a:0"
    } else {
        "0:a:0?"
    };
    if !audio_only {
        cmd.args(["-map", "0:v:0", "-vf"]);
        let retime = if (speed - 1.0).abs() > 0.001 { format!("setpts=PTS/{speed:.3},") } else { String::new() };
        cmd.arg(format!("{retime}fps={FPS},scale={w}:{h}:flags=fast_bilinear,format=rgba"));
        cmd.args(["-an", "-f", "rawvideo", "pipe:1"]);
    }
    {
        let target = if split { &mut acmd } else { &mut cmd };
        target.args(["-map", audio_map, "-vn"]);
        let mut chain: Vec<String> = vec![];
        if !afx.is_empty() {
            chain.push(afx.to_string());
        }
        if (speed - 1.0).abs() > 0.001 {
            chain.push(format!("atempo={speed:.3}"));
        }
        if !chain.is_empty() {
            target.args(["-af", &chain.join(",")]);
        }
        target.args(["-f", "f32le", "-ar"]);
        target.arg(rate.to_string());
        // fd 3 where it exists; otherwise the process's own stdout (a separate audio process,
        // or an audio-only decode that has no picture to put there)
        target.args(["-ac", "2", if split || !cfg!(unix) { "pipe:1" } else { "pipe:3" }]);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    acmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(move || {
            if wfd == 3 {
                let fl = libc::fcntl(3, libc::F_GETFD);
                libc::fcntl(3, libc::F_SETFD, fl & !libc::FD_CLOEXEC);
            } else if libc::dup2(wfd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if dbg {
        eprintln!("[spawn] proxies ready after {:.2}s", t0.elapsed().as_secs_f32());
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start ffmpeg ({e}). Is ffmpeg installed?"))?;
    let mut child2 = if split {
        Some(acmd.spawn().map_err(|e| format!("could not start ffmpeg ({e})"))?)
    } else {
        None
    };
    drop(pw);
    if dbg {
        eprintln!("[spawn] ffmpeg started after {:.2}s", t0.elapsed().as_secs_f32());
    }

    let ended = Arc::new(AtomicBool::new(false));
    let err = Arc::new(Mutex::new(String::new()));
    let (tx, rx): (SyncSender<Frame>, Receiver<Frame>) = sync_channel(VIDEO_QUEUE);

    // video
    let mut stdout = child.stdout.take().unwrap();
    let mut audio_stdout: Option<std::process::ChildStdout> = None;
    let (e2, s2) = (ended.clone(), stop.clone());
    if !audio_only {
    std::thread::spawn(move || {
        let mut idx = 0u64;
        loop {
            let mut buf = vec![0u8; w * h * 4];
            if stdout.read_exact(&mut buf).is_err() || s2.load(Ordering::Relaxed) {
                break;
            }
            if idx == 0 && dbg {
                eprintln!("[spawn] first video frame bytes after {:.2}s", t0.elapsed().as_secs_f32());
            }
            if tx.send(Frame { idx, rgba: buf }).is_err() {
                break;
            }
            idx += 1;
        }
        e2.store(true, Ordering::SeqCst);
    });
    } else if cfg!(unix) {
        drop((e2, s2, tx, stdout));
    } else {
        // audio-only without fd 3: the sound is on stdout
        drop((e2, s2, tx));
        audio_stdout = Some(stdout);
    }

    // audio
    let s3 = stop.clone();
    let audio_end = ended.clone();
    let mut pr: Box<dyn Read + Send> = match child2.as_mut().and_then(|c| c.stdout.take()) {
        Some(out) => Box::new(out),
        None => match audio_stdout {
            Some(out) => Box::new(out),
            None => Box::new(pr),
        },
    };
    if let Some(mut e) = child2.as_mut().and_then(|c| c.stderr.take()) {
        std::thread::spawn(move || {
            let mut sink = String::new();
            let _ = e.read_to_string(&mut sink);
        });
    }
    let ho = handoff.clone();
    std::thread::spawn(move || {
        let cap = rate as usize * 2 * 20;
        let mut first_audio = true;
        let mut buf = vec![0u8; 16384];
        let mut carry: Vec<u8> = vec![];
        // samples of the next track still to be blended into the tail of this one
        let (mut mix_total, mut mix_pos) = (0usize, 0usize);
        loop {
            let n = match pr.read(&mut buf) {
                Ok(0) | Err(_) => {
                    // end of this track: continue straight into the prepared next one
                    let next = if s3.load(Ordering::Relaxed) { None } else { ho.next.lock().unwrap().take() };
                    let Some(nx) = next else {
                        if dbg {
                            eprintln!("[audio] reader finished, nothing queued; pushed={} frames", ho.pushed.load(Ordering::Relaxed));
                        }
                        break;
                    };
                    if dbg {
                        eprintln!("[audio] handing over to the next track at frame {}", ho.pushed.load(Ordering::Relaxed));
                    }
                    let r = audio.ring.lock().unwrap();
                    if r.epoch != epoch {
                        return;
                    }
                    let fade = (ho.xfade.load(Ordering::Relaxed) as usize * 2).min(r.q.len() / 2 * 2);
                    mix_total = fade;
                    mix_pos = 0;
                    let pushed = ho.pushed.load(Ordering::Relaxed);
                    let boundary = pushed.saturating_sub((fade / 2) as u64);
                    ho.pushed.store(boundary, Ordering::Relaxed);
                    ho.boundary.store(boundary, Ordering::SeqCst);
                    drop(r);
                    *ho.next_streams.lock().unwrap() = Some(nx.streams);
                    *ho.active.lock().unwrap() = Some(nx.child);
                    pr = nx.reader;
                    carry.clear();
                    continue;
                }
                Ok(n) => n,
            };
            if first_audio && dbg {
                first_audio = false;
                eprintln!("[spawn] first audio bytes after {:.2}s", t0.elapsed().as_secs_f32());
            }
            carry.extend_from_slice(&buf[..n]);
            let usable = carry.len() / 4 * 4;
            loop {
                if s3.load(Ordering::Relaxed) {
                    return;
                }
                let mut r = audio.ring.lock().unwrap();
                if r.epoch != epoch {
                    return;
                }
                if r.q.len() < cap {
                    let mut samples: Vec<f32> = carry[..usable]
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    // crossfade: the first samples of the new track replace the tail of the old
                    if mix_pos < mix_total && !samples.is_empty() {
                        let k = (mix_total - mix_pos).min(samples.len());
                        let remaining = mix_total - mix_pos;
                        if r.q.len() >= remaining {
                            let base = r.q.len() - remaining;
                            for j in 0..k {
                                let t = (mix_pos + j) as f32 / mix_total as f32;
                                let old = r.q[base + j];
                                r.q[base + j] = old * (1.0 - t) + samples[j] * t;
                            }
                        }
                        mix_pos += k;
                        samples.drain(..k);
                    }
                    r.q.extend(samples.iter());
                    ho.pushed.fetch_add((usable / 8) as u64, Ordering::Relaxed);
                    break;
                }
                drop(r);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            carry.drain(..usable);
        }
        if audio_only {
            audio_end.store(true, Ordering::SeqCst);
        }
    });

    // stderr
    let mut stderr = child.stderr.take().unwrap();
    let e3 = err.clone();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        let tail: String = s.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        *e3.lock().unwrap() = tail;
    });

    Ok(Session { handoff: handoff.clone(), child: Some(child), child2, rx, stop, ended, err, start, got_frame: false, audio_only })
}

/// Opens the default audio output device; returns its sample rate.
fn open_output(audio: Arc<Audio>) -> Result<(cpal::Stream, u32), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let rate: u32 = cfg.sample_rate();
    let channels = cfg.channels() as usize;
    let config: cpal::StreamConfig = cfg.into();
    let stream = dev
        .build_output_stream(
            config,
            move |data: &mut [f32], _| {
                let paused = audio.paused.load(Ordering::Relaxed);
                let vol = f32::from_bits(audio.volume.load(Ordering::Relaxed));
                let mut r = audio.ring.lock().unwrap();
                let mut consumed = 0u64;
                let mut tap: Vec<f32> = Vec::with_capacity(data.len() / channels.max(1));
                for frame in data.chunks_mut(channels) {
                    frame.fill(0.0);
                    if paused || r.q.len() < 2 {
                        continue;
                    }
                    let l0 = r.q.pop_front().unwrap();
                    let r0 = r.q.pop_front().unwrap();
                    tap.push((l0 + r0) * 0.5);
                    let l = l0 * vol;
                    let rr = r0 * vol;
                    consumed += 1;
                    if channels == 1 {
                        frame[0] = (l + rr) * 0.5;
                    } else {
                        frame[0] = l;
                        frame[1] = rr;
                    }
                }
                if consumed > 0 {
                    audio.played.fetch_add(consumed, Ordering::Relaxed);
                }
                drop(r);
                if !tap.is_empty() {
                    if let Ok(mut v) = audio.viz.try_lock() {
                        v.extend_from_slice(&tap);
                        let len = v.len();
                        if len > 4096 {
                            v.drain(..len - 4096);
                        }
                    }
                }
            },
            |e| eprintln!("audio error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate))
}
