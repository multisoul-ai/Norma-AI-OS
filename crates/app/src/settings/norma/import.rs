use super::*;
use gpui::PathPromptOptions;

impl MemoryView {
    pub(super) fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_is_dirty(cx) {
            self.status = "请先保存或取消当前草稿。".into();
            cx.notify();
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("导入 Markdown 记忆".into()),
        });
        let paths = self.paths.clone();
        let root = self.root.clone();
        let scope = self.scope;
        let service = self.service.clone();
        self.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let selected = picker.await;
            let result = if let Ok(Ok(Some(selected))) = selected {
                cx.background_executor()
                    .spawn(async move {
                        let mut entries = Vec::new();
                        for path in selected {
                            if path.extension().and_then(|extension| extension.to_str())
                                != Some("md")
                            {
                                return Err("请选择 .md 文件".to_string());
                            }
                            let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
                            if !meta.is_file()
                                || meta.len() > storage::memory::MAX_TOPIC_BYTES as u64
                            {
                                return Err("导入文件超过主题容量或不是普通文件".into());
                            }
                            let id = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .ok_or_else(|| "无效文件名".to_string())?
                                .to_string();
                            let markdown =
                                std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                            entries.push((id, markdown));
                        }
                        let mut access = storage::MemoryAccess::new(paths.clone(), root);
                        let snapshot =
                            access.settings_snapshot(scope).map_err(|e| e.to_string())?;
                        storage::MemoryStore::open(&paths)
                            .map_err(|e| e.to_string())?
                            .preview_import(&snapshot, &entries)
                            .map_err(|e| e.to_string())?;
                        // Keep the same service for the subsequent manual commit.
                        drop(service);
                        Ok(Some((snapshot.revision(), entries)))
                    })
                    .await
            } else {
                Ok(None)
            };
            let _ = this.update_in(cx, |view, _, cx| {
                view.busy = false;
                match result {
                    Ok(Some((version, entries))) => {
                        view.version = version;
                        view.imports = entries;
                        view.error = false;
                        view.status =
                            "请检查导入预览；同 ID 将覆盖当前主题，确认后一次保存全部文件。".into();
                    }
                    Ok(None) => {}
                    Err(error) => {
                        view.error = true;
                        view.status = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
