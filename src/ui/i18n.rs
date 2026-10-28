//! UI language catalogs. Caption and content language settings are intentionally separate.
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

static CATALOGS: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();
static ACTIVE_LANGUAGE: OnceLock<RwLock<String>> = OnceLock::new();

fn catalogs() -> &'static HashMap<String, HashMap<String, String>> {
    CATALOGS.get_or_init(|| {
        CATALOG_SOURCES
            .iter()
            .map(|(language, json)| {
                let catalog = serde_json::from_str(json).unwrap_or_else(|error| {
                    panic!("invalid {language}.json locale catalog: {error}")
                });
                ((*language).to_owned(), catalog)
            })
            .collect()
    })
}

fn active_language() -> &'static RwLock<String> {
    ACTIVE_LANGUAGE.get_or_init(|| RwLock::new("en".to_owned()))
}

/// Normalize a locale tag to its base language, then select it if a catalog exists.
pub(crate) fn set_language(preference: &str) {
    let requested = if preference == "system" {
        system_language()
    } else {
        preference.to_owned()
    };
    let language = requested
        .split(['-', '_', '.', ':'])
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase();
    let language = if catalogs().contains_key(&language) {
        language
    } else {
        "en".to_owned()
    };
    if let Ok(mut active) = active_language().write() {
        *active = language;
    }
}

/// Read the standard locale environment variables used by Linux, macOS, and Windows runtimes.
fn system_language() -> String {
    #[cfg(windows)]
    if let Some(locale) = windows_user_language() {
        return locale;
    }
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        let locale = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if output.status.success() && !locale.is_empty() {
            return locale;
        }
    }
    ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "en".to_owned())
}

#[cfg(windows)]
fn windows_user_language() -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(locale_name: *mut u16, count: i32) -> i32;
    }
    let mut buffer = [0u16; 85];
    // The Windows API writes a null-terminated locale name when the buffer is large enough.
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    (written > 1).then(|| String::from_utf16_lossy(&buffer[..(written - 1) as usize]))
}

/// Locale codes and translator-provided names for the language selector.
pub(crate) fn language_options() -> Vec<(String, String)> {
    let mut options = catalogs()
        .iter()
        .map(|(code, catalog)| {
            let name = catalog
                .get("__language_name")
                .cloned()
                .unwrap_or_else(|| code.clone());
            (code.clone(), name)
        })
        .collect::<Vec<_>>();
    options.sort_by(|a, b| a.0.cmp(&b.0));
    options
}

/// Return a translated UI string, falling back to English (and then to the source text).
pub(crate) fn tr(key: &str) -> String {
    let language = active_language()
        .read()
        .map(|active| active.clone())
        .unwrap_or_else(|_| "en".to_owned());
    catalogs()
        .get(&language)
        .and_then(|catalog| catalog.get(key))
        .or_else(|| catalogs().get("en").and_then(|catalog| catalog.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_owned())
}

/// Translate a string and substitute named values after lookup, so catalog text stays external.
pub(crate) fn trf(key: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(tr(key), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

/// Catalog validation shared by automated tests and contributor checks.
#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((start, ch)) = chars.next() {
            if ch != '{' {
                continue;
            }
            if chars.peek().is_some_and(|(_, next)| *next == '{') {
                chars.next();
                continue;
            }
            if let Some((end, _)) = text[start + 1..].char_indices().find(|(_, ch)| *ch == '}') {
                result.push(text[start + 1..start + 1 + end].to_owned());
            }
        }
        result.sort();
        result
    }

    #[test]
    fn catalogs_have_no_missing_keys_or_invalid_placeholders() {
        let english = catalogs().get("en").expect("English catalog is required");
        for (language, catalog) in catalogs() {
            for (key, source) in english.iter().filter(|(key, _)| !key.starts_with("__")) {
                let translated = catalog
                    .get(key)
                    .unwrap_or_else(|| panic!("{language}.json is missing key {key:?}"));
                assert_eq!(
                    placeholders(source),
                    placeholders(translated),
                    "invalid placeholders in {language}.json for {key:?}"
                );
            }
        }
    }

    #[test]
    fn unsupported_and_regional_languages_fall_back_safely() {
        set_language("id-ID");
        assert_eq!(tr("Settings"), "Pengaturan");
        assert_eq!(
            trf(
                "Everything plays through it. In use: {ffmpeg}",
                &[("ffmpeg", "ffmpeg")]
            ),
            "Semua media diputar dengannya. Sedang digunakan: ffmpeg"
        );
        set_language("xx-ZZ");
        assert_eq!(tr("Settings"), "Settings");
    }
}
