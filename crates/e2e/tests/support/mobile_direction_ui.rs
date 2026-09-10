use app::{LiveCommandRequest, MobileCommand, MobileSources};
use norma::mobile::CardSnapshot;
use serde_json::json;
use std::{fs, path::Path};

/// Only model inference is synthetic. Commands, scoped records, CAS publication and WSS are real.
pub struct Fixture {
    calls: async_channel::Receiver<LiveCommandRequest>,
    published: bool,
    requests: Vec<serde_json::Value>,
}
impl Fixture {
    pub fn sources(mut sources: MobileSources) -> (MobileSources, Option<Self>) {
        if std::env::var_os("NORMA_PLAN_DIRECTION_UI").is_none() {
            return (sources, None);
        }
        assert!(sources.commands.is_none(), "方向不能混用其他命令fixture");
        let (tx, calls) = async_channel::bounded(8); // 沿用服务最多8条在途命令预算。
        sources.commands = Some(tx);
        (
            sources,
            Some(Self {
                calls,
                published: false,
                requests: vec![],
            }),
        )
    }
    pub fn advance(&mut self, dir: &Path, workspace: &str, card: &CardSnapshot) {
        let store = storage::ProjectPlanStore::new(dir.join("app.sqlite"));
        if !self.published && dir.join("ui-direction-flow").exists() {
            store
                .publish(
                    "ui-direction-plan",
                    workspace,
                    "notification-route",
                    "先核对连接，再整理界面",
                    &agent::ProjectPlanDraft {
                        summary: "两项待办，等待你的方向。".into(),
                        todos: vec![
                            agent::PlanTodo {
                                target: card.clone(),
                                title: "核对连接路径".into(),
                                instruction: "核对两种连接并报告结果".into(),
                            },
                            agent::PlanTodo {
                                target: card.clone(),
                                title: "整理界面布局".into(),
                                instruction: "核对原型与手机页面".into(),
                            },
                        ],
                    },
                )
                .expect("保存两项真实计划记录");
            self.published = true;
        }
        while let Ok(call) = self.calls.try_recv() {
            assert!(call.lease.begin(), "必须经过正式控制授权租约");
            assert_eq!(call.request.workspace_id, workspace, "只允许操作隔离项目");
            assert_eq!(
                call.request.route_id, "notification-route",
                "不能发送到替换窗口"
            );
            let MobileCommand::GeneratePlan {
                direction,
                priority: Some(priority),
            } = &call.request.command
            else {
                panic!("方向只能生成带优先事项的建议，不能执行Agent任务");
            };
            let source = store
                .get(workspace, &priority.plan_id)
                .expect("读取原计划")
                .expect("必须存在");
            let selected = source
                .draft
                .todos
                .get(priority.todo_index)
                .expect("合法优先项");
            self.requests.push(json!({"direction":direction,"priority":{"plan_id":priority.plan_id,"todo_index":priority.todo_index}}));
            fs::write(
                dir.join("ui-direction-requests.json"),
                serde_json::to_vec(&self.requests).expect("记录请求"),
            )
            .expect("保存证据");
            if dir.join("ui-direction-race").exists() {
                store
                    .publish(
                        "peer-direction",
                        workspace,
                        "notification-route",
                        "另一端已调整方向",
                        &source.draft,
                    )
                    .expect("模拟真实另一端发布");
            }
            let mut draft = source.draft.clone();
            draft.prioritize(selected).expect("同生产排序校验");
            draft.todos[0].instruction =
                format!("{}；目标与取舍：{}", selected.instruction, direction);
            let reply = store
                .publish_after(&call.request.operation_id, &source, direction, &draft)
                .map(|plan| json!({"plan_id":plan.plan_id,"status":"proposed"}))
                .map_err(|_| "plan_changed");
            let _ = call.reply.send(reply);
        }
    }
}
