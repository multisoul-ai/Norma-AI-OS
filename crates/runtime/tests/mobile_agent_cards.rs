use canvas::{AgentProfile, CanvasNodeKind, CanvasRect};
use runtime::Runtime;

/// 历史空卡恢复成原生终端；打开 CLI 不等于安排任务。
///
/// 数据构造：一个空 Runtime，Codex 与 Claude 各一张卡片；640×400 只是画布占位尺寸。
/// checkpoint 与 session 数量均为 0；另传一条不属于手机卡片的桌面发现记录。
///
/// 执行过程：创建两卡 → 保存并恢复快照 → 分别走批量与单卡恢复入口。
///
/// 预期结果：卡片与控制模式保留；单卡恢复生成一个原生终端，保留安全模式，不伪造 Provider 会话或猜测历史。
#[test]
fn idle_mobile_cards_survive_restore_without_starting_agents() {
    let mut runtime = Runtime::new();
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let codex = runtime
        .create_mobile_agent_card(AgentProfile::Codex, None, None, bounds)
        .expect("应能创建空闲 Codex 手机卡片");
    let claude = runtime
        .create_mobile_agent_card(AgentProfile::Claude, None, None, bounds)
        .expect("应能创建空闲 Claude 手机卡片");
    assert_ne!(codex, claude, "不同卡片必须具有独立节点身份");
    assert_eq!(runtime.live_len(), 0, "创建卡片不得启动终端或 Agent");
    assert_eq!(
        runtime.node_run_state(codex),
        None,
        "空闲卡片不得产生虚假的运行状态"
    );
    let snapshot = runtime.collect_canvas_snapshot();
    runtime.restore_canvas_snapshot(snapshot);
    assert!(
        matches!(&runtime.document.node(codex).expect("Codex 卡片必须保留").kind,
        CanvasNodeKind::Agent(spec) if spec.mobile_control),
        "Codex 卡片必须保持受控模式"
    );
    assert!(
        runtime.document.node(claude).is_some(),
        "Claude 卡片不得在恢复时丢失"
    );
    assert_eq!(
        runtime.restore_live_sessions(&[]),
        0,
        "无任务的受控卡片不得被普通恢复启动"
    );
    assert_eq!(
        runtime.restore_live_sessions_with_node_sessions(&[], &[]),
        0,
        "无任务的受控卡片不得被会话恢复启动"
    );
    assert!(
        runtime.restore_live_session_for_node_with_node_sessions(codex, &[], &[], None),
        "单卡恢复必须打开原生终端，等待用户输入"
    );
    assert!(
        !runtime.restore_live_session_for_node_with_node_sessions(
            codex,
            &[],
            &[],
            Some("unrelated-desktop-session")
        ),
        "桌面历史发现不得把空闲手机卡片迁移到另一个会话"
    );
    assert_eq!(runtime.live_len(), 1, "重复恢复不能创建第二个终端");
    assert_eq!(
        runtime.agent_session_id(codex),
        None,
        "未安排任务的卡片没有 Provider 会话"
    );
}

/// 手机控制接受 Cursor，并拒绝尚未接入的 Provider，同时保留已有卡片。
///
/// 数据构造：一个 Claude 手机卡片、一次 Cursor 创建请求和一次 Grok 创建请求。
/// 执行过程：创建 Claude → 创建 Cursor → 尝试创建 Grok → 检查返回值及文档。
/// 预期结果：Cursor成功；Grok被拒绝且没有新增节点；已有两张卡元数据完整保留。
#[test]
fn cursor_mobile_provider_creates_card_and_grok_is_rejected() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let claude = runtime
        .create_mobile_agent_card(AgentProfile::Claude, None, None, bounds)
        .expect("Claude 属于首版支持范围");
    let cursor = runtime
        .create_mobile_agent_card(AgentProfile::Cursor, None, None, bounds)
        .expect("Cursor属于移动端支持范围");
    assert!(
        runtime.document.node(cursor).is_some(),
        "Cursor卡片必须真实写入文档"
    );
    assert!(
        runtime
            .create_mobile_agent_card(AgentProfile::Grok, None, None, bounds)
            .is_err(),
        "尚未支持的手机Provider不得悄悄退回不受控终端"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        2,
        "失败的创建不得留下额外卡片"
    );
    assert!(
        runtime.document.node(claude).is_some(),
        "失败请求不得删除之前的 Claude 卡片"
    );
    assert_eq!(runtime.live_len(), 0, "拒绝路径不得启动任何进程");
}

/// 手机只读快照来自当前卡片状态，不能启动空卡或把已删除卡片留在列表中。
///
/// 数据构造：Codex、Claude 各一张空卡，640×400 为现有卡片默认占位；执行器数量始终为零。
/// 执行过程：创建两卡 → 读取完整快照 → 修改 Codex 别名 → 删除 Claude → 再读取。
/// 预期结果：首次列出两种真实 Provider 且均 idle，无伪造会话；下一次只含改名后的 Codex。
#[test]
fn mobile_snapshot_reflects_live_metadata_without_launching() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let codex = runtime
        .create_mobile_agent_card(AgentProfile::Codex, None, None, bounds)
        .expect("Codex");
    let claude = runtime
        .create_mobile_agent_card(AgentProfile::Claude, None, None, bounds)
        .expect("Claude");
    let before = runtime.mobile_card_snapshots();
    assert_eq!(before.len(), 2, "两张卡片都必须展示");
    assert_eq!(before[0].node_id, codex, "按稳定节点 ID 排序");
    assert_eq!(
        before[0].profile, "Codex",
        "Codex 图标必须来自真实 Provider"
    );
    assert_eq!(before[1].profile, "Claude", "Claude 不能显示成 Codex");
    assert_eq!(before[0].state, "idle", "未派任务不能显示运行");
    assert!(before[0].session_id.is_none(), "空卡没有 Provider 会话");
    if let CanvasNodeKind::Agent(spec) = &mut runtime.document.node_mut(codex).expect("Codex").kind
    {
        spec.alias = Some("修复登录".into());
    }
    runtime.remove_node(claude);
    let after = runtime.mobile_card_snapshots();
    assert_eq!(after.len(), 1, "已删除 Claude 不得留在快照");
    assert_eq!(after[0].node_id, codex, "保留 Codex 的独立身份");
    assert_eq!(after[0].title, "修复登录", "手机必须看到最新别名");
    assert_eq!(runtime.live_len(), 0, "读取和改名不能启动 Agent");
}
