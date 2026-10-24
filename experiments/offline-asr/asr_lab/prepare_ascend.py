"""Export a pinned ASCEND selection from already downloaded local Parquet."""

import argparse
from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import wave

from .corpus import select_rows, validate_disjoint
from .manifest import file_sha256, load_manifest
from .scoring import mixed_tokens


REVISION = "737e9800ae31be9932ba8464c80366559bd28424"
SEED = "norma-offline-asr-20261024-v1"
SHARDS = {
    "validation": (106532266, "3bdec53d2abfd3dd4f0d86a6df4e27e60f20660edc9b66055ae0ef8ec05cf7e2"),
    "test": (105756434, "a4c81d2b5ed6124f052089a695972808c16e0ce0c365ec9773c5d1a8fcf043a7"),
}


def prepare(source: Path, destination: Path):
    import pyarrow.parquet as parquet
    verified = {}
    for split, (size, digest) in SHARDS.items():
        path = source / f"{split}.parquet"
        if not path.exists():
            path = source / f"{split}.parquet.part"
        if path.stat().st_size != size or file_sha256(path) != digest:
            raise ValueError(f"{split}: pinned shard size/SHA-256 mismatch")
        verified[split] = path
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "audio").mkdir()
    manifest_rows, dev_texts, audit = {}, set(), {}
    for split in ("validation", "test"):
        rows = parquet.read_table(verified[split]).to_pylist()
        audit[split] = {"rows": len(rows), "languages": dict(Counter(r["language"] for r in rows)),
                        "speakers": dict(Counter(r["original_speaker_id"] for r in rows))}
        eligible = [r for r in rows if tuple(mixed_tokens(r["transcription"])) not in dev_texts]
        chosen = select_rows(eligible, per_speaker=20 if split == "validation" else 120,
                             language="mixed", seed=SEED)
        for language in ("zh", "en"):
            chosen += select_rows(eligible, per_speaker=8 if split == "validation" else 16,
                                  language=language, seed=SEED)
        exported = []
        for row in chosen:
            basename = f"{split}-{hashlib.sha256(str(row['id']).encode()).hexdigest()[:20]}.wav"
            audio = row["audio"]["bytes"]
            with wave.open(io.BytesIO(audio), "rb") as reader:
                if (reader.getnchannels(), reader.getsampwidth(), reader.getframerate(),
                        reader.getcomptype()) != (1, 2, 16000, "NONE"):
                    raise ValueError(f"unexpected source encoding: {row['id']}")
                duration = reader.getnframes() / 16000
            (destination / "audio" / basename).write_bytes(audio)
            exported.append({
                "id": f"ascend-{split}-{row['id']}", "audio": f"audio/{basename}",
                "sha256": hashlib.sha256(audio).hexdigest(), "reference": row["transcription"],
                "speaker": str(row["original_speaker_id"]), "session": str(row["session_id"]),
                "split": "dev" if split == "validation" else "test", "original_split": split,
                "language": row["language"], "duration_s": duration,
                "source": f"CAiRE/ASCEND@{REVISION}", "source_id": row["id"],
                "license": "CC-BY-SA-4.0", "speech_onset_s": None, "speech_offset_s": None,
                "boundary_evidence": "human segmented utterance; precise onset/offset not annotated",
            })
        manifest_rows[split] = exported
        if split == "validation":
            dev_texts = {tuple(mixed_tokens(r["reference"])) for r in exported}
    validate_disjoint(manifest_rows["validation"], manifest_rows["test"])
    for split, rows in manifest_rows.items():
        name = "dev" if split == "validation" else "test"
        for suffix, selected in (("", rows), ("-mixed", [r for r in rows if r["language"] == "mixed"])):
            manifest = destination / f"{name}{suffix}.jsonl"
            manifest.write_text("".join(json.dumps(row, ensure_ascii=False) + "\n" for row in selected))
            load_manifest(manifest)
    metadata = {"dataset": "CAiRE/ASCEND", "revision": REVISION, "seed": SEED,
                "license": "CC-BY-SA-4.0", "source_distribution": audit,
                "selected": {split: len(rows) for split, rows in manifest_rows.items()},
                "speaker_count": len({r["speaker"] for rows in manifest_rows.values() for r in rows}),
                "transformation": "metadata selection only; audio bytes unchanged",
                "attribution": "Lovenia et al., ASCEND, LREC 2022, https://aclanthology.org/2022.lrec-1.788/",
                "limitations": "5 speakers; no code-command/noise/user-microphone validation; no exact speech boundaries"}
    (destination / "dataset.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
    return metadata


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.source, args.output), ensure_ascii=False, indent=2))
