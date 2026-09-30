use super::{fs, Write, LIMIT};
use image::{ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Mutex,
};

// ponytail: serialize the small quota-check/atomic-save section; per-store locks if many servers contend.
static WRITES: Mutex<()> = Mutex::new(());

pub(super) fn validate(bytes: &[u8]) -> Result<&'static str, &'static str> {
    let format = image::guess_format(bytes).map_err(|_| "invalid_image")?;
    let extension = match format {
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Png => "png",
        _ => return Err("invalid_image"),
    };
    let reader = ImageReader::with_format(Cursor::new(bytes), format);
    let (width, height) = reader.into_dimensions().map_err(|_| "invalid_image")?;
    // Both iOS encoding and desktop decoding stay within2048² pixels (~16MiB RGBA).
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return Err("invalid_image");
    }
    image::load_from_memory_with_format(bytes, format).map_err(|_| "invalid_image")?;
    Ok(extension)
}

fn directory(path: &Path) -> Result<(), &'static str> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_dir() => Ok(()),
        Ok(_) => Err("image_storage_unavailable"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| "image_storage_unavailable")?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|_| "image_storage_unavailable")
        }
        Err(_) => Err("image_storage_unavailable"),
    }
}

pub(super) fn save(
    folder: &Path,
    id: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<String, &'static str> {
    let _lock = WRITES.lock().map_err(|_| "image_storage_unavailable")?;
    let root = folder.parent().ok_or("image_storage_unavailable")?;
    directory(root)?;
    directory(folder)?;
    let path = folder.join(format!("{id}.{extension}"));
    if path.exists() {
        let (_, previous) = read(folder, id)?;
        if previous != bytes {
            return Err("invalid_image");
        }
    } else {
        //256MiB per desktop store, including all projects; refuse at the boundary rather than silently deleting attachments used by queued Agents.
        let mut total = 0u64;
        let mut entries = 0;
        for dir in fs::read_dir(root).map_err(|_| "image_storage_unavailable")? {
            entries += 1;
            if entries >= 2048 {
                return Err("image_storage_full");
            }
            let dir = dir.map_err(|_| "image_storage_unavailable")?;
            if !dir
                .file_type()
                .map_err(|_| "image_storage_unavailable")?
                .is_dir()
            {
                return Err("image_storage_unavailable");
            }
            for file in fs::read_dir(dir.path()).map_err(|_| "image_storage_unavailable")? {
                // 2048 directory/file entries also bound scans of tiny images.
                entries += 1;
                if entries >= 2048 {
                    return Err("image_storage_full");
                }
                total += file
                    .map_err(|_| "image_storage_unavailable")?
                    .metadata()
                    .map_err(|_| "image_storage_unavailable")?
                    .len();
                if total + bytes.len() as u64 > 256 * 1024 * 1024 {
                    return Err("image_storage_full");
                }
            }
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(folder).map_err(|_| "image_storage_unavailable")?;
        temporary
            .write_all(bytes)
            .map_err(|_| "image_storage_unavailable")?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| "image_storage_unavailable")?;
        temporary
            .persist_noclobber(&path)
            .map_err(|_| "image_storage_unavailable")?;
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or("image_storage_unavailable")
}

pub(super) fn read(folder: &Path, id: &str) -> Result<(String, Vec<u8>), &'static str> {
    for parent in [folder.parent().ok_or("invalid_image")?, folder] {
        if !fs::symlink_metadata(parent)
            .map_err(|_| "image_not_found")?
            .file_type()
            .is_dir()
        {
            return Err("image_not_found");
        }
    }
    for extension in ["jpg", "png"] {
        let path = folder.join(format!("{id}.{extension}"));
        let Ok(file) = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
        else {
            continue;
        };
        let metadata = file.metadata().map_err(|_| "image_not_found")?;
        if !metadata.is_file() || metadata.len() > LIMIT as u64 {
            return Err("invalid_image");
        }
        let mut bytes = Vec::new();
        file.take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "image_not_found")?;
        if bytes.len() > LIMIT || format!("{:x}", Sha256::digest(&bytes)) != id {
            return Err("invalid_image");
        }
        return Ok((path.to_str().ok_or("invalid_image")?.into(), bytes));
    }
    Err("image_not_found")
}
