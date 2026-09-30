use super::{
    tests::{page, terminal},
    *,
};
use crate::NodeInput;

/// 续期只延长已持有的输入权，不能把重连后重建的能力变成一次隐式接管。
///
/// 数据构造：一个原PTY与原turn-a；两个Mac租约代表原连接和重连，均无手机可构造身份。
/// 执行过程：未接管续期→显式接管→原租约续期→新连接续期→撤销原租约→迟到续期。
/// 预期结果：只有已接管原租约可续期；重连和撤销都不能获得输入权；无PTY输入副作用。
#[test]
fn renewal_never_acquires_an_unclaimed_or_reconnected_lease() {
    let (mut runtime, key, writes) = terminal();
    let owner = TerminalInputLease::new(key.clone()).expect("原连接能力");
    assert_eq!(
        runtime.renew_mobile_terminal_input(&key, &owner),
        Err("terminal_control_expired"),
        "没有持有输入权时续期不得变成接管"
    );
    runtime
        .claim_mobile_terminal_input(&key, &owner)
        .expect("明确接管");
    runtime
        .renew_mobile_terminal_input(&key, &owner)
        .expect("原持有者续期");
    let reconnect = TerminalInputLease::new(key.clone()).expect("重连能力");
    assert_eq!(
        runtime.renew_mobile_terminal_input(&key, &reconnect),
        Err("terminal_controlled"),
        "相同原任务不等于同一连接输入权"
    );
    owner.revoke();
    assert_eq!(
        runtime.renew_mobile_terminal_input(&key, &owner),
        Err("terminal_control_expired"),
        "已撤销能力不能被迟到续期恢复"
    );
    assert!(
        !runtime.mobile_terminal_is_controlled(key.node_id),
        "原输入权已经失效"
    );
    assert!(
        writes.lock().expect("录制PTY").is_empty(),
        "接管与续期不能发送控制字节"
    );
}

/// 后台撤权永久关闭旧输入租约，后续接管/交还不能让迟到续期重新占用终端。
///
/// 数据构造：一个80×24录制PTY、同一个turn-a、Mac创建的两个租约a/b；没有真实网络或模型。
/// 执行过程：接管a→后台撤销a→新b接管/交还→迟到a再次申请及Enter→桌面继续输入。
/// 预期结果：a撤销后立即不再占用；a/b均不能复活；只有最后的desktop字节写入原PTY，无Enter。
#[test]
fn revoked_lease_cannot_reacquire_after_another_owner_returns() {
    let (mut runtime, key, writes) = terminal();
    let owner = TerminalInputLease::new(key.clone()).expect("Mac创建原任务租约");
    runtime
        .claim_mobile_terminal_input(&key, &owner)
        .expect("原接管");
    let revoked = owner.clone();
    std::thread::spawn(move || revoked.revoke())
        .join()
        .expect("后台撤权不依赖GPUI");
    assert!(
        !runtime.mobile_terminal_is_controlled(key.node_id),
        "撤权后立即解除输入占用"
    );
    let next = TerminalInputLease::new(key.clone()).expect("新租约");
    runtime
        .claim_mobile_terminal_input(&key, &next)
        .expect("新持有者可接管");
    runtime
        .release_mobile_terminal_input(&key, &next)
        .expect("新持有者交还");
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &owner),
        Err("terminal_control_expired"),
        "旧租约不能因当前持有者已变化而复活"
    );
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &next),
        Err("terminal_control_expired"),
        "已交还的新租约也不能复活"
    );
    let screen = page(&runtime, &key);
    assert_eq!(
        runtime.send_controlled_terminal_input(
            &key,
            &owner,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Enter
        ),
        Err("terminal_control_expired"),
        "撤权后迟到Enter必须被拒绝"
    );
    runtime
        .send_node_input(key.node_id, NodeInput::Text("desktop".into()))
        .expect("桌面恢复输入");
    assert_eq!(
        *writes.lock().expect("字节"),
        vec![b"desktop".to_vec()],
        "不允许旧Enter或额外控制字节"
    );
    assert_eq!(runtime.live_len(), 1, "复用唯一原PTY");
}

