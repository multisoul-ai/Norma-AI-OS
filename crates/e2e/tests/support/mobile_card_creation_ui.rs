use app::{LiveCommandRequest, MobileCommand, MobileProvider, MobileSources};
use norma::mobile::CardSnapshot;
use serde_json::json;
use std::{fs, path::Path};

/// UI fixture uses real Runtime metadata/SQLite; no Provider process is needed to select an Agent.
pub struct Fixture {
    commands: async_channel::Receiver<LiveCommandRequest>,
    runtime: runtime::Runtime,
    store: storage::CanvasStore,
    requests: Vec<serde_json::Value>,
}
impl Fixture {
    pub fn sources(mut sources: MobileSources, dir: &Path) -> (MobileSources, Option<Self>) {
        if std::env::var_os("NORMA_CARD_CREATION_UI").is_none() {
            return (sources, None);
        }
        fs::write(dir.join("ui-card-creation"), b"")
            .expect("显式创建验收必须向原生测试声明fixture能力");
        assert!(
            sources.commands.is_none(),
            "创建与审批fixture不可混用命令源"
        );
        let (tx, rx) = async_channel::bounded(8); // 同现有快照源容量，仅供隔离测试。
        sources.commands = Some(tx);
        (
            sources,
            Some(Self {
                commands: rx,
                runtime: runtime::Runtime::new_for_workspace(dir),
                store: storage::CanvasStore::new(dir.join("canvas.sqlite")),
                requests: vec![],
            }),
        )
    }
    pub fn cards(&self, dir: &Path) -> Vec<CardSnapshot> {
        self.runtime
            .mobile_card_snapshots()
            .into_iter()
            .map(|mut card| {
                // 基础通知fixture保留node1/2；偏移100避免合成投影与真实Runtime局部ID重叠。
                card.node_id += 100;
                if card.profile == "Cursor" && dir.join("ui-cursor-busy").exists() {
                    // 合成运行态只验证能力驱动的UI显隐；真实Cursor Hook身份另由Rust集成测试覆盖。
                    card.state = "thinking".into();
                    card.run_id = Some("cursor-ui-run".into());
                    card.session_id = Some("cursor-ui-session".into());
                    card.turn_id = Some("cursor-ui-turn".into());
                    card.can_interrupt = Some(false);
                }
                card
            })
            .collect()
    }
    pub fn advance(&mut self, dir: &Path, workspace: &str) {
        while let Ok(call) = self.commands.try_recv() {
            assert_eq!(
                call.request.workspace_id, workspace,
                "创建必须绑定当前隔离项目"
            );
            assert_eq!(
                call.request.route_id, "notification-route",
                "创建不得跨窗口"
            );
            assert!(call.lease.begin(), "正式执行租约必须先确认");
            let MobileCommand::CreateCard {
                profile,
                title,
                definition_id,
            } = &call.request.command
            else {
                panic!("选择Agent不能发送任务或其他指令");
            };
            let reject = dir.join("ui-create-reject");
            if reject.exists() {
                fs::remove_file(reject).expect("仅拒绝这一次隔离创建");
                let _ = call.reply.send(Err("route_expired"));
                continue;
            }
            let agent = match profile {
                MobileProvider::Codex => canvas::AgentProfile::Codex,
                MobileProvider::Claude => canvas::AgentProfile::Claude,
                MobileProvider::Cursor => canvas::AgentProfile::Cursor,
            };
            // 640×400为现有空卡默认画布占位，UI选择不会初始化Provider或发送目标。
            let node = self
                .runtime
                .create_mobile_agent_card(
                    agent,
                    *definition_id,
                    title.clone(),
                    canvas::CanvasRect::new(0., 0., 640., 400.),
                )
                .expect("创建Runtime空卡");
            self.runtime
                .save_canvas_snapshot(&self.store)
                .expect("正式SQLite持久化");
            assert_eq!(self.runtime.live_len(), 0, "仅选择类型不能派发任务");
            let card = self
                .runtime
                .mobile_card_snapshots()
                .into_iter()
                .find(|c| c.node_id == node)
                .expect("创建后的准确卡片");
            if let Some(title) = title {
                assert_eq!(&card.title, title, "可选名称必须保存在原卡上");
            }
            assert!(card.task_title.is_none(), "空卡不能自动安排目标");
            assert!(card.session_id.is_none(), "空卡不能伪造执行会话");
            self.requests
                .push(json!({"profile":profile,"title":title,"definition_id":definition_id,"node_id":node+100}));
            fs::write(
                dir.join("ui-created-cards.json"),
                serde_json::to_vec(&self.requests).expect("编码"),
            )
            .expect("保存验收证据");
            let _ = call.reply.send(Ok(json!({"node_id":node+100})));
        }
    }
}
