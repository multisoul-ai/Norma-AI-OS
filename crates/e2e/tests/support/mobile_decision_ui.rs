use app::{
    LiveApprovalRequest, LiveCommandRequest, LiveConversationRequest, LiveSnapshotRequest,
    LiveTerminalRequest, MobileCommand, MobileSources,
};
use norma::mobile::{ApprovalSnapshot, CardSnapshot};
use serde_json::json;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

/// Explicit UI-only provider data; commands still use the real TLS, grant, lease and receipt path.
pub struct Fixture {
    approvals: async_channel::Receiver<LiveApprovalRequest>,
    commands: async_channel::Receiver<LiveCommandRequest>,
    pending: Vec<ApprovalSnapshot>,
    cards: Vec<CardSnapshot>,
    conversations: async_channel::Receiver<LiveConversationRequest>,
    terminals: async_channel::Receiver<LiveTerminalRequest>,
    received: Vec<serde_json::Value>,
    change_at: Option<Instant>,
    changed: bool,
}

impl Fixture {
    pub fn sources(
        snapshots: async_channel::Sender<LiveSnapshotRequest>,
    ) -> (MobileSources, Option<Self>) {
        let mut sources = MobileSources {
            snapshots: Some(snapshots),
            ..Default::default()
        };
        if std::env::var_os("NORMA_DECISION_UI").is_none() {
            return (sources, None);
        }
        // 8 matches the existing bounded snapshot source; never creates a Provider process.
        let (approvals, ar) = async_channel::bounded(8);
        let (commands, cr) = async_channel::bounded(8);
        sources.approvals = Some(approvals);
        sources.commands = Some(commands);
        let (history, conversations) = async_channel::bounded(8);
        sources.conversations = Some(history);
        let (terminal, terminals) = async_channel::bounded(8);
        sources.terminals = Some(terminal);
        let pending = vec![
            approval(7, "request-first", "检查登录测试", "Bash"),
            approval(8, "request-second", "核对连接路径", "Bash"),
            approval(9, "request-desktop", "电脑扩展确认", "native_terminal"),
        ];
        let cards = pending
            .iter()
            .map(|p| CardSnapshot {
                node_id: p.target.node_id,
                profile: p.target.profile.clone(),
                title: p.card_title.clone(),
                mobile_control: false,
                can_interrupt: Some(false),
                task_timing: None,
                task_progress: None,
                task_summary: None,
                state: "waiting_approval".into(),
                run_id: Some(p.target.run_id.clone()),
                session_id: Some(p.target.session_id.clone()),
                turn_id: Some(p.target.turn_id.clone()),
                task_title: Some("核对原会话".into()),
            })
            .collect();
        (
            sources,
            Some(Self {
                approvals: ar,
                commands: cr,
                pending,
                cards,
                conversations,
                terminals,
                received: vec![],
                change_at: None,
                changed: false,
            }),
        )
    }

    pub fn cards(&self, dir: &Path) -> Vec<CardSnapshot> {
        let mut cards = self.cards.clone();
        if dir.join("ui-chat-interactions").exists() {
            cards[0].mobile_control = true;
            cards[0].state = if self.received.is_empty() {
                "idle"
            } else {
                "thinking"
            }
            .into();
        }
        if dir.join("ui-chat-live").exists() {
            cards[0].state = if dir.join("ui-chat-final").exists() {
                "done"
            } else {
                "thinking"
            }
            .into();
        }
        if dir.join("ui-decision-session-replaced").exists() {
            cards[0].run_id = Some("replacement-run".into());
            cards[0].session_id = Some("replacement-session".into());
        }
        cards
    }

