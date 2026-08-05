# Norma patch for `sherpa-onnx-sys 1.13.3`

This directory is the upstream `sherpa-onnx-sys 1.13.3` crate with one
manifest-only change:

- The build dependency on `ureq 2.12` enables `socks-proxy` in addition to
  `proxy-from-env`.

The upstream build script downloads the pinned native archive when the current
Cargo target directory does not already contain it. `ureq` checks `ALL_PROXY`
before `HTTPS_PROXY`; without its SOCKS feature, developer environments using a
SOCKS `ALL_PROXY` fail before the application can compile.

Keep the Rust FFI declarations and build script aligned with upstream version
`1.13.3`. Run `bash scripts/check-socks-proxy-support.sh` after dependency
changes.
