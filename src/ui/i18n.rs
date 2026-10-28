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
                .filter(|name| !name.trim().is_empty())
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
    lookup(catalogs(), &language, key)
}

fn lookup(catalogs: &HashMap<String, HashMap<String, String>>, language: &str, key: &str) -> String {
    catalogs
        .get(language)
        .and_then(|catalog| catalog.get(key))
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            catalogs
                .get("en")
                .and_then(|catalog| catalog.get(key))
                .filter(|value| !value.trim().is_empty())
        })
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
    fn english_is_complete_and_locales_have_known_keys_and_valid_placeholders() {
        let english = catalogs().get("en").expect("English catalog is required");
        let total = english.keys().filter(|key| key.as_str() != "__language_name").count();
        assert!(english.iter().filter(|(key, _)| key.as_str() != "__language_name").all(|(_, value)| !value.trim().is_empty()), "English source values must be complete");
        for (language, catalog) in catalogs() {
            let mut translated = 0;
            for (key, value) in catalog {
                if key == "__language_name" {
                    continue;
                }
                let source = english.get(key).unwrap_or_else(|| panic!("{language}.json has unknown key {key:?}"));
                if value.trim().is_empty() {
                    continue;
                }
                translated += 1;
                assert_eq!(
                    placeholders(source),
                    placeholders(value),
                    "invalid placeholders in {language}.json for {key:?}"
                );
            }
            eprintln!("{language}: {translated}/{total} UI strings translated ({:.1}%)", translated as f64 * 100.0 / total as f64);
        }
    }

    #[test]
    fn unsupported_and_regional_languages_fall_back_safely() {
        set_language("en");
        assert_eq!(trf("{count} items", &[("count", "2")]), "2 items");
        set_language("id-ID");
        assert_eq!(tr("Settings"), "Pengaturan");
        assert_eq!(trf("{count} items", &[("count", "2")]), "2 item");
        assert_eq!(
            trf(
                "Everything plays through it. In use: {ffmpeg}",
                &[("ffmpeg", "ffmpeg")]
            ),
            "Semua media diputar dengannya. Sedang digunakan: ffmpeg"
        );
        set_language("xx-ZZ");
        assert_eq!(tr("Settings"), "Settings");
        let uncatalogued = ["Uncatalogued", " text"].concat();
        assert_eq!(tr(&uncatalogued), uncatalogued);
    }

    #[test]
    fn partial_locales_fall_back_for_missing_and_blank_values() {
        let catalogs = HashMap::from([
            ("en".to_owned(), HashMap::from([
                ("Settings".to_owned(), "Settings".to_owned()),
                ("Downloads".to_owned(), "Downloads".to_owned()),
            ])),
            ("fr".to_owned(), HashMap::from([
                ("Settings".to_owned(), "Paramètres".to_owned()),
                ("Downloads".to_owned(), String::new()),
            ])),
        ]);
        assert_eq!(lookup(&catalogs, "fr", "Settings"), "Paramètres");
        assert_eq!(lookup(&catalogs, "fr", "Downloads"), "Downloads");
    }
}