    pub fn advance(&mut self, dir: &Path, workspace: &str) {
        let enabled = dir.join("ui-decision-deck").exists();
        while let Ok(call) = self.terminals.try_recv() {
            if !dir.join("ui-chat-live").exists() {
                let _ = call.reply.send(Err("terminal_unavailable"));
                continue;
            }
            assert_eq!(
                call.request.workspace_id, workspace,
                "实时输出只属于隔离项目"
            );
            assert_eq!(call.request.node_id, 7, "实时输出不能混入其他卡");
            assert_eq!(call.request.run_id, "run-7", "不能显示旧运行的屏幕");
            let changed = dir.join("ui-chat-live-delta").exists();
            let mut rows = vec![String::new(); 24];
            rows[23] = if changed {
                "正在写第二句，还未写入对话记录"
            } else {
                "正在写第一句，还未写入对话记录"
            }
            .into();
            let _ = call.reply.send(Ok(norma::mobile::TerminalPage {
                card: self.cards(dir)[0].clone(),
                revision: if changed { 2 } else { 1 },
                screen_signature: 1,
                input_sequence: 0,
                start_line: 0,
                total_lines: 24,
                cols: 80,
                running: true,
                input_ready: false,
                rows,
            }));
        }
        if enabled && !self.changed && dir.join("ui-decision-change").exists() {
            // Change after 1s so the native drag is already held; normal feed heartbeat is 2s.
            let at = *self
                .change_at
                .get_or_insert_with(|| Instant::now() + Duration::from_secs(1));
            if Instant::now() >= at {
                self.pending[1].details = json!({"command":"printf changed-input"});
                self.changed = true;
                fs::write(dir.join("ui-decision-changed"), b"").expect("记录输入变化");
            }
        }
        while let Ok(request) = self.approvals.try_recv() {
            assert_eq!(
                request.request.workspace_id, workspace,
                "不能读取隔离项目之外的审批"
            );
            assert_eq!(
                request.request.route_id, "notification-route",
                "只能读取当前窗口"
            );
            let _ = request.reply.send(if enabled {
                Ok(self.pending.clone())
            } else {
                Err("unavailable")
            });
        }
        while let Ok(call) = self.conversations.try_recv() {
            assert_eq!(call.request.workspace_id, workspace, "详情只能读取原项目");
            assert_eq!(
                call.request.route_id, "notification-route",
                "详情不能读取替换窗口"
            );
            assert!(call.archive.is_none(), "详情必须复用原实时对话");
            // The shared notification fixture also owns node1. It deliberately has no Provider history.
            // This lets root-page navigation exercise a real unavailable response without inventing content.
            if call.request.node_id == 1 {
                assert_eq!(call.request.run_id, "notification-run", "基础卡不能跨运行");
                assert_eq!(
                    call.request.session_id, "notification-session",
                    "基础卡不能跨会话"
                );
                let _ = call.reply.send(Err("unavailable"));
                continue;
            }
            let card = self
                .cards
                .iter()
                .find(|c| c.node_id == call.request.node_id)
                .expect("只能请求原审批卡");
            assert_eq!(
                card.run_id.as_ref(),
                Some(&call.request.run_id),
                "不能读取另一运行"
            );
            assert_eq!(
                card.session_id.as_ref(),
                Some(&call.request.session_id),
                "不能读取另一会话"
            );
            let mut items = vec![json!({"id":"original-reply","role":"assistant",
                "text":format!("来自原会话 {} 的完整回复", call.request.session_id),"details":null})];
            if dir.join("ui-tool-cards").exists() {
                items.extend(tool_records(dir));
            }
            if dir.join("ui-chat-interactions").exists() {
                items = chat_records(dir, self.received.last());
            }
            let page = serde_json::from_value(json!({"items":items,
                "cursor":"fixture-current", "has_more":false, "pending_tail":false,"direction":"newer"})).expect("准确会话页");
            let _ = call.reply.send(Ok(page));
        }
        while let Ok(call) = self.commands.try_recv() {
            assert!(enabled, "未显式启用时不能产生审批决定");
            assert_eq!(call.request.workspace_id, workspace, "不能跨项目决策");
            assert_eq!(
                call.request.route_id, "notification-route",
                "不能给替换窗口发决定"
            );
            assert!(call.lease.begin(), "必须先获得正式执行租约");
            if dir.join("ui-chat-interactions").exists() {
                let MobileCommand::SendMessage { expected, text } = &call.request.command else {
                    panic!("聊天验收只能提交发送消息");
                };
                assert_eq!(expected.node_id, 7, "只能向指定聊天卡发送");
                assert_eq!(expected.run_id.as_deref(), Some("run-7"), "不能跨运行发送");
                assert_eq!(
                    expected.session_id.as_deref(),
                    Some("session-7"),
                    "不能跨会话发送"
                );
                self.received
                    .push(json!({"text":text,"operation_id":call.request.operation_id}));
                fs::write(
                    dir.join("ui-chat-sent.json"),
                    serde_json::to_vec(&self.received).expect("消息记录编码"),
                )
                .expect("保留正式收到的测试消息");
                let _ = call.reply.send(Ok(json!({"node_id":7,"run_id":"run-7","session_id":"session-7","turn_id":"chat-turn"})));
                continue;
            }
            let MobileCommand::DecideApproval { expected, decision } = &call.request.command else {
                panic!("手势只能提交decide_approval");
            };
            let index = self
                .pending
                .iter()
                .position(|p| p.target == *expected)
                .expect("决定必须匹配尚未处理的完整目标");
            assert_ne!(
                self.pending[index].tool, "native_terminal",
                "电脑审批不能从手机执行"
            );
            let choice = serde_json::to_value(decision).expect("决定");
            self.received
                .push(json!({"request":expected.request_id,"decision":choice}));
            fs::write(
                dir.join("ui-decisions.json"),
                serde_json::to_vec(&self.received).expect("记录编码"),
            )
            .expect("记录真实收到的命令");
            self.pending.remove(index);
            let _ = call.reply.send(Ok(
                json!({"node_id":expected.node_id,"run_id":expected.run_id,"session_id":expected.session_id,"turn_id":expected.turn_id,"request_id":expected.request_id,"decision":choice,"status":"submitted"}),
            ));
        }
    }
}

