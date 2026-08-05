use super::controller::inspect_integrations;
use crate::app::MultisoulApp;
use gpui::{App, Global, WeakEntity};
use norma::orchestration::{AggregateStatus, ProviderIntegration};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SharedHealthPhase {
    #[default]
    Checking,
    Ready,
    Degraded,
}

#[derive(Clone)]
pub(crate) struct SharedHealthSnapshot {
    integrations: Vec<ProviderIntegration>,
    authorizations: Vec<storage::OrchestrationAuthorizationRecord>,
}

#[derive(Default)]
pub(crate) struct SharedOrchestrationHealth {
    phase: SharedHealthPhase,
    refresh_in_flight: bool,
    refresh_generation: u64,
    snapshot: Option<SharedHealthSnapshot>,
    subscribers: Vec<WeakEntity<MultisoulApp>>,
    last_error: Option<String>,
}

impl Global for SharedOrchestrationHealth {}

impl SharedOrchestrationHealth {
    pub(crate) fn begin_refresh(&mut self) -> Option<u64> {
        if self.refresh_in_flight {
            return None;
        }
        self.refresh_generation = self.refresh_generation.saturating_add(1);
        self.refresh_in_flight = true;
        self.phase = SharedHealthPhase::Checking;
        Some(self.refresh_generation)
    }

    fn begin_publish(&mut self) -> u64 {
        self.refresh_generation = self.refresh_generation.saturating_add(1);
        self.refresh_in_flight = false;
        self.refresh_generation
    }

    pub(crate) fn phase(&self) -> SharedHealthPhase {
        self.phase
    }

    fn finish(
        &mut self,
        generation: u64,
        result: Result<
            (
                Vec<ProviderIntegration>,
                Vec<storage::OrchestrationAuthorizationRecord>,
            ),
            String,
        >,
    ) -> Option<SharedHealthSnapshot> {
        if generation != self.refresh_generation {
            return None;
        }
        self.refresh_in_flight = false;
        match result {
            Ok((integrations, authorizations)) => {
                self.phase = phase_for_integrations(&integrations);
                self.last_error = None;
                let snapshot = SharedHealthSnapshot {
                    integrations,
                    authorizations,
                };
                self.snapshot = Some(snapshot.clone());
                Some(snapshot)
            }
            Err(error) => {
                self.phase = SharedHealthPhase::Degraded;
                self.last_error = Some(error);
                None
            }
        }
    }
}

pub(crate) fn install(cx: &mut App) {
    if !cx.has_global::<SharedOrchestrationHealth>() {
        cx.set_global(SharedOrchestrationHealth::default());
    }
}

#[cfg(not(test))]
pub(crate) fn register_window(
    app: WeakEntity<MultisoulApp>,
    cx: &mut App,
) -> Option<SharedHealthSnapshot> {
    install(cx);
    let snapshot = {
        let health = cx.global_mut::<SharedOrchestrationHealth>();
        health.subscribers.push(app);
        health.snapshot.clone()
    };
    snapshot
}

pub(crate) fn refresh(
    database_path: PathBuf,
    install_paths: Option<norma::orchestration::InstallPaths>,
    cx: &mut App,
) {
    install(cx);
    let Some(generation) = cx.global_mut::<SharedOrchestrationHealth>().begin_refresh() else {
        return;
    };
    let work = cx
        .background_executor()
        .spawn(async move { inspect_integrations(database_path, install_paths) });
    cx.spawn(async move |cx| {
        let result = work.await;
        let _ = cx.update(|cx| finish_refresh(generation, result, cx));
    })
    .detach();
}

pub(crate) fn apply_snapshot(app: &mut MultisoulApp, snapshot: SharedHealthSnapshot) {
    app.orchestration_settings
        .apply_shared_integrations(snapshot.integrations);
    app.orchestration_authorizations =
        storage::OrchestrationAuthorizationCache::new(snapshot.authorizations);
}

pub(crate) fn publish(
    integrations: Vec<ProviderIntegration>,
    authorizations: Vec<storage::OrchestrationAuthorizationRecord>,
    cx: &mut App,
) {
    install(cx);
    let generation = cx.global_mut::<SharedOrchestrationHealth>().begin_publish();
    finish_refresh(generation, Ok((integrations, authorizations)), cx);
}

pub(crate) fn is_checking(cx: &App) -> bool {
    cx.try_global::<SharedOrchestrationHealth>()
        .is_none_or(|health| health.phase == SharedHealthPhase::Checking)
}

pub(crate) fn provider_health(cx: &App) -> &'static str {
    match cx
        .try_global::<SharedOrchestrationHealth>()
        .map(SharedOrchestrationHealth::phase)
        .unwrap_or(SharedHealthPhase::Checking)
    {
        SharedHealthPhase::Checking => "checking",
        SharedHealthPhase::Ready => "ready",
        SharedHealthPhase::Degraded => "degraded",
    }
}

pub(crate) fn skill_health(cx: &App) -> &'static str {
    let Some(health) = cx.try_global::<SharedOrchestrationHealth>() else {
        return "checking";
    };
    let Some(snapshot) = health.snapshot.as_ref() else {
        return provider_health(cx);
    };
    if snapshot.integrations.iter().all(|integration| {
        integration.aggregate_status == AggregateStatus::Unsupported
            || matches!(
                integration.skill_component,
                norma::orchestration::ComponentState::Ready { .. }
            )
    }) {
        "ready"
    } else {
        "degraded"
    }
}

pub(crate) fn phase_for_integrations(integrations: &[ProviderIntegration]) -> SharedHealthPhase {
    if !integrations.is_empty()
        && integrations.iter().all(|integration| {
            matches!(
                integration.aggregate_status,
                AggregateStatus::Ready | AggregateStatus::Unsupported
            )
        })
    {
        SharedHealthPhase::Ready
    } else {
        SharedHealthPhase::Degraded
    }
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn mark_ready_for_e2e(cx: &mut App) {
    install(cx);
    let health = cx.global_mut::<SharedOrchestrationHealth>();
    health.begin_publish();
    health.phase = SharedHealthPhase::Ready;
    health.last_error = None;
}

fn finish_refresh(
    generation: u64,
    result: Result<
        (
            Vec<ProviderIntegration>,
            Vec<storage::OrchestrationAuthorizationRecord>,
        ),
        String,
    >,
    cx: &mut App,
) {
    let (snapshot, subscribers) = {
        let health = cx.global_mut::<SharedOrchestrationHealth>();
        let snapshot = health.finish(generation, result);
        (snapshot, health.subscribers.clone())
    };
    let Some(snapshot) = snapshot else {
        return;
    };
    for subscriber in subscribers {
        let snapshot = snapshot.clone();
        let _ = subscriber.update(cx, |app, cx| {
            apply_snapshot(app, snapshot);
            cx.notify();
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration_health_tests.rs"]
mod tests;
