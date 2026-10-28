use super::{
    bridge::DelegatedWork,
    durable::LiveDurableAdapter,
    report::{DelegatedReport, PlannerFailure},
};
use crate::app::{
    agent_runtime::{start_gpt_live_adapter, LiveAdapterScope},
    MultisoulApp,
};
use gpui::{Context, Window};
use std::sync::Arc;

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_question_delegation_tests.rs"]
mod question_tests;

pub(super) struct DelegatedCompletion {
    pub work: DelegatedWork,
    pub report: DelegatedReport,
}

pub(super) fn start(
    app: &mut MultisoulApp,
    mut work: DelegatedWork,
    scope: LiveAdapterScope,
    finished: async_channel::Sender<DelegatedCompletion>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    if work.snapshot.history_truncated {
        let _ = finished.try_send(DelegatedCompletion {
            work,
            report: DelegatedReport::default(),
        });
        return;
    }
    if let Some(report) = super::question::delegate(app, &work, &scope) {
        work.question_delegated = report.effects;
        let _ = finished.try_send(DelegatedCompletion { work, report });
        cx.notify();
        return;
    }
    let runtime = super::super::shared_tokio_runtime();
    let credentials = app.voice_settings.credential_store().zip(
        app.voice_settings
            .values()
            .gpt_live_profile()
            .credential_ref()
            .map(str::to_owned),
    );
    let (credentials, runtime) = match (credentials, runtime) {
        (Some(credentials), Ok(runtime)) => (credentials, runtime),
        (credentials, _) => {
            let failure = if credentials.is_none() {
                PlannerFailure::Configuration
            } else {
                PlannerFailure::Runtime
            };
            let _ = finished.try_send(DelegatedCompletion {
                work,
                report: DelegatedReport {
                    planner: Err(failure),
                    ..Default::default()
                },
            });
            return;
        }
    };
    let project = scope.project.clone();
    let delegate = start_gpt_live_adapter(scope, app.database_paths.clone(), window, cx);
    let shared = match app.shared_context_service() {
        Ok(shared) => shared,
        Err(_) => {
            let _ = finished.try_send(DelegatedCompletion {
                work,
                report: DelegatedReport {
                    planner: Err(PlannerFailure::Runtime),
                    ..Default::default()
                },
            });
            return;
        }
    };
    let delegate = crate::app::shared_context::adapter::wrap(delegate, shared);
    let durable = Arc::new(LiveDurableAdapter::new(
        delegate,
        app.database_paths.app_db_path().to_path_buf(),
        project.clone(),
        work.intent.clone(),
        format!("live-owner-{}", uuid::Uuid::new_v4()),
    ));
    let canvas = app.current_canvas_context_for_provider();
    let roster = app.agent_roster_for_provider();
    runtime.spawn(async move {
        let config = tokio::task::spawn_blocking(move || {
            super::backend::load_config(&credentials.0, &credentials.1)
        })
        .await;
        let config = match config {
            Ok(Ok(config)) => config,
            _ => {
                let _ = finished
                    .send(DelegatedCompletion {
                        work,
                        report: DelegatedReport {
                            planner: Err(PlannerFailure::Configuration),
                            ..Default::default()
                        },
                    })
                    .await;
                return;
            }
        };
        let planner = match agent::NormaSession::new(project, &config, durable.clone()) {
            Ok(mut session) => {
                if let Some(canvas) = canvas {
                    session.update_environment(canvas, roster);
                }
                let conversation: String = work
                    .snapshot
                    .conversation
                    .iter()
                    .map(|part| format!("{:?}: {}\n", part.speaker, part.delta))
                    .collect::<String>()
                    .chars()
                    .take(16000)
                    .collect();
                session
                    .turn_delegated(agent::NormaDelegation {
                        request_id: work.intent.clone(),
                        revision: work.snapshot.revision,
                        user_input: work.snapshot.user_input.clone(),
                        conversation,
                        cancellation: work.cancellation.clone(),
                    })
                    .await
                    .map(|result| Some(result.final_text))
                    .map_err(PlannerFailure::from)
            }
            Err(error) => Err(PlannerFailure::from(error)),
        };
        // The durable adapter's detached execution owns final recording after planner cancellation.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while durable.is_pending() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        let (pending, receipts) = durable.completion_snapshot();
        let _ = finished
            .send(DelegatedCompletion {
                work,
                report: DelegatedReport {
                    receipts,
                    pending,
                    effects: durable.has_effects(),
                    planner,
                },
            })
            .await;
    });
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_dispatch_integration_tests.rs"]
mod tests;
