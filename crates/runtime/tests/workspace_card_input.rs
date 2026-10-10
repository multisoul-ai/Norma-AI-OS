use canvas::{AgentProfile, CanvasRect};
use runtime::{NodeInput, Runtime};
use std::sync::{Arc, Mutex};

fn recording() -> (Runtime, u64, Arc<Mutex<Vec<Vec<u8>>>>) {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let seen = writes.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            seen.clone(),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::OpenCode,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("普通卡片应启动");
    (runtime, node, writes)
}

/// 空间任务输入必须独占卡片：接受后其他输入与旧队列不能插入正文和 Enter 之间。
/// 数据构造：OpenCode 无 definition/run；node 1，一个 lease，正文与 Enter 共 2 次写入。
/// 执行过程：捕获 target，保留输入，尝试手动输入与其他任务，再由 lease 写正文和 Enter。
/// 预期结果：竞态输入均零写入；只有本任务正文和 CR(0x0d)；发送后释放正常输入。
#[test]
fn reserved_card_input_excludes_other_writers_and_preserves_exact_target() {
    let (mut runtime, node, writes) = recording();
    let target = runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("冻结目标");
    runtime
        .reserve_workspace_card_input(&target, "assignment", "独有正文")
        .expect("独占输入");
    assert!(
        runtime
            .send_node_input(node, NodeInput::Text("用户竞态".into()))
            .is_err(),
        "普通输入不能插入保留正文"
    );
    assert!(
        runtime.send_agent_task_text(node, "旧队列竞态").is_err(),
        "旧任务入口必须互斥"
    );
    assert!(
        writes.lock().expect("记录").is_empty(),
        "拒绝竞态时 PTY 零输入"
    );
    runtime
        .paste_workspace_card_input(&target, "assignment", "独有正文")
        .expect("正文应写入");
    runtime
        .enter_workspace_card_input(&target, "assignment")
        .expect("Enter 应确认");
    assert_eq!(
        *writes.lock().expect("记录"),
        vec!["独有正文".as_bytes().to_vec(), vec![13]],
        "唯一正文和 ASCII CR=13=0x0d"
    );
    assert!(
        !runtime.workspace_card_input_held(node),
        "确认 Enter 后输入占用应释放"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 1, &target),
        Ok(()),
        "发送不能把目标换成定义首卡"
    );
}

/// 用户尚未提交的草稿与审批状态不能被空间派工覆盖，解除后才能输入。
/// 数据构造：OpenCode 普通卡片 1 张，用户草稿 1 次写入；一个 WaitingApproval 状态。
/// 执行过程：用户输入草稿后 reserve 失败；用户 Enter 后设审批，reserve 再失败；Ready 后 reserve 成功。
/// 预期结果：两次拒绝均证明零派工输入；用户原正文保留，Ready 后本任务才可独占。
#[test]
fn drafts_and_approval_prevent_automation_without_writing() {
    let (mut runtime, node, writes) = recording();
    let target = runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("目标");
    runtime
        .send_node_input(node, NodeInput::Text("人类草稿".into()))
        .expect("用户输入");
    let draft = runtime
        .reserve_workspace_card_input(&target, "assignment", "任务")
        .expect_err("用户草稿应保护");
    assert_eq!(draft.code, "desktop_input_pending", "说明待处理用户输入");
    assert!(draft.zero_input_proven, "预检拒绝能证明本任务零写入");
    runtime
        .send_node_input(
            node,
            NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .expect("用户提交自身输入");
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::OpenCode,
        provider_event: "PermissionRequest".into(),
        to: runtime::AgentTaskState::WaitingApproval {
            tool_name: Some("shell".into()),
        },
        task_title: None,
        session_id: None,
    });
    let approval = runtime
        .reserve_workspace_card_input(&target, "assignment", "任务")
        .expect_err("审批应等待");
    assert_eq!(approval.code, "waiting_approval", "不能把派工 Enter 当审批");
    assert_eq!(
        writes.lock().expect("记录").len(),
        2,
        "拒绝派工后仅用户正文与 Enter 两次写入"
    );
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::OpenCode,
        provider_event: "SessionStart".into(),
        to: runtime::AgentTaskState::Ready,
        task_title: None,
        session_id: None,
    });
    runtime
        .reserve_workspace_card_input(&target, "assignment", "任务")
        .expect("解除草稿和审批后可输入");
    assert!(
        runtime.workspace_card_input_held(node),
        "正常占用必须明确反馈"
    );
}

