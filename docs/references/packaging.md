# Packaging Notes

`Packager.toml` configures a macOS app and DMG with entitlements + Developer ID signing:

- product name: `Norma OS`
- bundle id: `com.norma.os`
- formats: `app`, `dmg`
- binary: `target/release/norma-os`
- secondary binary: `target/release/multisoul-agent-hook`
- minimum macOS: `12.0`

Release workflow:

```bash
cargo install cargo-packager --locked
cargo packager --release -c Packager.toml
cargo packager --notarize --release -c Packager.toml
./scripts/release-local.sh
./scripts/release-local.sh --notarize
```

Release binaries use the workspace `[profile.release]` settings: ThinLTO,
one codegen unit, and symbol stripping. Keep these settings covered by
`scripts/test-packager-config.sh`; removing them makes the statically linked
Rust/macOS binary materially larger.

仓库里的 `.cargo/config.toml` 会把 `cargo packager ...` 路由到
`tools/cargo-packager-wrapper`，再由 wrapper 调用真实的 `cargo-packager`。
默认执行 `cargo packager --release -c Packager.toml` 时，wrapper 会清掉
`APPLE_API_*`、`APPLE_KEYCHAIN_PROFILE`、`APPLE_ID`、`APPLE_PASSWORD`、
`APPLE_TEAM_ID`，因此只做 Developer ID 签名打包，不提交 Apple 公证。

需要真实发版公证时，显式传 `--notarize`：

```bash
cargo packager --notarize --release -c Packager.toml
```

wrapper 会把
`APP_STORE_CONNECT_API_KEY_ID` / `APP_STORE_CONNECT_API_ISSUER_ID` /
`APP_STORE_CONNECT_API_KEY_PATH` 映射成 cargo-packager 读取的 `APPLE_API_*`，
并移除个人 Apple ID 方式的 notary 凭证，避免账号串用。

`./scripts/release-local.sh` 保留为完整本地 release 流程入口：它会先做
codesign readiness 检查和 `git lfs pull`，再调用同一个 `cargo packager` 入口。
`./scripts/release-local.sh --notarize` 与 `cargo packager --notarize ...` 使用同一组
`APP_STORE_CONNECT_*` 凭证约定。

注意：这里依赖项目内 `.cargo/config.toml` 的 Cargo alias。如果绕过 Cargo alias 直接执行
`cargo-packager --release -c Packager.toml`，上游 cargo-packager 0.11.8 仍会直接读取当前
shell 的 `APPLE_*` notary 环境变量。

`Packager.toml` runs a `before-packaging-command` that first calls
`scripts/bump-release-version.sh`, then builds both release binaries before the
`.app` layout copies them into `Contents/MacOS/`. The bump script increments the
patch version in both `Packager.toml` and the root `Cargo.toml`
`[workspace.package]` section, and fails if those versions do not match.

`cargo-packager 0.11.8` parses `Packager.toml` before running
`before-packaging-command`, so a hook cannot change the already-loaded packager
config for the current process. The persisted files are advanced before the
release build, and the next `cargo packager --release -c Packager.toml`
invocation starts from that bumped version.

## DMG Finder layout

The release and development configurations use the same 640x400-point Finder
layout. The app and Applications anchors are `(180, 175)` and `(460, 175)`.
Finder draws its 16pt item labels below those anchors; the previous `y=200`
positions put both labels across the bottom border of the background wells.

The Retina background is authored in `assets/dmg-background.svg` and rendered
to the packaged `assets/dmg-background.png` with:

```bash
bash scripts/render-dmg-background.sh
```

The renderer composites the real app icon into the header, emits a 1280x800 PNG
at 144 DPI, and keeps the release asset reproducible. The two Finder label
landing zones use light backing plates so the system-owned dark labels remain
readable over the dark installer background. `scripts/test-packager-config.sh`
checks the exact label zones for sufficient luminance and verifies that the
header mark was not dropped during SVG rendering.

Finder owns the item-label rendering. Its icon-view scripting interface exposes
icon size, text size, label position, background picture, and background color,
but not label text color. `cargo-packager`/`create-dmg` therefore cannot force
these two labels to white. Keep the background readable with Finder's native
label color and verify actual appearance on the supported macOS versions.

## macOS app icon

`assets/app-icon-source.png` is the tightly cropped brand artwork used by
product surfaces and the DMG header. It is not packaged directly as the Dock
icon. macOS needs additional transparent optical padding: the generated
`assets/app-icon-macos.png` keeps the visible artwork at approximately
824×769 px on a 1024×1024 canvas, then `assets/AppIcon.icns` supplies the
standard 16–1024 px representations consumed by `cargo-packager`.

Regenerate both macOS assets after changing the brand artwork:

```bash
bash scripts/render-app-icon.sh
bash scripts/test-packager-config.sh
```

The packaging test checks the safe-area dimensions, centering, and pixel-level
agreement between the reviewable 1024 px PNG and the largest ICNS entry. This
prevents a tightly cropped source from making the Dock icon appear larger than
neighboring apps.

Codesigning 配置见 `packaging/macos/NormaOS.entitlements` 与
`Packager.toml` 的 `[macos]` 段；notarization 只通过
`cargo packager --notarize ...` 或 `scripts/release-local.sh --notarize` 显式开启，详见
`docs/design-docs/2026-07-05-developer-id-notarize-design.md`。

## Wallpaper resources

Built-in wallpapers live in `crates/app/assets/wallpapers/`: the Canvas catalog
contains 9 static images, 2 GIFs, and 5 MP4s; `home.mp4` and
`license-gate.mp4` are dedicated shell backgrounds. The MP4s are tracked by
**git-lfs** (see root `.gitattributes`); `git lfs pull` materializes the real
bytes before `release-local.sh` packages them.

`Packager.toml` / `Packager.dev.toml` 的 `resources` glob 在签名前把 wallpapers 目录拷进 app bundle 的
`Contents/Resources/wallpapers/`，并把 realtime card planner helper 拷进
`Contents/Resources/openai-card-planner.mjs`：

```toml
resources = [
  "crates/app/assets/wallpapers",
  "sidecars/openai-card-planner/openai-card-planner.mjs",
]
```

不再需要手动 `copy-wallpaper-resources.sh`——`resources` glob 在 cargo-packager
签名 pass 之前执行，确保 wallpapers 和 card planner helper 进入已签名 bundle。

Static/GIF wallpapers are loaded from their resolved files under
`Contents/Resources/wallpapers/`; they must not also be served by
`EmbeddedAssets`/`include_bytes!`. The old dual path stored roughly 81 MB twice:
once in the `norma-os` Mach-O `__const` section and once in Resources.

Six large static PNGs use WebP in the current catalog. The always-visible default
uses lossless WebP (SSIM 1.0); the other five use `cwebp -q 100` and retain their
original pixel dimensions (measured SSIM 0.9896–0.9977). Persisted selections that
still name the old PNGs are migrated and rewritten to the matching WebP path.

`scripts/test-packager-config.sh` enforces a 140 MiB source budget for the
packaged wallpaper directory (current total: 135.72 MiB). Video optimization
keeps the original dimensions, frame rate, H.264/yuv420p compatibility, and
removes unused audio; changes must be compared against the source before
replacing an asset.
