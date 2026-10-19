use live_voice::{DelegationNotice, DelegationRequest, LiveContext, Speaker, TranscriptFragment};

fn fragment(id: &str, speaker: Speaker, delta: &str, start_ms: f64) -> TranscriptFragment {
    TranscriptFragment {
        event_id: id.into(),
        speaker,
        delta: delta.into(),
        start_ms,
        end_ms: start_ms + 100.0,
    }
}

/// AC04/AC05/AC06：委派可能先于转写；助手附和不结束用户输入，迟到文字使旧快照失效。
///
/// 数据构造：d1通知先到，用户“创建一个”→助手“好”→用户“ Terminal”。
/// 执行过程：空上下文准备→观察首片段并准备→观察助手与迟到用户片段再准备。
/// 预期结果：先Pending，后保留双向上下文；权限用原始用户文字，旧版本不可执行。
#[test]
fn delegation_waits_for_user_context_and_late_input_revises_the_snapshot() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("本地话语起点");
    let notice = DelegationNotice {
        id: "d1".into(),
        offset_ms: 1200.0,
    };
    assert_eq!(
        context.prepare(&notice),
        DelegationRequest::Pending,
        "无用户正文不能启动后台"
    );
    context
        .observe(fragment("u1", Speaker::User, "创建一个", 1000.0))
        .expect("用户片段有效");
    let DelegationRequest::Ready(first) = context.prepare(&notice) else {
        panic!("有实际用户文本后必须可准备请求");
    };
    assert_eq!(first.user_input, "创建一个", "授权原文不是委派元数据");
    assert!(context.is_current(&first), "刚准备的版本应当前有效");
    context
        .observe(fragment("a1", Speaker::Assistant, "好", 1050.0))
        .expect("助手附和有效");
    assert!(context.is_current(&first), "助手附和不能使用户请求失效");
    context
        .observe(fragment("u2", Speaker::User, " Terminal", 1100.0))
        .expect("迟到用户片段有效");
    let DelegationRequest::Ready(latest) = context.prepare(&notice) else {
        panic!("完整的新快照必须可用");
    };
    assert_eq!(
        latest.user_input, "创建一个 Terminal",
        "跨助手附和拼接用户实际原文，保留空格"
    );
    assert_eq!(latest.conversation.len(), 3, "后台还需看见双向交谈上下文");
    assert!(
        !context.is_current(&first),
        "迟到用户文本必须阻止旧规划直接写入"
    );
    assert!(context.is_current(&latest), "最新规划仍可继续");
}

/// AC05/AC07/AC08：同一段用户输入的新委派ID只运行一次，完成后旧授权不能被“可以”继承。
///
/// 数据构造：创建终端原文→d1/d2两个通知→完成→助手建议删除→用户“可以”→d3。
/// 执行过程：claim首快照两次，检查第二通知，settle后准备新请求。
/// 预期结果：首claim成功，重复失败；新输入只含“可以”，双向历史仍供模型理解。
#[test]
fn claimed_input_is_deduplicated_and_settled_authorization_is_not_reused() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("本地话语起点");
    context
        .observe(fragment("u1", Speaker::User, "创建一个终端", 0.0))
        .expect("首请求");
    let notice = DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    };
    let DelegationRequest::Ready(first) = context.prepare(&notice) else {
        panic!("首请求就绪");
    };
    assert!(context.claim(&first), "首次规划应被认领");
    assert!(!context.claim(&first), "同一规划不能重复认领");
    let other = DelegationNotice {
        id: "d2".into(),
        offset_ms: 210.0,
    };
    assert_eq!(
        context.prepare(&other),
        DelegationRequest::Duplicate {
            original_delegation_id: "d1".into(),
            revision: first.revision
        },
        "新委派ID不能重做同一用户输入"
    );
    assert!(context.settle(&first), "核验过的首请求应结算");
    assert!(!context.is_current(&first), "结算后旧快照不可重新执行");
    context
        .observe(fragment("a1", Speaker::Assistant, "要删除吗？", 300.0))
        .expect("助手建议");
    context
        .observe(fragment("u2", Speaker::User, "可以", 400.0))
        .expect("模糊回答");
    let next = DelegationNotice {
        id: "d3".into(),
        offset_ms: 600.0,
    };
    let DelegationRequest::Ready(next) = context.prepare(&next) else {
        panic!("新请求单独准备");
    };
    assert_eq!(
        next.user_input, "可以",
        "不得借历史授权把模糊回答升级成写操作"
    );
    assert_eq!(next.conversation.len(), 3, "旧轮仍是模型上下文");
}

