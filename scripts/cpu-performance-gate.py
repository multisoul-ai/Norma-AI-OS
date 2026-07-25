#!/usr/bin/env python3
"""Summarize CPU samples and enforce the P0/P1 same-machine reduction gate."""

from __future__ import annotations

import argparse
import csv
import json
import math
import statistics
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1


def read_cpu_samples(path: Path) -> list[float]:
    samples: list[float] = []
    with path.open(newline="", encoding="utf-8") as source:
        reader = csv.DictReader(source)
        if reader.fieldnames is None or "cpu" not in reader.fieldnames:
            raise ValueError(f"{path} must contain a cpu column")
        for row_number, row in enumerate(reader, start=2):
            raw = (row.get("cpu") or "").strip()
            if not raw:
                continue
            value = float(raw)
            if not math.isfinite(value) or value < 0:
                raise ValueError(
                    f"{path}:{row_number} cpu must be a finite non-negative number"
                )
            samples.append(value)
    if not samples:
        raise ValueError(f"{path} does not contain any CPU samples")
    return samples


def cpu_statistics(samples: list[float]) -> dict[str, float | int]:
    ordered = sorted(samples)
    p95_index = max(0, math.ceil(len(ordered) * 0.95) - 1)
    return {
        "sample_count": len(ordered),
        "median": round(statistics.median(ordered), 4),
        "p95": round(ordered[p95_index], 4),
        "max": round(ordered[-1], 4),
    }


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def summarize(args: argparse.Namespace) -> int:
    app_samples = read_cpu_samples(args.app_csv)
    window_server_samples = read_cpu_samples(args.window_server_csv)
    summary = {
        "schema_version": SCHEMA_VERSION,
        "stage": args.stage,
        "scenario": args.scenario,
        "commit": args.commit,
        "host_id": args.host_id,
        "duration_seconds": args.duration_seconds,
        "node_count": args.node_count,
        "active_agent_count": args.active_agent_count,
        "active_terminal_count": args.active_terminal_count,
        "app": cpu_statistics(app_samples),
        "window_server": cpu_statistics(window_server_samples),
    }
    write_json(args.output, summary)
    print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
    return 0


