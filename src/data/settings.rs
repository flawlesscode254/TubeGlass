//! User settings and the preferences kept for single channels.

use crate::*;

/// What happens when an item ends.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub(crate) enum Repeat {
    #[default]
    Off,
    One,
    All,
}

/// Preferences kept for one channel.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChanPref {
    /// 0 = live streams only, 1 = live streams and new uploads, 2 = nothing
    pub(crate) notify: u8,
    /// playback speed remembered for this channel (0 = use the global one)
    pub(crate) speed: f32,
    /// recommend less from this channel
    pub(crate) fewer: bool,
}

/// All user settings; every field has a default so old settings files keep loading.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// equalizer gains in dB
    pub(crate) eq_bass: f32,
    pub(crate) eq_mid: f32,
    pub(crate) eq_treble: f32,
    /// even out loud and quiet parts
    pub(crate) normalize: bool,
    /// cut long silences (audio-only playback of spoken content)
    pub(crate) skip_silence: bool,
    pub(crate) chan_prefs: HashMap<String, ChanPref>,
    pub(crate) default_quality: u32,
    pub(crate) resume: bool,
    pub(crate) autoplay: bool,
    pub(crate) sponsorblock: bool,
    pub(crate) captions_default: bool,
    pub(crate) caption_lang: String,
    pub(crate) dl_extras: bool,
    pub(crate) notifications: bool,
    pub(crate) accent: usize,
    /// look for a newer release on GitHub now and then
    pub(crate) update_check: bool,
    /// a version the user chose not to be reminded about
    pub(crate) skipped_version: String,
    /// unix time of the last successful check (so the API is not asked at every start)
    pub(crate) last_update_check: u64,
    pub(crate) accent_custom: [u8; 3],
    /// 0 dark, 1 light, 2 follow the system
    pub(crate) theme: u8,
    /// UI language preference: system, en, or a language catalog code.
    pub(crate) language: String,
    pub(crate) font_scale: f32,
    /// 0 compact, 1 comfortable, 2 spacious
    pub(crate) density: u8,
    /// 0 grid, 1 compact grid, 2 list
    pub(crate) view_mode: u8,
    /// rank the Home feed by what you watch instead of newest first
    pub(crate) home_for_you: bool,
    /// videos marked "not interested"
    pub(crate) hidden: HashSet<String>,
    pub(crate) download_dir: String,
    pub(crate) speed: f32,
    pub(crate) repeat: Repeat,
    pub(crate) shuffle: bool,
    pub(crate) ambient: bool,
    pub(crate) gapless: bool,
    /// crossfade length in seconds (0 = off)
    pub(crate) crossfade: u32,
}

impl Default for Settings {
    /// The settings a fresh install starts with.
    fn default() -> Self {
        Self {
            eq_bass: 0.0,
            eq_mid: 0.0,
            eq_treble: 0.0,
            normalize: false,
            skip_silence: false,
            chan_prefs: HashMap::new(),
            default_quality: 720,
            resume: true,
            autoplay: true,
            sponsorblock: false,
            captions_default: false,
            caption_lang: "en".into(),
            dl_extras: true,
            notifications: false,
            accent: 0,
            update_check: true,
            skipped_version: String::new(),
            last_update_check: 0,
            accent_custom: [255, 154, 31],
            theme: 0,
            language: "system".into(),
            font_scale: 1.0,
            density: 1,
            view_mode: 0,
            home_for_you: false,
            hidden: HashSet::new(),
            download_dir: String::new(),
            speed: 1.0,
            repeat: Repeat::Off,
            shuffle: false,
            ambient: true,
            gapless: true,
            crossfade: 0,
        }
    }
}

/// Where settings.json lives.
pub(crate) fn settings_path() -> Option<std::path::PathBuf> {
    let d = dirs::config_dir()?.join("tubeglass");
    std::fs::create_dir_all(&d).ok()?;
    Some(d.join("settings.json"))
}

/// Reads the settings file (defaults when it is missing or unreadable).
pub(crate) fn load_settings() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes the settings file.
pub(crate) fn save_settings(s: &Settings) {
    if let Some(p) = settings_path() {
        if let Ok(b) = serde_json::to_vec_pretty(s) {
            let _ = std::fs::write(p, b);
        }
    }
}

/// Push settings that are read from global state (accent colour, download folder).
pub(crate) fn apply_settings(s: &Settings) {
    ACCENT_IDX.store(
        s.accent.min(ACCENTS.len()),
        std::sync::atomic::Ordering::Relaxed,
    );
    let c = s.accent_custom;
    ACCENT_CUSTOM.store(
        ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32,
        std::sync::atomic::Ordering::Relaxed,
    );
    download::set_dir(if s.download_dir.trim().is_empty() {
        None
    } else {
        Some(s.download_dir.clone().into())
    });
}
