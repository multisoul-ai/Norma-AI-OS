use norma::mobile::{CardSnapshot, ProjectChange, ProjectProjection, ProjectSnapshot};

fn project() -> ProjectSnapshot {
    let card = |node_id| CardSnapshot {
        node_id,
        profile: "Codex".into(),
        title: format!("Card {node_id}"),
        mobile_control: true,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
        state: "idle".into(),
        run_id: None,
        session_id: None,
        turn_id: None,
        task_title: None,
    };
    ProjectSnapshot {
        workspace_id: "project-a".into(),
        display_name: "A".into(),
        state: "open".into(),
        route_id: Some("route-a".into()),
        card_count: Some(2),
        cards: Some(vec![card(1), card(2)]),
    }
}

/// 项目投影仅发送实际变化，稳定顺序与删除被明确表达；窗口变更必须回到全量快照。
///
/// 数据构造：A/route-a 包含空卡1/2；一次把2改为ready并移除1，随后route-b替代原窗口。
/// 执行过程：初始sequence0 → 相同快照无事件 → 变化sequence1 → 心跳保留1 → 换窗口sequence2全量。
/// 预期结果：delta只有卡2、明确删除1，order不含1；重开不能用旧route的增量认领新窗口。
#[test]
fn projection_sends_exact_changes_and_resets_on_route_replacement() {
    let first = project();
    let mut stream = ProjectProjection::new("stream-a".into(), first.clone());
    assert_eq!(stream.snapshot().sequence, 0, "新连接从明确全量基线开始");
    assert!(
        stream.update(first.clone()).expect("未溢出").is_none(),
        "相同投影不生成伪进度"
    );
    let mut next = first.clone();
    let cards = next.cards.as_mut().expect("卡片");
    cards.remove(0);
    cards[0].state = "ready".into();
    next.card_count = Some(1);
    let delta = stream
        .update(next.clone())
        .expect("更新")
        .expect("真实变化");
    assert_eq!(delta.sequence, 1, "一次投影变化仅推进一次");
    let ProjectChange::Delta {
        upserted,
        removed,
        order,
        ..
    } = delta.change
    else {
        panic!("同窗口应增量");
    };
    assert_eq!(upserted[0].node_id, 2, "只携带变化的卡2");
    assert_eq!(upserted.len(), 1, "不发送已删除卡或未变化卡");
    assert_eq!(removed, vec![1], "卡1必须明确删除");
    assert_eq!(order, vec![2], "新顺序不能保留卡1");
    assert_eq!(stream.heartbeat().sequence, 1, "心跳不是新的状态变化");
    next.route_id = Some("route-b".into());
    let changed = stream
        .update(next.clone())
        .expect("窗口变化")
        .expect("应有事件");
    assert_eq!(changed.sequence, 2, "窗口变更继续有序");
    assert_eq!(
        changed.change,
        ProjectChange::Snapshot { project: next },
        "窗口替换必须发送全量，不能复用旧卡身份"
    );
    assert_eq!(
        stream.snapshot().workspace_id,
        "project-a",
        "来源项目始终明确"
    );
}

/// 仅计时变化仍须通过WSS增量送达，但不能使旧客户端的命令目标失效。
///
/// 数据构造：卡1由无耗时变为起点1000ms，随后完成耗时60000ms=60s；卡2保持不变。
/// 执行过程：保留原命令目标 → 增加计时 → 更新相同快照 → 完成计时。
/// 预期结果：两次真实变化各推送卡1，卡2不重复发送；旧目标相等且重复快照无网络增量。
#[test]
fn timing_updates_display_without_invalidating_command_target() {
    let initial = project();
    let expected = initial.cards.as_ref().expect("卡片")[0].clone();
    let mut stream = ProjectProjection::new("timing-stream".into(), initial.clone());
    let mut current = initial;
    current.cards.as_mut().expect("卡片")[0].task_timing = Some(norma::mobile::TaskTiming {
        started_at_unix_ms: 1000,
        completed_elapsed_ms: None,
    });
    assert_eq!(
        current.cards.as_ref().expect("卡片")[0],
        expected,
        "展示信息不能改变操作目标身份"
    );
    let delta = stream
        .update(current.clone())
        .expect("更新")
        .expect("仅计时变化也须投递");
    let ProjectChange::Delta {
        upserted, removed, ..
    } = delta.change
    else {
        panic!("应为增量")
    };
    assert_eq!(upserted.len(), 1, "不能重复发送未变卡2");
    assert_eq!(upserted[0].node_id, 1, "只更新卡1");
    assert!(removed.is_empty(), "不能删除卡片");
    assert!(
        stream.update(current.clone()).expect("相同快照").is_none(),
        "时钟不能制造每秒流量"
    );
    current.cards.as_mut().expect("卡片")[0]
        .task_timing
        .as_mut()
        .expect("计时")
        .completed_elapsed_ms = Some(60 * 1000);
    let final_event = stream
        .update(current)
        .expect("结束")
        .expect("冻结耗时须投递");
    assert_eq!(final_event.sequence, 2, "开始与完成各发送一次");
}

