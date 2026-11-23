//! Offline downloads: stream-copies the chosen video + audio into an MP4 with ffmpeg
//! (through the same range proxy playback uses) and keeps a small sidecar file so the
//! library can be listed and played without the network.

use crate::yt::{CaptionTrack, Cue, Streams};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// A finished download as listed in the library (one sidecar .json per file).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DlItem {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub file: String,
    pub thumb: String,
    #[serde(default)]
    pub audio_only: bool,
    /// a song (shows lyrics when played back)
    #[serde(default)]
    pub music: bool,
    /// album / playlist this was downloaded from, empty for single items
    #[serde(default)]
    pub group: String,
}

/// A caption track saved next to a download so it can be shown without a connection.
#[derive(Serialize, Deserialize)]
struct SavedCaption {
    lang: String,
    name: String,
    auto: bool,
    cues: Vec<Cue>,
}

/// File name of the download without its extension.
fn stem_of(item: &DlItem) -> String {
    PathBuf::from(&item.file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Files saved alongside a download: caption tracks (`.cc*.cues`) and lyrics (`.lyrics`).
fn extra_files(item: &DlItem) -> Vec<PathBuf> {
    let stem = stem_of(item);
    let Ok(rd) = std::fs::read_dir(dir()) else { return vec![] };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.starts_with(&format!("{stem}.")) && (n.ends_with(".cues") || n.ends_with(".lyrics"))
        })
        .collect()
}

/// Caption tracks that were saved with this download; their `url` is `file:<path>`.
pub fn offline_captions(item: &DlItem) -> Vec<CaptionTrack> {
    let mut v: Vec<CaptionTrack> = extra_files(item)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "cues"))
        .filter_map(|p| {
            let sc: SavedCaption = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            Some(CaptionTrack { lang: sc.lang, name: format!("{} (saved)", sc.name), auto: sc.auto, url: format!("file:{}", p.display()) })
        })
        .collect();
    v.sort_by(|a, b| a.auto.cmp(&b.auto).then(a.lang.cmp(&b.lang)));
    v
}

/// Reads a saved caption track (`file:<path>` as produced by `offline_captions`).
pub fn read_cues(url: &str) -> Result<Vec<Cue>, String> {
    let path = url.strip_prefix("file:").unwrap_or(url);
    let sc: SavedCaption = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(sc.cues)
}

