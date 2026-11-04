# Norma patch for `sherpa-onnx-sys 1.13.3`

This directory is the upstream `sherpa-onnx-sys 1.13.3` crate with these patches:

- The build dependency on `ureq 2.12` enables `socks-proxy` in addition to
  `proxy-from-env`.
- The build script watches the resolved native library directory with
  `cargo:rerun-if-changed`, so cached build-script output does not outlive
  pruned native libraries. Rebuild restores the pinned archive before linking.

The upstream build script downloads the pinned native archive when the current
Cargo target directory does not already contain it. `ureq` checks `ALL_PROXY`
before `HTTPS_PROXY`; without its SOCKS feature, developer environments using a
SOCKS `ALL_PROXY` fail before the application can compile.

Keep the Rust FFI declarations and build script aligned with upstream version
`1.13.3`. Run `bash scripts/check-socks-proxy-support.sh` after dependency
changes.

The cache regression first builds the real crate, removes its generated native
library directory and Rust library artifacts, retains build-script output, and
builds again. Source `7db4be3d` failed at the second build with a missing
`sherpa-onnx-c-api` library on GitHub macOS runner (run `41507397341`, job
`125490089087`). The patched build must restore the actual native library;
skipping the link or hiding the missing file cannot pass the regression.