/// AC04/AC07：相同转写重放不累加原文，同ID不同内容拒绝且不污染已准备快照。
///
/// 数据构造：u1“创建终端”完全重放，然后以同u1发送“删除终端”。
/// 执行过程：保存首快照，逐一观察重放和冲突，再次准备。
/// 预期结果：快照仍有效、原文不重复，冲突返回InvalidEvent。
#[test]
fn replayed_fragments_are_idempotent_and_conflicting_ids_are_rejected() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("本地话语起点");
    let input = fragment("u1", Speaker::User, "创建终端", 0.0);
    context.observe(input.clone()).expect("首片段");
    let notice = DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    };
    let DelegationRequest::Ready(first) = context.prepare(&notice) else {
        panic!("首快照");
    };
    context.observe(input).expect("重放应幂等");
    assert!(context.is_current(&first), "重放不能制造新修订");
    assert_eq!(
        context.observe(fragment("u1", Speaker::User, "删除终端", 0.0)),
        Err(live_voice::LiveError::InvalidEvent),
        "同ID冲突不可覆盖授权原文"
    );
    assert_eq!(
        context.prepare(&notice),
        DelegationRequest::Ready(first),
        "快照保持原始正文和版本"
    );
}

/// AC03/AC04：转写历史有界，拒绝不合法片段时不改变授权快照。
///
/// 数据构造：空ID与NaN时间片段；32个4096字节片段共128KiB，然后再加入一个字节。
/// 执行过程：拒绝畸形数据→填满128KiB正文预算→检查越界与准备结果。
/// 预期结果：InvalidEvent/Capacity可区分；越界未追加，历史无静默驱逐。
#[test]
fn transcript_context_rejects_invalid_fragments_and_bounds_retained_bytes() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("本地话语起点");
    assert_eq!(
        context.observe(fragment("", Speaker::User, "非法", 0.0)),
        Err(live_voice::LiveError::InvalidEvent),
        "ID必须非空"
    );
    assert_eq!(
        context.observe(fragment("nan", Speaker::User, "非法", f64::NAN)),
        Err(live_voice::LiveError::InvalidEvent),
        "时间必须有限"
    );
    // 32 × 4096 = 131072 bytes = 128KiB，限制正文而非网络帧数。
    for index in 0..32 {
        context
            .observe(fragment(
                &format!("u{index}"),
                Speaker::User,
                &"x".repeat(4096),
                index as f64,
            ))
            .expect("预算内片段");
    }
    assert_eq!(
        context.observe(fragment("overflow", Speaker::User, "x", 40.0)),
        Err(live_voice::LiveError::Capacity),
        "总字节超限明确失败"
    );
    let notice = DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    };
    let DelegationRequest::Ready(snapshot) = context.prepare(&notice) else {
        panic!("预算内正文保留");
    };
    assert_eq!(
        snapshot.user_input.len(),
        131072,
        "失败追加不能改变已保留的正文"
    );
    assert_eq!(
        snapshot.conversation.len(),
        32,
        "不得静默驱逐旧片段或保留非法片段"
    );
}

/// AC06/AC07/AC09：已接受工具的授权结算独立于迟到转写，不能在新修订中再次执行。
///
/// 数据构造：u1“创建一个终端”已claim并假定工具已接受；执行中u2“嗯”抵达。
/// 执行过程：观察u2→确认旧快照过期→按真实副作用结算u1→准备d2。
/// 预期结果：结算成功但旧结果仍不可回传；新授权仅有“嗯”，旧创建原文不再进入规划。
#[test]
fn accepted_side_effect_consumes_its_original_authorization_even_after_revision() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("本地话语起点");
    context
        .observe(fragment("u1", Speaker::User, "创建一个终端", 0.0))
        .expect("首请求");
    let notice = DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    };
    let DelegationRequest::Ready(first) = context.prepare(&notice) else {
        panic!("首请求就绪");
    };
    assert!(context.claim(&first), "首先认领执行");
    context
        .observe(fragment("u2", Speaker::User, "嗯", 300.0))
        .expect("执行中的用户片段");
    assert!(!context.is_current(&first), "新片段使旧回复过期");
    assert!(context.settle(&first), "已接受副作用仍必须消费对应授权");
    assert!(!context.settle(&first), "同一授权不能结算两次");
    assert!(!context.is_current(&first), "结算不能让旧回复重新有效");
    let notice = DelegationNotice {
        id: "d2".into(),
        offset_ms: 500.0,
    };
    let DelegationRequest::Ready(next) = context.prepare(&notice) else {
        panic!("后续输入应单独准备");
    };
    assert_eq!(next.user_input, "嗯", "已执行的创建授权不得复用");
    assert!(
        context.claim(&next),
        "新输入可认领，权限判断随后使用本轮原文"
    );
}

