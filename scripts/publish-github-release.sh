#!/usr/bin/env bash
set -euo pipefail

repo="multisoul-ai/norma-os-releases"
tag=""
assets=""
plist=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --repo) repo="${2:-}"; shift 2 ;;
    --tag) tag="${2:-}"; shift 2 ;;
    --assets) assets="${2:-}"; shift 2 ;;
    --plist) plist="${2:-}"; shift 2 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

: "${GH_TOKEN:?GH_TOKEN must contain a GitHub App installation token}"
[ -n "$tag" ] || fail "--tag is required"
[ -d "$assets" ] || fail "--assets directory not found: $assets"
[ -f "$plist" ] || fail "--plist not found: $plist"

expected_assets='Norma-OS_aarch64.dmg
Norma-OS_aarch64.dmg.sha256
appcast.xml'
actual_assets=$(find "$assets" -mindepth 1 -maxdepth 1 -exec basename {} \; | LC_ALL=C sort)
[ "$actual_assets" = "$expected_assets" ] || fail "local release assets are not canonical"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
python3 "$repo_root/scripts/verify-sparkle-appcast.py" \
  --tag "$tag" --assets "$assets" --plist "$plist"

dmg_digest=$(shasum -a 256 "$assets/Norma-OS_aarch64.dmg" | awk '{print $1}')
expected_summary=$(printf 'Norma OS %s\n\nDMG SHA-256: %s' "$tag" "$dmg_digest")

release_matches=$(gh api --paginate "repos/$repo/releases?per_page=100" \
  --jq ".[] | select(.tag_name == \"$tag\") | @base64")
match_count=$(printf '%s\n' "$release_matches" | awk 'NF { count += 1 } END { print count + 0 }')
[ "$match_count" -le 1 ] || fail "multiple GitHub releases unexpectedly use tag $tag"
tag_ref_count=$(gh api "repos/$repo/git/matching-refs/tags/$tag" \
  --jq "[.[] | select(.ref == \"refs/tags/$tag\")] | length")
[[ "$tag_ref_count" =~ ^[0-9]+$ ]] || fail "GitHub did not return a tag ref count"
if [ "$tag_ref_count" -gt 0 ] && [ "$match_count" -eq 0 ]; then
  fail "tag conflict: $repo already has git ref refs/tags/$tag without a resumable draft"
fi

if [ "$match_count" -eq 1 ]; then
  release_json=$(printf '%s' "$release_matches" | base64 --decode)
  draft=$(printf '%s' "$release_json" | jq -r '.draft')
  [ "$draft" = true ] || fail "tag conflict: $repo already has a non-draft release for $tag"
  remote_summary=$(printf '%s' "$release_json" | jq -r '.body // ""')
  [ "$remote_summary" = "$expected_summary" ] \
    || fail "draft summary mismatch; refusing to resume an unrelated release"
else
  payload=$(jq -n \
    --arg tag "$tag" \
    --arg body "$expected_summary" \
    '{tag_name: $tag, name: $tag, body: $body, draft: true, prerelease: false}')
  release_json=$(printf '%s' "$payload" | gh api --method POST "repos/$repo/releases" --input -)
fi

release_id=$(printf '%s' "$release_json" | jq -r '.id')
[[ "$release_id" =~ ^[0-9]+$ ]] || fail "GitHub did not return a numeric release id"

tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/norma-release-readback.XXXXXX")
trap 'rm -rf "$tmp_root"' EXIT

for name in Norma-OS_aarch64.dmg Norma-OS_aarch64.dmg.sha256 appcast.xml; do
  asset_ids=$(gh api "repos/$repo/releases/$release_id/assets?per_page=100" \
    --jq ".[] | select(.name == \"$name\") | .id")
  asset_count=$(printf '%s\n' "$asset_ids" | awk 'NF { count += 1 } END { print count + 0 }')
  [ "$asset_count" -le 1 ] || fail "draft contains duplicate asset $name"
  if [ "$asset_count" -eq 1 ]; then
    existing_dir="$tmp_root/existing-$name"
    mkdir -p "$existing_dir"
    gh release download "$tag" --repo "$repo" --pattern "$name" --dir "$existing_dir"
    local_digest=$(shasum -a 256 "$assets/$name" | awk '{print $1}')
    remote_digest=$(shasum -a 256 "$existing_dir/$name" | awk '{print $1}')
    [ "$local_digest" = "$remote_digest" ] \
      || fail "remote asset digest mismatch for $name; refusing to overwrite draft"
  else
    gh release upload "$tag" "$assets/$name" --repo "$repo"
  fi
done

readback="$tmp_root/all-assets"
mkdir -p "$readback"
gh release download "$tag" --repo "$repo" --dir "$readback"
remote_names=$(find "$readback" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort)
[ "$remote_names" = "$expected_assets" ] \
  || fail "remote Draft Release must contain exactly the three canonical assets"

for name in Norma-OS_aarch64.dmg Norma-OS_aarch64.dmg.sha256 appcast.xml; do
  local_digest=$(shasum -a 256 "$assets/$name" | awk '{print $1}')
  remote_digest=$(shasum -a 256 "$readback/$name" | awk '{print $1}')
  [ "$local_digest" = "$remote_digest" ] \
    || fail "remote asset digest mismatch for $name after upload"
done
python3 "$repo_root/scripts/verify-sparkle-appcast.py" \
  --tag "$tag" --assets "$readback" --plist "$plist"

publish_payload='{"draft":false,"make_latest":"true"}'
published=$(printf '%s' "$publish_payload" | gh api --method PATCH \
  "repos/$repo/releases/$release_id" --input -)
[ "$(printf '%s' "$published" | jq -r '.draft')" = false ] \
  || fail "GitHub release remained a draft after publish"
[ "$(printf '%s' "$published" | jq -r '.tag_name')" = "$tag" ] \
  || fail "published release tag changed unexpectedly"

public_verified=false
for attempt in 1 2 3 4 5 6; do
  public_dir="$tmp_root/public-latest-$attempt"
  mkdir -p "$public_dir"
  public_download_ok=true
  for name in Norma-OS_aarch64.dmg Norma-OS_aarch64.dmg.sha256 appcast.xml; do
    public_url="https://github.com/$repo/releases/latest/download/$name?readback=$attempt-$(date +%s)"
    if ! curl --fail --location --retry 2 --silent --show-error \
      --output "$public_dir/$name" "$public_url"; then
      public_download_ok=false
      break
    fi
  done

  if [ "$public_download_ok" = true ]; then
    public_matches=true
    for name in Norma-OS_aarch64.dmg Norma-OS_aarch64.dmg.sha256 appcast.xml; do
      local_digest=$(shasum -a 256 "$assets/$name" | awk '{print $1}')
      public_digest=$(shasum -a 256 "$public_dir/$name" | awk '{print $1}')
      if [ "$local_digest" != "$public_digest" ]; then
        public_matches=false
        break
      fi
    done
    if [ "$public_matches" = true ] && python3 "$repo_root/scripts/verify-sparkle-appcast.py" \
      --tag "$tag" --assets "$public_dir" --plist "$plist"; then
      public_verified=true
      break
    fi
  fi

  [ "$attempt" -eq 6 ] || sleep 10
done
[ "$public_verified" = true ] \
  || fail "public latest URL readback did not match the published release after bounded retries"

echo "published $repo release $tag after remote readback verification"
