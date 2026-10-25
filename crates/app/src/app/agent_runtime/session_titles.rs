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
        {
            let (node_id, event) = (event.node_id, event.clone());
            let read = cx
                .background_executor()
                .spawn(async move { session_title::session_title(&event) });
            cx.spawn(async move |this, cx| {
                let Some(title) = read.await else {
                    return;
                };
                let _ = this.update(cx, |app, cx| {
                    if app.runtime.set_agent_session_title(node_id, title) {
                        crate::mobile::project_events::changed();
                        app.note_workspace_activity_changed(cx);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}
