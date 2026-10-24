//! 适配器检测、目标加载和服务创建都在后台，过期启动结果不能重启已关闭的幕僚。

use super::{config, BrainHost, MultisoulApp, START_RETRY};
use brain::{BrainConfig, BrainService};
use gpui::Context;
use std::time::Instant;

impl MultisoulApp {
    pub(super) fn try_start_brain(&mut self, cx: &mut Context<Self>) {
        if self.brain.starting.is_some()
            || self.brain.proposals_path.is_none()
            || self.brain.proposals_loading
            || Instant::now() < self.brain.next_start
            || !self.brain.settings.as_ref().is_some_and(|s| s.enabled)
        {
            return;
        }
        let (Some(root), Some(workspace_id), Some(route_id)) = (
            self.runtime.workspace_root().map(|root| root.to_path_buf()),
            self.workspace_id.clone(),
            self.workspace_route_id.clone(),
        ) else {
            return;
        };
        self.brain.next_start = Instant::now() + START_RETRY;
        let revision = self.brain.start_generation;
        self.brain.starting = Some(revision);
        let settings = self.brain.settings.clone().expect("设置已加载");
        let identity = workspace_id.clone();
        let work = cx.background_executor().spawn(async move {
            let backend = config::pick_backend(&settings)
                .ok_or_else(|| "没有可用且满足权限隔离要求的 ACP 后端".to_string())?;
            let state_dir = config::state_dir().ok_or_else(|| "找不到幕僚状态目录".to_string())?;
            let paths = norma::orchestration::InstallPaths::for_current_process()
                .map_err(|e| e.to_string())?;
            let token = uuid::Uuid::new_v4().simple().to_string();
            let models = settings.models_for(backend.kind);
            let mut config = BrainConfig::new(
                backend,
                paths.bundled_cli,
                root,
                config::session_dir(&state_dir, &workspace_id),
            );
            config.strategy = settings.strategy;
            config.main_model = models.main;
            config.approval_model = models.approval;
            config.approval_workers = if settings.parallel_approvals { 3 } else { 1 };
            config.router.summary_every = settings.summary.duration();
            config.router.quiet_window = config::quiet_window();
            config.env = config::adapter_env(&token, &workspace_id, &route_id);
            let (service, status) = BrainService::start(config);
            Ok::<_, String>(BrainHost {
                service: Some(service),
                status,
                token,
            })
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.complete_brain_start(revision, &identity, result, cx);
            });
        })
        .detach();
    }

    pub(super) fn complete_brain_start(
        &mut self,
        generation: u64,
        identity: &str,
        result: Result<BrainHost, String>,
        cx: &mut Context<Self>,
    ) {
        if self.brain.starting != Some(generation) {
            return;
        }
        self.brain.starting = None;
        if self.brain.start_generation != generation
            || self.workspace_id.as_deref() != Some(identity)
        {
            return;
        }
        match result {
            Ok(host) => {
                self.brain.tracked.clear();
                self.brain.host = Some(host);
                let pending: Vec<_> = self.brain.pending_events.borrow_mut().drain(..).collect();
                for event in pending {
                    self.push_brain(event);
                }
                tracing::info!("Norma 大脑已启动");
            }
            Err(reason) => tracing::warn!(%reason, "幕僚未启动"),
        }
        cx.notify();
    }
}
