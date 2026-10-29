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

/// Resolve the raw catalog message before decoding braces or substituting values.
fn message(key: &str) -> String {
    let language = active_language()
        .read()
        .map(|active| active.clone())
        .unwrap_or_else(|_| "en".to_owned());
    lookup(catalogs(), &language, key)
}

/// Return translated UI text, decoding literal braces and falling back to English.
pub(crate) fn tr(key: &str) -> String {
    let text = message(key);
    if !text.bytes().any(|byte| matches!(byte, b'{' | b'}')) {
        return text;
    }
    crate::locale_format::render(&text, &[]).unwrap_or(text)
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
    crate::locale_format::render(&message(key), values).unwrap_or_else(|_| {
        let english = lookup(catalogs(), "en", key);
        crate::locale_format::render(&english, values).unwrap_or(english)
    })
}

/// Catalog validation shared by automated tests and contributor checks.
#[cfg(test)]
mod tests {
    use super::*;

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
                    crate::locale_format::placeholder_names(source).unwrap(),
                    crate::locale_format::placeholder_names(value).unwrap_or_else(|error| panic!("{language}.json for {key:?}: {error}")),
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
        let escaped = ["{{", "count", "}}"].concat();
        assert_eq!(tr(&escaped), "{count}");
        assert_eq!(trf(&escaped, &[("count", "2")]), "{count}");
        let now = 1_000_000;
        let boundaries = [0, 59, 60, 3599, 3600, 86399, 86400];
        for (seconds, expected) in boundaries.into_iter().zip([
            "just now", "just now", "1 min ago", "59 min ago", "1 h ago", "23 h ago", "1 d ago",
        ]) {
            assert_eq!(crate::util::ago_at(now - seconds, now), expected);
        }
        assert_eq!(crate::util::ago_at(now + 60, now), "just now");
        set_language("id-ID");
        assert_eq!(tr("Settings"), "Pengaturan");
        assert_eq!(trf("{count} items", &[("count", "2")]), "2 item");
        for (seconds, expected) in boundaries.into_iter().zip([
            "baru saja", "baru saja", "1 mnt lalu", "59 mnt lalu", "1 jam lalu", "23 jam lalu", "1 hari lalu",
        ]) {
            assert_eq!(crate::util::ago_at(now - seconds, now), expected);
        }
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