/// AC05/AC06/AC08：检测到新语音后，完整VAD段尚未返回期间不能借用旧请求授权。
/// 数据构造：旧话语“删除卡片8”已有Ready快照；新语音先检测、后确认500ms起点。
/// 执行过程：冻结输入→收到新notice和转写→确认精确起点→再次准备。
/// 预期结果：冻结期Pending且旧快照失效；确认后只使用新话语，过期确认票据不能解除冻结。
#[test]
fn speech_detection_blocks_old_authorization_until_its_precise_boundary_arrives() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("旧话语");
    context
        .observe(fragment("old", Speaker::User, "删除卡片8", 0.0))
        .expect("旧转写");
    let notice = DelegationNotice {
        id: "old-request".into(),
        offset_ms: 200.0,
    };
    let DelegationRequest::Ready(old) = context.prepare(&notice) else {
        panic!("旧请求应可准备");
    };
    let ticket = context.suspend_user_input();
    assert!(!context.is_current(&old), "新语音检测立即使旧规划失效");
    let next = DelegationNotice {
        id: "new-request".into(),
        offset_ms: 1000.0,
    };
    assert_eq!(
        context.prepare(&next),
        DelegationRequest::Pending,
        "精确边界未到前不能借用旧授权"
    );
    context
        .observe(fragment("new", Speaker::User, "你好", 500.0))
        .expect("新转写");
    assert_eq!(
        context.prepare(&next),
        DelegationRequest::Pending,
        "转写本身也不能解除授权冻结"
    );
    assert!(
        context
            .confirm_user_utterance(ticket, 500.0)
            .expect("精确边界"),
        "当前票据可以确认"
    );
    let DelegationRequest::Ready(current) = context.prepare(&next) else {
        panic!("新话语现在应就绪");
    };
    assert_eq!(current.user_input, "你好", "新话语不能继承删除授权");
    let newest = context.suspend_user_input();
    assert!(
        !context
            .confirm_user_utterance(ticket, 500.0)
            .expect("迟到边界"),
        "旧票据不能解除新的冻结"
    );
    assert_eq!(
        context.prepare(&next),
        DelegationRequest::Pending,
        "迟到确认之后仍保持冻结"
    );
    assert!(
        context
            .confirm_user_utterance(newest, 1000.0)
            .expect("最新确认"),
        "最新票据仍可用"
    );
}

/// AC06/AC08：旧起点的重复登记必须幂等，不能越过新语音的确认票据解除冻结。
/// 数据构造：旧话语0ms与删除请求，随后冻结并重放同一起点。
/// 执行过程：suspend→重复begin旧起点→prepare→用当前ticket确认新起点。
/// 预期结果：重放后仍Pending；仅当前ticket的新边界能解除冻结。
#[test]
fn replayed_old_boundary_cannot_bypass_pending_confirmation() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("旧边界");
    context
        .observe(fragment("old-command", Speaker::User, "删除卡片8", 0.0))
        .expect("旧转写");
    let ticket = context.suspend_user_input();
    context.begin_user_utterance(0.0).expect("重复旧边界应幂等");
    assert_eq!(
        context.prepare(&DelegationNotice {
            id: "replay".into(),
            offset_ms: 200.0
        }),
        DelegationRequest::Pending,
        "旧起点不能解除新的冻结"
    );
    assert!(
        context
            .confirm_user_utterance(ticket, 500.0)
            .expect("新边界"),
        "当前票据仍可确认真正的新话语"
    );
}