/// 撤权保留人工草稿；在排队接管前已撤销的租约不得延迟取得输入权。
///
/// 数据构造：一个PTY、人工文字draft=5字节；一个预先撤销租约，一个有效租约。
/// 执行过程：撤销待接管租约→尝试接管→有效接管并写draft→后台撤权→自动任务尝试覆盖→桌面接续。
/// 预期结果：预先撤销不占用；草稿不被清除/自动提交；仅draft和桌面接续到达原PTY。
#[test]
fn revocation_preserves_human_draft_and_rejects_queued_claim() {
    let (mut runtime, key, writes) = terminal();
    let cancelled = TerminalInputLease::new(key.clone()).expect("排队租约");
    cancelled.revoke();
    assert_eq!(
        runtime.claim_mobile_terminal_input(&key, &cancelled),
        Err("terminal_control_expired"),
        "前台迟到的接管不能越过撤权"
    );
    assert!(
        !runtime.mobile_terminal_is_controlled(key.node_id),
        "未成功接管不得占用终端"
    );
    let owner = TerminalInputLease::new(key.clone()).expect("有效租约");
    runtime
        .claim_mobile_terminal_input(&key, &owner)
        .expect("接管");
    let screen = page(&runtime, &key);
    runtime
        .send_controlled_terminal_input(
            &key,
            &owner,
            &screen.card,
            screen.screen_signature,
            screen.input_sequence,
            &TerminalInput::Text {
                text: "draft".into(),
            },
        )
        .expect("人工草稿");
    owner.revoke();
    assert!(
        runtime.agent_task_input_held(key.node_id),
        "撤权后仍须保护未提交的人工草稿"
    );
    assert!(
        runtime
            .send_agent_task_text(key.node_id, "overwrite")
            .is_err(),
        "自动任务不能覆盖人工草稿"
    );
    runtime
        .send_node_input(key.node_id, NodeInput::Text(" continued".into()))
        .expect("桌面处理原草稿");
    assert_eq!(
        *writes.lock().expect("字节"),
        vec![b"draft".to_vec(), b" continued".to_vec()],
        "撤权不发送Enter/Ctrl-U或自动任务正文"
    );
}

/// 只有原租约能读取可输入终端；人工产生新回合后页面报告新回合，来源租约仍保持原任务。
///
/// 数据构造：一个PTY、原turn-a、后续turn-b；当前页和历史页共2种读取，竞争租约不拥有输入权。
/// 执行过程：接管→普通页/持有者页/竞争页→人工新回合→原租约读取→撤销→迟到读取。
/// 预期结果：普通页及历史页不可输入；持有者当前页可输入且显示真实turn-b；竞争/撤销/改目标均被拒。
#[test]
fn controlled_page_reports_current_turn_without_rebinding_original_task() {
    let (mut runtime, key, _) = terminal();
    let owner = TerminalInputLease::new(key.clone()).expect("原租约");
    let competitor = TerminalInputLease::new(key.clone()).expect("竞争租约");
    runtime
        .claim_mobile_terminal_input(&key, &owner)
        .expect("接管");
    let request = norma::mobile::TerminalRequest {
        workspace_id: "project-a".into(),
        route_id: "route-a".into(),
        node_id: key.node_id,
        run_id: key.run_id.clone(),
        session_id: Some(key.session_id.clone()),
        before: None,
        revision: None,
    };
    assert!(
        !page(&runtime, &key).input_ready,
        "普通页面不能获得独占输入权"
    );
    let controlled = runtime
        .controlled_mobile_terminal_page(&key, &owner, &request)
        .expect("持有者页面");
    assert!(controlled.input_ready, "持有者在当前可用终端可输入");
    assert_eq!(
        runtime
            .controlled_mobile_terminal_page(&key, &competitor, &request)
            .err(),
        Some("terminal_controlled"),
        "相同任务也不等于持有输入权"
    );
    let history = norma::mobile::TerminalRequest {
        before: Some(controlled.total_lines),
        revision: Some(controlled.revision),
        ..request.clone()
    };
    assert!(
        !runtime
            .controlled_mobile_terminal_page(&key, &owner, &history)
            .expect("历史页")
            .input_ready,
        "历史内容不可作为可输入屏幕"
    );
    let hook = norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::Codex, key.node_id,
        serde_json::json!({"hook_event_name":"UserPromptSubmit","session_id":key.session_id,"turn_id":"turn-b"})).expect("后续Hook");
    assert!(runtime.observe_mobile_hook(&hook), "人工新回合被实际观察");
    let current = runtime
        .controlled_mobile_terminal_page(&key, &owner, &request)
        .expect("同一租约查看后续实际回合");
    assert_eq!(
        current.card.turn_id.as_deref(),
        Some("turn-b"),
        "终端页面必须报告实际新回合"
    );
    assert_eq!(owner.key.turn_id, "turn-a", "不能把租约来源改绑成新回合");
    let wrong = norma::mobile::TerminalRequest {
        node_id: key.node_id + 1,
        ..request.clone()
    };
    assert_eq!(
        runtime
            .controlled_mobile_terminal_page(&key, &owner, &wrong)
            .err(),
        Some("session_changed"),
        "不可用原租约读取另一个节点"
    );
    owner.revoke();
    assert_eq!(
        runtime
            .controlled_mobile_terminal_page(&key, &owner, &request)
            .err(),
        Some("terminal_control_expired"),
        "撤权后的旧读取不能返回可输入页"
    );
}
