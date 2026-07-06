//! CEF subprocess handoff for the optional Chromium Browser backend.

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
use std::ffi::CString;
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
use std::sync::OnceLock;

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
const CEF_FRAMEWORK_DIR: &str = "Chromium Embedded Framework.framework";
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
const CEF_FRAMEWORK_BINARY: &str = "Chromium Embedded Framework";
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
const CEF_RUNTIME_DYLIBS: &[&str] = &["libEGL.dylib", "libGLESv2.dylib"];

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
static CEF_FRAMEWORK_PATHS: OnceLock<Result<CefFrameworkPaths, String>> = OnceLock::new();
#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
static CEF_LIBRARY_LOADED: OnceLock<Result<(), String>> = OnceLock::new();

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
#[derive(Clone, Debug)]
struct CefFrameworkPaths {
    framework_dir: PathBuf,
    framework_binary: PathBuf,
    resources_dir: PathBuf,
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
pub(crate) fn exit_if_cef_subprocess() {
    if !std::env::args().any(is_cef_subprocess_arg) {
        return;
    }
    if let Err(err) = load_cef_library() {
        eprintln!("{err}");
        std::process::exit(1);
    }
    let args = cef::args::Args::new();
    let exit_code = cef::execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if exit_code >= 0 {
        std::process::exit(exit_code);
    }
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
pub(crate) fn load_cef_library() -> Result<(), String> {
    CEF_LIBRARY_LOADED
        .get_or_init(|| {
            let paths = cef_framework_paths()?;
            prepare_cef_runtime_libraries(&paths)?;
            let library_path = c_string_from_path(&paths.framework_binary)?;
            let loaded = unsafe { cef::load_library(Some(&*library_path.as_ptr().cast())) == 1 };
            if loaded {
                Ok(())
            } else {
                Err(format!(
                    "failed to load CEF framework: {}",
                    paths.framework_binary.display()
                ))
            }
        })
        .clone()
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn prepare_cef_runtime_libraries(paths: &CefFrameworkPaths) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|err| format!("failed to resolve current executable for CEF dylibs: {err}"))?;
    if is_app_bundle_executable(&exe) {
        return Ok(());
    }
    let exe_dir = exe
        .parent()
        .ok_or_else(|| format!("CEF executable has no parent directory: {}", exe.display()))?;
    let libraries_dir = paths.framework_dir.join("Libraries");
    for library in CEF_RUNTIME_DYLIBS {
        install_runtime_library_link(&libraries_dir.join(library), &exe_dir.join(library))?;
    }
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn install_runtime_library_link(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!(
            "CEF runtime library not found: {}",
            source.display()
        ));
    }

    if let Ok(metadata) = std::fs::symlink_metadata(destination) {
        if metadata.file_type().is_symlink() {
            if destination.canonicalize().ok() == source.canonicalize().ok() {
                return Ok(());
            }
            std::fs::remove_file(destination).map_err(|err| {
                format!(
                    "failed to replace stale CEF runtime link {}: {err}",
                    destination.display()
                )
            })?;
        } else if metadata.is_file() {
            return Ok(());
        } else {
            return Err(format!(
                "CEF runtime library destination is not a file: {}",
                destination.display()
            ));
        }
    }