/// 正文写入之后目标会话变化必须待核对，不能向新会话发 Enter 或清行。
/// 数据构造：同一卡片原 session-a 与新 session-b，正文写入 1 次，Enter/清行预期 0 次。
/// 执行过程：冻结 a 并写正文，替换 session 后尝试原任务 Enter，再尝试另一自动任务。
/// 预期结果：旧任务 OutcomeUnknown 资格为非安全重试；新会话零输入，后续自动任务受阻。
#[test]
fn replacement_after_paste_never_submits_or_clears_the_new_session() {
    let (mut runtime, node, writes) = recording();
    runtime.bind_discovered_agent_session(node, "session-a");
    let target = runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("a 冻结");
    runtime
        .reserve_workspace_card_input(&target, "assignment", "原正文")
        .expect("reserve");
    runtime
        .paste_workspace_card_input(&target, "assignment", "原正文")
        .expect("paste");
    runtime.bind_discovered_agent_session(node, "session-b");
    let error = runtime
        .enter_workspace_card_input(&target, "assignment")
        .expect_err("替换后拒绝 Enter");
    assert_eq!(error.code, "session_changed", "应明确原会话失效");
    assert!(
        !error.zero_input_proven,
        "已写正文无法证明零副作用，禁止安全重试"
    );
    assert_eq!(
        *writes.lock().expect("记录"),
        vec!["原正文".as_bytes().to_vec()],
        "新会话不收 Enter 或 Ctrl+U"
    );
    assert!(
        runtime.send_agent_task_text(node, "后续任务").is_err(),
        "待核对输入阻止自动后续投递"
    );
    assert_eq!(
        writes.lock().expect("记录").len(),
        1,
        "阻止后续不能写新正文"
    );
}

struct PartialWriteFailure {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}
impl terminal::PtyBackend for PartialWriteFailure {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        self.writes.lock().expect("部分写记录").push(bytes.to_vec());
        Err(terminal::PtyError::Io(std::io::Error::other(
            "partially written before IO failure",
        )))
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), terminal::PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

/// PTY 的错误可能发生在部分正文已写入之后，不能把任何 IO Err 当作安全零写入。
/// 数据构造：1 个边界故障 backend，每次 write 记录 bytes 后返回 IO Err；正文 1 次，Enter/清行 0 次。
/// 执行过程：冻结、reserve、paste 触发部分错误；尝试后续自动正文；检查错误资格与字节记录。
/// 预期结果：outcome_unknown 且 zero_input_proven=false；后续与清行均不发生；原输入占用保留。
#[test]
fn partial_paste_failure_is_unknown_and_never_blindly_retried() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let seen = writes.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, _| {
        Ok(Box::new(PartialWriteFailure {
            writes: seen.clone(),
        }))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::OpenCode,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("故障 PTY 仍可启动");
    let target = runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("目标");
    runtime
        .reserve_workspace_card_input(&target, "assignment", "部分正文")
        .expect("预检无错误");
    let error = runtime
        .paste_workspace_card_input(&target, "assignment", "部分正文")
        .expect_err("注入部分写入失败");
    assert_eq!(error.code, "outcome_unknown", "IO错误不能假装正文未写入");
    assert!(!error.zero_input_proven, "不能授权安全重试");
    assert!(
        runtime.workspace_card_input_held(node),
        "未知输入继续阻止自动队列"
    );
    assert!(
        runtime.send_agent_task_text(node, "重试正文").is_err(),
        "不得自动重试正文"
    );
    assert_eq!(
        *writes.lock().expect("边界记录"),
        vec!["部分正文".as_bytes().to_vec()],
        "只尝试一次正文，不写 Enter 或 Ctrl+U"
    );
}

/// 未运行卡片的正式提交预检可以证明零写入，并且不能为了预检启动 PTY。
/// 数据构造：只有 Document Agent 节点，live_len=0；节点 target 有持久身份但 terminal_session_id=None。
/// 执行过程：捕获只读 target，再尝试 reserve 一项真实任务。
/// 预期结果：terminal_unavailable 且零输入证明为真；live_len 仍 0，不能隐式新建卡片/PTY。
#[test]
fn unavailable_terminal_is_a_proven_zero_write_failure() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let node = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    let target = runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("持久卡片存在");
    let error = runtime
        .reserve_workspace_card_input(&target, "assignment", "正文")
        .expect_err("未运行需交共享服务先准备");
    assert_eq!(error.code, "terminal_unavailable", "明确没有运行终端");
    assert!(error.zero_input_proven, "尚未调用写边界，安全失败资格成立");
    assert_eq!(runtime.live_len(), 0, "预检绝不启动 PTY");
    assert!(
        !runtime.workspace_card_input_held(node),
        "没有未知正文不能留不安全占用"
    );
}

/// RPC 等非 PTY 输送发生不确定结果时，同卡片也必须隔离后续自动输入，即使此前没有文本租约。
/// 数据构造：1 张运行中的卡片、0 次 PTY 写入；显式原 target 的一次 unknown 标记。
/// 执行过程：冻结原目标，隔离未知请求，再尝试另一个 assignment 的 reserve。
/// 预期结果：后续被 input_owned 拒绝；原卡输入仍 0；不能以租约不存在为由重放未知请求。
#[test]
fn unknown_provider_request_without_a_paste_lease_quarantines_automation() {
    let (mut runtime, node, writes) = recording();
    let target = runtime
        .workspace_card_target("workspace", "area", 1, node)
        .expect("原目标");
    runtime.mark_workspace_card_input_unknown(&target, "rpc-assignment");
    let error = runtime
        .reserve_workspace_card_input(&target, "next", "新正文")
        .expect_err("未知请求必须阻止后续输入");
    assert_eq!(
        error.code, "input_owned",
        "未知的 Provider 请求继续拥有自动输入通道"
    );
    assert!(runtime.workspace_card_input_held(node), "隔离必须可查询");
    assert!(
        writes.lock().expect("记录").is_empty(),
        "隔离与拒绝皆零 PTY 输入"
    );
}
