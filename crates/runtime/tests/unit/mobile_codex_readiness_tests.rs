use super::*;
use crate::NodeInput;

/// Codex 0.160 的大写模型页脚可接收手机首条消息，加载画面和菜单仍不可输入。
///
/// 数据构造：80×24 原生终端；真实首屏为 5 行，5 < 24，且没有旧版 model: 头部。
///   页脚为 GPT-6.1-Sol xhigh · /tmp；bracketed paste 已开启，不伪造 Provider 会话。
/// 执行过程：PTY 输出加载占位 → 校验拒绝 → 清屏输出真实首屏 → 校验接受 → 清屏显示菜单。
/// 预期结果：加载与菜单状态 unknown 且拒绝消息；真实首屏 ready 且接受消息，始终只有原卡。
#[test]
fn native_codex_display_model_footer_accepts_workspace_message() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let (capture, output) = crossbeam_channel::bounded(1);
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        capture.send(tx.clone()).expect("记录原 PTY 输出入口");
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_mobile_agent_card(
            AgentProfile::Codex,
            None,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("手机原生卡");
    let output = output.try_recv().expect("原 PTY sender");
    output
        .send(terminal::PtyEvent::Output(
            b"\x1b[?2004hmodel: loading\r\n".to_vec(),
        ))
        .expect("加载输出");
    runtime.drain_all();
    let loading = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(loading.state, "unknown", "加载中不能提前显示就绪");
    assert_eq!(
        runtime.validate_workspace_message(&loading, "hi"),
        Err("state_not_ready"),
        "加载中不能接收手机消息"
    );
    output
        .send(terminal::PtyEvent::Output(
            "\x1b[2J\x1b[H\r\n› Ask Codex to do anything\r\n\r\nGPT-6.1-Sol xhigh · /tmp\r\n"
                .as_bytes()
                .to_vec(),
        ))
        .expect("Codex 0.160 首屏输出");
    runtime.drain_all();
    let ready = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(ready.state, "ready", "大写显示名必须识别为已加载模型");
    assert_eq!(
        runtime.validate_workspace_message(&ready, "hi"),
        Ok(()),
        "真实 Codex 首屏必须接受手机消息"
    );
    assert!(ready.session_id.is_none(), "首条消息前不能伪造会话");
    output
        .send(terminal::PtyEvent::Output(
            b"\x1b[2J\x1b[HUpdate menu\r\n".to_vec(),
        ))
        .expect("菜单输出");
    runtime.drain_all();
    let menu = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(menu.state, "unknown", "菜单不能借用历史提示符成为就绪");
    assert_eq!(
        runtime.validate_workspace_message(&menu, "hi"),
        Err("state_not_ready"),
        "手机消息不能输入更新菜单"
    );
    assert_eq!(runtime.live_len(), 1, "就绪判断不能启动另一张卡");
    assert_eq!(ready.node_id, node, "就绪快照必须属于原卡");
}

/// 手机 Codex 新建和恢复均默认完全访问，同时避免启动升级菜单。
///
/// 数据构造：新建与 session-a 恢复各 1 条命令，共 2 条；参数仅影响当前 CLI 进程。
/// 执行过程：生成两种正式启动命令 → 检查完全访问、恢复身份和关闭升级提示的参数。
/// 预期结果：两条命令均绕过审批和沙箱；没有只读或逐次审批参数，也不会自动升级 CLI。
#[test]
fn mobile_codex_startup_skips_update_dialog_with_full_access() {
    let fresh =
        crate::nodes::agent::mobile_agent_command(AgentProfile::Codex, None, None).expect("新建");
    let resume =
        crate::nodes::agent::mobile_agent_command(AgentProfile::Codex, Some("session-a"), None)
            .expect("恢复");
    assert!(
        fresh.args[1].contains("-c check_for_update_on_startup=false"),
        "新卡不能卡在升级菜单"
    );
    assert!(
        resume.args[1].contains("-c check_for_update_on_startup=false"),
        "恢复原卡不能卡在升级菜单"
    );
    assert!(
        fresh.args[1].contains("--dangerously-bypass-approvals-and-sandbox"),
        "手机新卡应与桌面一样默认完全访问"
    );
    assert!(
        !fresh.args[1].contains("--sandbox") && !fresh.args[1].contains("--ask-for-approval"),
        "新卡不能残留只读或逐次审批覆盖"
    );
    assert!(
        resume.args[1].contains("--dangerously-bypass-approvals-and-sandbox"),
        "恢复手机卡片也应默认完全访问"
    );
    assert!(
        !resume.args[1].contains("--sandbox") && !resume.args[1].contains("--ask-for-approval"),
        "恢复时不能重新引入逐次审批"
    );
    assert!(
        resume.args[1].contains("resume 'session-a'"),
        "必须恢复指定会话"
    );
    assert!(
        !fresh.args[1].contains("npm install"),
        "不得自动升级用户全局CLI"
    );
}