fn chat_records(dir: &Path, sent: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    // 24条独立历史使内容明显超过一屏；每条约4行，不能靠输入框上方偶然可见通过滚动验收。
    let mut items: Vec<_> = (0..24).map(|index| json!({"id":format!("history-{index}"),"role":"assistant",
        "text":format!("历史消息 {index}\n核对连接路径。\n保留已确认的结果。\n下一步由用户决定。"),"details":null})).collect();
    items
        .push(json!({"id":"initial-tail","role":"assistant","text":"初始历史末条","details":null}));
    if dir.join("ui-chat-final").exists() {
        items.push(
            json!({"id":"final-reply","role":"assistant","text":"最终整理好的回复","details":null}),
        );
    }
    if let Some(sent) = sent {
        items.push(json!({"id":"sent-user","role":"user","text":sent["text"],"details":null}));
        let text = if dir.join("ui-chat-more").exists() {
            format!(
                "实时回复第一段\n{}继续生成的末段",
                "正在生成的正文\n".repeat(64)
            )
        } else if dir.join("ui-chat-growth").exists() {
            format!(
                "实时回复第一段\n{}实时回复最新部分",
                "正在生成的正文\n".repeat(32)
            )
        } else {
            "实时回复第一段".into()
        };
        items.push(json!({"id":"streaming-reply","role":"assistant","text":text,"details":null}));
    }
    items
}

fn tool_records(dir: &Path) -> Vec<serde_json::Value> {
    let failed = dir.join("ui-tool-failed").exists();
    vec![
        json!({"id":"tool-command","role":"tool","text":"","details":json!({
            "type":"commandExecution","id":"cmd-1","command":"swift test","cwd":"/isolated/project",
            "status":if failed { "failed" } else { "inProgress" },"exitCode":if failed { Some(7) } else { None },
            "aggregatedOutput":format!("工具输出开始\n{}工具输出结束", "完整输出\n".repeat(700))
        }).to_string()}),
        json!({"id":"tool-read","role":"assistant","text":"正在查看文件。","details":json!([
            {"type":"tool_use","id":"read-1","name":"Read","input":{"file_path":"/isolated/project/README.md"}}
        ]).to_string()}),
        json!({"id":"tool-result","role":"tool","text":"","details":json!([
            {"type":"tool_result","tool_use_id":"read-1","is_error":false,"content":"原文件内容"}
        ]).to_string()}),
    ]
}

fn approval(node: u64, request: &str, title: &str, tool: &str) -> ApprovalSnapshot {
    serde_json::from_value(json!({
        "target":{"node_id":node,"profile":"Claude","run_id":format!("run-{node}"),"session_id":format!("session-{node}"),"turn_id":format!("turn-{node}"),"request_id":request},
        "card_title":title, "tool":tool, "details":{"command":"printf exact-input"},
        "decisions":if tool == "native_terminal" { vec![] } else { vec!["allow", "deny"] }
    })).expect("有效的合成请求")
}
