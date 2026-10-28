//! Small helpers: ids, clocks, text and size formatting, the audio spectrum.

use crate::*;

/// A widget id from any hashable value (egui deprecated `Id::new` in favour of salts).
#[allow(deprecated)]
pub(crate) fn uid(salt: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::new(salt)
}

/// Current unix time in seconds.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "5 min ago" style text for a unix time.
pub(crate) fn ago(ts: u64) -> String {
    ago_at(ts, now_secs())
}

/// Format a relative age with an explicit clock, including future timestamps.
pub(crate) fn ago_at(ts: u64, now: u64) -> String {
    let d = now.saturating_sub(ts);
    match d {
        0..=59 => tr("just now"),
        60..=3599 => trf("{count} min ago", &[("count", &(d / 60).to_string())]),
        3600..=86399 => trf("{count} h ago", &[("count", &(d / 3600).to_string())]),
        _ => trf("{count} d ago", &[("count", &(d / 86400).to_string())]),
    }
}

/// How old a video is, in seconds, from the text YouTube shows ("3d ago", "2 weeks ago",
/// "Streamed 5mo ago", "12 min ago"). Videos with no such text count as the oldest.
pub(crate) fn age_secs(meta: &str) -> u64 {
    let Some(part) = meta.split('•').map(str::trim).find(|p| p.ends_with("ago")) else { return u64::MAX };
    let part = part.trim_end_matches("ago").trim().trim_start_matches("Streamed").trim();
    let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
    let Ok(n) = digits.parse::<u64>() else { return u64::MAX };
    let unit = part[digits.len()..].trim_matches(|c: char| c.is_whitespace() || c == '\u{a0}' || c == '\u{202f}');
    let per = match unit {
        u if u.starts_with("mo") => 2_592_000,
        u if u.starts_with('m') => 60,
        u if u.starts_with('s') => 1,
        u if u.starts_with('h') => 3600,
        u if u.starts_with('d') => 86_400,
        u if u.starts_with('w') => 604_800,
        u if u.starts_with('y') => 31_536_000,
        _ => return u64::MAX,
    };
    n.saturating_mul(per)
}

/// The view count in a video's meta text ("1.2M  •  3d ago", "845 views  •  Streamed 2mo ago").
pub(crate) fn view_count(meta: &str) -> Option<u64> {
    for part in meta.split('•').map(str::trim) {
        let p = part.trim_end_matches("views").trim_end_matches("view").trim();
        if !p.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let (num, mult) = match p.chars().last()? {
            'K' | 'k' => (&p[..p.len() - 1], 1e3),
            'M' | 'm' => (&p[..p.len() - 1], 1e6),
            'B' | 'b' => (&p[..p.len() - 1], 1e9),
            _ => (p, 1.0),
        };
        if let Ok(n) = num.replace(',', "").trim().parse::<f64>() {
            return Some((n * mult) as u64);
        }
    }
    None
}

/// Length of a video from its "1:02:03" style text (0 when unknown).
pub(crate) fn duration_secs(text: &str) -> u64 {
    let mut total = 0u64;
    for part in text.trim().split(':') {
        let Ok(n) = part.trim().parse::<u64>() else { return 0 };
        total = total * 60 + n;
    }
    total
}

/// Playlist ids in pasted text: links with `list=…` or bare ids such as `PLxxxxxxxxxxxxxxxx`.
pub(crate) fn playlist_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for tok in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let id = match tok.split_once("list=") {
            Some((_, r)) => r.split(['&', '#']).next().unwrap_or("").to_string(),
            None => tok.to_string(),
        };
        let ok = id.len() >= 13
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && (tok.contains("list=") || ["PL", "OL", "UU", "FL", "LL", "RD"].iter().any(|p| id.starts_with(p)));
        if ok && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Formats bytes as KB, MB or GB.
pub(crate) fn fmt_size(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 { format!("{:.1} GB", b / 1e9) } else if b >= 1e6 { format!("{:.0} MB", b / 1e6) } else { format!("{:.0} KB", b / 1e3) }
}

/// Band levels (0..1) for `samples`, log-spaced 50 Hz – 14 kHz, using Goertzel filters.
pub(crate) fn spectrum(samples: &[f32], rate: u32, bands: usize) -> Vec<f32> {
    let n = samples.len();
    if n < 8 || bands == 0 {
        return vec![0.0; bands];
    }
    let win: Vec<f32> = samples
        .iter()
        .enumerate()
        .map(|(i, s)| s * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n - 1) as f32).cos()))
        .collect();
    let nyq = rate as f32 * 0.5 * 0.95;
    (0..bands)
        .map(|k| {
            let f = (50.0 * (14000.0f32 / 50.0).powf(k as f32 / (bands - 1).max(1) as f32)).min(nyq);
            let coeff = 2.0 * (std::f32::consts::TAU * f / rate as f32).cos();
            let (mut s1, mut s2) = (0.0f32, 0.0f32);
            for x in &win {
                let s0 = x + coeff * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            let power = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0);
            let amp = power.sqrt() / (n as f32 * 0.25);
            let db = 20.0 * (amp + 1e-6).log10();
            // gentle tilt so the highs are visible next to loud bass
            let tilt = 6.0 * (k as f32 / bands as f32);
            ((db + 64.0 - tilt * -1.0) / 54.0).clamp(0.0, 1.0)
        })
        .collect()
}

/// Lower-cases and strips common accents so "nova" finds "Nóva".
pub(crate) fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'ď' | 'đ' => 'd',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => 'i',
            'ł' => 'l',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
            'ř' => 'r',
            'ś' | 'š' | 'ş' => 's',
            'ť' => 't',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            other => other,
        })
        .collect()
}

/// Formats seconds as m:ss or h:mm:ss.
pub(crate) fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{sec:02}") } else { format!("{m}:{sec:02}") }
}
