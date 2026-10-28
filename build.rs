//! Build script: gives the Windows executable its icon. Nothing happens on other systems.

fn main() {
    println!("cargo:rerun-if-changed=locales");
    generate_locale_manifest();
    println!("cargo:rerun-if-changed=packaging/windows/tubeglass.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_icon();
    }
}

/// Discover catalogs without requiring new languages to be registered in Rust code.
fn generate_locale_manifest() {
    let dir = std::path::Path::new("locales");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("locales.rs");
    let mut catalogs = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    catalogs.sort();
    validate_catalogs(&catalogs);
    let mut generated = String::from("pub static CATALOG_SOURCES: &[(&str, &str)] = &[\n");
    for path in catalogs {
        println!("cargo:rerun-if-changed={}", path.display());
        let Some(code) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        generated.push_str(&format!(
            "    ({code:?}, include_str!({:?})),\n",
            path.canonicalize()
                .unwrap_or_else(|_| path.clone())
                .display()
                .to_string()
        ));
    }
    generated.push_str("];\n");
    std::fs::write(out, generated).expect("write locale manifest");
}

/// Catch untranslated literal `tr("...")` calls during every build.
fn validate_catalogs(catalogs: &[std::path::PathBuf]) {
    let locale_sources = catalogs
        .iter()
        .map(|path| {
            (
                path,
                std::fs::read_to_string(path).expect("read locale catalog"),
            )
        })
        .collect::<Vec<_>>();
    let english = locale_sources
        .iter()
        .find(|(path, _)| path.file_stem().is_some_and(|stem| stem == "en"))
        .map(|(_, source)| source)
        .expect("locales/en.json is required");
    let english_keys = json_catalog_keys(english);
    for (path, source) in &locale_sources {
        if path.file_stem().is_some_and(|stem| stem == "en") {
            continue;
        }
        for key in json_catalog_keys(source) {
            if key != "__language_name" && !english_keys.contains(&key) {
                panic!("{} contains unknown translation key {key:?}", path.display());
            }
        }
    }

    let mut source_files = ["src/ui", "src/app"]
        .into_iter()
        .flat_map(|directory| {
            std::fs::read_dir(directory)
                .expect("read UI source directory")
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    source_files.sort();
    for source_path in source_files {
        let source = std::fs::read_to_string(&source_path).expect("read UI source file");
        let mut keys = Vec::new();
        for call in ["tr", "trf"] {
            for args in call_arguments(&source, call) {
                if let Some(key) = args.first().and_then(|arg| string_literal(arg)) {
                    keys.push(key);
                }
            }
        }
        // These helpers translate their label/hint arguments internally. Keep their
        // literal call-site keys checked even though the call does not spell `tr(...)`.
        for (call, indexes) in [("row", &[1usize, 2][..]), ("section", &[1][..]), ("ask", &[2, 3][..])] {
            for args in call_arguments(&source, call) {
                for index in indexes {
                    if let Some(key) = args.get(*index).and_then(|arg| string_literal(arg)) {
                        if !key.is_empty() {
                            keys.push(key);
                        }
                    }
                }
            }
        }
        keys.sort_unstable();
        keys.dedup();
        for key in keys {
            if !english_keys.contains(&key) {
                panic!(
                    "locales/en.json is missing source key {key:?} used in {}",
                    source_path.display()
                );
            }
        }

        // Static strings passed directly to egui text sinks are a common source of
        // English-only UI regressions. Require those literals to go through tr/trf.
        let sinks: &[(&str, &[usize])] = &[
            ("RichText::new", &[0]),
            ("Button::new", &[0]),
            ("ui.button", &[0]),
            ("ui.small_button", &[0]),
            ("ui.menu_button", &[0]),
            ("ui.link", &[0]),
            ("ui.label", &[0]),
            ("ui.colored_label", &[1]),
            ("ui.selectable_label", &[1]),
            (".on_hover_text", &[0]),
            ("Window::new", &[0]),
            (".set_title", &[0]),
            (".set_description", &[0]),
            (".add_filter", &[0]),
            ("toast", &[0]),
            ("text_field", &[2, 6]),
            ("icon_btn", &[2]),
            ("icon_label", &[3]),
            ("pill_button", &[1]),
            (".text", &[2]),
        ];
        for (call, indexes) in sinks {
            for args in call_arguments(&source, call) {
                for index in *indexes {
                    if let Some(value) = args.get(*index).and_then(|arg| string_literal(arg)) {
                        let neutral = value.is_empty()
                            || value == "TubeGlass"
                            || value == "◌"
                            || value == "●"
                            || value.starts_with("https://");
                        if !neutral {
                            panic!(
                                "untranslated UI literal {value:?} in {}; use tr/trf",
                                source_path.display()
                            );
                        }
                    }
                }
            }
        }

        // A full-text source audit catches app-authored phrases stored in label tables or
        // formatted branches before they reach a widget through a variable. Provider content,
        // operating-system error fragments, and shortcut glyphs are intentionally excluded.
        if source_path.starts_with("src/ui") && source_path.file_name().is_some_and(|name| name == "i18n.rs") {
            continue;
        }
        for value in string_literals(&source) {
            let human_text = value.chars().next().is_some_and(char::is_alphabetic)
                && value.chars().any(char::is_whitespace);
            if !human_text || is_non_ui_literal(&value) {
                continue;
            }
            if !english_keys.contains(&value) {
                panic!(
                    "locales/en.json is missing source string {value:?} from {}",
                    source_path.display()
                );
            }
        }
    }
}

fn json_catalog_keys(source: &str) -> std::collections::HashSet<String> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if !line.starts_with('"') {
                return None;
            }
            line[1..]
                .find("\":")
                .map(|end| line[1..end + 1].to_owned())
        })
        .collect()
}

