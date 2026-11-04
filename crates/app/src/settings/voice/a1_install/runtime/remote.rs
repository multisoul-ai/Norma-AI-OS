use super::{file_hash, managed};
use crate::settings::voice::a1_install::{
    check_cancelled, disk, download_verified_file, A1InstallFailure, InstallCancellation,
};
use model_store::VerifiedFile;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    schema: u32,
    base_url: String,
    manifest_bytes: u64,
    manifest_sha256: String,
    minimum_macos_major: u32,
    target: String,
    requirements_sha256: String,
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn load_release(bundle: &Path) -> Result<Release, A1InstallFailure> {
    let path = bundle.join("release.json");
    let metadata = fs::symlink_metadata(&path).map_err(|_| A1InstallFailure::EngineMissing)?;
    if !metadata.is_file() || metadata.len() > 16_384 {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    let release: Release = serde_json::from_slice(&fs::read(&path).map_err(|e| disk(&path, e))?)
        .map_err(|_| A1InstallFailure::EngineIntegrity)?;
    let prefix = "https://github.com/multisoul-ai/norma-os-releases/releases/download/a1-runtime-";
    let trusted_url = release.base_url.strip_prefix(prefix).is_some_and(|tag| {
        !tag.is_empty()
            && tag
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    });
    // HTTP is accepted only by unit fixtures; production descriptors are immutable HTTPS releases.
    let fixture_url = cfg!(test)
        && reqwest::Url::parse(&release.base_url).is_ok_and(|url| {
            url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.port().is_some()
        });
    if release.schema != 1
        || !(trusted_url || fixture_url)
        || release.manifest_bytes == 0
        || release.manifest_bytes > 16 * 1024 * 1024
        || !valid_hash(&release.manifest_sha256)
    {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    if release.target != "aarch64-apple-darwin"
        || !cfg!(all(target_os = "macos", target_arch = "aarch64"))
        || release.minimum_macos_major != 26
        || release.requirements_sha256 != format!("{:x}", Sha256::digest(asr::A1_REQUIREMENTS))
    {
        return Err(A1InstallFailure::EngineIncompatible);
    }
    let version = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| v.split('.').next()?.parse::<u32>().ok());
    if version.is_none_or(|major| major < release.minimum_macos_major) {
        return Err(A1InstallFailure::EngineIncompatible);
    }
    Ok(release)
}

fn verified(path: &Path, length: u64, hash: &str) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.len() == length)
        && file_hash(path).is_ok_and(|actual| actual == hash)
}

fn packaged_manifest(
    bundle: &Path,
    release: &Release,
) -> Result<Option<Vec<u8>>, A1InstallFailure> {
    let path = bundle.join("engine.json.gz");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(disk(&path, error)),
    };
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    let source = fs::File::open(&path).map_err(|e| disk(&path, e))?;
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(source)
        .take(release.manifest_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| A1InstallFailure::EngineIntegrity)?;
    if bytes.len() as u64 != release.manifest_bytes
        || format!("{:x}", Sha256::digest(&bytes)) != release.manifest_sha256
    {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    Ok(Some(bytes))
}

pub(super) fn prepare(
    root: &Path,
    bundle: &Path,
    cancelled: &impl InstallCancellation,
    allow_download: bool,
    mut progress: impl FnMut(&str, u64, u64),
) -> Result<PathBuf, A1InstallFailure> {
    check_cancelled(cancelled)?;
    let release = load_release(bundle)?;
    let cache = root
        .join("a1/engine-downloads")
        .join(&release.manifest_sha256);
    let packaged = packaged_manifest(bundle, &release)?;
    if !allow_download && !cache.is_dir() && packaged.is_none() {
        return Err(A1InstallFailure::EngineMissing);
    }
    fs::create_dir_all(&cache).map_err(|e| disk(&cache, e))?;
    let lock_path = cache.join("install.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| disk(&lock_path, e))?;
    loop {
        check_cancelled(cancelled)?;
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(50)),
            Err(TryLockError::Error(error)) => return Err(disk(&lock_path, error)),
        }
    }
    let manifest = cache.join("engine.json");
    if !verified(&manifest, release.manifest_bytes, &release.manifest_sha256) {
        if let Some(bytes) = packaged {
            let mut temporary =
                tempfile::NamedTempFile::new_in(&cache).map_err(|e| disk(&manifest, e))?;
            temporary
                .write_all(&bytes)
                .map_err(|e| disk(&manifest, e))?;
            temporary
                .as_file()
                .sync_all()
                .map_err(|e| disk(&manifest, e))?;
            temporary
                .persist(&manifest)
                .map_err(|e| disk(&manifest, e.error))?;
        } else {
            if !allow_download {
                return Err(A1InstallFailure::EngineMissing);
            }
            download_verified_file(
                &format!("{}/engine.json", release.base_url),
                &VerifiedFile::new(
                    "engine.json",
                    release.manifest_bytes,
                    &release.manifest_sha256,
                ),
                &cache,
                cancelled,
                |done, total| progress("engine.json", done, total),
            )?;
        }
    }
    let specification =
        managed::validate_manifest(&fs::read(&manifest).map_err(|e| disk(&manifest, e))?)?;
    if specification.archive_bytes == 0
        || specification.archive_bytes > 1024 * 1024 * 1024
        || !valid_hash(&specification.archive_sha256)
    {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    let installed = managed::prepare(root, &cache, cancelled);
    let result = match installed {
        Err(A1InstallFailure::EngineMissing | A1InstallFailure::EngineIntegrity)
            if allow_download =>
        {
            download_verified_file(
                &format!("{}/engine.tar.bz2", release.base_url),
                &VerifiedFile::new(
                    "engine.tar.bz2",
                    specification.archive_bytes,
                    &specification.archive_sha256,
                ),
                &cache,
                cancelled,
                |done, total| progress("engine.tar.bz2", done, total),
            )?;
            managed::prepare(root, &cache, cancelled)
        }
        other => other,
    };
    if result.is_ok() {
        // The installed tree is verified independently; retain only the small manifest for offline reuse.
        let _ = fs::remove_file(cache.join("engine.tar.bz2"));
    }
    result
}
