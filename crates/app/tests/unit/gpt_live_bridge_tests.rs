use super::*;

fn input(id: &str, text: &str, start: f64, end: f64) -> live_voice::TranscriptFragment {
    live_voice::TranscriptFragment {
        event_id: id.into(),
        speaker: live_voice::Speaker::User,
        delta: text.into(),
        start_ms: start,
        end_ms: end,
    }
}

/// AC06/AC11：挂断必须立即撤销未提交规划，不等待100ms UI pump或Bridge释放。
/// 数据构造：媒体根token、可信单话语快照，以及已认领但尚未执行的work。
/// 执行过程：保留Bridge与work，直接取消媒体根token，不推进任何计时器。
/// 预期结果：规划token同步取消，Bridge不能启动后续操作；accepted任务由原coordinator独立管理。
#[test]
fn gpt_live_bridge_call_close_cancels_planning_without_waiting_for_pump() {
    let media = CancellationToken::new();
    let mut bridge = LiveBridge::new("host-call".into(), media.clone());
    bridge
        .timeline
        .lock()
        .expect("时间线锁")
        .begin_user_utterance(0.0)
        .expect("宿主确认的服务端边界");
    bridge
        .transcript(input("u1", "创建一个终端", 0.0, 200.0))
        .expect("用户转写");
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d1".into(),
            offset_ms: 300.0,
        })
        .expect("委派");
    let work = bridge.claim_next().expect("已确认请求可以规划");
    media.cancel();
    assert!(
        work.cancellation.is_cancelled(),
        "挂断必须立即撤销MemoryAdapter所持的规划token"
    );
    assert!(
        bridge.claim_next().is_none(),
        "关闭后的Bridge不能再启动规划"
    );
}

/// AC05/AC06/AC07/AC09：无真实原文不执行，迟到转写补齐后一次规划，新语音撤销规划但保留已接受事实。
/// 数据构造：宿主call身份、先到delegation与后到用户片段，不提供虚构完成边界。
/// 执行过程：空原文Pending→原文到达后claim→重复notice→新语音suspend→旧accepted完成→新增原文。
/// 预期结果：同意图只有一份work；新语音取消规划token；旧事实消费授权但不口播；新意图可执行。
#[test]
fn gpt_live_bridge_plans_from_observed_evidence_and_preserves_accepted_receipts_on_correction() {
    let mut bridge = LiveBridge::new("host-call".into(), CancellationToken::new());
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d1".into(),
            offset_ms: 300.0,
        })
        .expect("notice");
    assert!(bridge.claim_next().is_none(), "无用户原文时不得启动规划");
    bridge
        .transcript(input("u1", "创建一个终端", 0.0, 200.0))
        .expect("迟到片段");
    let work = bridge.claim_next().expect("完整上下文只认领一次");
    let old_intent = work.intent.clone();
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d2".into(),
            offset_ms: 300.0,
        })
        .expect("重复意图notice");
    assert!(
        bridge.claim_next().is_none(),
        "新model delegation ID不能生成第二份并发work"
    );
    bridge.speech_started();
    assert!(work.cancellation.is_cancelled(), "新语音必须撤销未提交规划");
    let update = bridge.complete(
        &work,
        DelegatedReport {
            receipts: vec![Ok(
                agent::AdapterResult::accepted("任务已接收").with_task_id(7)
            )],
            pending: false,
            effects: true,
            ..Default::default()
        },
    );
    assert!(update.is_none(), "修正后的旧任务结果不能作为当前请求口播");
    bridge
        .transcript(input("u2", "创建一个浏览器", 400.0, 650.0))
        .expect("新请求");
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d3".into(),
            offset_ms: 700.0,
        })
        .expect("新notice");
    let next = bridge.claim_next().expect("明确新话语可以执行");
    assert_ne!(
        next.intent, old_intent,
        "意图身份由首个未消费原文事件区分，不由delegation ID区分"
    );
    assert_eq!(
        next.snapshot.user_input, "创建一个浏览器",
        "已接受的旧终端授权不能混入新请求"
    );
    let update = bridge
        .complete(
            &next,
            DelegatedReport {
                receipts: vec![Ok(
                    agent::AdapterResult::accepted("任务已接收").with_task_id(8)
                )],
                pending: false,
                effects: true,
                ..Default::default()
            },
        )
        .expect("当前回执可汇报");
    assert!(
        update.contains("accepted") && update.contains('8'),
        "回传保留accepted状态和真实task身份"
    );
    assert!(!update.contains("已完成"), "accepted不能被格式化成任务完成");
}

