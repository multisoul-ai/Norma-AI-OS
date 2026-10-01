#!/bin/bash
# Explicit isolated Provider check, authorized by the user. Never modifies global Hook settings.
set -euo pipefail
echo "Disabled: this legacy runner depends on retired crates/e2e; use crate tests and isolated real-App acceptance." >&2
exit 2
root=$(cd "$(dirname "$0")/.." && pwd)
case "${1:-macos}" in
  macos) swift build --package-path "$root/ios" --product PairingProbe
    probe="$root/ios/.build/debug/PairingProbe" ;;
  ios) probe="$root/scripts/check-norma-ios-transport.sh" ;;
  *) echo 'usage: check-norma-mobile-approvals.sh [macos|ios]' >&2; exit 2 ;;
esac
codex_bin=$(command -v codex)
probe_dir=$(mktemp -d /tmp/norma-approval-shell.XXXXXX)
trap 'rm -rf "$probe_dir"' EXIT
mkdir "$probe_dir/bin"
# Production still invokes its normal command. Only this test process inherits the temporary
# shell startup files, which resolve Codex to a wrapper without external permission Hooks.
printf '#!/bin/bash\nexec %q --disable hooks "$@"\n' "$codex_bin" > "$probe_dir/bin/codex"
chmod 700 "$probe_dir/bin/codex"
printf 'export PATH=%q/bin:"$PATH"\n' "$probe_dir" > "$probe_dir/.zshrc"
cd "$root"
cargo build -p norma --bin multisoul-agent-hook
echo 'Isolated Codex external Hooks disabled; read-only sandbox and per-request approval retained.'
env ZDOTDIR="$probe_dir" PATH="$root/target/debug:$PATH" NORMA_SWIFT_PROBE="$probe" \
  cargo test -p e2e --lib approvals_native -- --ignored --nocapture --test-threads=1
