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
./scripts/release-local.sh
./scripts/release-local.sh --notarize
```

默认使用 `./scripts/release-local.sh`，它会清掉 `APPLE_API_*`、
`APPLE_KEYCHAIN_PROFILE`、`APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID` 后再调用
`cargo packager --release -c Packager.toml`，因此只做 Developer ID 签名打包，不提交
Apple 公证。

需要真实发版公证时，显式传 `--notarize`。脚本会把
`APP_STORE_CONNECT_API_KEY_ID` / `APP_STORE_CONNECT_API_ISSUER_ID` /
`APP_STORE_CONNECT_API_KEY_PATH` 映射成 cargo-packager 读取的 `APPLE_API_*`，
然后再执行同一个底层打包命令。

不要把裸 `cargo packager --release -c Packager.toml` 当作日常入口：cargo-packager
0.11.8 没有 `--notarize` / `--no-notarize` 参数，`cargo packager --release -c Packager.toml` 会直接读取当前 shell 的 APPLE_* notary 环境变量；如果这些变量残留，它会在 `.app` 签名后自动尝试 notarize。

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

Codesigning 配置见 `packaging/macos/NormaOS.entitlements` 与
`Packager.toml` 的 `[macos]` 段；notarization 只通过
`scripts/release-local.sh --notarize` 显式开启，详见
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
