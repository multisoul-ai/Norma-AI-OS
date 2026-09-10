use app::{LiveCommandRequest, MobileCommand, MobileSources};
use norma::mobile::CardSnapshot;
use serde_json::json;
use std::{fs, path::Path, time::Duration};

/// UI-only Agent sink. Pairing, Todo decisions, queue, cancellation and receipts use production code.
pub struct Fixture {
    calls: async_channel::Receiver<LiveCommandRequest>,
    published: bool,
    sent: bool,
}

impl Fixture {
    pub fn sources(mut sources: MobileSources) -> (MobileSources, Option<Self>) {
        if std::env::var_os("NORMA_TODO_UI").is_none() {
            return (sources, None);
        }
        assert!(sources.commands.is_none(), "Todo不能混用其他UI命令源");
        let (tx, calls) = async_channel::bounded(8); // 与既有服务最多8个连接一致。
        sources.commands = Some(tx);
        (
            sources,
            Some(Self {
                calls,
                published: false,
                sent: false,
            }),
        )
    }

    pub fn advance(&mut self, dir: &Path, workspace: &str, card: &CardSnapshot) {
        if !self.published && dir.join("ui-todo-flow").exists() {
            storage::ProjectPlanStore::new(dir.join("app.sqlite"))
                .publish(
                    "ui-todo-plan",
                    workspace,
                    "notification-route",
                    "验证待办执行闭环",
                    &agent::ProjectPlanDraft {
                        summary: "先安排连接核对，再决定是否补充报告。".into(),
                        todos: vec![
                            agent::PlanTodo {
                                target: card.clone(),
                                title: "核对连接路径".into(),
                                instruction: "记录局域网与 Tailscale 使用的连接路径。".into(),
                            },
                            agent::PlanTodo {
                                target: card.clone(),
                                title: "补充连接报告".into(),
                                instruction: "这项建议将被拒绝，不得发送。".into(),
                            },
                        ],
                    },
                )
                .expect("只保存合成建议，不预写任何决定");
            self.published = true;
        }
        while let Ok(call) = self.calls.try_recv() {
            assert!(self.published, "必须先通过真实计划读取和确认");
            assert!(!self.sent, "撤回及拒绝不得发送，重排只能发送一次");
            assert!(
                !dir.join("ui-task-running").exists(),
                "Agent忙碌时不能发送队列"
            );
            assert_eq!(call.request.workspace_id, workspace, "只能安排隔离项目");
            assert_eq!(call.request.route_id, "notification-route", "不能跨窗口");
            let MobileCommand::SendMessage { expected, text } = &call.request.command else {
                panic!("必须复用正式SendMessage执行Todo");
            };
            assert_eq!(expected.node_id, card.node_id, "不能换Agent卡片");
            assert_eq!(expected.run_id, card.run_id, "不能更换运行");
            assert_eq!(expected.session_id, card.session_id, "必须使用同一会话");
            assert_eq!(
                text, "记录局域网与 Tailscale 使用的连接路径。",
                "只执行已保存的第一项原文"
            );
            assert!(call.lease.begin(), "发送必须持有有效执行租约");
            let turn = format!("todo-{}", call.request.operation_id);
            let writer = storage::BackgroundApprovalAudit::new(
                storage::ApprovalAuditStore::new(dir.join("project.sqlite")).with_turn_store(
                    storage::MobileTurnStore::for_workspace(
                        dir.join("app.sqlite"),
                        workspace.into(),
                    ),
                ),
            )
            .expect("真实后台终态写入器");
            writer
                .record_turn(storage::MobileTurnOutcome {
                    key: storage::MobileTurnKey {
                        node_id: card.node_id,
                        profile: card.profile.clone(),
                        run_id: card.run_id.clone().expect("原运行"),
                        session_id: card.session_id.clone().expect("原会话"),
                        turn_id: turn.clone(),
                    },
                    state: storage::MobileTurnState::Completed,
                })
                .expect("终态入队")
                .recv_timeout(Duration::from_secs(5)) // 5s沿用后台落盘验收预算。
                .expect("有界落盘")
                .expect("准确回合完成");
            fs::write(
                dir.join("ui-todo-sent.json"),
                json!({"text":text,"operation_id":call.request.operation_id,"turn_id":turn})
                    .to_string(),
            )
            .expect("发送证据");
            self.sent = true;
            let _ = call.reply.send(Ok(json!({"node_id":card.node_id,"run_id":card.run_id,"session_id":card.session_id,"turn_id":turn})));
        }
    }
}
