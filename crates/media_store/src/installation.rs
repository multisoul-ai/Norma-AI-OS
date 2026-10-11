use crate::{MediaArtifact, MediaError};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Blocking filesystem operations; callers must execute them in the background.
#[derive(Clone, Debug)]
pub struct MediaStore {
    root: PathBuf,
}

impl MediaStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn find_verified(&self, artifact: &MediaArtifact) -> Result<Option<PathBuf>, MediaError> {
        validate_root(&self.root)?;
        let path = self.root.join(artifact.cache_filename());
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() || self.root.is_symlink() {
            return Err(MediaError::UnsafeCachePath);
        }
        if metadata.len() != artifact.bytes() {
            return Err(MediaError::LengthMismatch {
                expected: artifact.bytes(),
                actual: metadata.len(),
            });
        }
        if hash_file(&path)? != artifact.sha256() {
            return Err(MediaError::HashMismatch);
        }
        Ok(Some(path))
    }

    pub fn install(
        &self,
        artifact: &MediaArtifact,
        reader: &mut impl Read,
        cancelled: &AtomicBool,
    ) -> Result<PathBuf, MediaError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(MediaError::Cancelled);
        }
        validate_root(&self.root)?;
        if let Ok(Some(existing)) = self.find_verified(artifact) {
            return Ok(existing);
        }
        fs::create_dir_all(&self.root)?;
        validate_root(&self.root)?;
        let final_path = self.root.join(artifact.cache_filename());
        if final_path.is_symlink() {
            return Err(MediaError::UnsafeCachePath);
        }
        let mut output = tempfile::Builder::new()
            .prefix(".media-partial-")
            .tempfile_in(&self.root)?;
        let mut sha = Sha256::new();
        let mut received = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(MediaError::Cancelled);
            }
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            received = received
                .checked_add(count as u64)
                .ok_or_else(|| MediaError::InvalidArtifact("media length overflow".into()))?;
            if received > artifact.bytes() {
                return Err(MediaError::LengthMismatch {
                    expected: artifact.bytes(),
                    actual: received,
                });
            }
            output.write_all(&buffer[..count])?;
            sha.update(&buffer[..count]);
        }
        if received != artifact.bytes() {
            return Err(MediaError::LengthMismatch {
                expected: artifact.bytes(),
                actual: received,
            });
        }
        if format!("{:x}", sha.finalize()) != artifact.sha256() {
            return Err(MediaError::HashMismatch);
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(MediaError::Cancelled);
        }
        output.as_file_mut().sync_all()?;
        output
            .persist(&final_path)
            .map_err(|error| MediaError::Io(error.error))?;
        Ok(final_path)
    }
}

fn validate_root(root: &Path) -> Result<(), MediaError> {
    if !root.is_absolute()
        || root.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(MediaError::UnsafeCachePath);
    }
    for ancestor in root.ancestors() {
        if ancestor.is_symlink() && !is_system_alias(ancestor) {
            return Err(MediaError::UnsafeCachePath);
        }
        if ancestor.exists() && !ancestor.is_dir() {
            return Err(MediaError::UnsafeCachePath);
        }
    }
    Ok(())
}

// macOS exposes these system-owned aliases to its real /private directories.
// They are the only accepted aliases; application/user-created aliases fail closed.
fn is_system_alias(path: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let expected = match path.to_str() {
        Some("/var") => Path::new("/private/var"),
        Some("/tmp") => Path::new("/private/tmp"),
        _ => return false,
    };
    path.canonicalize().is_ok_and(|actual| actual == expected)
}

fn hash_file(path: &Path) -> Result<String, MediaError> {
    let mut sha = Sha256::new();
    let mut input = fs::File::open(path)?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
