"""Validated, immutable PCM16 corpus manifests; no network or automatic repair."""

import hashlib
import json
from pathlib import Path
import wave


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as reader:
        for chunk in iter(lambda: reader.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_manifest(path: Path) -> list[dict]:
    path = Path(path).resolve()
    rows, identifiers = [], set()
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        required = ("id", "audio", "sha256", "reference", "speaker", "split",
                    "language", "source", "license", "duration_s")
        if any(key not in row for key in required):
            raise ValueError(f"manifest line {line_number}: missing required metadata")
        if row["id"] in identifiers:
            raise ValueError(f"duplicate sample id: {row['id']}")
        identifiers.add(row["id"])
        audio = (path.parent / row["audio"]).resolve()
        if not audio.is_relative_to(path.parent):
            raise ValueError("audio must stay inside the corpus directory")
        if file_sha256(audio) != row["sha256"]:
            raise ValueError(f"SHA-256 mismatch: {row['id']}")
        with wave.open(str(audio), "rb") as reader:
            if (reader.getnchannels(), reader.getsampwidth(), reader.getframerate(),
                    reader.getcomptype()) != (1, 2, 16000, "NONE"):
                raise ValueError("expected 16 kHz mono PCM16 WAV")
            samples = reader.getnframes()
            duration = samples / 16000
            if abs(duration - row["duration_s"]) > 1 / 16000:
                raise ValueError(f"duration mismatch: {row['id']}")
            if len(reader.readframes(samples)) != samples * 2:
                raise ValueError(f"truncated WAV: {row['id']}")
        for key in ("speech_onset_s", "speech_offset_s"):
            if row.get(key) is not None and not 0 <= row[key] <= duration:
                raise ValueError(f"invalid {key}: {row['id']}")
        if (row.get("speech_onset_s") is not None and row.get("speech_offset_s") is not None
                and row["speech_onset_s"] > row["speech_offset_s"]):
            raise ValueError(f"speech boundaries reversed: {row['id']}")
        rows.append({**row, "audio_path": str(audio), "sample_count": samples})
    if not rows:
        raise ValueError("manifest contains no samples")
    return rows