/// AC05/AC06：只读查询完成后收到补字，不应消费尚未执行的写授权。
/// 构造：首片段“请记住偏好”，规划读取记忆时新片段补充实际偏好。
/// 执行：旧规划被修订取消，回传只读succeeded，不再发送第二条委派通知。
/// 预期：旧回复不口播，完整写授权保留，修订前后稳定意图相同。
#[test]
fn gpt_live_bridge_read_only_receipt_preserves_revised_write_authorization() {
    let mut bridge = LiveBridge::new("host-call".into(), CancellationToken::new());
    bridge
        .transcript(input("u1", "请记住偏好", 0.0, 100.0))
        .expect("首原文");
    bridge
        .delegation(live_voice::DelegationNotice {
            id: "d1".into(),
            offset_ms: 50.0,
        })
        .expect("委派");
    let first = bridge.claim_next().expect("不依赖虚构完成事件即可规划");
    bridge
        .transcript(input("u2", "，默认中文回复", 100.0, 200.0))
        .expect("补字");
    assert!(first.cancellation.is_cancelled(), "补字应取消旧规划");
    assert!(
        bridge
            .complete(
                &first,
                DelegatedReport {
                    receipts: vec![Ok(agent::AdapterResult::succeeded("已读取记忆"))],
                    ..Default::default()
                }
            )
            .is_none(),
        "只读的旧回执不能当作新请求结果口播"
    );
    let next = bridge.claim_next().expect("没有副作用可按完整证据重规划");
    assert_eq!(next.snapshot.delegation_id, "d1", "补字后保留原委派关联");
    assert_eq!(
        next.intent, first.intent,
        "补字与新delegation不能另造副作用身份"
    );
    assert_eq!(
        next.snapshot.user_input, "请记住偏好，默认中文回复",
        "只读不能消耗写授权前缀"
    );
}

/// 场景：补字后的规划重排不能重做已有副作用或结果待定的操作。
/// 构造：同一原文和委派，在补字取消后分别返回effects=true或pending=true。
/// 执行：完成过期规划→尝试认领，且不提供第二次服务端委派。
/// 预期：旧结果不口播，不重排已产生或可能产生副作用的原请求。
#[test]
fn gpt_live_bridge_does_not_requeue_effects_or_pending_receipts() {
    for pending in [false, true] {
        let mut bridge = LiveBridge::new("call".into(), CancellationToken::new());
        bridge
            .transcript(input("u1", "创建一个终端", 0.0, 100.0))
            .expect("首原文");
        bridge
            .delegation(DelegationNotice {
                id: "d1".into(),
                offset_ms: 100.0,
            })
            .expect("单次委派");
        let work = bridge.claim_next().expect("开始规划");
        bridge
            .transcript(input("u2", "，放在右边", 100.0, 200.0))
            .expect("补字");
        assert!(work.cancellation.is_cancelled(), "补字撤销旧规划");
        assert!(
            bridge
                .complete(
                    &work,
                    DelegatedReport {
                        pending,
                        effects: !pending,
                        ..Default::default()
                    }
                )
                .is_none(),
            "旧版本结果不能口播"
        );
        assert!(
            bridge.claim_next().is_none(),
            "副作用或待核对结果不能触发自动重做，pending={pending}"
        );
    }
}
