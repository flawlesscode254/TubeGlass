//! Exercise the real build audit with JSON variants and successive source edits.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tubeglass-locale-audit-{}-{nonce}",
            std::process::id()
        ));
        for directory in ["src/ui", "src/app", "locales", "packaging/windows"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        fs::copy(repo.join("build.rs"), root.join("build.rs")).unwrap();
        fs::copy(
            repo.join("src/locale_format.rs"),
            root.join("src/locale_format.rs"),
        )
        .unwrap();
        fs::write(root.join("packaging/windows/tubeglass.ico"), b"fixture").unwrap();
        let mut manifest = String::from(
            "[package]\nname='locale-audit-fixture'\nversion='0.0.0'\nedition='2024'\n[build-dependencies]\nserde_json='1.0.154'\n",
        );
        // Match the app's platform-specific build dependency when running this test on Windows.
        if cfg!(windows) {
            manifest.push_str("winresource='0.1'\n");
        }
        fs::write(root.join("Cargo.toml"), manifest).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/util.rs"), "").unwrap();
        Self(root)
    }

    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap();
    }

    fn check(&self) -> Output {
        Command::new(env!("CARGO"))
            .args(["check", "--offline", "--quiet"])
            .env("CARGO_TARGET_DIR", self.0.join("target"))
            .current_dir(&self.0)
            .output()
            .unwrap()
    }

    fn passes(&self) {
        let output = self.check();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn rejects(&self, expected: &str) {
        let output = self.check();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "audit unexpectedly passed");
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error}"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn audit_handles_json_and_reruns_for_source_edits() {
    let fixture = Fixture::new();
    let source = "fn example() { tr(\"Settings\"); tr(\"Say \\\"hello\\\"\"); }\n";
    fixture.write("src/ui/example.rs", source);
    fixture.write(
        "locales/en.json",
        r#"{"Settings":"Settings","Say \"hello\"":"Say \"hello\""}"#,
    );
    fixture.passes();

    // Valid JSON whitespace must not affect key decoding.
    fixture.write(
        "locales/en.json",
        "{\n  \"Settings\" : \"Settings\",\n  \"Say \\\"hello\\\"\" : \"Say \\\"hello\\\"\"\n}\n",
    );
    fixture.passes();

    fixture.write(
        "src/ui/example.rs",
        "fn example() { tr(\"Missing UI key\"); }\n",
    );
    fixture.rejects("missing source key");
    fixture.write("src/ui/example.rs", source);
    fixture.passes();

    fixture.write(
        "src/app/notification.rs",
        "fn example() { tr(\"Missing app key\"); }\n",
    );
    fixture.rejects("missing source key");
    fs::remove_file(fixture.0.join("src/app/notification.rs")).unwrap();
    fixture.passes();

    fixture.write(
        "src/util.rs",
        "fn example() { tr(\"Missing helper key\"); }\n",
    );
    fixture.rejects("missing source key");
    fixture.write("src/util.rs", "");
    fixture.passes();

    // Partial catalogs remain valid, including minified JSON.
    fixture.write(
        "locales/fr.json",
        r#"{"__language_name":"Français","Settings":""}"#,
    );
    fixture.passes();
    fixture.write("locales/fr.json", r#"{"Unknown":"Texte"}"#);
    fixture.rejects("unknown translation key");
    fixture.write("locales/fr.json", r#"{"Settings":"Paramètres {oops"}"#);
    fixture.rejects("invalid placeholders");
    fixture.write("locales/fr.json", "{");
    fixture.rejects("invalid catalog");
}
