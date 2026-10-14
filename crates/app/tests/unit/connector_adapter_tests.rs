use crate::app::MultisoulApp;
use agent::{AdapterCommand, AgentInputSource};
use gpui::AppContext;
use norma::protocol::AgentProfile;

/// 场景：无窗口连接器来源禁止其它能力；使用echo PTY与临时项目。
/// 执行：内部Adapter提交禁止命令及合法建卡，再尝试旧revision派工。
/// 预期：禁止命令无节点副作用；建卡成功；revision错误零任务。
#[test]
fn connector_adapter_guards_without_window() {
    let test = gpui::TestAppContext::single();
    let dir = tempfile::tempdir().expect("临时项目");
    test.update(|cx| {
        let entity = cx.new(|cx| {
            let factory: runtime::PtyFactory =
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
            MultisoulApp::new_with_pty_factory(cx, factory)
        });
        entity.update(cx, |app, cx| {
            app.enter_canvas_for_test(dir.path(), cx);
            let before = app.runtime.document.nodes().len();
            let result = crate::app::execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                AdapterCommand::CreateTerminal,
                AgentInputSource::Connector,
            );
            assert!(result.is_err(), "连接器不能创建Terminal");
            assert_eq!(app.runtime.document.nodes().len(), before, "拒绝零副作用");
            let created = crate::app::execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Codex,
                    task: None,
                },
                AgentInputSource::Connector,
            )
            .expect("无窗口建卡");
            let node_id = created.node_id.expect("卡片身份");
            let identity = norma::connector::CardIdentity {
                workspace_id: "w".into(),
                area_id: "a".into(),
                canvas_id: "canvas".into(),
                node_id,
                definition_id: app
                    .runtime
                    .document
                    .node(node_id)
                    .and_then(|n| match &n.kind {
                        canvas::CanvasNodeKind::Agent(s) => s.definition_id,
                        _ => None,
                    })
                    .expect("定义"),
                created_at: 10,
            };
            let result = crate::app::execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                AdapterCommand::AssignAgentTask {
                    agent: "display only".into(),
                    task: "work".into(),
                    node_id: Some(node_id),
                    expected_revision: Some("old".into()),
                    target_identity: Some(identity),
                    allow_queue: true,
                },
                AgentInputSource::Connector,
            );
            assert_eq!(
                result.expect_err("旧revision拒绝"),
                agent::AdapterError::Rejected("target_changed".into()),
                "稳定拒绝"
            );
            assert!(
                app.agent_task_coordinator
                    .as_ref()
                    .is_none_or(|c| c.tasks().is_empty()),
                "校验前零任务"
            );
            app.runtime.kill_all();
        });
    });
}

struct RecordingPty(std::sync::Arc<std::sync::Mutex<Vec<Vec<u8>>>>);
impl terminal::PtyBackend for RecordingPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        self.0.lock().expect("输入记录").push(bytes.to_vec());
        Ok(())
    }
    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), terminal::PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}
/// 场景：同一定义的两张同名卡片；精确派工与回复不能选择第一张。
/// 构造：两路独立PTY记录，两个node_id、同一个definition_id；先派第二张，再回归按名派工。
/// 执行：带节点与正确revision派工/回复、取消首任务，再不带节点按名派工。
/// 预期：只第二路收到正文，回复不建任务；按名入口仍选择第一路。
#[test]
fn connector_exact_node_dispatch_reply_and_name_fallback() {
    let test = gpui::TestAppContext::single();
    let dir = tempfile::tempdir().expect("临时项目");
    let outputs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = outputs.clone();
    test.update(|cx| {
        let entity = cx.new(|cx| {
            let factory: runtime::PtyFactory = Box::new(move|_,_,_,_| {
                let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
                captured.lock().expect("PTY列表").push(output.clone());
                Ok(Box::new(RecordingPty(output)))
            });
            MultisoulApp::new_with_pty_factory(cx,factory)
        });
        entity.update(cx,|app,cx| {
            app.enter_canvas_for_test(dir.path(),cx);
            let first = crate::app::execute_norma_adapter_command_with_source(app,None,cx,AdapterCommand::CreateAgent{profile:AgentProfile::Codex,task:None},AgentInputSource::Connector).expect("创建").node_id.expect("节点");
            let kind = app.runtime.document.node(first).expect("第一卡").kind.clone();
            let second = app.runtime.document.add_node(kind,canvas::CanvasRect::new(700.0,0.0,640.0,400.0));
            app.runtime.start_native_agent_terminal(second,80,24).expect("第二路PTY");
            app.pending_agent_submissions.mark_ready(first);
            app.pending_agent_submissions.mark_ready(second);
            let definition = app.agent_definitions.definitions().iter().find(|d|matches!(&app.runtime.document.node(second).expect("第二卡").kind,canvas::CanvasNodeKind::Agent(s) if s.definition_id==Some(d.id))).expect("定义");
            let name = definition.name.clone();
            let identity = norma::connector::CardIdentity{workspace_id:"w".into(),area_id:"a".into(),canvas_id:"canvas".into(),node_id:second,definition_id:definition.id,created_at:10};
            let revision = super::live::card_revision(app,&identity);
            let sent = crate::app::execute_norma_adapter_command_with_source(app,None,cx,AdapterCommand::AssignAgentTask{agent:"同名展示字段不是地址".into(),task:"exact task".into(),node_id:Some(second),expected_revision:Some(revision.clone()),target_identity:Some(identity.clone()),allow_queue:true},AgentInputSource::Connector).expect("精确派工");
            assert_eq!(sent.node_id,Some(second),"回执精确节点");
            let streams = outputs.lock().expect("PTY列表");
            assert!(streams[0].lock().expect("首路").is_empty(),"第一卡零输入");
            assert_eq!(streams[1].lock().expect("次路").concat(),b"exact task","只有第二卡收到正文");
            drop(streams);
            let before = app.agent_task_coordinator.as_ref().expect("协调器").tasks().len();
            let reply = crate::app::execute_norma_adapter_command_with_source(app,None,cx,AdapterCommand::ReplyAgent{agent:"忽略展示名".into(),message:"reply".into(),node_id:Some(second),expected_revision:Some(revision),target_identity:Some(identity)},AgentInputSource::Connector).expect("精确回复");
            assert_eq!(reply.node_id,Some(second),"回复绑定第二卡");
            assert_eq!(app.agent_task_coordinator.as_ref().expect("协调器").tasks().len(),before,"回复不创建任务");
            crate::app::execute_norma_adapter_command_with_source(app,None,cx,AdapterCommand::CancelAgentTask{task_id:sent.task_id.expect("任务号"), expected_task_ref: None },AgentInputSource::Connector).expect("取消未提交目标");
            let legacy = crate::app::execute_norma_adapter_command_with_source(app,None,cx,AdapterCommand::AssignAgentTask{agent:name,task:"name fallback".into(),node_id:None,expected_revision:None,target_identity:None,allow_queue:true},AgentInputSource::Voice).expect("旧按名行为");
            assert_eq!(legacy.node_id,Some(first),"语音/手机旧寻址仍选第一卡");
            app.runtime.kill_all();
        });
    });
}
