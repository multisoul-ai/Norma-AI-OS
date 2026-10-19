pub(super) fn set_blocked(
    blocked: bool,
    playback: &crate::app::doubao::playback::PlaybackQueue,
    control: &live_voice::LiveControl,
    events: &async_channel::Sender<super::session::CallEvent>,
) -> Result<(), String> {
    control.set_output_blocked(blocked);
    playback.interrupt();
    control.send_command(&live_voice::LiveCommand::Append {
        event_id: uuid::Uuid::new_v4().to_string(),
        kind: live_voice::AppendKind::Instructions,
        delegation_id: None,
        content: if blocked {
            "宿主已停止本地播报并丢弃已接收的待播音频；麦克风继续工作，已接受任务继续执行。不要声称用户听到了被丢弃的语音。"
        } else {
            "宿主已恢复接收新语音；本地停止期间已接收音频已丢弃，不会补播，已接受任务继续执行。"
        }.into(),
    }).map_err(|error|error.to_string())?;
    events
        .try_send(super::session::CallEvent::OutputBlocked(blocked))
        .map_err(|_| "Live UI event queue is full or closed".into())
}
