#!/bin/bash
set -euo pipefail
lab_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$lab_root"
exec /usr/bin/sandbox-exec -p '(version 1) (allow default) (deny network*)' \
  "$lab_root/.venv/bin/python" -m asr_lab.run "$@"
