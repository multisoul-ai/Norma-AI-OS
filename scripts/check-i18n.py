#!/usr/bin/env python3
"""Validate macOS product catalogs, referenced keys, and direct UI literals."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLACEHOLDER = re.compile(r"%\{([A-Za-z_][A-Za-z_0-9]*)\}")
REFERENCE = re.compile(
    r'(?:i18n::(?:text|text_for|format|message|input_placeholder)|'
    r'LocalizedText::new)\s*\(\s*(?:Locale::\w+\s*,\s*)?"([\w.]+)"'
)
UI_LITERAL = re.compile(
    r'\.(?:child|label|placeholder|tooltip|title)\(\s*(?:format!\(\s*)?'
    r'"((?:\\.|[^"\\])*)"'
)
CONDITIONAL_UI = re.compile(r'\.(?:child|label|placeholder|tooltip|title)\(\s*(?:match|if)\b')
BRANCH_LITERAL = re.compile(r'(?:=>|\{)\s*"((?:\\.|[^"\\])*)"')
# Explicit exceptions are brands, protocols, example URLs, file formats, or
# glyphs. Product sentences and statuses must never be added here.
CACHED_LITERAL = re.compile(
    r'(?:workspace_state|desktop_workspace|editor)\.error\s*=\s*Some\(\s*"((?:\\.|[^"\\])*)"'
)
UNCHANGED = {
    "Norma", "Norma OS", "Ego Lite", "N", "i", "x", "R", "HTTPS", "SSH", "GIF",
    "PNG / JPG / GIF / MP4 / MOV",
    "https://github.com/owner/repository.git",
    "https://push.example/v1/notifications",
}


def drawer_literals(path, source):
    """Catch indirect Chinese copy in the drawer's presentation helpers too."""
    domain = "crates/workspace/src/content/" in path.as_posix()
    if not domain:
        if "workspace/desktop/" not in path.as_posix():
            return []
        if "drawers" not in path.parts and not path.stem.startswith(("ideas", "idea_", "overview_", "status")):
            return []
    # Test fixtures and comments may contain Chinese prose. Keep offsets for diagnostics.
    source = re.split(r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{", source)[0]
    tokens = re.finditer(r'//[^\n]*|/\*[\s\S]*?\*/|"((?:\\.|[^"\\])*)"', source)
    return [(token[1], token.start()) for token in tokens
            if token[1] is not None and re.search(r"[\u4e00-\u9fff]", token[1])]


def conditional_literals(source):
    # Bound each sink at its closing parenthesis, skipping string contents.
    # Only bare branch results count; keys inside translation calls do not.
    for start in CONDITIONAL_UI.finditer(source):
        depth = 1
        for token in re.finditer(r'"(?:\\.|[^"\\])*"|[()]', source[start.end():]):
            if token[0] == "(":
                depth += 1
            elif token[0] == ")":
                depth -= 1
                if depth == 0:
                    body = source[start.end():start.end() + token.start()]
                    for match in BRANCH_LITERAL.finditer(body):
                        yield match[1], start.end() + match.start()
                    break


def check_catalogs(directory):
    errors, catalogs = [], {}
    for locale in ("en", "zh-CN"):
        catalog = catalogs[locale] = {}
        for path in sorted(directory.glob(f"*.{locale}.json")):
            def collect(pairs):
                result = {}
                for key, value in pairs:
                    if key in result:
                        errors.append(f"{path}: duplicate key {key}")
                    result[key] = value
                return result
            data = json.loads(path.read_text(), object_pairs_hook=collect)
            for key, value in data.items():
                if key in catalog and catalog[key] != value:
                    errors.append(f"{path}: conflicting key {key}")
                if not isinstance(value, str) or not value.strip():
                    errors.append(f"{path}: empty or non-string value for {key}")
                catalog[key] = value
    en, zh = catalogs["en"], catalogs["zh-CN"]
    for key in sorted(en.keys() ^ zh.keys()):
        errors.append(f"unpaired bilingual key: {key}")
    for key in sorted(en.keys() & zh.keys()):
        if set(PLACEHOLDER.findall(en[key])) != set(PLACEHOLDER.findall(zh[key])):
            errors.append(f"placeholder mismatch: {key}")
    return errors, set(en)


def check_sources(root, keys):
    errors = []
    for crate in ("app", "ui", "workspace"):
        for path in sorted((root / "crates" / crate / "src").rglob("*.rs")):
            if "e2e" in path.parts or path.name == "e2e.rs":
                continue
            source = path.read_text()
            for match in REFERENCE.finditer(source):
                if match[1] not in keys:
                    errors.append(f"{path}: unknown translation key {match[1]}")
            literals = [(match[1], match.start()) for pattern in (UI_LITERAL, CACHED_LITERAL)
                        for match in pattern.finditer(source)]
            literals += list(conditional_literals(source))
            literals += drawer_literals(path, source)
            # This shared helper returns visible card badges to the UI sink.
            if path.name == "node_label.rs" and path.parent.name == "canvas":
                literals += [(match[1], match.start()) for match in BRANCH_LITERAL.finditer(source)]
            reported = set()
            for value, position in literals:
                literal = re.sub(r"\{[^{}]*\}", "", value).replace(" MB", "")
                if re.search(r"[A-Za-z\u4e00-\u9fff]", literal) and value not in UNCHANGED:
                    line = source.count("\n", 0, position) + 1
                    if (line, value) in reported:
                        continue
                    reported.add((line, value))
                    errors.append(f"{path}:{line}: hardcoded UI copy {value!r}")
    return errors


def main():
    errors, keys = check_catalogs(ROOT / "crates/ui/locales")
    errors += check_sources(ROOT, keys)
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print(f"i18n: {len(keys)} bilingual keys; placeholders, references and UI literals verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
