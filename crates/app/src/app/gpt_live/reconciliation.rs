#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_reconciliation_tests.rs"]
mod tests;
use crate::app::MultisoulApp;
use gpui::Context;
use storage::{AgentDispatchTaskStore, LiveOperationState, LiveOperationStore, StorageResult};

fn load_review(
    store: &LiveOperationStore,
    project: &str,
    tasks: Option<&AgentDispatchTaskStore>,
    cards: &[agent::CanvasCardContext],
) -> StorageResult<String> {
    let rows = store.recent(project, 20)?;
    let mut text = String::from("只读核对：没有匹配回执的操作保持待核对，不会自动重做。\n");
    if rows.is_empty() {
        text.push_str("本项目暂无语音操作记录。\n");
    }
    for row in rows {
        let status = if row.state == LiveOperationState::Recorded {
            "已落账（accepted不代表任务完成）"
        } else {
            "待核对"
        };
        text.push_str(&format!(
            "\n{status} · {} / {}\n原命令：{}\n",
            row.key.intent_id, row.key.operation_id, row.command
        ));
        if let Some(outcome) = row.outcome {
            text.push_str(&format!("原始回执：{}\n", serde_json::to_string(&outcome)?));
            if let (Ok(result), Some(tasks)) = (outcome, tasks) {
                for task in result.tasks.iter().take(3) {
                    if let Some(task_ref) = task.get("task_ref").and_then(serde_json::Value::as_str)
                    {
                        match tasks.get_by_task_ref(task_ref)? {
                            Some(task) => text.push_str(&format!(
                                "持久任务：{} · {:?} · node={}\n",
                                task.task_ref, task.state, task.node_id
                            )),
                            None => {
                                text.push_str(&format!("持久任务：{task_ref} 尚无可核对记录\n"))
                            }
                        }
                    }
                }
            }
        }
        if text.len() > 64 * 1024 {
            text.push_str("\n后续记录已省略，请缩小核对范围。\n");
            break;
        }
    }
    text.push_str("\n当前画布事实（卡片存在不能单独证明原请求已执行）：\n");
    for card in cards.iter().take(64) {
        text.push_str(&format!(
            "node={} · {:?} · {}\n",
            card.node_id, card.kind, card.title
        ));
    }
    Ok(text)
}

impl MultisoulApp {
    pub(crate) fn request_gpt_live_reconciliation(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string())
        else {
            self.gpt_live.reconciliation =
                Some((String::new(), "请先打开项目，再核对操作。".into()));
            cx.notify();
            return;
        };
        self.gpt_live.reconciliation_generation =
            self.gpt_live.reconciliation_generation.wrapping_add(1);
        let generation = self.gpt_live.reconciliation_generation;
        self.gpt_live.reconciling = true;
        let store = LiveOperationStore::new(self.database_paths.app_db_path().to_path_buf());
        let tasks = self
            .persistence
            .project_store()
            .map(|store| store.agent_dispatch_task_store());
        let cards = self
            .current_canvas_context_for_provider()
            .map(|context| context.cards)
            .unwrap_or_default();
        let queried = project.clone();
        let work = cx
            .background_executor()
            .spawn(async move { load_review(&store, &queried, tasks.as_ref(), &cards) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if app.gpt_live.reconciliation_generation != generation {
                    return;
                }
                app.gpt_live.reconciling = false;
                if app
                    .runtime
                    .workspace_root()
                    .map(|root| root.display().to_string())
                    .as_deref()
                    == Some(&project)
                {
                    let text = result.unwrap_or_else(|_| {
                        "操作核对暂不可用，请稍后重试；不会自动重做原操作。".into()
                    });
                    app.gpt_live.reconciliation = Some((project, text));
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
