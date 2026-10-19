use live_voice::{DelegationNotice, DelegationRequest, LiveContext, Speaker, TranscriptFragment};

fn observe(context: &mut LiveContext, id: &str, text: &str, start: f64) {
    context
        .observe(TranscriptFragment {
            event_id: id.into(),
            speaker: Speaker::User,
            delta: text.into(),
            start_ms: start,
            end_ms: start + 100.0,
        })
        .expect("有效转写");
}

/// AC05/AC06：委派offset只引用当时的原文，不能误拿后续请求执行。
/// 数据构造：先收到创建终端和后续删除文字，再收到offset=200ms的旧委派通知。
/// 执行过程：准备旧通知，核对授权和背景；随后准备1200ms的新通知。
/// 预期结果：旧授权只有创建原文，未来用户文字不能成为旧委派授权。
#[test]
fn delegation_offset_does_not_borrow_future_user_text() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("本地话语起点");
    observe(&mut context, "u1", "创建一个终端", 0.0);
    observe(&mut context, "u2", "删除终端9", 1000.0);
    let DelegationRequest::Ready(old) = context.prepare(&DelegationNotice {
        id: "old".into(),
        offset_ms: 200.0,
    }) else {
        panic!("旧委派有对应原文");
    };
    assert_eq!(old.user_input, "创建一个终端", "后续请求不能借给旧委派");
    assert_eq!(
        old.conversation.len(),
        1,
        "旧委派的模型背景也应截至其offset"
    );
    assert!(!old.user_input.contains("删除"), "禁止未来删除授权回流");
    assert!(
        !context.is_current(&old),
        "后续用户请求已到达，旧委派不能继续执行"
    );
    assert!(!context.claim(&old), "旧offset快照不得认领副作用");
}

/// AC05/AC08：新一段真实用户话语不能继承之前未委派的话题作为授权。
/// 数据构造：本地采集/VAD给出0ms和1000ms话语起点；两段转写分别为修复与创建。
/// 执行过程：新话语开始立即检查旧快照失效，再接收其转写并准备新offset。
/// 预期结果：新授权只有创建原文；修复文字只保留为背景，助手插话不生成边界。
#[test]
fn new_observed_user_utterance_separates_authorization_from_history() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("真实采集起点");
    observe(&mut context, "u1", "修复登录", 0.0);
    let DelegationRequest::Ready(first) = context.prepare(&DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    }) else {
        panic!("首话语可以准备");
    };
    context.begin_user_utterance(1000.0).expect("新话语来自VAD");
    assert!(
        !context.is_current(&first),
        "新语音开始即应撤销旧规划资格，不能等转写到达"
    );
    observe(&mut context, "u2", "创建一个终端", 1000.0);
    let DelegationRequest::Ready(next) = context.prepare(&DelegationNotice {
        id: "d2".into(),
        offset_ms: 1200.0,
    }) else {
        panic!("新话语正文到达");
    };
    assert_eq!(next.user_input, "创建一个终端", "旧话题不能污染本次授权");
    assert_eq!(next.conversation.len(), 2, "背景仍可供模型理解上下文");
    assert!(
        !next.user_input.contains("修复"),
        "不得将旧执行动词借给新请求"
    );
}

/// AC05/AC08：无本地话语边界时只保留转写，不能悄悄退回拼接全部用户历史。
/// 数据构造：只有一条可执行转写，没有采集/VAD起点。
/// 执行过程：准备对应offset的委派。
/// 预期结果：Pending；宿主必须提供真实语音边界才能决定授权范围。
#[test]
fn missing_host_utterance_boundary_keeps_delegation_pending() {
    let mut context = LiveContext::default();
    observe(&mut context, "u1", "创建终端", 0.0);
    assert_eq!(
        context.prepare(&DelegationNotice {
            id: "d1".into(),
            offset_ms: 200.0
        }),
        DelegationRequest::Pending,
        "不得用全部历史替代缺失的话语边界"
    );
}

/// AC06/AC07：同一话语的旧规划未结算时，迟到转写和新委派ID不能并发重做。
/// 数据构造：创建终端已claim，随后同话语补“命名为日志”，并发新ID委派。
/// 执行过程：认领旧快照，补充转写，再准备新通知。
/// 预期结果：返回Duplicate指向旧规划；先核对旧副作用状态，不能直接执行第二次。
#[test]
fn revised_utterance_waits_for_in_flight_side_effect_reconciliation() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("话语起点");
    observe(&mut context, "u1", "创建终端", 0.0);
    let DelegationRequest::Ready(first) = context.prepare(&DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    }) else {
        panic!("首次准备");
    };
    assert!(context.claim(&first), "先开始首规划");
    observe(&mut context, "u2", "命名为日志", 250.0);
    assert_eq!(
        context.prepare(&DelegationNotice {
            id: "d2".into(),
            offset_ms: 400.0
        }),
        DelegationRequest::Duplicate {
            original_delegation_id: "d1".into(),
            revision: first.revision
        },
        "旧操作结果未核实时不得凭新修订再次派工"
    );
    assert!(
        !context.is_current(&first),
        "旧规划也不能继续使用不完整原文执行"
    );
}

