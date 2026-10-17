#!/usr/bin/env bash
# 旧入口。实现已迁到 test/desktop-e2e，这里只转发工作空间生命周期场景。
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/desktop-e2e.sh" workspace-lifecycle "$@"
