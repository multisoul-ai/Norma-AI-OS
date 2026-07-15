#!/usr/bin/env python3
"""Check small cross-file domain contracts that Rust compilation cannot enforce."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    return Path(
        subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            text=True,
        ).strip()
    )


def asr_catalog_model_ids(source: str) -> set[str]:
    return set(
        re.findall(
            r'pub const [A-Z0-9_]+_MODEL_ID: &str = "([^"]+)";',
            source,
        )
    )


def check_asr_release_gate(root: Path) -> list[str]:
    catalog = root / "crates/asr/src/catalog.rs"
    release_gate = root / "docs/quality/asr-model-release-gate.md"
    model_ids = asr_catalog_model_ids(catalog.read_text())
    documented = release_gate.read_text()
    if not model_ids:
        return [f"{catalog}: could not find any built-in ASR model IDs"]
    return [
        f"{release_gate}: catalog model id `{model_id}` is missing from the ASR release gate"
        for model_id in sorted(model_ids)
        if f"`{model_id}`" not in documented
    ]


def rust_enum_variants(source: str, enum_name: str) -> set[str]:
    match = re.search(
        rf"pub enum {re.escape(enum_name)}\s*\{{(?P<body>.*?)\n\}}",
        source,
        re.DOTALL,
    )
    if match is None:
        return set()
    return set(re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*,\s*$", match.group("body"), re.MULTILINE))


def create_agent_schema_profiles(source: str) -> set[str]:
    create_agent = source.split('"create_agent"', 1)[1].split('"activate_agent"', 1)[0]
    profile = re.search(
        r'"profile"\s*:\s*\{[^{}]*"enum"\s*:\s*\[(?P<values>[^]]+)\]',
        create_agent,
    )
    if profile is None:
        return set()
    return set(re.findall(r'"([a-z0-9_]+)"', profile.group("values")))


def agent_profile_parser_mappings(source: str) -> dict[str, str]:
    return dict(
        re.findall(
            r'Some\("([a-z0-9_]+)"\)\s*=>\s*Ok\(AgentProfile::([A-Za-z0-9_]+)\)',
            source,
        )
    )


def storage_profile_mappings(source: str) -> tuple[dict[str, str], dict[str, str]]:
    encoded = {
        serialized: variant
        for variant, serialized in re.findall(
            r'AgentProfile::([A-Za-z0-9_]+)\s*=>\s*"([a-z0-9_]+)"',
            source,
        )
    }
    decoded = dict(
        re.findall(
            r'"([a-z0-9_]+)"\s*=>\s*Ok\(AgentProfile::([A-Za-z0-9_]+)\)',
            source,
        )
    )
    return encoded, decoded


def check_agent_profile_contract(root: Path) -> list[str]:
    protocol = root / "crates/norma/src/protocol.rs"
    canvas = root / "crates/canvas/src/node.rs"
    tools = root / "crates/agent/src/tools.rs"
    storage = root / "crates/storage/src/agent_definition_store.rs"

    protocol_variants = rust_enum_variants(protocol.read_text(), "AgentProfile")
    canvas_variants = rust_enum_variants(canvas.read_text(), "AgentProfile")
    expected = {variant.lower(): variant for variant in protocol_variants}
    schema_profiles = create_agent_schema_profiles(tools.read_text())
    parser_profiles = agent_profile_parser_mappings(tools.read_text())
    encoded_profiles, decoded_profiles = storage_profile_mappings(storage.read_text())

    failures: list[str] = []
    if not protocol_variants:
        failures.append(f"{protocol}: could not find public AgentProfile variants")
    if canvas_variants != protocol_variants:
        failures.append(
            f"{canvas}: Canvas AgentProfile variants {sorted(canvas_variants)} do not match "
            f"wire variants {sorted(protocol_variants)}"
        )
    if schema_profiles != set(expected):
        failures.append(
            f"{tools}: create_agent schema profiles {sorted(schema_profiles)} do not match "
            f"AgentProfile values {sorted(expected)}"
        )
    if parser_profiles != expected:
        failures.append(
            f"{tools}: profile parser mappings {parser_profiles} do not match {expected}"
        )
    if encoded_profiles != expected:
        failures.append(
            f"{storage}: profile encoder mappings {encoded_profiles} do not match {expected}"
        )
    if decoded_profiles != expected:
        failures.append(
            f"{storage}: profile decoder mappings {decoded_profiles} do not match {expected}"
        )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check cross-file domain contract consistency."
    )
    parser.add_argument(
        "--root",
        type=Path,
        help="Repository root override for focused checker tests.",
    )
    args = parser.parse_args()

    root = args.root.resolve() if args.root else repo_root()
    failures = check_asr_release_gate(root)
    failures.extend(check_agent_profile_contract(root))
    if failures:
        print("ERROR: domain contract violations found.", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
