//! 卡片标题取 CLI 自己的会话名（Claude ai-title、Codex thread_name）。
//! 文件读取在后台线程完成，前台只写入结果并刷新概览；读不到时保持首条提示词。

use crate::app::MultisoulApp;
use gpui::Context;
use norma::agent_hook::{session_title, AgentHookEvent};

impl MultisoulApp {
    /// 提交提示词和一轮结束时 CLI 才会写入或更新会话名，只在这两类事件后读取。
    pub(crate) fn refresh_session_titles(
        &mut self,
        events: &[AgentHookEvent],
        cx: &mut Context<Self>,
    ) {
        for event in events
            .iter()
            .filter(|event| session_title::should_refresh(event))
            // 工具调用很频繁：已有会话名时只在提交提示词和一轮结束时重读。
            .filter(|event| {
                event.provider_event != "PostToolUse"
                    || !self.runtime.has_agent_session_title(event.node_id)
            })
        {
            let (node_id, session, event) =
                (event.node_id, event.session_id.clone(), event.clone());
            let read = cx
                .background_executor()
                .spawn(async move { session_title::session_title(&event) });
            cx.spawn(async move |this, cx| {
                let title = read.await;
                tracing::debug!(node_id, ?title, "session title probe");
                let Some(title) = title else {
                    return;
                };
                let _ = this.update(cx, |app, cx| {
                    app.apply_session_title(node_id, session, title, cx);
                });
            })
            .detach();
        }
    }

    /// 打开项目恢复出的会话没有 hook 事件，按绑定的会话 id 读一次 CLI 的会话名。
    pub(crate) fn refresh_restored_session_titles(&mut self, cx: &mut Context<Self>) {
        for (node_id, profile, session_id) in self.runtime.take_unprobed_session_titles() {
            let profile = super::identity::norma_agent_profile(profile);
            let session = Some(session_id.clone());
            let read = cx
                .background_executor()
                .spawn(async move { session_title::session_title_by_id(profile, &session_id) });
            cx.spawn(async move |this, cx| {
                let title = read.await;
                tracing::debug!(node_id, ?title, "session title probe");
                let Some(title) = title else {
                    return;
                };
                let _ = this.update(cx, |app, cx| {
                    app.apply_session_title(node_id, session, title, cx);
                });
            })
            .detach();
        }
    }

    /// 后台读取返回时节点可能已换会话：会话 id 对不上就丢弃，避免旧会话名写到新会话上。
    fn apply_session_title(
        &mut self,
        node_id: canvas::NodeId,
        session: Option<String>,
        title: String,
        cx: &mut Context<Self>,
    ) {
        let current = self.runtime.agent_session_id(node_id);
        if matches!((&session, &current), (Some(read), Some(now)) if read != now) {
            return;
        }
        if self.runtime.set_agent_session_title(node_id, title) {
            crate::mobile::project_events::changed();
            self.note_workspace_activity_changed(cx);
            cx.notify();
        }
    }
}