/// Lyrics saved with a download, if any.
pub fn offline_lyrics(item: &DlItem) -> Option<crate::ytmusic::Lyrics> {
    let p = extra_files(item).into_iter().find(|p| p.extension().is_some_and(|e| e == "lyrics"))?;
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// What to save next to a download besides the media itself.
#[derive(Clone, Default)]
pub struct Extras {
    pub captions: bool,
    pub lyrics: bool,
    pub lang: String,
    /// the second caption language (empty = none), saved translated so it works offline
    pub lang2: String,
    /// a podcast episode's transcript, saved so it reads without a connection
    pub transcript: Vec<Cue>,
}

/// Saves captions, lyrics or a transcript next to a finished download.
fn save_extras(stem: &str, d: &std::path::Path, v_title: &str, channel: &str, streams: &Streams, audio_only: bool, ex: &Extras) {
    if !ex.transcript.is_empty() {
        let sc = SavedCaption { lang: "transcript".into(), name: "Transcript".into(), auto: false, cues: ex.transcript.clone() };
        let _ = std::fs::write(d.join(format!("{stem}.cc0.cues")), serde_json::to_vec(&sc).unwrap_or_default());
    }
    if ex.captions && !audio_only {
        let lang = ex.lang.to_lowercase();
        let mut picks: Vec<&CaptionTrack> = streams.captions.iter().filter(|c| !c.auto).take(4).collect();
        if !picks.iter().any(|c| c.lang.to_lowercase().starts_with(&lang)) {
            if let Some(a) = streams.captions.iter().find(|c| c.auto && c.lang.to_lowercase().starts_with(&lang)) {
                picks.push(a);
            }
        }
        // no track in the preferred language: keep a translation of the first one too
        let translate_from = if picks.iter().any(|c| c.lang.to_lowercase().starts_with(&lang)) { None } else { picks.first().copied() };
        if let Some(tr) = translate_from {
            let url = format!("{}&tlang={}", tr.url, ex.lang);
            if let Ok(cues) = crate::yt::caption_cues(&url) {
                let sc = SavedCaption { lang: ex.lang.clone(), name: format!("{} (translated)", ex.lang), auto: true, cues };
                let _ = std::fs::write(d.join(format!("{stem}.cc9.cues")), serde_json::to_vec(&sc).unwrap_or_default());
            }
        }
        // the second language, saved translated when the video has no track in it
        let lang2 = ex.lang2.to_lowercase();
        if !lang2.is_empty() && lang2 != lang {
            let have = streams.captions.iter().any(|c| c.lang.to_lowercase().starts_with(&lang2.split('-').next().unwrap_or("").to_string()));
            let svc = crate::translate::Service::default();
            let base = streams.captions.iter().position(|c| !c.auto).unwrap_or(0);
            if !streams.captions.is_empty() {
                if let Ok(cues) = crate::translate::second_cues(&streams.captions, base, &ex.lang2, &svc) {
                    let name = if have { format!("{} (second language)", ex.lang2) } else { format!("{} (translated)", ex.lang2) };
                    let sc = SavedCaption { lang: ex.lang2.clone(), name, auto: !have, cues };
                    let _ = std::fs::write(d.join(format!("{stem}.cc8.cues")), serde_json::to_vec(&sc).unwrap_or_default());
                }
            }
        }
        for (i, tr) in picks.into_iter().enumerate() {
            if let Ok(cues) = crate::yt::caption_cues(&tr.url) {
                let sc = SavedCaption { lang: tr.lang.clone(), name: tr.name.clone(), auto: tr.auto, cues };
                let _ = std::fs::write(d.join(format!("{stem}.cc{i}.cues")), serde_json::to_vec(&sc).unwrap_or_default());
            }
        }
    }
    if ex.lyrics && audio_only {
        // the lyrics service is flaky now and then, so give it a few tries
        for attempt in 0..3 {
            match crate::ytmusic::lyrics("", v_title, channel, streams.duration) {
                Ok(l) => {
                    let _ = std::fs::write(d.join(format!("{stem}.lyrics")), serde_json::to_vec(&l).unwrap_or_default());
                    break;
                }
                Err(e) if e == crate::ytmusic::RETRYABLE && attempt < 2 => std::thread::sleep(std::time::Duration::from_secs(2)),
                Err(e) => {
                    if std::env::var_os("TG_DEBUG").is_some() {
                        eprintln!("[download] no lyrics saved: {e}");
                    }
                    break;
                }
            }
        }
    }
}

/// Download folder chosen in the settings (None = the default).
static DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Use `dir` instead of `~/Videos/TubeGlass` (None restores the default).
pub fn set_dir(dir: Option<PathBuf>) {
    if let Ok(mut g) = DIR_OVERRIDE.write() {
        *g = dir;
    }
}

/// The folder downloads are saved in.
pub fn dir() -> PathBuf {
    if let Ok(g) = DIR_OVERRIDE.read() {
        if let Some(d) = g.as_ref() {
            return d.clone();
        }
    }
    dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TubeGlass")
}

/// All finished downloads, newest first.
pub fn list() -> Vec<DlItem> {
    let mut items: Vec<(std::time::SystemTime, DlItem)> = vec![];
    let Ok(rd) = std::fs::read_dir(dir()) else { return vec![] };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let Ok(item) = serde_json::from_slice::<DlItem>(&bytes) else { continue };
        if std::path::Path::new(&item.file).exists() {
            let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            items.push((t, item));
        }
    }
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.into_iter().map(|(_, i)| i).collect()
}

/// Deletes a download with its thumbnail, sidecar, captions and lyrics.
pub fn delete(item: &DlItem) {
    let _ = std::fs::remove_file(&item.file);
    let _ = std::fs::remove_file(&item.thumb);
    let _ = std::fs::remove_file(PathBuf::from(&item.file).with_extension("json"));
    for p in extra_files(item) {
        let _ = std::fs::remove_file(p);
    }
}

/// Makes a title safe to use in a file name.
pub fn sanitize(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_().,&'".contains(c) { c } else { '_' })
        .collect();
    t.trim().chars().take(90).collect()
}

