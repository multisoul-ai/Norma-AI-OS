//! One launch for the App. The initiating project may disappear while launch runs.
use super::{config, owner, BrainHost, MultisoulApp};
use brain::{BrainConfig, BrainService};
use gpui::Context;
use std::time::Instant;

impl MultisoulApp {
    pub(super) fn try_start_brain(&mut self, cx: &mut Context<Self>) {
        let owner = owner::shared(cx);
        if owner.is_running() {
            return;
        }
        let Some(ticket) = owner.begin_start(Instant::now()) else {
            return;
        };
        let settings = owner.settings().expect("enabled owner has settings");
        let directory = owner
            .directory()
            .expect("start requires prepared global data");
        let read_roots = owner.read_roots();
        let work = cx.background_executor().spawn(async move {
            let backend =
                config::pick_backend(&settings).ok_or_else(|| "没有可用的 ACP 后端".to_string())?;
            let state_dir = config::state_dir().ok_or_else(|| "找不到幕僚状态目录".to_string())?;
            let global_dir = state_dir.join("global");
            std::fs::create_dir_all(&global_dir).map_err(|error| error.to_string())?;
            let paths = norma::orchestration::InstallPaths::for_current_process()
                .map_err(|error| error.to_string())?;
            let token = uuid::Uuid::new_v4().simple().to_string();
            let models = settings.models_for(backend.kind);
            let mut config = BrainConfig::new(
                backend,
                config::shell_safe_bin(&state_dir, paths.bundled_cli),
                global_dir,
                directory,
            );
            config.read_roots = read_roots;
            config.strategy = settings.strategy;
            config.main_model = models.main;
            config.approval_model = models.approval;
            config.approval_workers = if settings.parallel_approvals { 3 } else { 1 };
            config.router.summary_every = settings.summary.duration();
            config.router.quiet_window = config::quiet_window();
            // Global adapters must name each event's exact source on every CLI action.
            config.env = config::adapter_env(&token, "", "");
            let (service, status) = BrainService::start(config);
            Ok::<_, String>(BrainHost {
                service: Some(service),
                status,
                token,
            })
        });
        cx.spawn(async move |_, _| {
            owner.complete_start(ticket, work.await);
        })
        .detach();
    }
}
