use super::plan::asr::SpeechEvent;

impl crate::app::MultisoulApp {
    /// 桌面 E2E 控制通道专用：把转写当作 ASR 最终结果送入豆包会话，之后走生产回复路径。
    /// 失败时返回具体原因，便于定位是通道没启动还是会话已退出。
    pub(crate) fn inject_doubao_transcript(
        &mut self,
        text: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        // 桌面工作空间里豆包通道属于当前展开项目的会话，而不是窗口宿主。
        let session = self
            .desktop_workspace
            .active_id
            .as_ref()
            .zip(self.desktop_workspace.expanded_area_id.as_ref())
            .and_then(|(workspace, area)| {
                self.desktop_workspace
                    .sessions
                    .get(&(workspace.clone(), area.clone()))
                    .cloned()
            });
        match session {
            Some(session) => {
                session.update(cx, |app, cx| app.inject_into_own_channel(text, window, cx))
            }
            None => self.inject_into_own_channel(text, window, cx),
        }
    }

    fn inject_into_own_channel(
        &mut self,
        text: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        if !self.start_doubao_channel(window, cx) {
            return Err(format!("channel not started: {:?}", self.doubao.status));
        }
        let speech = self
            .doubao
            .speech
            .as_ref()
            .ok_or("speech sender missing after start")?;
        speech
            .try_send(SpeechEvent::Final(text))
            .map_err(|error| format!("speech channel closed: {error}"))
    }
}
