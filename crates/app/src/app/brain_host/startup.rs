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
        let work = cx.background_executor().spawn(async move {
            let backend =
                config::pick_backend(&settings).ok_or_else(|| "没有可用的 ACP 后端".to_string())?;
            let state_dir = config::state_dir().ok_or_else(|| "找不到幕僚状态目录".to_string())?;
            let global_dir = state_dir.join("global");
            std::fs::create_dir_all(&global_dir).map_err(|error| error.to_string())?;
            let paths = norma::orchestration::InstallPaths::for_current_process()
                .map_err(|error| error.to_string())?;
            let token = uuid::Uuid::new_v4().simple().to_string();
            let staff_tokens = brain::staff::gate::StaffTokens::default();
            let models = settings.models_for(backend.kind);
            let norma_bin = config::shell_safe_bin(&state_dir, paths.bundled_cli.clone());
            let skill_ready = config::sync_brain_skill(&paths, backend.kind, &norma_bin);
            let mut config = BrainConfig::new(backend, norma_bin, global_dir, directory);
            config.skill_ready = skill_ready;
            config.tasks_dir = state_dir;
            config.strategy = settings.strategy;
            config.main_model = models.main;
            config.approval_model = models.approval;
            config.approval_workers = if settings.parallel_approvals { 3 } else { 1 };
            config.router.summary_every = settings.summary.duration();
            config.router.quiet_window = config::quiet_window();
            // Global adapters must name each event's exact source on every CLI action.
            config.env = config::adapter_env(&token, "", "");
            config.staff_tokens = Some(staff_tokens.clone());
            let (service, status) = BrainService::start(config);
            Ok::<_, String>(BrainHost {
                service: Some(service),
                status,
                tokens: brain::staff::gate::Tokens::with_staff(&token, staff_tokens),
            })
        });
        cx.spawn(async move |_, _| {
            owner.complete_start(ticket, work.await);
        })
        .detach();
    }
}