/// AC03/AC07：长通话历史滚动时不保留无界转写，也不能让已驱逐的授权复活。
/// 数据构造：700段各1秒的话语、每段一个独立事件；超过512事件与边界预算。
/// 执行过程：持续标记真实话语起点并追加文本，准备最后委派，再重放第一段事件和通知。
/// 预期结果：当前原文准确、历史≤512；过旧事件明确拒绝，过旧通知保持Pending。
#[test]
fn long_conversation_rotates_history_without_resurrecting_old_authorization() {
    let mut context = LiveContext::default();
    for index in 0..700 {
        let start = index as f64 * 1000.0;
        context
            .begin_user_utterance(start)
            .expect("话语起点应可滚动");
        observe(
            &mut context,
            &format!("u{index}"),
            &format!("创建终端{index}"),
            start,
        );
    }
    let DelegationRequest::Ready(last) = context.prepare(&DelegationNotice {
        id: "last".into(),
        offset_ms: 699_200.0,
    }) else {
        panic!("最后话语应可准备");
    };
    assert_eq!(last.user_input, "创建终端699", "授权只来自当前话语");
    assert!(last.conversation.len() <= 512, "历史不得无界增长");
    let replay = context.observe(TranscriptFragment {
        event_id: "u0".into(),
        speaker: Speaker::User,
        delta: "创建终端0".into(),
        start_ms: 0.0,
        end_ms: 100.0,
    });
    assert_eq!(
        replay,
        Err(live_voice::LiveError::InvalidEvent),
        "驱逐前事件不能重新注入"
    );
    assert_eq!(
        context.prepare(&DelegationNotice {
            id: "old".into(),
            offset_ms: 200.0
        }),
        DelegationRequest::Pending,
        "旧offset不能重借新授权"
    );
    assert!(context.is_current(&last), "拒绝旧重放不能破坏当前有效快照");
}

/// AC06/AC07：宿主确认旧规划没有副作用后才可释放认领，以完整修订重新规划。
/// 数据构造：首规划被claim但尚未执行，迟到文字补充；随后明确释放原认领。
/// 执行过程：释放两次，准备新通知并重新claim，尝试结算已释放的旧快照。
/// 预期结果：仅首次释放成功，新规划包含完整原文且可执行，旧快照不能伪造已接受结果。
#[test]
fn verified_unexecuted_claim_can_be_replanned_after_correction() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("话语起点");
    observe(&mut context, "u1", "创建终端", 0.0);
    let DelegationRequest::Ready(first) = context.prepare(&DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    }) else {
        panic!("首快照");
    };
    assert!(context.claim(&first), "首规划已认领");
    observe(&mut context, "u2", "命名为日志", 250.0);
    assert!(
        context.release_unexecuted(&first),
        "宿主确认尚无副作用后允许释放"
    );
    assert!(!context.release_unexecuted(&first), "认领只能释放一次");
    let DelegationRequest::Ready(next) = context.prepare(&DelegationNotice {
        id: "d2".into(),
        offset_ms: 400.0,
    }) else {
        panic!("完整修订可以重新规划");
    };
    assert_eq!(next.user_input, "创建终端命名为日志", "保留实际修订原文");
    assert!(context.claim(&next), "只允许新规划认领");
    assert!(!context.settle(&first), "已释放旧快照不能再结算");
}

/// AC05/AC08：跨越委派offset的整段转写无法按字可靠切分，不能借其后半段授权。
/// 数据构造：一个0–1000ms片段含完整创建指令，委派只位于100ms。
/// 执行过程：标记话语起点、观察片段，再准备早于片段结束的委派。
/// 预期结果：Pending；不能把整段或猜测切分后的文本交给工具执行。
#[test]
fn transcript_spanning_the_delegation_offset_is_not_execution_authorization() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("话语起点");
    context
        .observe(TranscriptFragment {
            event_id: "u1".into(),
            speaker: Speaker::User,
            delta: "创建一个终端".into(),
            start_ms: 0.0,
            end_ms: 1000.0,
        })
        .expect("有效片段");
    assert_eq!(
        context.prepare(&DelegationNotice {
            id: "d1".into(),
            offset_ms: 100.0
        }),
        DelegationRequest::Pending,
        "跨offset文字不能升级为旧委派的执行授权"
    );
}

/// AC06/AC07：释放本身必须撤销旧snapshot，即使没有新转写且重试同一委派ID。
/// 数据构造：同一d1、同一原文，无额外语音事件；释放后立即重新准备和认领。
/// 执行过程：claim→release→prepare同ID→claim，再尝试旧snapshot claim/settle。
/// 预期结果：新修订递增，旧snapshot既不能认领也不能结算新认领，最新snapshot可结算。
#[test]
fn release_invalidates_old_snapshot_even_when_same_notice_is_retried() {
    let mut context = LiveContext::default();
    context.begin_user_utterance(0.0).expect("话语起点");
    observe(&mut context, "u1", "创建终端", 0.0);
    let notice = DelegationNotice {
        id: "d1".into(),
        offset_ms: 200.0,
    };
    let DelegationRequest::Ready(first) = context.prepare(&notice) else {
        panic!("首次准备");
    };
    assert!(context.claim(&first), "首次认领");
    assert!(context.release_unexecuted(&first), "确认未执行后释放");
    let DelegationRequest::Ready(next) = context.prepare(&notice) else {
        panic!("重新准备");
    };
    assert!(context.claim(&next), "新规划可重新认领");
    assert!(!context.is_current(&first), "释放必须使旧快照失效");
    assert!(!context.claim(&first), "旧快照不得重新认领");
    assert!(!context.settle(&first), "旧快照不能结算新认领");
    assert!(next.revision > first.revision, "释放产生新修订，避免ABA");
    assert!(context.settle(&next), "最新快照仍可正常结算");
}
