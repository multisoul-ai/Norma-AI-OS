//! 想法附带的图片：文件落在数据库同级的 `idea-images` 目录，库里只记文件名。

use crate::ideas_store::invalid;
use crate::{IdeasStore, StorageResult, StoredIdea};
use std::path::PathBuf;

const IMAGES_DIR: &str = "idea-images";
const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff"];

/// 文件名只允许 `<字母数字>.<白名单扩展名>`，杜绝路径穿越。
fn valid_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && EXTENSIONS.contains(&ext)
}

pub(crate) fn encode_names(names: &[String]) -> StorageResult<String> {
    if let Some(bad) = names.iter().find(|name| !valid_name(name)) {
        return Err(invalid(&format!("invalid image name: {bad}")));
    }
    Ok(names.join("\n"))
}

pub(crate) fn decode_names(stored: &str) -> Vec<String> {
    stored
        .lines()
        .filter(|name| valid_name(name))
        .map(str::to_owned)
        .collect()
}

impl IdeasStore {
    /// 由存储生成 ID 的 `add_with_images`。
    pub fn capture_with_images(
        &self,
        workspace_id: &str,
        body: &str,
        source: workspace::ideas::IdeaSource,
        images: &[String],
    ) -> StorageResult<StoredIdea> {
        let id = crate::workspace_identity_store::random_workspace_id()?;
        self.add_with_images(workspace_id, &id, body, source, images)
    }

    fn images_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(IMAGES_DIR)
    }

    /// 保存一张图片，返回文件名（写入想法时传给 `add_with_images`）。
    pub fn save_image(&self, bytes: &[u8], extension: &str) -> StorageResult<String> {
        let extension = extension.to_ascii_lowercase();
        if !EXTENSIONS.contains(&extension.as_str()) {
            return Err(invalid("unsupported image type"));
        }
        let name = format!(
            "{}.{extension}",
            crate::workspace_identity_store::random_workspace_id()?
        );
        let dir = self.images_dir();
        std::fs::create_dir_all(&dir).map_err(|error| invalid(&error.to_string()))?;
        std::fs::write(dir.join(&name), bytes).map_err(|error| invalid(&error.to_string()))?;
        Ok(name)
    }

    /// 文件名对应的绝对路径；文件名不合法时报错。
    pub fn image_path(&self, name: &str) -> StorageResult<PathBuf> {
        if !valid_name(name) {
            return Err(invalid("invalid image name"));
        }
        Ok(self.images_dir().join(name))
    }

    /// 删除一张图片文件；不存在或名字不合法时静默忽略。
    pub fn discard_image(&self, name: &str) {
        if let Ok(path) = self.image_path(name) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// 查出满足条件的想法的图片文件名，供删除行之前先拿到。
    pub(crate) fn images_where(
        connection: &rusqlite::Connection,
        clause: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> StorageResult<Vec<String>> {
        let mut query = connection.prepare(&format!(
            "SELECT images FROM workspace_ideas WHERE {clause}"
        ))?;
        let rows = query.query_map(params, |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for row in rows {
            names.extend(decode_names(&row?));
        }
        Ok(names)
    }

    /// 交给智能体的任务文本：正文，加上每张图片的绝对路径。
    pub fn dispatch_text(&self, idea: &StoredIdea) -> String {
        let paths: Vec<String> = idea
            .images
            .iter()
            .filter_map(|name| self.image_path(name).ok())
            .map(|path| path.display().to_string())
            .collect();
        if paths.is_empty() {
            return idea.body.clone();
        }
        let list = format!("附带图片：\n{}", paths.join("\n"));
        if idea.body.is_empty() {
            list
        } else {
            format!("{}\n\n{list}", idea.body)
        }
    }
}
