---
name: release-dmg
description: Build, package, sign, notarize, and validate the Norma OS macOS DMG. Use when the user asks to build or package the app, 打包 DMG, 生成安装包, 构建发布包, 正式发布, 发版, notarize, or locate and verify a distributable Norma OS artifact.
---

# Release DMG

Use the repository-owned release entrypoint so versioning, helper binaries,
resources, signing, and notarization follow one deterministic path.

## Inspect before running

1. Work from the repository root.
2. Read `docs/references/packaging.md` when packaging configuration or credentials
   may have changed.
3. Run `git status --short`. Preserve existing user changes. Packaging advances
   the patch version in `Cargo.toml` and `Packager.toml`; report those changes.
4. Confirm the host is macOS and required tooling is available. Let the release
   script perform codesign readiness and Git LFS checks.

## Select the mode

- For “build”, “package”, “打包 DMG”, “生成安装包”, or local verification, run:

  ```bash
  ./scripts/release-local.sh
  ```

  This builds release binaries, creates the `.app` and `.dmg`, and applies the
  configured Developer ID signature without Apple notarization.

- For “publish”, “正式发布”, “发版”, “可对外分发”, or an explicitly notarized
  artifact, run:

  ```bash
  ./scripts/release-local.sh --notarize
  ```

  Require `APP_STORE_CONNECT_API_KEY_ID`,
  `APP_STORE_CONNECT_API_ISSUER_ID`, and
  `APP_STORE_CONNECT_API_KEY_PATH`. If any credential is missing, stop and
  report the missing variable; never silently fall back to an unnotarized build.

- Use `./scripts/release-local.sh --ad-hoc` only when the user explicitly asks
  for an ad-hoc local test package and the release binaries already exist.

If the user only says “DMG” and does not indicate public distribution, choose
the default non-notarized mode. Do not invoke a lower-level packager executable
or bypass the repository's `cargo packager` wrapper.

## Monitor and verify

Allow the command to finish. Do not treat the presence of an older file under
`build/` as success. The release script must verify the newly produced app and
DMG, then write an adjacent `.sha256` file.

For notarized mode, require successful Gatekeeper and stapler validation from
the script. For default mode, clearly label the artifact as signed but not
notarized.

## Handoff

Report:

- exact DMG path under `build/` or `build/dev/`;
- DMG size and SHA-256 value;
- signing/notarization mode and validation result;
- version change made by the packaging hook;
- any warnings or failed prerequisite.

Do not upload, publish, create a remote release, or replace an existing external
artifact unless the user separately authorizes that external action.
