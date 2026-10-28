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
    let mut source_files = std::fs::read_dir("src/ui")
        .expect("read UI source directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect::<Vec<_>>();
    source_files.sort();
    for source_path in source_files {
        let source = std::fs::read_to_string(&source_path).expect("read UI source file");
        for part in source.split("tr(\"").skip(1) {
            let Some((key, _)) = part.split_once("\")") else {
                continue;
            };
            // Catalog keys are plain UI strings; require the same key in every shipped locale.
            let catalog_key = format!("\"{key}\":");
            for catalog_path in catalogs {
                let catalog = std::fs::read_to_string(catalog_path).expect("read locale catalog");
                if !catalog.contains(&catalog_key) {
                    panic!(
                        "{} is missing translation key {key:?} used in {}",
                        catalog_path.display(),
                        source_path.display()
                    );
                }
            }
        }
    }
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