/// Size of each ranged request (1 MiB).
const CHUNK: u64 = 1024 * 1024;

/// A short-lived HTTP agent for ranged downloads.
fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into()
}

/// Byte length of a stream URL: the `clen` query parameter, or a 1-byte ranged probe.
fn total_len(url: &str, ua: &str) -> Result<u64, String> {
    if let Some(l) = url
        .split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("clen="))
        .and_then(|v| v.parse().ok())
    {
        return Ok(l);
    }
    let r = http_agent()
        .get(url)
        .header("User-Agent", ua)
        .header("Range", "bytes=0-0")
        .call()
        .map_err(|e| e.to_string())?;
    r.headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('/').next()?.parse().ok())
        .ok_or_else(|| "unknown stream length".to_string())
}

/// Blocks while paused. Returns Err when cancelled.
fn wait_if_paused(cancel: &AtomicBool, pause: &AtomicBool) -> Result<(), String> {
    while pause.load(Ordering::Relaxed) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    Ok(())
}

/// Resumable download of `url` into `path`. Existing bytes in `path` are kept and the
/// transfer continues from there, so pausing (or a dropped connection) loses nothing.
/// Pausing only happens between 1 MiB chunks, so no connection is ever left stalled.
#[allow(clippy::too_many_arguments)]
fn fetch_to_file(
    url: &str,
    ua: &str,
    path: &std::path::Path,
    total: u64,
    base: u64,
    grand_total: u64,
    cancel: &AtomicBool,
    pause: &AtomicBool,
    on_progress: &dyn Fn(f32, u64),
) -> Result<(), String> {
    use std::io::Write;
    let agent = http_agent();
    let mut pos = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if pos > total {
        let _ = std::fs::remove_file(path);
        pos = 0;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let report = |done: u64| {
        let f = (done as f64 / grand_total.max(1) as f64).clamp(0.0, 0.99) as f32;
        on_progress(f, done);
    };
    report(base + pos);
    while pos < total {
        wait_if_paused(cancel, pause)?;
        let end = (pos + CHUNK - 1).min(total - 1);
        let mut attempt = 0;
        loop {
            let resp = agent
                .get(url)
                .header("User-Agent", ua)
                .header("Range", format!("bytes={pos}-{end}"))
                .call();
            match resp {
                Ok(r) => {
                    let mut body = r.into_body();
                    let mut rd = body.as_reader();
                    let mut got = 0u64;
                    loop {
                        match rd.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                                got += n as u64;
                            }
                            Err(_) => break,
                        }
                    }
                    if got == 0 {
                        attempt += 1;
                        if attempt >= 4 {
                            return Err("download stalled".into());
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                    pos += got;
                    break;
                }
                Err(e) => {
                    attempt += 1;
                    if attempt >= 4 {
                        return Err(format!("network error: {e}"));
                    }
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
        report(base + pos);
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Runs ffmpeg with the given arguments, returning its last error lines on failure.
fn run_ffmpeg_copy(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = crate::platform::command("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .output()
        .map_err(|e| format!("could not start ffmpeg ({e})"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(2).collect();
        Err(format!("ffmpeg failed: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" ")))
    }
}

/// Downloads a video or its audio to the downloads folder, resumable and cancellable, then writes its sidecar.
#[allow(clippy::too_many_arguments)]
pub fn run(
    id: &str,
    title: &str,
    channel: &str,
    thumb_url: &str,
    streams: &Streams,
    quality: u32,
    cancel: &AtomicBool,
    pause: &AtomicBool,
    audio_only: bool,
    music: bool,
    group: &str,
    extras: &Extras,
    on_progress: &dyn Fn(f32, u64),
) -> Result<DlItem, String> {
    if streams.is_live {
        return Err("live streams can't be downloaded".into());
    }
    let opt = streams
        .video
        .iter()
        .filter(|v| v.height <= quality)
        .max_by_key(|v| v.height)
        .or_else(|| streams.video.iter().min_by_key(|v| v.height))
        .cloned()
        .ok_or("no stream available")?;

    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    let stem = if audio_only {
        format!("{} [{id}] (audio)", sanitize(title))
    } else {
        format!("{} [{id}]", sanitize(title))
    };
    let out = d.join(format!("{stem}.{}", if audio_only { "m4a" } else { "mp4" }));

    if opt.hls && !audio_only || (opt.hls && streams.audio.is_none()) {
        run_hls(&opt.url, opt.audio_url.as_deref(), &opt.ua, &out, &d, &stem, streams, audio_only, cancel, pause, on_progress)?;
    } else {
        // plain progressive streams: fetch the raw files (resumable), then mux / rename
        let vpart = d.join(format!("{stem}.v.part"));
        let apart = d.join(format!("{stem}.a.part"));
        // (url, ua, file) in download order
        let mut jobs: Vec<(String, String, std::path::PathBuf)> = vec![];
        if audio_only {
            match &streams.audio {
                Some(a) => jobs.push((a.url.clone(), a.ua.clone(), apart.clone())),
                None => jobs.push((opt.url.clone(), opt.ua.clone(), vpart.clone())),
            }
        } else {
            jobs.push((opt.url.clone(), opt.ua.clone(), vpart.clone()));
            if !opt.muxed {
                let a = streams.audio.as_ref().ok_or("no audio stream")?;
                jobs.push((a.url.clone(), a.ua.clone(), apart.clone()));
            }
        }
        let mut totals = vec![];
        for (u, ua, _) in &jobs {
            totals.push(total_len(u, ua)?);
        }
        let grand: u64 = totals.iter().sum();
        let mut base = 0u64;
        for ((u, ua, path), t) in jobs.iter().zip(&totals) {
            if let Err(e) = fetch_to_file(u, ua, path, *t, base, grand, cancel, pause, on_progress) {
                if e == "cancelled" {
                    for (_, _, p) in &jobs {
                        let _ = std::fs::remove_file(p);
                    }
                }
                return Err(e);
            }
            base += *t;
        }
        let tmp = d.join(format!("{stem}.mux.part"));
        let result = if audio_only {
            let (_, _, src) = &jobs[0];
            if streams.audio.is_some() {
                std::fs::rename(src, &out).map_err(|e| e.to_string())
            } else {
                run_ffmpeg_copy(&[
                    "-i".as_ref(), src.as_os_str(), "-vn".as_ref(), "-map".as_ref(), "0:a:0".as_ref(),
                    "-c".as_ref(), "copy".as_ref(), "-f".as_ref(), "mp4".as_ref(), tmp.as_os_str(),
                ])
                .and_then(|_| std::fs::rename(&tmp, &out).map_err(|e| e.to_string()))
            }
        } else if jobs.len() == 1 {
            std::fs::rename(&jobs[0].2, &out).map_err(|e| e.to_string())
        } else {
            run_ffmpeg_copy(&[
                "-i".as_ref(), jobs[0].2.as_os_str(), "-i".as_ref(), jobs[1].2.as_os_str(),
                "-map".as_ref(), "0:v:0".as_ref(), "-map".as_ref(), "1:a:0".as_ref(),
                "-c".as_ref(), "copy".as_ref(), "-movflags".as_ref(), "+faststart".as_ref(),
                "-f".as_ref(), "mp4".as_ref(), tmp.as_os_str(),
            ])
            .and_then(|_| std::fs::rename(&tmp, &out).map_err(|e| e.to_string()))
        };
        for (_, _, p) in &jobs {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_file(&tmp);
        result?;
    }

    // thumbnail (best effort) + sidecar
    let thumb_path = d.join(format!("{stem}.jpg"));
    if let Ok(mut r) = ureq::get(thumb_url).call() {
        if let Ok(b) = r.body_mut().with_config().limit(5_000_000).read_to_vec() {
            let _ = std::fs::write(&thumb_path, b);
        }
    }
    let item = DlItem {
        id: id.to_string(),
        title: title.to_string(),
        channel: channel.to_string(),
        duration: streams.duration,
        width: if audio_only { 0 } else { opt.width },
        height: if audio_only { 0 } else { opt.height },
        audio_only,
        music,
        group: group.to_string(),
        file: out.display().to_string(),
        thumb: thumb_path.display().to_string(),
    };
    let extras = if id.starts_with("pod:") { Extras { lyrics: false, captions: false, ..extras.clone() } } else { extras.clone() };
    save_extras(&stem, &d, title, channel, streams, audio_only, &extras);
    let _ = std::fs::write(out.with_extension("json"), serde_json::to_vec(&item).unwrap_or_default());
    on_progress(1.0, std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0));
    Ok(item)
}

/// HLS variants have no single file to resume, so they go through one ffmpeg process.
/// Pausing suspends that process (SIGSTOP) and resuming continues it (SIGCONT).
#[allow(clippy::too_many_arguments)]
fn run_hls(
    url: &str,
    audio_url: Option<&str>,
    ua: &str,
    out: &std::path::Path,
    dir: &std::path::Path,
    stem: &str,
    streams: &Streams,
    audio_only: bool,
    cancel: &AtomicBool,
    pause: &AtomicBool,
    on_progress: &dyn Fn(f32, u64),
) -> Result<(), String> {
    let part = dir.join(format!("{stem}.part"));
    let mut cmd = crate::platform::command("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-user_agent", ua]);
    // an HLS variant with a separate audio rendition needs both playlists as inputs
    match audio_url {
        Some(a) if audio_only => {
            cmd.args(["-i", a, "-vn", "-map", "0:a:0"]);
        }
        Some(a) => {
            cmd.args(["-i", url, "-user_agent", ua, "-i", a, "-map", "0:v:0", "-map", "1:a:0"]);
        }
        None if audio_only => {
            cmd.args(["-i", url, "-vn", "-map", "0:a:0"]);
        }
        None => {
            cmd.args(["-i", url, "-map", "0:v:0", "-map", "0:a:0?"]);
        }
    }
    cmd.args(["-c", "copy", "-f", "mp4", "-movflags", "+faststart", "-progress", "pipe:1", "-nostats"]);
    cmd.arg(&part);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("could not start ffmpeg ({e})"))?;
    let pid = child.id();
    let mut err = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let dur = streams.duration.max(1.0);
    let reader = BufReader::new(child.stdout.take().unwrap());
    for line in reader.lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err("cancelled".into());
        }
        if pause.load(Ordering::Relaxed) {
            // where processes cannot be frozen, the transfer simply carries on
            let frozen = crate::platform::suspend_process(pid);
            while pause.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            if frozen {
                crate::platform::resume_process(pid);
            }
        }
        if let Some(us) = line
            .strip_prefix("out_time_us=")
            .or_else(|| line.strip_prefix("out_time_ms="))
            .and_then(|v| v.trim().parse::<f64>().ok())
        {
            let bytes = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            on_progress(((us / 1e6) / dur).clamp(0.0, 0.99) as f32, bytes);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let stderr = err_thread.join().unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        let tail: Vec<&str> = stderr.lines().rev().take(2).collect();
        return Err(format!("ffmpeg failed: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" ")));
    }
    std::fs::rename(&part, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "network"]
    fn saves_captions_and_lyrics() {
        let tmp = std::env::temp_dir().join("tg-dl-test");
        let _ = std::fs::remove_dir_all(&tmp);
        set_dir(Some(tmp.clone()));
        let no = AtomicBool::new(false);
        let ex = Extras { captions: true, lyrics: true, lang: "en".into(), lang2: "fr".into(), transcript: vec![] };
        let st = crate::yt::streams("jNQXAC9IVRw").expect("streams");
        let v = run("jNQXAC9IVRw", "Me at the zoo", "jawed", "", &st, 144, &no, &no, false, false, "", &ex, &|_, _| {}).expect("video");
        let caps = offline_captions(&v);
        println!("captions: {:?}", caps.iter().map(|c| (&c.lang, c.auto)).collect::<Vec<_>>());
        assert!(!caps.is_empty());
        assert!(!crate::yt::caption_cues(&caps[0].url).unwrap().is_empty());
        let st = crate::yt::streams("fJ9rUzIMcZQ").expect("streams");
        let a = run("fJ9rUzIMcZQ", "Bohemian Rhapsody", "Queen", "", &st, 144, &no, &no, true, true, "Album", &ex, &|_, _| {}).expect("audio");
        let l = offline_lyrics(&a).expect("lyrics");
        println!("lyrics: {} lines synced={}", l.lines.len(), l.synced);
        assert_eq!(list().iter().filter(|i| i.group == "Album").count(), 1);
        delete(&a);
        assert!(offline_lyrics(&a).is_none());
        set_dir(None);
    }
}
