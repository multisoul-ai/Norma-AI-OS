use super::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub(crate) fn refresh_orchestration_authorizations(&mut self, cx: &mut Context<Self>) {
        let path = match storage::default_app_db_path() {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(%error, "failed to resolve orchestration authorization database");
                return;
            }
        };
        let work = cx
            .background_executor()
            .spawn(async move { storage::OrchestrationAuthorizationStore::new(path).list() });
        cx.spawn(async move |this, cx| {
            let records = work.await;
            let _ = this.update(cx, |app, cx| {
                match records {
                    Ok(records) => {
                        app.orchestration_authorizations =
                            storage::OrchestrationAuthorizationCache::new(records);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "failed to refresh orchestration authorizations");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
