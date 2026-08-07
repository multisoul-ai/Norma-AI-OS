//! Chrome Native Messaging host transport.

use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

pub(crate) const MAX_NATIVE_MESSAGE_BYTES: usize = 1024 * 1024;
const NATIVE_HOST_NAME: &str = "com.norma.os.browser_auth";
const EXTENSION_ORIGIN: &str = "chrome-extension://fkhlnglmgbebphicanlcckhpflbdfopf/";

pub(crate) fn read_native_message(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut prefix = [0_u8; 4];
    reader.read_exact(&mut prefix)?;
    let length = u32::from_ne_bytes(prefix) as usize;
    if length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native Messaging frame exceeded the 1 MiB safety limit",
        ));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}

pub(crate) fn write_native_message(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_NATIVE_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Native Messaging frame exceeded the 1 MiB safety limit",
        ));
    }
    let length = u32::try_from(payload.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Native Messaging frame length did not fit u32",
        )
    })?;
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

pub(crate) fn native_host_manifest(host_binary: &Path) -> Result<Vec<u8>, String> {
    if !host_binary.is_absolute() {
        return Err("Chrome Native Messaging host binary path must be absolute".to_string());
    }
    let host_binary = host_binary
        .to_str()
        .ok_or_else(|| "Chrome Native Messaging host path is not valid UTF-8".to_string())?;
    serde_json::to_vec_pretty(&serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "Norma OS Browser authentication bridge",
        "path": host_binary,
        "type": "stdio",
        "allowed_origins": [EXTENSION_ORIGIN],
    }))
    .map_err(|_| "Chrome Native Messaging host manifest could not be encoded".to_string())
}

pub(crate) fn chrome_native_host_manifest_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("Google")
        .join("Chrome")
        .join("NativeMessagingHosts")
        .join(format!("{NATIVE_HOST_NAME}.json"))
}

pub(crate) fn browser_auth_socket_path() -> Result<PathBuf, String> {
    storage::default_app_data_dir()
        .map(|root| root.join("browser-auth").join("native.sock"))
        .map_err(|error| format!("could not resolve Browser auth socket directory: {error}"))
}

pub(crate) fn bundled_extension_path_for_executable(executable: &Path) -> Result<PathBuf, String> {
    let macos_dir = executable
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "MacOS"))
        .ok_or_else(|| "Norma executable was not inside Contents/MacOS".to_string())?;
    let contents_dir = macos_dir
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "Contents"))
        .ok_or_else(|| "Norma executable had no app Contents directory".to_string())?;
    Ok(contents_dir
        .join("Resources")
        .join("browser-auth-extension"))
}

pub(crate) fn bundled_extension_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not resolve Norma executable: {error}"))?;
    let extension = bundled_extension_path_for_executable(&executable)?;
    if !extension.join("manifest.json").is_file() {
        return Err(format!(
            "bundled Chrome authorization extension is missing: {}",
            extension.display()
        ));
    }
    Ok(extension)
}

pub(crate) fn install_native_host_manifest() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not resolve Norma executable: {error}"))?;
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "Norma executable had no parent directory".to_string())?;
    let host_binary = executable_dir.join("norma-browser-auth-host");
    if !host_binary.is_file() {
        return Err(format!(
            "Browser auth helper is missing next to Norma OS: {}",
            host_binary.display()
        ));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "could not resolve the user home for Chrome integration".to_string())?;
    let destination = chrome_native_host_manifest_path(&home);
    let parent = destination
        .parent()
        .ok_or_else(|| "Chrome Native Messaging manifest had no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create Chrome Native Messaging directory {}: {error}",
            parent.display()
        )
    })?;
    let manifest = native_host_manifest(&host_binary)?;
    let temporary = destination.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&temporary, manifest).map_err(|error| {
        format!(
            "could not write Chrome Native Messaging manifest {}: {error}",
            temporary.display()
        )
    })?;
    std::fs::rename(&temporary, &destination).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!(
            "could not install Chrome Native Messaging manifest {}: {error}",
            destination.display()
        )
    })?;
    Ok(destination)
}

#[cfg(unix)]
pub fn run_browser_auth_native_host() -> Result<(), String> {
    let caller = std::env::args().nth(1).unwrap_or_default();
    if caller != EXTENSION_ORIGIN {
        return Err("caller was not the authorized Norma Chrome extension".to_string());
    }
    let stream = UnixStream::connect(browser_auth_socket_path()?)
        .map_err(|error| format!("could not connect to the running Norma OS app: {error}"))?;
    proxy_native_stdio(stream)
}

#[cfg(not(unix))]
pub fn run_browser_auth_native_host() -> Result<(), String> {
    Err("Norma Browser auth host is currently implemented for macOS only".to_string())
}

#[cfg(unix)]
fn proxy_native_stdio(stream: UnixStream) -> Result<(), String> {
    let mut app_writer = stream
        .try_clone()
        .map_err(|error| format!("could not clone Browser auth socket: {error}"))?;
    std::thread::Builder::new()
        .name("browser-auth-chrome-to-app".to_string())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut stdin = stdin.lock();
            while let Ok(payload) = read_native_message(&mut stdin) {
                if write_native_message(&mut app_writer, &payload).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| format!("could not start Browser auth input proxy: {error}"))?;

    let mut app_reader = stream;
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    loop {
        match read_native_message(&mut app_reader) {
            Ok(payload) => write_native_message(&mut stdout, &payload).map_err(|error| {
                format!("could not write Chrome Native Messaging frame: {error}")
            })?,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "could not read Browser auth frame from Norma OS: {error}"
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/browser_auth_host_tests.rs"]
mod tests;