def load_summary(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise ValueError(
            f"{path} schema_version must be {SCHEMA_VERSION}, "
            f"got {payload.get('schema_version')!r}"
        )
    return payload


def reduction_percent(baseline: float, candidate: float) -> float:
    if baseline <= 0:
        raise ValueError("baseline app median must be greater than zero")
    return round((baseline - candidate) / baseline * 100.0, 4)


def compare(args: argparse.Namespace) -> int:
    baseline = load_summary(args.baseline)
    candidate = load_summary(args.candidate)
    compatibility_fields = (
        "scenario",
        "host_id",
        "duration_seconds",
        "node_count",
        "active_agent_count",
        "active_terminal_count",
    )
    checks: list[dict[str, Any]] = []
    for field in compatibility_fields:
        expected = baseline.get(field)
        actual = candidate.get(field)
        checks.append(
            {
                "name": f"same_{field}",
                "passed": actual == expected,
                "expected": expected,
                "actual": actual,
            }
        )

    baseline_median = float(baseline["app"]["median"])
    candidate_median = float(candidate["app"]["median"])
    reduction = reduction_percent(baseline_median, candidate_median)
    checks.append(
        {
            "name": "app_median_cpu_reduction",
            "passed": reduction >= args.min_reduction_percent,
            "expected_min_percent": args.min_reduction_percent,
            "actual_percent": reduction,
        }
    )
    if args.candidate_max_median is not None:
        checks.append(
            {
                "name": "candidate_absolute_median",
                "passed": candidate_median <= args.candidate_max_median,
                "expected_max": args.candidate_max_median,
                "actual": candidate_median,
            }
        )

    report = {
        "schema_version": SCHEMA_VERSION,
        "baseline": str(args.baseline),
        "candidate": str(args.candidate),
        "baseline_app_median": baseline_median,
        "candidate_app_median": candidate_median,
        "reduction_percent": reduction,
        "required_reduction_percent": args.min_reduction_percent,
        "checks": checks,
        "passed": all(check["passed"] for check in checks),
    }
    write_json(args.output, report)
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0 if report["passed"] else 1


def repeatability(args: argparse.Namespace) -> int:
    reference = load_summary(args.reference)
    rerun = load_summary(args.rerun)
    compatibility_fields = (
        "scenario",
        "host_id",
        "duration_seconds",
        "node_count",
        "active_agent_count",
        "active_terminal_count",
    )
    checks: list[dict[str, Any]] = []
    for field in compatibility_fields:
        expected = reference.get(field)
        actual = rerun.get(field)
        checks.append(
            {
                "name": f"same_{field}",
                "passed": actual == expected,
                "expected": expected,
                "actual": actual,
            }
        )

    reference_median = float(reference["app"]["median"])
    rerun_median = float(rerun["app"]["median"])
    if reference_median <= 0:
        raise ValueError("reference app median must be greater than zero")
    median_delta = round(
        abs(rerun_median - reference_median) / reference_median * 100.0, 4
    )
    checks.append(
        {
            "name": "app_median_repeatability",
            "passed": median_delta <= args.max_median_delta_percent,
            "expected_max_percent": args.max_median_delta_percent,
            "actual_percent": median_delta,
        }
    )
    report = {
        "schema_version": SCHEMA_VERSION,
        "reference": str(args.reference),
        "rerun": str(args.rerun),
        "reference_app_median": reference_median,
        "rerun_app_median": rerun_median,
        "median_delta_percent": median_delta,
        "allowed_median_delta_percent": args.max_median_delta_percent,
        "checks": checks,
        "passed": all(check["passed"] for check in checks),
    }
    write_json(args.output, report)
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0 if report["passed"] else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Summarize Norma OS CPU samples and enforce the P0/P1 gate."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    summarize_parser = subparsers.add_parser("summarize")
    summarize_parser.add_argument("--app-csv", type=Path, required=True)
    summarize_parser.add_argument("--window-server-csv", type=Path, required=True)
    summarize_parser.add_argument("--output", type=Path, required=True)
    summarize_parser.add_argument("--stage", choices=("baseline", "candidate"), required=True)
    summarize_parser.add_argument("--scenario", required=True)
    summarize_parser.add_argument("--commit", required=True)
    summarize_parser.add_argument("--host-id", required=True)
    summarize_parser.add_argument("--duration-seconds", type=int, required=True)
    summarize_parser.add_argument("--node-count", type=int, required=True)
    summarize_parser.add_argument("--active-agent-count", type=int, required=True)
    summarize_parser.add_argument("--active-terminal-count", type=int, default=0)
    summarize_parser.set_defaults(handler=summarize)

    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--baseline", type=Path, required=True)
    compare_parser.add_argument("--candidate", type=Path, required=True)
    compare_parser.add_argument("--output", type=Path, required=True)
    compare_parser.add_argument("--min-reduction-percent", type=float, default=50.0)
    compare_parser.add_argument("--candidate-max-median", type=float)
    compare_parser.set_defaults(handler=compare)

    repeatability_parser = subparsers.add_parser("repeatability")
    repeatability_parser.add_argument("--reference", type=Path, required=True)
    repeatability_parser.add_argument("--rerun", type=Path, required=True)
    repeatability_parser.add_argument("--output", type=Path, required=True)
    repeatability_parser.add_argument("--max-median-delta-percent", type=float, default=20.0)
    repeatability_parser.set_defaults(handler=repeatability)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        return args.handler(args)
    except (KeyError, TypeError, ValueError, OSError, json.JSONDecodeError) as error:
        parser.error(str(error))
    return 2


if __name__ == "__main__":
    sys.exit(main())
