use super::{Project, Result};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_TEXT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct FileOptions {
    pub hidden: bool,
    pub ignored: bool,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
    pub symlink: bool,
}

#[derive(Clone, Debug)]
pub struct TextFile {
    pub text: String,
    pub version: String,
}

pub(super) fn fingerprint(bytes: &[u8]) -> String {
    git2::Oid::hash_object(git2::ObjectType::Blob, bytes)
        .expect("blob hashing has no external dependencies")
        .to_string()
}

impl Project {
    pub fn directory(&self, relative: &Path, options: FileOptions) -> Result<Vec<FileEntry>> {
        let path = self.resolve(relative)?;
        let repo = git2::Repository::discover(&self.root).ok();
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git" || (!options.hidden && name.starts_with('.')) {
                continue;
            }
            let path = relative.join(&name);
            if !options.ignored
                && repo.as_ref().is_some_and(|repo| {
                    repo.workdir()
                        .and_then(|work| {
                            entry.path().strip_prefix(work).ok().map(Path::to_path_buf)
                        })
                        .is_some_and(|p| repo.status_should_ignore(&p).unwrap_or(false))
                })
            {
                continue;
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            entries.push(FileEntry {
                name,
                path,
                directory: kind.is_dir(),
                symlink: kind.is_symlink(),
            });
        }
        entries.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub fn search(&self, query: &str, options: FileOptions) -> Result<Vec<FileEntry>> {
        let mut pending = vec![PathBuf::new()];
        let mut matches = Vec::new();
        let query = query.to_lowercase();
        while let Some(dir) = pending.pop() {
            for entry in self.directory(&dir, options)? {
                if entry.directory {
                    pending.push(entry.path.clone());
                }
                if entry.path.to_string_lossy().to_lowercase().contains(&query) {
                    matches.push(entry);
                }
                if matches.len() > 20_000 {
                    return Err("筛选结果过多，请输入更具体的文件名".into());
                }
            }
        }
        Ok(matches)
    }

    pub fn read_text(&self, relative: &Path) -> Result<TextFile> {
        let path = self.resolve(relative)?;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("不是普通文件".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_TEXT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_TEXT_BYTES {
            return Err("文件超过 1 MiB，请使用系统应用打开".into());
        }
        if bytes.contains(&0) {
            return Err("二进制文件不支持文本编辑".into());
        }
        let version = fingerprint(&bytes);
        let text = String::from_utf8(bytes).map_err(|_| "不是 UTF-8 文本，请使用系统应用打开")?;
        Ok(TextFile { text, version })
    }

    pub fn save_text(&self, relative: &Path, version: &str, text: &str) -> Result<TextFile> {
        let _guard = super::mutation::WRITE_LOCK
            .lock()
            .map_err(|e| e.to_string())?;
        if text.len() as u64 > MAX_TEXT_BYTES {
            return Err("文本超过 1 MiB".into());
        }
        let target = self.resolve(relative)?;
        let permissions = std::fs::metadata(&target)
            .map_err(|e| e.to_string())?
            .permissions();
        if permissions.readonly() {
            return Err("文件为只读".into());
        }
        let mut file = tempfile::NamedTempFile::new_in(target.parent().ok_or("缺少父目录")?)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.as_file()
            .set_permissions(permissions)
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        if self.read_text(relative)?.version != version {
            return Err("文件已被外部修改，草稿已保留；请重新载入后合并".into());
        }
        super::atomic::replace_checked(file, &target, version)?;
        Ok(TextFile {
            text: text.to_string(),
            version: fingerprint(text.as_bytes()),
        })
    }
}
