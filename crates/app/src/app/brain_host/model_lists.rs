//! 设置页要列出的 Codex、OpenCode 模型：打开设置时在后台建一次会话读出来，缓存在内存里。

use super::config;
use crate::app::MultisoulApp;
use brain::{Backend, BackendKind, ModelChoice};
use gpui::Context;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ModelList {
    Loading,
    Ready(Vec<ModelChoice>),
    Failed(String),
}

impl MultisoulApp {
    /// 为还没读过（或上次失败）的后端各建一次会话读模型。Claude Code 的模型是固定别名，不用读。
    pub(super) fn discover_brain_models(&mut self, backends: Vec<Backend>, cx: &mut Context<Self>) {
        let Some(dir) = config::state_dir() else {
            return;
        };
        for backend in backends {
            let kind = backend.kind;
            if let Some(reason) = brain::backend::unavailable_reason(kind) {
                self.brain
                    .model_lists
                    .insert(kind, ModelList::Failed(reason.into()));
                continue;
            }
            let known = matches!(
                self.brain.model_lists.get(&kind),
                Some(ModelList::Loading | ModelList::Ready(_))
            );
            if kind == BackendKind::ClaudeCode || known {
                continue;
            }
            self.brain.model_lists.insert(kind, ModelList::Loading);
            let cwd = dir.clone();
            let work = cx.background_executor().spawn(async move {
                let path = std::env::join_paths(config::search_dirs())
                    .map(|joined| joined.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let env = vec![("PATH".to_string(), path)];
                brain::discover_models(&backend, cwd, env).map_err(|error| error.to_string())
            });
            cx.spawn(async move |this, cx| {
                let result = work.await;
                let _ = this.update(cx, |app, cx| {
                    let list = match result {
                        Ok(models) => ModelList::Ready(models),
                        Err(reason) => ModelList::Failed(reason),
                    };
                    app.brain.model_lists.insert(kind, list);
                    cx.notify();
                });
            })
            .detach();
        }
    }
}