/// 仅步骤变化和字段撤销都须生成增量，不破坏旧客户端的操作目标。
///
/// 数据构造：卡1有2步、完成1步；卡2不变；随后完整计划未知而移除字段。
/// 执行过程：无进度 → 1/2 → 相同快照 → 移除进度。
/// 预期结果：两次显示变化各推送卡1，重复不推送；旧操作目标依旧匹配。
#[test]
fn progress_updates_and_removal_are_display_deltas() {
    let initial = project();
    let expected = initial.cards.as_ref().expect("卡片")[0].clone();
    let mut stream = ProjectProjection::new("progress-stream".into(), initial.clone());
    let mut current = initial;
    current.cards.as_mut().expect("卡片")[0].task_progress = Some(norma::mobile::TaskProgress {
        completed: 1,
        total: 2,
        current_step: Some("Checking beta".into()),
    });
    assert_eq!(
        current.cards.as_ref().expect("卡片")[0],
        expected,
        "旧操作目标忽略进度"
    );
    let event = stream
        .update(current.clone())
        .expect("更新")
        .expect("步骤须及时投递");
    let ProjectChange::Delta {
        upserted, removed, ..
    } = event.change
    else {
        panic!("应增量");
    };
    assert_eq!(upserted.len(), 1, "未变卡2不能重传");
    assert_eq!(
        upserted[0].task_progress.as_ref().expect("进度").completed,
        1,
        "完成数必须送达"
    );
    assert!(removed.is_empty(), "不能删除卡片");
    assert!(
        stream.update(current.clone()).expect("相同快照").is_none(),
        "重复不生成流量"
    );
    current.cards.as_mut().expect("卡片")[0].task_progress = None;
    assert_eq!(
        stream
            .update(current)
            .expect("移除")
            .expect("撤销错误数字")
            .sequence,
        2,
        "字段移除也必须送达"
    );
}

/// 摘录新增、替换、撤销都更新原卡，但相同文本与心跳不重传正文。
/// 数据构造：卡1有两条短摘要，卡2不变，旧操作目标缺task_summary；3次真实变化=3个序号。
/// 执行过程：添加 → 重复 → 替换 → 移除；比较旧目标和新的展示投影。
/// 预期结果：只发送卡1，旧目标继续有效，相同投影无事件，心跳无正文，移除后无残留。
#[test]
fn summary_updates_are_display_only_deltas() {
    let mut current = project();
    let target = current.cards.as_ref().expect("cards")[0].clone();
    let mut stream = ProjectProjection::new("summary-stream".into(), current.clone());
    current.cards.as_mut().expect("cards")[0].task_summary = Some("正在检查连接".into());
    let event = stream
        .update(current.clone())
        .expect("update")
        .expect("摘要变化");
    let ProjectChange::Delta {
        upserted, removed, ..
    } = event.change
    else {
        panic!("应为增量")
    };
    assert_eq!(upserted.len(), 1, "未变化的卡2不能重传");
    assert_eq!(upserted[0], target, "展示字段不能使旧操作目标失效");
    assert_eq!(upserted[0].node_id, 1, "只有卡1变化");
    assert_eq!(
        upserted[0].task_summary.as_deref(),
        Some("正在检查连接"),
        "摘要必须送达"
    );
    assert!(removed.is_empty(), "不能删除原卡");
    assert!(
        stream.update(current.clone()).expect("same").is_none(),
        "相同文本无重复流量"
    );
    let heartbeat = serde_json::to_value(stream.heartbeat()).expect("encode");
    assert!(
        !heartbeat.to_string().contains("正在检查连接"),
        "心跳不能携带正文"
    );
    current.cards.as_mut().expect("cards")[0].task_summary = Some("连接检查通过".into());
    assert_eq!(
        stream
            .update(current.clone())
            .expect("replace")
            .expect("替换")
            .sequence,
        2,
        "替换必须及时投递"
    );
    current.cards.as_mut().expect("cards")[0].task_summary = None;
    let removed = stream.update(current).expect("remove").expect("撤销");
    let ProjectChange::Delta { upserted, .. } = removed.change else {
        panic!("应为增量")
    };
    assert_eq!(removed.sequence, 3, "三次真实展示变化");
    assert_eq!(upserted[0].task_summary, None, "新回合不能残留旧摘要");
}
