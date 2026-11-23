//! Second caption track: picking or translating the cues, and the optional translation service
//! for captions YouTube cannot translate itself (saved captions, podcast transcripts).

use crate::*;
use crate::yt::{CaptionTrack, Cue};

/// A LibreTranslate-compatible translation service chosen in Settings.
#[derive(Clone, Default)]
pub struct Service {
    pub url: String,
    pub key: String,
}

/// The primary subtag of a language code ("fr-CA" → "fr", "zh-Hans" → "zh").
fn primary(code: &str) -> String {
    code.to_lowercase().split(['-', '_']).next().unwrap_or("").to_string()
}

/// True when the track is in the wanted language.
fn is_lang(t: &CaptionTrack, lang: &str) -> bool {
    primary(&t.lang) == primary(lang)
}

/// Translates cues with the configured service, a batch at a time.
pub fn translate_cues(svc: &Service, cues: &[Cue], target: &str) -> Result<Vec<Cue>, String> {
    if svc.url.trim().is_empty() {
        return Err("this caption track cannot be translated by YouTube: add a translation service in Settings".into());
    }
    let endpoint = format!("{}/translate", svc.url.trim().trim_end_matches('/'));
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(40))).build().into();
    let mut out = Vec::with_capacity(cues.len());
    for chunk in cues.chunks(40) {
        let q: Vec<&str> = chunk.iter().map(|c| c.text.as_str()).collect();
        let mut body = serde_json::json!({ "q": q, "source": "auto", "target": primary(target), "format": "text" });
        if !svc.key.trim().is_empty() {
            body["api_key"] = serde_json::Value::String(svc.key.trim().to_string());
        }
        let resp: serde_json::Value = agent
            .post(&endpoint)
            .send_json(&body)
            .map_err(|e| format!("translation service: {e}"))?
            .body_mut()
            .read_json()
            .map_err(|e| format!("translation service: {e}"))?;
        let texts: Vec<String> = match &resp["translatedText"] {
            serde_json::Value::Array(a) => a.iter().map(|t| t.as_str().unwrap_or("").to_string()).collect(),
            serde_json::Value::String(s) if chunk.len() == 1 => vec![s.clone()],
            _ => return Err(resp["error"].as_str().map_or("the translation service sent an unexpected answer".to_string(), |e| format!("translation service: {e}"))),
        };
        if texts.len() != chunk.len() {
            return Err("the translation service returned the wrong number of lines".into());
        }
        for (c, t) in chunk.iter().zip(texts) {
            out.push(Cue { start: c.start, end: c.end, text: t });
        }
    }
    Ok(out)
}

/// The cues of the second language: a published or automatic track in that language when there
/// is one, otherwise YouTube's own translation of the base track, otherwise the translation
/// service (for saved captions and transcripts, which YouTube cannot translate).
pub fn second_cues(tracks: &[CaptionTrack], base: usize, lang: &str, svc: &Service) -> Result<Vec<Cue>, String> {
    let exact = tracks.iter().find(|t| !t.auto && is_lang(t, lang)).or_else(|| tracks.iter().find(|t| is_lang(t, lang)));
    if let Some(t) = exact {
        return crate::yt::caption_cues(&t.url);
    }
    let Some(b) = tracks.get(base).or_else(|| tracks.first()) else { return Err("this video has no captions to translate".into()) };
    let local = b.url.starts_with("file:") || b.url.starts_with("podtr:");
    if !local {
        return crate::yt::caption_cues(&format!("{}&tlang={lang}", b.url));
    }
    let cues = crate::yt::caption_cues(&b.url)?;
    translate_cues(svc, &cues, lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_compare_by_their_first_part() {
        let t = |l: &str| CaptionTrack { lang: l.into(), name: String::new(), auto: false, url: String::new() };
        assert!(is_lang(&t("fr-CA"), "fr"));
        assert!(is_lang(&t("zh-Hans"), "zh-Hant"));
        assert!(!is_lang(&t("en"), "fr"));
    }

    #[test]
    fn a_service_is_required_for_saved_captions() {
        let cues = vec![Cue { start: 0.0, end: 1.0, text: "Hi".into() }];
        assert!(translate_cues(&Service::default(), &cues, "fr").is_err());
    }

    #[test]
    fn the_service_is_spoken_to_in_libretranslate_form() {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            // the headers and the body can arrive separately: read until the body is complete
            let mut data: Vec<u8> = vec![];
            let mut tmp = [0u8; 4096];
            loop {
                let n = s.read(&mut tmp).unwrap();
                data.extend_from_slice(&tmp[..n]);
                let text = String::from_utf8_lossy(&data).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let want: usize = head.to_lowercase().lines().find_map(|l| l.strip_prefix("content-length:")?.trim().parse().ok()).unwrap_or(0);
                    if body.len() >= want || n == 0 {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let req = String::from_utf8_lossy(&data).to_string();
            let body = r#"{"translatedText":["Bonjour","Au revoir"]}"#;
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            req
        });
        let svc = Service { url: format!("http://127.0.0.1:{port}"), key: "k".into() };
        let cues = vec![Cue { start: 0.0, end: 1.0, text: "Hello".into() }, Cue { start: 1.0, end: 2.0, text: "Bye".into() }];
        let out = translate_cues(&svc, &cues, "fr").unwrap();
        assert_eq!(out[0].text, "Bonjour");
        assert_eq!(out[1].end, 2.0);
        let req = h.join().unwrap();
        assert!(req.starts_with("POST /translate"));
        let body: serde_json::Value = serde_json::from_str(req.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["target"], "fr");
        assert_eq!(body["api_key"], "k");
        assert_eq!(body["q"][1], "Bye");
    }
}
