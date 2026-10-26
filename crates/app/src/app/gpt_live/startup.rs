use super::{
    capture::run_capture,
    session::{run_audio_loop, CallCommand, CallEnd, CallEvent, CaptureInput, CaptureMode},
};
use crate::app::voice_window_inset::VoiceWindowInsetEventSink;
use tokio_util::sync::CancellationToken;

pub(super) struct CallConfig {
    pub reports: async_channel::Receiver<super::native_report::NativeReport>,
    pub shared_context: crate::app::shared_context::SharedContext,
    pub session_id: String,
    pub input_enabled: bool,
    pub account: String,
    pub voice: String,
    pub store: storage::SettingsStore,
    pub input_device: Option<String>,
    pub canvas: Option<agent::CurrentCanvasContext>,
    pub roster: Vec<agent::AgentRosterEntry>,
    pub playback: VoiceWindowInsetEventSink,
    pub vad_root: std::path::PathBuf,
}

pub(super) async fn run_call(
    config: CallConfig,
    commands: async_channel::Receiver<CallCommand>,
    events: async_channel::Sender<CallEvent>,
    cancellation: CancellationToken,
) -> CallEnd {
    let loaded = tokio::task::spawn_blocking(move || {
        let secret = crate::settings::voice::VoiceSettingsState::load_credential(
            &config.store,
            &config.account,
        )?
        .filter(|secret| !secret.trim().is_empty())
        .ok_or("GPT-Live API key is missing".to_owned())?;
        let live_config = super::prompt::live_config(
            &config.store,
            config.canvas.as_ref(),
            &config.roster,
            &config.voice,
        )?;
        let vad = prepare_vad(&config.vad_root.join("gpt_live"));
        Ok::<_, String>((secret, live_config, vad))
    })
    .await;
    let (secret, live_config, vad) = match loaded {
        Ok(Ok(loaded)) => loaded,
        error => {
            return CallEnd {
                usage: Default::default(),
                error: Some(match error {
                    Ok(Err(error)) => error,
                    Err(error) => error.to_string(),
                    _ => unreachable!(),
                }),
            }
        }
    };
    let live_config = match super::context::prepare(
        live_config,
        &config.shared_context,
        &config.session_id,
        &cancellation,
    )
    .await
    {
        Ok(config) => config,
        Err(error) => {
            return CallEnd {
                usage: Default::default(),
                error: Some(error),
            }
        }
    };
    let vad = match vad {
        Ok(vad) => Some(vad),
        Err(error) => {
            let _ = events.try_send(CallEvent::ToolsUnavailable(format!(
                "语音检测自动准备失败，项目操作暂不可用；结束通话后重连可重试。{error}"
            )));
            None
        }
    };
    if cancellation.is_cancelled() {
        return CallEnd {
            usage: Default::default(),
            error: None,
        };
    }
    let connected = live_voice::connect(live_config, &secret, cancellation.child_token()).await;
    drop(secret);
    let connection = match connected {
        Ok(connection) => connection,
        Err(error) => {
            return CallEnd {
                usage: Default::default(),
                error: Some(error.to_string()),
            }
        }
    };
    let (desired, mode) = tokio::sync::watch::channel(CaptureMode {
        generation: 0,
        muted: !config.input_enabled,
    });
    let (captured, input) = async_channel::bounded(8);
    let capture_cancellation = cancellation.child_token();
    let capture_task = tokio::spawn(run_capture(
        config.input_device,
        mode,
        captured,
        capture_cancellation.clone(),
    ));
    let playback = crate::app::doubao::playback::start_live_playback(config.playback);
    let result = run_audio_loop(
        connection,
        CaptureInput {
            captured: input,
            capture_mode: desired,
            vad,
        },
        playback,
        commands,
        config.reports,
        events,
        cancellation,
    )
    .await;
    capture_cancellation.cancel();
    let _ = capture_task.await;
    result
}

fn prepare_vad(root: &std::path::Path) -> Result<voice_input::SherpaVadClient, String> {
    static PREPARATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _preparation = PREPARATION.lock().map_err(|_| "语音检测准备锁不可用")?;
    crate::settings::voice::ensure_silero_vad(root)?;
    voice_input::SherpaVadClient::from_vad_model_dir(&root.join(asr::SILERO_VAD_DIRECTORY))
        .map_err(|error| error.to_string())
}