fn string_literals(source: &str) -> Vec<String> {
    let mut values = Vec::new();
    for line in source.lines() {
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut in_string = false;
        let mut escaped = false;
        let mut start = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    values.push(line[start + 1..i].replace("\\\"", "\"").replace("\\n", "\n").replace("\\t", "\t"));
                    in_string = false;
                }
            } else if c == '/' && bytes.get(i + 1) == Some(&b'/') {
                break;
            } else if c == '"' {
                start = i;
                in_string = true;
            }
            i += 1;
        }
    }
    values
}

fn is_non_ui_literal(value: &str) -> bool {
    matches!(
        value,
        "opening input"
            | "Input/output error"
            | "Server returned"
            | "channel not found"
            | "Space / K"
            | "← →"
            | "J / L"
            | "↑ ↓"
            | "[  /  ]"
            | "N / P"
    ) || value.starts_with("[recover]")
        || value.starts_with("[gapless]")
}

/// Split a Rust call's top-level arguments without treating nested calls or strings as separators.
fn call_arguments<'a>(source: &'a str, name: &str) -> Vec<Vec<&'a str>> {
    let token = format!("{name}(");
    let mut result = Vec::new();
    let mut search = 0;
    while let Some(relative) = source[search..].find(&token) {
        let start = search + relative;
        let body_start = start + token.len();
        let mut i = body_start;
        let mut depth = 1usize;
        let mut in_string = false;
        let mut escaped = false;
        while i < source.len() && depth > 0 {
            let c = source.as_bytes()[i] as char;
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            }
            i += 1;
        }
        if depth != 0 {
            break;
        }
        let body_end = i - 1;
        let body = &source[body_start..body_end];
        let mut args = Vec::new();
        let mut arg_start = 0;
        let mut nested = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, byte) in body.bytes().enumerate() {
            let c = byte as char;
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if matches!(c, '(' | '[' | '{') {
                nested += 1;
            } else if matches!(c, ')' | ']' | '}') {
                nested = nested.saturating_sub(1);
            } else if c == ',' && nested == 0 {
                args.push(body[arg_start..offset].trim());
                arg_start = offset + 1;
            }
        }
        if !body[arg_start..].trim().is_empty() {
            args.push(body[arg_start..].trim());
        }
        result.push(args);
        search = i;
    }
    result
}

fn string_literal(expression: &str) -> Option<String> {
    let expression = expression.trim();
    if !expression.starts_with('"') || !expression.ends_with('"') || expression.len() < 2 {
        return None;
    }
    Some(expression[1..expression.len() - 1].replace("\\\"", "\"").replace("\\n", "\n").replace("\\t", "\t"))
}

#[cfg(windows)]
fn embed_windows_icon() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("packaging/windows/tubeglass.ico");
    res.set("ProductName", "TubeGlass");
    res.set("FileDescription", "TubeGlass");
    if let Err(e) = res.compile() {
        // a missing resource compiler only costs the icon, never the build
        println!("cargo:warning=could not embed the icon: {e}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {
    // cross-compiling to Windows from another system: the icon is skipped
}