    std::os::unix::fs::symlink(source, destination)
        .or_else(|_| std::fs::copy(source, destination).map(|_| ()))
        .map_err(|err| {
            format!(
                "failed to expose CEF runtime library {} at {}: {err}",
                source.display(),
                destination.display()
            )
        })
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn is_app_bundle_executable(exe: &Path) -> bool {
    let Some(macos_dir) = exe.parent() else {
        return false;
    };
    if macos_dir.file_name().and_then(|name| name.to_str()) != Some("MacOS") {
        return false;
    }
    let Some(contents_dir) = macos_dir.parent() else {
        return false;
    };
    if contents_dir.file_name().and_then(|name| name.to_str()) != Some("Contents") {
        return false;
    }
    contents_dir
        .parent()
        .and_then(|bundle| bundle.extension())
        .and_then(|extension| extension.to_str())
        == Some("app")
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
pub(crate) fn cef_framework_dir_cef_string() -> Result<cef::CefString, String> {
    load_cef_library()?;
    let paths = cef_framework_paths()?;
    let path = path_to_utf8(&paths.framework_dir, "CEF framework directory")?;
    Ok(cef::CefString::from(path))
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
pub(crate) fn cef_resources_dir_cef_string() -> Result<cef::CefString, String> {
    load_cef_library()?;
    let paths = cef_framework_paths()?;
    let path = path_to_utf8(&paths.resources_dir, "CEF resources directory")?;
    Ok(cef::CefString::from(path))
}

#[cfg(not(all(target_os = "macos", feature = "chromium-browser")))]
pub(crate) fn exit_if_cef_subprocess() {}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn cef_framework_paths() -> Result<CefFrameworkPaths, String> {
    CEF_FRAMEWORK_PATHS
        .get_or_init(resolve_cef_framework_paths)
        .clone()
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn resolve_cef_framework_paths() -> Result<CefFrameworkPaths, String> {
    for key in ["MULTISOUL_CEF_FRAMEWORK_PATH", "CEF_PATH", "CEF_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            let candidate = PathBuf::from(value);
            if let Some(paths) = framework_paths_from_candidate(&candidate) {
                return Ok(paths);
            }
        }
    }

    let exe = std::env::current_exe()
        .map_err(|err| format!("failed to resolve current executable for CEF: {err}"))?;
    if let Some(paths) = bundled_framework_paths(&exe) {
        return Ok(paths);
    }
    if let Some(paths) = dev_build_framework_paths(&exe) {
        return Ok(paths);
    }

    Err(format!(
        "CEF framework not found. Set MULTISOUL_CEF_FRAMEWORK_PATH to {}",
        Path::new(CEF_FRAMEWORK_DIR)
            .join(CEF_FRAMEWORK_BINARY)
            .display()
    ))
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn bundled_framework_paths(exe: &Path) -> Option<CefFrameworkPaths> {
    let macos_dir = exe.parent()?;
    framework_paths_from_candidate(&macos_dir.join("../Frameworks"))
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn dev_build_framework_paths(exe: &Path) -> Option<CefFrameworkPaths> {
    let exe_dir = exe.parent()?;
    let profile_dir = if exe_dir.file_name()?.to_string_lossy() == "deps" {
        exe_dir.parent()?
    } else {
        exe_dir
    };
    let build_dir = profile_dir.join("build");
    let mut entries = std::fs::read_dir(build_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    entries.reverse();
    entries.into_iter().find_map(|entry| {
        let name = entry.file_name()?.to_string_lossy();
        if !name.starts_with("cef-dll-sys-") {
            return None;
        }
        framework_paths_from_candidate(&entry.join("out").join(cef_arch_dir()))
    })
}

#[cfg(all(
    target_os = "macos",
    target_arch = "aarch64",
    feature = "chromium-browser"
))]
fn cef_arch_dir() -> &'static str {
    "cef_macos_aarch64"
}

#[cfg(all(
    target_os = "macos",
    target_arch = "x86_64",
    feature = "chromium-browser"
))]
fn cef_arch_dir() -> &'static str {
    "cef_macos_x86_64"
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn framework_paths_from_candidate(candidate: &Path) -> Option<CefFrameworkPaths> {
    let framework_dir = if candidate.file_name()? == CEF_FRAMEWORK_BINARY {
        candidate.parent()?.to_path_buf()
    } else if candidate.file_name()? == CEF_FRAMEWORK_DIR {
        candidate.to_path_buf()
    } else {
        candidate.join(CEF_FRAMEWORK_DIR)
    };
    let framework_binary = framework_dir.join(CEF_FRAMEWORK_BINARY);
    let resources_dir = framework_dir.join("Resources");
    if !framework_binary.is_file() {
        return None;
    }
    if !resources_dir.is_dir() {
        return None;
    }
    Some(CefFrameworkPaths {
        framework_dir: framework_dir.canonicalize().ok()?,
        framework_binary: framework_binary.canonicalize().ok()?,
        resources_dir: resources_dir.canonicalize().ok()?,
    })
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn c_string_from_path(path: &Path) -> Result<CString, String> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("CEF path contains an interior NUL byte: {}", path.display()))
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn path_to_utf8<'a>(path: &'a Path, label: &str) -> Result<&'a str, String> {
    path.to_str()
        .ok_or_else(|| format!("{label} is not valid UTF-8: {}", path.display()))
}

#[cfg(any(test, all(target_os = "macos", feature = "chromium-browser")))]
fn is_cef_subprocess_arg(arg: impl AsRef<str>) -> bool {
    arg.as_ref().starts_with("--type=")
}

#[cfg(test)]
#[path = "../../tests/unit/cef_process_tests.rs"]
mod tests;
