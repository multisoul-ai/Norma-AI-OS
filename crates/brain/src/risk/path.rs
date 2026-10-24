//! 词法路径判断；符号链接由调用方先解析。

use std::path::{Component, Path, PathBuf};

const ALLOWED_DEVICES: [&str; 3] = ["/dev/null", "/dev/stdout", "/dev/stderr"];

pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub(super) fn is_inside(root: &Path, raw: &str) -> bool {
    if ALLOWED_DEVICES.contains(&raw) {
        return true;
    }
    if raw.starts_with('~') || raw.starts_with('$') {
        return false;
    }
    let candidate = Path::new(raw);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    normalize(&joined).starts_with(normalize(root))
}

pub(super) fn is_secret(raw: &str) -> bool {
    const DIRS: [&str; 5] = [".ssh", ".aws", ".gnupg", ".kube", ".docker"];
    const FILES: [&str; 4] = [".npmrc", ".netrc", "credentials", ".pgpass"];
    let path = Path::new(raw);
    let in_secret_dir = path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|name| DIRS.contains(&name))
    });
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    in_secret_dir
        || name.starts_with(".env")
        || FILES.contains(&name.as_str())
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.ends_with(".pem")
}
