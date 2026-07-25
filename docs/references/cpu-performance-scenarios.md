# CPU Performance Scenarios

This guide defines the reproducible evidence used by the Norma OS P0/P1 CPU gate.
The runner is [`scripts/run-cpu-performance-l3.sh`](../../scripts/run-cpu-performance-l3.sh).

## Release gates

- Baseline repeatability: two consecutive runs of the same scene must have app median CPU
  within 20%.
- Candidate reduction: candidate app median CPU must be at least 50% below the compatible
  same-machine baseline.
- Absolute reference targets:
  - `idle-seven-cards`: app median CPU below 5%.
  - `background-seven-cards`: app median CPU below 2%.
  - `two-active-terminals` and the manual `two-active-codex` scene: app median CPU below 20%.
- A performance pass never replaces Runtime/app/e2e tests or the real-app interaction checklist.

The comparison gate rejects different scenario names, host identities, durations, node counts,
active Agent counts, or active synthetic Terminal counts. This prevents a lighter scene from
being presented as an optimization.

## Frozen application artifacts

Build and copy the baseline `.app` before modifying production code:

```bash
SHERPA_ONNX_ARCHIVE_DIR=/path/to/sherpa/archive cargo build --release -p app
SHERPA_ONNX_ARCHIVE_DIR=/path/to/sherpa/archive cargo packager -c Packager.dev.toml
ditto "build/dev/Norma OS.app" \
  "artifacts/performance/cpu-p0-p1/baseline-<sha>/Norma OS.app"
```

The candidate must be built with the same toolchain and `Packager.dev.toml`.

## Automated scenes

Each automated scene uses a new packaged app instance, an isolated `HOME`, an isolated empty
benchmark workspace, seven visible cards, a four-second animation settle period, 60 one-second
CPU samples, and a five-second `/usr/bin/sample` stack capture. The opt-in bootstrap is enabled
only by `NORMA_CPU_BENCHMARK_SCENARIO` plus `NORMA_CPU_BENCHMARK_WORKSPACE`; normal launches do
not create benchmark cards.

| Scene | Construction | Purpose |
|---|---|---|
| `idle-seven-cards` | Seven idle Terminal cards | Idle animation/runtime wakeups |
| `background-seven-cards` | Same seven cards, then `Cmd+H` | Background repaint budget |
| `one-active-terminal` | Six idle cards plus one deterministic 50 Hz synthetic TUI | One active frame source |
| `two-active-terminals` | Five idle cards plus two deterministic 50 Hz synthetic TUIs | Stable two-source stress |

The synthetic TUI scenes provide deterministic CI-independent load. They do not replace the
manual two-Codex business scenario below.

Example:

```bash
bash scripts/run-cpu-performance-l3.sh baseline \
  --app "artifacts/performance/cpu-p0-p1/baseline-<sha>/Norma OS.app" \
  --scenario idle-seven-cards \
  --commit <baseline-sha>

bash scripts/run-cpu-performance-l3.sh candidate \
  --app "build/dev/Norma OS.app" \
  --scenario idle-seven-cards \
  --commit <candidate-sha>
```

The automatic launcher records existing PIDs and resolves the newly launched PID. The app emits
an info-level `cpu_benchmark_ready` record containing the actual scenario, node count, active
Agent count, and active Terminal count; sampling does not begin until these values match the
request. Its isolated `HOME` and workspace prevent the benchmark from sharing the user's app
database, logs, project state, or sessions. System Events is used only to hide the exact PID for
the background scene.

## Manual two-Codex business scene

Use the normal user environment and arrange exactly seven visible cards, with two Codex Agent
cards continuously updating. Enable `MULTISOUL_FRAME_DIAGNOSTICS=1`, then sample the known PID:

```bash
bash scripts/run-cpu-performance-l3.sh baseline \
  --pid <pid> \
  --no-prepare \
  --scenario two-active-codex \
  --node-count 7 \
  --active-agent-count 2 \
  --active-terminal-count 0 \
  --commit <baseline-sha>
```

Repeat with the candidate application. Use Computer Use for the functional checklist: Terminal
and Agent input, final output, cursor, working snake, breathing, Voice inset, theme, zoom,
selection, scroll, resize, and node close.

## Repeatability and candidate comparison

```bash
bash scripts/run-cpu-performance-l3.sh repeatability \
  <baseline-run-1>/summary.json \
  <baseline-run-2>/summary.json \
  <baseline-repeatability-report.json>

bash scripts/run-cpu-performance-l3.sh compare \
  <baseline>/summary.json \
  <candidate>/summary.json \
  <candidate-gate-report.json>
```

Every run writes raw CSV, metadata, summarized diagnostics, a stack sample, and `summary.json`
under `artifacts/performance/cpu-p0-p1/<timestamp>-<stage>-<scenario>/`. Artifacts are evidence
and are not committed.
