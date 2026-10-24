"""Deterministic speaker-balanced selection using metadata alone."""

from collections import defaultdict
import hashlib

from .scoring import is_cjk, mixed_tokens


def select_rows(rows, *, per_speaker: int, language: str, seed: str):
    if per_speaker <= 0:
        raise ValueError("per_speaker must be positive")
    groups = defaultdict(list)
    for row in rows:
        text = row["transcription"]
        if (row["language"] != language or not 1 <= row["duration"] <= 15
                or "[unk]" in text.casefold()):
            continue
        if language == "mixed" and not (
                any(is_cjk(c) for c in text) and any(c.isascii() and c.isalpha() for c in text)):
            continue
        groups[row["original_speaker_id"]].append(row)
    if not groups:
        raise ValueError(f"no eligible {language} rows")
    selected = []
    for speaker, group in sorted(groups.items()):
        if len(group) < per_speaker:
            raise ValueError(f"speaker {speaker}: only {len(group)} eligible {language} rows")
        ranked = sorted(group, key=lambda row: (
            hashlib.sha256(f"{seed}:{row['id']}".encode()).hexdigest(), str(row["id"])))
        selected.extend(ranked[:per_speaker])
    return selected


def validate_disjoint(development, test):
    if {r["speaker"] for r in development} & {r["speaker"] for r in test}:
        raise ValueError("speaker leakage between development and test")
    if {r["sha256"] for r in development} & {r["sha256"] for r in test}:
        raise ValueError("audio leakage between development and test")
    dev_text = {tuple(mixed_tokens(r["reference"])) for r in development if r.get("reference")}
    test_text = {tuple(mixed_tokens(r["reference"])) for r in test if r.get("reference")}
    if dev_text & test_text:
        raise ValueError("reference text leakage between development and test")
