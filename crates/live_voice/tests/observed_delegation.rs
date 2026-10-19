use live_voice::{DelegationNotice, DelegationRequest, LiveContext, Speaker, TranscriptFragment};

fn input(context: &mut LiveContext, id: &str, text: &str, start: f64) {
    context
        .observe(TranscriptFragment {
            event_id: id.into(),
            speaker: Speaker::User,
            delta: text.into(),
            start_ms: start,
            end_ms: start + 100.0,
        })
        .expect("服务端用户转写");
}

fn notice(id: &str) -> DelegationNotice {
    DelegationNotice {
        id: id.into(),
        offset_ms: 50.0,
    }
}

/// 场景：Live没有转写完成fence，跨offset片段仍可用于语义规划。
/// 构造：先到委派，后到跨越50ms的用户指令，不注入虚构话语边界。
/// 执行：准备空上下文，再追加原文并认领；新用户片段使旧规划失效。
/// 预期：只在有真实文字后Ready，保留全部双向上下文，offset不裁字或授权。
#[test]
fn observed_context_plans_without_a_fabricated_turn_fence() {
    let mut context = LiveContext::default();
    assert_eq!(
        context.prepare_observed(&notice("d1")),
        DelegationRequest::Pending,
        "空转写不能发起任务"
    );
    input(&mut context, "u1", "创建一个终端", 0.0);
    let DelegationRequest::Ready(first) = context.prepare_observed(&notice("d1")) else {
        panic!("已有真实文字可进入语义规划，无需不存在的完成事件");
    };
    assert_eq!(
        first.user_input, "创建一个终端",
        "offset不得截断已观察的原文"
    );
    assert!(context.claim(&first), "首次证据可认领");
    input(&mut context, "u2", "，不对，创建一个浏览器", 100.0);
    assert!(!context.is_current(&first), "已观察更正必须撤销旧执行资格");
    assert!(
        matches!(
            context.prepare_observed(&notice("d2")),
            DelegationRequest::Duplicate { .. }
        ),
        "旧执行尚未核对时新委派不能另建意图"
    );
    assert!(
        context.release_unexecuted(&first),
        "核实没有副作用后可重规划"
    );
    let DelegationRequest::Ready(next) = context.prepare_observed(&notice("d2")) else {
        panic!("修订后的完整证据可重规划");
    };
    assert_eq!(
        next.intent_anchor(),
        first.intent_anchor(),
        "修订与新委派不能改变稳定意图身份"
    );
    assert!(next.user_input.contains("不对"), "不能丢弃更正信息");
}

/// 场景：新语音已开始但转写未到，排队通知不得重新借用旧指令。
/// 构造：首请求已认领，VAD触发取消，宿主核实未执行并释放认领。
/// 执行：同一证据重新准备，再追加用户更正。
/// 预期：没有新增证据时Pending；新原文允许语义重评，旧快照仍失效。
#[test]
fn speech_cancellation_requires_new_user_evidence_before_replanning() {
    let mut context = LiveContext::default();
    input(&mut context, "u1", "创建终端", 0.0);
    let DelegationRequest::Ready(first) = context.prepare_observed(&notice("d1")) else {
        panic!("首请求");
    };
    assert!(context.claim(&first), "认领首请求");
    context.suspend_user_input();
    assert!(
        context.release_unexecuted(&first),
        "只释放已核实未执行的规划"
    );
    assert_eq!(
        context.prepare_observed(&notice("d2")),
        DelegationRequest::Pending,
        "仅revision增加不能重借未变化的旧证据"
    );
    input(&mut context, "u2", "，算了", 100.0);
    assert!(
        matches!(
            context.prepare_observed(&notice("d2")),
            DelegationRequest::Ready(_)
        ),
        "新增原文允许语义评估撤回，不承诺完整话语"
    );
    assert!(!context.is_current(&first), "新原文不能复活旧快照");
}

/// 场景：没有人为话语边界的长通话仍需有界历史，截断的残句不能直接执行。
/// 构造：700个用户/助手事件，每条间隔200ms，超过512条历史预算。
/// 执行：持续观察而不虚构VAD服务端起点，再准备委派并重放第一条。
/// 预期：窗口滚动，截断标记要求澄清；旧事件拒绝，不能复活旧授权。
#[test]
fn observed_history_rotates_without_trusting_a_truncated_command() {
    let mut context = LiveContext::default();
    for index in 0..700 {
        input(
            &mut context,
            &format!("u{index}"),
            "聊天内容。",
            index as f64 * 200.0,
        );
    }
    let DelegationRequest::Ready(snapshot) = context.prepare_observed(&notice("recent")) else {
        panic!("滚动窗口仍可解释当前上下文");
    };
    assert!(snapshot.conversation.len() <= 512, "原文预算不能无界增长");
    assert!(
        snapshot.history_truncated,
        "未执行原文被截断后必须禁止依据残句写入"
    );
    let old = context.observe(TranscriptFragment {
        event_id: "u0".into(),
        speaker: Speaker::User,
        delta: "聊天内容。".into(),
        start_ms: 0.0,
        end_ms: 100.0,
    });
    assert_eq!(
        old,
        Err(live_voice::LiveError::InvalidEvent),
        "水位之前的旧事件不能再次授权"
    );
    assert!(context.claim(&snapshot), "宿主可认领以请求用户重述");
    assert!(context.settle(&snapshot), "澄清后应消费截断残文");
    input(&mut context, "fresh", "创建一个终端", 150_000.0);
    let DelegationRequest::Ready(fresh) = context.prepare_observed(&notice("fresh")) else {
        panic!("完整的新请求");
    };
    assert!(
        !fresh.history_truncated,
        "重述的完整请求不能永久被旧截断阻塞"
    );
    assert_eq!(fresh.user_input, "创建一个终端", "旧聊天内容只留作背景");
}

/// AC06：即使没有新用户文字，历史裁剪也不能让已驱逐来源的旧快照保持有效。
/// 构造：保存u1的未claim快照，再追加600条时间递增的助手事件。
/// 执行：触发原文裁剪，检查旧快照当前性和claim。
/// 预期：旧快照失效且不能claim；不能绕过截断输入必须澄清的边界。
#[test]
fn pruned_source_evidence_revokes_an_unclaimed_snapshot() {
    let mut context = LiveContext::default();
    input(&mut context, "u1", "创建一个终端", 0.0);
    let DelegationRequest::Ready(old) = context.prepare_observed(&notice("old")) else {
        panic!("原始快照");
    };
    for index in 1..600 {
        context
            .observe(TranscriptFragment {
                event_id: format!("a{index}"),
                speaker: Speaker::Assistant,
                delta: "好".into(),
                start_ms: index as f64 * 200.0,
                end_ms: index as f64 * 200.0 + 100.0,
            })
            .expect("滚动助手上下文");
    }
    assert!(
        !context.is_current(&old),
        "来源已被驱逐的快照不能保留false-truncated执行资格"
    );
    assert!(!context.claim(&old), "公开claim边界自身必须拒绝过时来源");
}
