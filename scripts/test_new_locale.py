import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("new-locale.py")
SPEC = importlib.util.spec_from_file_location("new_locale", SCRIPT)
new_locale = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(new_locale)


class NewLocaleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.locales = Path(self.temp.name)
        (self.locales / "en.json").write_text(
            json.dumps(
                {
                    "__language_name": "English",
                    "Settings": "Settings",
                    "Download {count} items": "Download {count} items",
                }
            ),
            encoding="utf-8",
        )

    def tearDown(self):
        self.temp.cleanup()

    def test_creates_full_blank_template_with_display_name(self):
        path = new_locale.create_locale("fr", "Français", self.locales)
        catalog = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(catalog["__language_name"], "Français")
        self.assertEqual(catalog["Settings"], "")
        self.assertEqual(catalog["Download {count} items"], "")
        self.assertEqual(set(catalog), {"__language_name", "Settings", "Download {count} items"})

    def test_rejects_invalid_codes_and_english(self):
        for code in ("FR", "fr-CA", "f", "four"):
            with self.subTest(code=code), self.assertRaises(ValueError):
                new_locale.create_locale(code, "Français", self.locales)
        with self.assertRaises(ValueError):
            new_locale.create_locale("en", "English", self.locales)

    def test_refuses_to_overwrite_existing_locale(self):
        target = self.locales / "fr.json"
        target.write_text('{"keep":"this"}\n', encoding="utf-8")
        with self.assertRaises(FileExistsError):
            new_locale.create_locale("fr", "Français", self.locales)
        self.assertEqual(target.read_text(encoding="utf-8"), '{"keep":"this"}\n')

    def test_coverage_counts_nonempty_valid_entries_only(self):
        (self.locales / "fr.json").write_text(
            json.dumps(
                {
                    "__language_name": "Français",
                    "Settings": "Paramètres",
                    "Download {count} items": "",
                }
            ),
            encoding="utf-8",
        )
        self.assertEqual(new_locale.locale_coverage(self.locales), [("fr", "Français", 1, 2, 50.0)])

    def test_coverage_rejects_bad_placeholders_and_unknown_keys(self):
        path = self.locales / "fr.json"
        path.write_text(json.dumps({"Settings": "Paramètres", "Download {count} items": "Télécharger"}), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "invalid placeholders"):
            new_locale.locale_coverage(self.locales)
        path.write_text(json.dumps({"Settings": "Paramètres", "Not a source key": "Valeur"}), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unknown keys"):
            new_locale.locale_coverage(self.locales)


if __name__ == "__main__":
    unittest.main()