/// Codex卡片变矮或启动提示较多时，已加载的model/directory被滚出可见区仍应可从手机输入。
///
/// 数据构造：80×12原PTY，2行已加载头部+18行启动提示+2行输入区=22行，大于12行视口。
/// 执行过程：输出启动让头部进入history → 电脑上滚10行(22-12)隐藏提示符 → 手机快照/首条输入。
/// 预期结果：手机ready不受电脑滚动位置影响；不提前伪造session或另开PTY，也不重置电脑滚动。
#[test]
fn native_codex_scrolled_startup_header_remains_ready_for_mobile_input() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 240.),
            80,
            12,
        )
        .expect("小尺寸原卡");
    let output = format!("\x1b[?2004hmodel: gpt-5.6-sol\r\ndirectory: /tmp\r\n{}› Ask Codex to do anything\r\ngpt-5.6-sol xhigh · /tmp", "Usage notice\r\n".repeat(18));
    let live = runtime.live.get_mut(&node).expect("同PTY");
    live.input(NodeInput::Text(output)).expect("真实启动布局");
    live.drain();
    let crate::NodeView::Term { snapshot, .. } = runtime.node_view(node).expect("终端");
    let visible: String = snapshot.rows.iter().flatten().map(|c| c.c).collect();
    assert!(
        !visible.contains("model:"),
        "复现必须让启动头部滚出可见区域"
    );
    runtime
        .scroll_node(node, crate::NodeScroll::TermLines(10))
        .expect("电脑回看启动历史");
    let crate::NodeView::Term { snapshot, .. } = runtime.node_view(node).expect("回看的终端");
    let scrolled: String = snapshot.rows.iter().flatten().map(|c| c.c).collect();
    assert_eq!(snapshot.display_offset, 10, "电脑应回看全部10行历史");
    assert!(
        !scrolled.contains("Ask Codex to do anything"),
        "回看的视口中不应包含底部输入符"
    );
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        card.state, "ready",
        "已就绪Codex不能因卡片视口裁切而显示状态未确认"
    );
    assert_eq!(
        runtime.validate_mobile_message(&card, "hello"),
        Ok(()),
        "同一就绪原卡必须可以接收手机首条输入"
    );
    assert!(
        card.session_id.is_none(),
        "首条提交前不能伪造Codex延迟生成的session"
    );
    assert_eq!(runtime.live_len(), 1, "不能换成另一个执行器");
    let crate::NodeView::Term { snapshot, .. } = runtime.node_view(node).expect("校验后的终端");
    assert_eq!(
        snapshot.display_offset, 10,
        "手机就绪判断不能重置电脑滚动位置"
    );
}

/// Codex紧凑首屏省略directory时仍可发送，但滚入历史的旧提示符不能让菜单变为就绪。
///
/// 数据构造：80×8原PTY，来自真实CLI失败画面的model、空提示符与页脚，共8行且无directory。
///   后续输出9行菜单：9 > 8，将旧提示符完整滚出当前视口，仍留在终端历史中。
/// 执行过程：输出紧凑首屏 → 查询手机状态并校验首条输入 → 菜单替换视口 → 再次校验。
/// 预期结果：省略目录仍ready且接受输入；菜单状态unknown且拒绝输入，不伪造会话或新建PTY。
#[test]
fn native_codex_compact_prompt_is_ready_but_history_prompt_is_not() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 180.),
            80,
            8,
        )
        .expect("真实CLI对应的8行终端");
    let compact = "\x1b[?2004h│ model: gpt-5.6-sol xhigh /model to change │\r\n\r\n\r\n› Ask Codex to do anything\r\n\r\n  gpt-5.6-sol xhigh · /tmp\r\n\r\n";
    let live = runtime.live.get_mut(&node).expect("原PTY");
    live.input(NodeInput::Text(compact.into()))
        .expect("紧凑首屏");
    live.drain();
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(card.state, "ready", "CLI省略directory不能导致状态未确认");
    assert_eq!(
        runtime.validate_mobile_message(&card, "hello"),
        Ok(()),
        "紧凑首屏应接受首条手机输入"
    );
    assert!(card.session_id.is_none(), "首条发送前不能伪造会话");
    let live = runtime.live.get_mut(&node).expect("仍为原PTY");
    live.input(NodeInput::Text("\r\nUpdate menu".repeat(9)))
        .expect("菜单替换视口");
    live.drain();
    let blocked = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        blocked.state, "unknown",
        "历史中的旧提示符不能让当前菜单成为ready"
    );
    assert_eq!(
        runtime.validate_mobile_message(&blocked, "hello"),
        Err("state_not_ready"),
        "不能将手机消息输入菜单"
    );
    assert_eq!(runtime.live_len(), 1, "始终使用同一个终端");
}

/// Codex启动占位提示符不能被当作可输入，须等模型初始化完成。
///
/// 数据构造：80×24原生echo终端，paste模式已开启；首屏model/directory为loading，第二屏为实际值。
/// 执行过程：输出启动占位 → 投影状态 → 清屏输出就绪信息 → 再投影。
/// 预期结果：loading阶段不可发送，真实模型加载后才ready；没有提前构造Provider会话。
#[test]
fn native_codex_loading_prompt_does_not_accept_first_message() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原生卡");
    let live = runtime.live.get_mut(&node).expect("原生终端");
    live.input(NodeInput::Text(
        "\x1b[?2004hmodel: loading\r\ndirectory: loading\r\n› Ask Codex to do anything".into(),
    ))
    .expect("启动画面");
    live.drain();
    assert!(
        !runtime.native_prompt_ready(node),
        "加载占位不能接收首条消息"
    );
    let live = runtime.live.get_mut(&node).expect("同一终端");
    live.input(NodeInput::Text(
        "\x1b[2J\x1b[Hmodel: gpt-test\r\ndirectory: /tmp\r\n› Ask Codex to do anything".into(),
    ))
    .expect("就绪画面");
    live.drain();
    assert!(
        runtime.native_prompt_ready(node),
        "模型目录加载完成才可以输入"
    );
    assert!(
        runtime.mobile_card_snapshots()[0].session_id.is_none(),
        "首次提交前不伪造会话"
    );
}
