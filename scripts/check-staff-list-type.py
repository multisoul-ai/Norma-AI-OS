#!/usr/bin/env python3
"""Staff task list and archive bar typography.

docs/design.md §4.3: one type role per line, and a hover control must not sit
on top of the row text. Scope is the task list, the task row, and
staff_archive_bar only.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIST_TYPE = "crates/app/src/app/staff/list_type.rs"
TASK_ROW = "crates/app/src/app/staff/task_row.rs"
TASK_LIST = "crates/app/src/app/staff/task_list.rs"
ARCHIVE = "crates/app/src/app/staff/archive.rs"

# docs/design.md §4.2. 12 and 16 are intentionally absent.
ROLES = {
    "BODY": (14.0, 1.48, "NORMAL"),
    "CALLOUT": (13.0, 1.42, "MEDIUM"),
    "CAPTION": (11.0, 1.36, "MEDIUM"),
}
ALLOWED_SIZES = {10, 11, 13, 14, 15, 20, 28, 40, 56}
SPACES = {"SPACE_1": 4.0, "SPACE_2": 8.0, "SPACE_3": 12.0}
NUMERIC_TEXT_SIZE = re.compile(r"text_size\(\s*px\(\s*\d")
NEXT_FN = re.compile(r"\n[ \t]*(?:pub\([^)]*\)\s+)?fn ")


def function_source(source: str, name: str) -> str | None:
    start = source.find(f"fn {name}")
    if start < 0:
        return None
    nxt = NEXT_FN.search(source, start + 1)
    end = nxt.start() if nxt else len(source)
    return source[start:end]


def check_roles(path: str, source: str) -> list[str]:
    errors: list[str] = []
    for name, (size, line, weight) in ROLES.items():
        match = re.search(
            rf"const\s+{name}:\s*Role\s*=\s*Role\s*\{{(?P<body>.*?)\}};",
            source,
            re.S,
        )
        if match is None:
            errors.append(f"{path}: missing {name} role from docs/design.md §4.2")
            continue
        body = match.group("body")
        found = (
            _float(body, r"size:\s*([0-9.]+)"),
            _float(body, r"line:\s*([0-9.]+)"),
            _word(body, r"FontWeight::([A-Z_]+)"),
        )
        if found != (size, line, weight):
            errors.append(
                f"{path}: {name} is {found}, expected size {size}, line {line}, {weight}"
            )
    for match in re.finditer(r"\bsize:\s*([0-9.]+)", source):
        if float(match.group(1)) not in ALLOWED_SIZES:
            errors.append(
                f"{path}:{_line(source, match.start())}: font size {match.group(1)} is outside §4.2"
            )
    hit = re.search(r"const\s+HIT:\s*f32\s*=\s*([0-9.]+)", source)
    if hit is None or float(hit.group(1)) < 36:
        errors.append(f"{path}: HIT must be at least 36 (§5.3)")
    for name, expected in SPACES.items():
        match = re.search(rf"const\s+{name}:\s*f32\s*=\s*([0-9.]+)", source)
        if match is None or float(match.group(1)) != expected:
            errors.append(f"{path}: {name} must be {expected} (§5.1)")
    return errors


def check_numeric_text_size(path: str, source: str) -> list[str]:
    return [
        f"{path}:{_line(source, match.start())}: raw text_size(px(N)) is outside the list type roles"
        for match in NUMERIC_TEXT_SIZE.finditer(source)
    ]


def check_task_row(path: str, source: str) -> list[str]:
    body = function_source(source, "staff_task_row")
    if body is None:
        return [f"{path}: missing staff_task_row"]
    errors = check_numeric_text_size(path, source)
    if ".absolute()" in body:
        errors.append(
            f"{path}: staff_task_row positions with .absolute(), which covers the meta line"
        )
    if ".tooltip(" in body:
        errors.append(
            f"{path}: staff_task_row uses a cursor tooltip, which escapes the sidebar"
        )
    if "staff.archive_named" in body:
        errors.append(f"{path}: staff_task_row must not swap the title for an archive label on hover")
    return errors


def check_archive_bar(path: str, source: str) -> list[str]:
    body = function_source(source, "staff_archive_bar")
    if body is None:
        return [f"{path}: missing staff_archive_bar"]
    errors = check_numeric_text_size(path, body)
    if body.count("CALLOUT") < 3:
        errors.append(
            f"{path}: staff_archive_bar must use the callout role for the back control, title, and count"
        )
    if "items_center()" not in body and "items_baseline()" not in body:
        errors.append(f"{path}: staff_archive_bar does not align its row")
    return errors


def check_text(files: dict[str, str]) -> list[str]:
    errors = check_roles(LIST_TYPE, files[LIST_TYPE])
    errors.extend(check_task_row(TASK_ROW, files[TASK_ROW]))
    errors.extend(check_numeric_text_size(TASK_LIST, files[TASK_LIST]))
    errors.extend(check_archive_bar(ARCHIVE, files[ARCHIVE]))
    return errors


def _float(body: str, pattern: str) -> float | None:
    match = re.search(pattern, body)
    return float(match.group(1)) if match else None


def _word(body: str, pattern: str) -> str | None:
    match = re.search(pattern, body)
    return match.group(1) if match else None


def _line(source: str, index: int) -> int:
    return source.count("\n", 0, index) + 1


def self_test() -> None:
    good_roles = """
