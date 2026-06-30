# Packaging Notes

`Packager.toml` configures an unsigned macOS app and DMG:

- product name: `Multisoul`
- bundle id: `com.multisoul.desktop`
- formats: `app`, `dmg`
- binary: `target/release/multisoul`
- secondary binary: `target/release/multisoul-agent-hook`
- minimum macOS: `12.0`

Release workflow:

```bash
cargo install cargo-packager --locked
cargo packager --release -c Packager.toml
```

`Packager.toml` runs a `before-packaging-command` that builds both release
binaries before the `.app` layout copies them into `Contents/MacOS/`.

Codesigning and notarization are explicitly Phase 6.

## Wallpaper resources

Built-in wallpapers live in `crates/app/assets/wallpapers/` (3 PNG, 2 GIF, 5 MP4).
The MP4s are tracked by **git-lfs** (see root `.gitattributes`); the working tree
holds the real bytes after `git lfs pull`, but the committed blobs are LFS
pointers, so the repository does not bloat by ~141 MB.

Release packaging must copy these into the app bundle's `Contents/Resources/`
directory after `cargo packager` lays out the bundle. There is no `Packager.toml`
glob wired up yet, so the step is manual:

```bash
# after `cargo packager --release` produces target/release/bundle/<bundle>/
bash scripts/copy-wallpaper-resources.sh \
    "target/release/bundle/osx/Multisoul.app/Contents/Resources"
```

The resulting `Contents/Resources/wallpapers/` holds all 10 built-ins, which the
runtime resolves at launch (see `EmbeddedAssets` / built-in resolver).
If a `Packager.toml` is added later, prefer a `resources` glob over the script.
