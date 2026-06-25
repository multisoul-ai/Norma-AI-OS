# Packaging Notes

`Packager.toml` configures an unsigned macOS app and DMG:

- product name: `Multisoul`
- bundle id: `com.multisoul.desktop`
- formats: `app`, `dmg`
- binary: `target/release/multisoul`
- minimum macOS: `12.0`

Release workflow:

```bash
cargo install cargo-packager --locked
cargo build --release -p app
cargo packager --release
```

Codesigning and notarization are explicitly Phase 6.