pub(super) const BODY: Role = Role { size: 14., line: 1.48, weight: FontWeight::NORMAL };
pub(super) const CALLOUT: Role = Role { size: 13., line: 1.42, weight: FontWeight::MEDIUM };
pub(super) const CAPTION: Role = Role { size: 11., line: 1.36, weight: FontWeight::MEDIUM };
pub(super) const HIT: f32 = 36.;
pub(super) const SPACE_1: f32 = 4.;
pub(super) const SPACE_2: f32 = 8.;
pub(super) const SPACE_3: f32 = 12.;
"""
    bad_roles = good_roles.replace("size: 11.", "size: 12.", 1)
    assert check_roles(LIST_TYPE, good_roles) == []
    assert any("outside §4.2" in error for error in check_roles(LIST_TYPE, bad_roles))
    assert any("HIT" in error for error in check_roles(LIST_TYPE, good_roles.replace("36.", "32.", 1)))

    row = """
fn avatar() { div().absolute() }
fn staff_task_row() {
    text(BODY)
}
"""
    assert check_task_row(TASK_ROW, row) == []
    covered = row.replace("text(BODY)", "div().absolute().text_size(px(12.))")
    covered_errors = check_task_row(TASK_ROW, covered)
    assert any(".absolute()" in error for error in covered_errors)
    assert any("text_size" in error for error in covered_errors)
    assert any("tooltip" in error for error in check_task_row(TASK_ROW, row + "\n.tooltip(tip)\n"))

    bar = """
fn staff_archive_bar() {
    items_center()
    text(CALLOUT)
    text(CALLOUT)
    text(CALLOUT)
}
fn staff_archive_link() { text_size(px(12.)) }
"""
    files = {
        LIST_TYPE: good_roles,
        TASK_ROW: row,
        TASK_LIST: "text(CALLOUT)",
        ARCHIVE: bar,
    }
    assert check_text(files) == []
    assert any(
        "text_size" in error
        for error in check_text({**files, TASK_LIST: "text_size(px(13.))"})
    )
    assert any(
        "callout" in error
        for error in check_archive_bar(ARCHIVE, bar.replace("text(CALLOUT)\n    text(CALLOUT)\n    text(CALLOUT)", "text(BODY)"))
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("staff list type self-test passed")
        return 0
    files = {
        path: (ROOT / path).read_text()
        for path in (LIST_TYPE, TASK_ROW, TASK_LIST, ARCHIVE)
    }
    errors = check_text(files)
    if errors:
        print("ERROR: staff list type violations found.", file=sys.stderr)
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("staff list type check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
