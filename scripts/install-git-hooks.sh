#!/usr/bin/env bash
# Install this repository's tracked native Git hooks.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

git config core.hooksPath .githooks

echo "Installed Git hooks: $(git config core.hooksPath)"
echo "To disable them for this repository: git config --unset core.hooksPath"
