# Packaging Notes

`Packager.toml` configures a macOS app and DMG with entitlements + (staged) Developer ID signing/notarize:

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
```

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

Codesigning and notarization 配置见 `packaging/macos/NormaOS.entitlements` 与
`Packager.toml` 的 `[macos]` 段；本地发版用 `scripts/release-local.sh`，详见
`docs/design-docs/2026-07-05-developer-id-notarize-design.md`。

## Wallpaper resources

Built-in wallpapers live in `crates/app/assets/wallpapers/` (3 PNG, 2 GIF, 5 MP4)。
The MP4s are tracked by **git-lfs** (see root `.gitattributes`)；commit 的是 LFS
指针，`git lfs pull` 后工作树才有真实字节（~141 MB），打包前 `release-local.sh`
会强制 lfs pull。

`Packager.toml` 的 `resources` glob 在签名前把整个目录拷进 app bundle 的
`Contents/Resources/wallpapers/`：

```toml
resources = ["crates/app/assets/wallpapers"]
```

不再需要手动 `copy-wallpaper-resources.sh`——`resources` glob 在 cargo-packager
签名 pass 之前执行，确保 wallpapers 进入已签名 bundle。
