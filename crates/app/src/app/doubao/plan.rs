use super::{playback::start_playback, ContextRequests, DoubaoStatus};
use crate::app::voice_window_inset::VoiceWindowInsetEventSink;
use agent::NormaToolRuntime;
use async_channel::{Receiver, Sender};
use reqwest::Client;
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

pub(super) mod asr;
pub(super) mod chat;
mod conversation;
mod tts;

#[cfg(test)]
#[path = "../../../tests/unit/doubao_hold_real_pipeline_tests.rs"]
mod hold_real_tests;

fn plan_url(env: &str, production: &str) -> String {
    #[cfg(feature = "e2e-test-support")]
    {
        std::env::var(env).unwrap_or_else(|_| production.into())
    }
    #[cfg(not(feature = "e2e-test-support"))]
    {
        let _ = env;
        production.into()
    }
}

fn request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(super) async fn run_session(
    (account, voice): (String, &'static ::tts::providers::doubao::DoubaoVoice),
    context: ContextRequests,
    project_id: Option<String>,
    database: PathBuf,
    tools: Option<NormaToolRuntime>,
    cancellation: CancellationToken,
    (events, reports, playback_events, input): (
        Sender<DoubaoStatus>,
        Receiver<String>,
        VoiceWindowInsetEventSink,
        super::input::SessionInput,
    ),
) -> Result<(), LocalizedText> {
    let credential_store = storage::SettingsStore::new(database.clone());
    let key = tokio::task::spawn_blocking(move || {
        crate::settings::voice::VoiceSettingsState::load_credential(&credential_store, &account)
    })
    .await
    .map_err(|_| LocalizedText::new("doubao.credential_unavailable"))??
    .ok_or_else(|| LocalizedText::new("doubao.credential_missing"))?;
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| LocalizedText::new("doubao.client_unavailable"))?;
    let playback = Arc::new(start_playback(playback_events));
    let chat = Arc::new(chat::ChatRuntime {
        client,
        key: key.clone(),
        voice,
        context,
        history: Default::default(),
        memory_epoch: Default::default(),
        tools: tools.map(Arc::new),
        project: project_id,
        database,
        session: request_id(),
        playback: Arc::clone(&playback),
    });
    let speech_tx = input.events;
    let speech_rx = input.speech;
    let asr_cancellation = cancellation.clone();
    let asr_task = tokio::spawn(async move {
        if let Some(recordings) = input.recordings {
            asr::listen_recordings(key, speech_tx, asr_cancellation, recordings).await
        } else {
            asr::run(key, speech_tx, asr_cancellation, None).await
        }
    });
    let status = events.clone();
    conversation::run(
        move |reply, token| {
            let chat = Arc::clone(&chat);
            let status = status.clone();
            async move {
                match reply {
                    conversation::Reply::User { transcript, turn } => {
                        chat.respond(transcript, turn, token, status).await
                    }
                    conversation::Reply::Report(report) => {
                        tts::speak(
                            &chat.client,
                            &chat.key,
                            &report,
                            chat.voice,
                            &chat.playback,
                            &token,
                        )
                        .await
                    }
                }
            }
        },
        playback,
        asr_task,
        cancellation,
        events,
        speech_rx,
        reports,
    )
    .await
}
