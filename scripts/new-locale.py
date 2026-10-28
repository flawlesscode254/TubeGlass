#!/usr/bin/env python3
"""Create a partial translation catalog or report locale coverage."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


LANGUAGE_CODE = re.compile(r"[a-z]{2,3}\Z")
PLACEHOLDER_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCALES = ROOT / "locales"


def load_catalog(path: Path) -> dict[str, str]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"could not read {path}: {error}") from error
    if not isinstance(data, dict) or not all(
        isinstance(key, str) and isinstance(value, str) for key, value in data.items()
    ):
        raise ValueError(f"{path} must be a JSON object of string values")
    return data


def placeholder_names(text: str) -> list[str]:
    names = []
    index = 0
    while index < len(text):
        char = text[index]
        if char == "{":
            if index + 1 < len(text) and text[index + 1] == "{":
                index += 2
                continue

            end = text.find("}", index + 1)
            if end == -1:
                raise ValueError("malformed placeholder braces")
            name = text[index + 1 : end]
            if "{" in name or not PLACEHOLDER_NAME.fullmatch(name):
                raise ValueError("malformed placeholder braces")
            names.append(name)
            index = end + 1
            continue

        if char == "}":
            if index + 1 < len(text) and text[index + 1] == "}":
                index += 2
                continue
            raise ValueError("malformed placeholder braces")

        index += 1

    return sorted(names)


def create_locale(code: str, display_name: str, locales_dir: Path = DEFAULT_LOCALES) -> Path:
    if not LANGUAGE_CODE.fullmatch(code):
        raise ValueError("language code must be a lowercase two- or three-letter base code, such as 'fr'")
    if code == "en":
        raise ValueError("English is the source catalog and cannot be scaffolded")
    if not display_name.strip():
        raise ValueError("display name cannot be empty")

    english = load_catalog(locales_dir / "en.json")
    output = {"__language_name": display_name.strip()}
    output.update({key: "" for key in english if key != "__language_name"})
    destination = locales_dir / f"{code}.json"
    # Exclusive creation prevents accidentally replacing an existing translation.
    with destination.open("x", encoding="utf-8", newline="\n") as file:
        json.dump(output, file, ensure_ascii=False, indent=2)
        file.write("\n")
    return destination


def locale_coverage(locales_dir: Path = DEFAULT_LOCALES) -> list[tuple[str, str, int, int, float]]:
    english = load_catalog(locales_dir / "en.json")
    source_keys = {key for key in english if key != "__language_name"}
    if not source_keys or any(not english[key].strip() for key in source_keys):
        raise ValueError("locales/en.json must contain a nonempty value for every source key")

    source_placeholder_names = {}
    for key in source_keys:
        try:
            key_names = placeholder_names(key)
            value_names = placeholder_names(english[key])
        except ValueError as error:
            raise ValueError(f"locales/en.json has invalid placeholders for {key!r}") from error
        if key_names != value_names:
            raise ValueError(f"locales/en.json has invalid placeholders for {key!r}")
        source_placeholder_names[key] = key_names

    report = []
    for path in sorted(locales_dir.glob("*.json")):
        if path.stem == "en":
            continue
        catalog = load_catalog(path)
        unknown = set(catalog) - source_keys - {"__language_name"}
        if unknown:
            raise ValueError(f"{path.name} has unknown keys: {', '.join(sorted(unknown))}")
        translated = 0
        for key, value in catalog.items():
            if key == "__language_name" or not value.strip():
                continue
            try:
                translation_names = placeholder_names(value)
            except ValueError as error:
                raise ValueError(f"{path.name} has invalid placeholders for {key!r}") from error
            if source_placeholder_names[key] != translation_names:
                raise ValueError(f"{path.name} has invalid placeholders for {key!r}")
            translated += 1
        name = catalog.get("__language_name", "").strip() or path.stem
        percentage = translated * 100.0 / len(source_keys)
        report.append((path.stem, name, translated, len(source_keys), percentage))
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("code", nargs="?", help="lowercase base language code, such as fr")
    parser.add_argument("display_name", nargs="?", help="language name shown in Settings")
    parser.add_argument("--coverage", action="store_true", help="report translation coverage for all catalogs")
    args = parser.parse_args()

    try:
        if args.coverage:
            for code, name, translated, total, percentage in locale_coverage():
                print(f"{code} ({name}): {translated}/{total} translated ({percentage:.1f}%)")
            return 0
        if not args.code or not args.display_name:
            parser.error("provide CODE and DISPLAY_NAME, or use --coverage")
        destination = create_locale(args.code, args.display_name)
        print(f"Created {destination.relative_to(ROOT)} with blank translations that fall back to English.")
        return 0
    except (FileExistsError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
