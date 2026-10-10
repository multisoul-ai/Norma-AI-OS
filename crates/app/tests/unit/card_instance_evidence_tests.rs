use super::test_support::{accept, observe, with_cards};
use agent::card_instance::CardInstanceStage;
use agent::card_instance::CardInstanceTracking;
use std::time::{Duration, Instant};

/// 新实例队列只串行正文传输；A 未结束可提交 B，但 A 的结果不能推进 B。
/// 数据构造：第二卡绑定一个 session，A/B 两项、两个独立 turn，输入总量 2×(正文+Enter)=4 次。
/// 执行过程：提交 A 并接收 start；A 运行时提交 B；处理 A 终态，再处理 B 的独立 start/终态。
/// 预期结果：B 在自身 start 前 Submitted；A 完成只影响 A；没有重发，非目标始终零输入。
#[test]
fn live_delivery_and_hook_results_keep_each_task_independent() {
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            app.runtime
                .bind_discovered_agent_session(nodes[1], "session");
            accept(
                app,
                nodes[1],
                "a",
                "正文A",
                CardInstanceTracking::Managed,
                None,
            );
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            assert!(
                observe(app, nodes[1], "UserPromptSubmit", "turn-a", "正文A"),
                "A start 应精确绑定"
            );
            accept(
                app,
                nodes[1],
                "b",
                "正文B",
                CardInstanceTracking::Managed,
                None,
            );
            app.advance_card_instance_tasks_at(now + Duration::from_millis(102));
            app.advance_card_instance_tasks_at(now + Duration::from_millis(203));
            assert_eq!(
                app.card_instance_receipt("a").expect("A").stage,
                CardInstanceStage::Running,
                "提交 B 不结束 A"
            );
            assert_eq!(
                app.card_instance_receipt("b").expect("B").stage,
                CardInstanceStage::Submitted,
                "B 不能继承卡片运行态"
            );
            assert!(observe(app, nodes[1], "Stop", "turn-a", ""), "A 终态应接受");
            assert_eq!(
                app.card_instance_receipt("b").expect("B").stage,
                CardInstanceStage::Submitted,
                "A 完成不能启动 B"
            );
            assert!(
                observe(app, nodes[1], "UserPromptSubmit", "turn-b", "正文B"),
                "B 自身 start 才运行"
            );
            assert!(
                !observe(app, nodes[1], "Stop", "turn-a", ""),
                "A 迟到终态不能完成 B"
            );
            assert_eq!(
                app.card_instance_receipt("b").expect("B").stage,
                CardInstanceStage::Running,
                "B 仍在独立运行"
            );
            assert!(
                observe(app, nodes[1], "Stop", "turn-b", ""),
                "B 自身终态应接受"
            );
            app.advance_card_instance_tasks_at(now + Duration::from_secs(1));
            assert_eq!(
                app.card_instance_receipt("b")
                    .expect("B")
                    .bound_turn
                    .as_ref()
                    .expect("B 绑定")
                    .turn_id,
                "turn-b",
                "精确取消与结果仍使用原 turn"
            );
        });
        assert_eq!(
            writes[1].lock().expect("目标输入").len(),
            4,
            "A/B 只各发送一次正文和 Enter，无终态重发"
        );
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "所有证据处理不向另一卡片写入"
        );
    });
}

/// 精确运行取消仅中断原 turn；排队 B 与较新的用户直接 turn 不可被错中断。
/// 数据构造：A 绑定 turn-a，B 仅 Accepted；随后直接用户 turn-direct 替换当前活动。
/// 执行过程：尝试中断 B 失败，直接新 turn 后尝试中断 A 失败；全程读取 PTY 写入数。
/// 预期结果：两次拒绝都零 Ctrl-C；A 原引用/绑定不改投直接新任务，也不猜其已完成。
#[test]
fn cancellation_never_interrupts_another_queued_or_direct_turn() {
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            app.runtime
                .bind_discovered_agent_session(nodes[1], "session");
            accept(
                app,
                nodes[1],
                "a",
                "正文A",
                CardInstanceTracking::Managed,
                None,
            );
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            assert!(
                observe(app, nodes[1], "UserPromptSubmit", "turn-a", "正文A"),
                "A 应运行"
            );
            accept(
                app,
                nodes[1],
                "b",
                "正文B",
                CardInstanceTracking::Managed,
                None,
            );
            let queued = app
                .interrupt_card_instance_assignment("b")
                .expect_err("B 未运行不能 Ctrl-C");
            assert_eq!(queued.code, "not_running", "明确 B 未开始");
            assert!(
                !observe(
                    app,
                    nodes[1],
                    "UserPromptSubmit",
                    "turn-direct",
                    "用户直接任务"
                ),
                "直接任务不属于 A/B"
            );
            assert!(
                !observe(app, nodes[1], "UserPromptSubmit", "turn-a", "正文A"),
                "A 重复旧 start 不能回退取消使用的最新 turn"
            );
            let replaced = app
                .interrupt_card_instance_assignment("a")
                .expect_err("新直接 turn 不能中断");
            assert_eq!(replaced.code, "turn_changed", "原任务已不是当前 turn");
            assert!(replaced.zero_input_proven, "拒绝在写入前，零新副作用");
            assert_eq!(
                app.card_instance_receipt("a")
                    .expect("A")
                    .bound_turn
                    .as_ref()
                    .expect("原绑定")
                    .turn_id,
                "turn-a",
                "不能重绑定较新直接任务"
            );
        });
        assert_eq!(
            writes[1].lock().expect("输入").len(),
            2,
            "仅 A 的正文与 Enter，两次取消不得发 Ctrl-C"
        );
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "其他实例也不被中断"
        );
    });
}

/// 已观察的旧直接 turn 即使正文与新任务相同，也不能因迟到 start/Stop 被绑定给新任务。
/// 数据构造：无空间任务时先观察直接 turn-old，再观察较新的 turn-direct；B 使用与 old 相同正文。
/// 执行过程：Hook 经生产 drain 接收旧直接活动，提交 B，重到 old start 和 Stop，再送 B 独立新 turn。
/// 预期结果：重复旧 start/Stop 零提升 B；仅新 turn 开始/完成 B，无新输入和其他卡片副作用。
#[test]
fn replayed_direct_turn_with_equal_text_cannot_bind_new_assignment() {
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            assert!(
                !observe(app, nodes[1], "UserPromptSubmit", "turn-old", "相同正文"),
                "旧直接任务没有 assignment"
            );
            assert!(
                !observe(app, nodes[1], "Stop", "turn-old", ""),
                "旧直接终态不属于新派工"
            );
            assert!(
                !observe(
                    app,
                    nodes[1],
                    "UserPromptSubmit",
                    "turn-direct",
                    "较新直接正文"
                ),
                "历史不只记录最新 turn"
            );
            assert!(
                !observe(app, nodes[1], "Stop", "turn-direct", ""),
                "较新直接任务结束"
            );
            accept(
                app,
                nodes[1],
                "b",
                "相同正文",
                CardInstanceTracking::Managed,
                None,
            );
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            assert!(
                !observe(app, nodes[1], "UserPromptSubmit", "turn-old", "相同正文"),
                "旧已观察 turn 不可绑定 B"
            );
            assert!(
                !observe(app, nodes[1], "Stop", "turn-old", ""),
                "旧终态不完成 B"
            );
            let before = app.card_instance_receipt("b").expect("B");
            assert_eq!(
                before.stage,
                CardInstanceStage::Submitted,
                "旧重复证据不能升级新任务"
            );
            assert_eq!(before.bound_turn, None, "旧 turn 不可归属新任务");
            assert!(
                observe(app, nodes[1], "UserPromptSubmit", "turn-b", "相同正文"),
                "B 新 turn 可绑定"
            );
            assert!(
                observe(app, nodes[1], "Stop", "turn-b", ""),
                "仅 B 自身终态完成"
            );
        });
        assert_eq!(
            writes[1].lock().expect("目标").len(),
            2,
            "只有 B 唯一正文和 Enter"
        );
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "历史证据不可改投其他实例"
        );
    });
}

/// 同批旧与新任务的 Hook 必须按完整 turn 认领，不能以 node 过滤吞掉旧协调器事件。
/// 数据构造：一张目标卡 B 已提交；同批旧 turn-a 的 start/Stop 和 B turn-b 的 start/Stop。
/// 执行过程：四项进入真实 drain，由生产 claim/filter seam 核对，再送 B 的重复错误终态。
/// 预期结果：B 独立成功；未归属空间任务的 A start/Stop 留给旧协调器；B 自身重复终态不转交旧队列。
#[test]
fn batch_hook_claims_preserve_unrelated_legacy_turns_on_the_same_card() {
    use crate::settings::agent_hooks::AgentHookServer;
    use norma::agent_hook::{parse_hook_event, HookSource};
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            app.runtime
                .bind_discovered_agent_session(nodes[1], "session");
            accept(
                app,
                nodes[1],
                "b",
                "正文B",
                CardInstanceTracking::Managed,
                None,
            );
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            let (sender, receiver) = crossbeam_channel::unbounded();
            let server = AgentHookServer::for_test_events(receiver);
            let event = |name: &str, turn: &str, prompt: &str| {
                parse_hook_event(
                    HookSource::OpenCode,
                    nodes[1],
                    serde_json::json!({
                        "hook_event_name":name, "session_id":"session", "turn_id":turn,
                        "prompt":prompt
                    }),
                )
                .expect("Hook 载荷")
            };
            for value in [
                event("UserPromptSubmit", "turn-a", "旧任务正文"),
                event("Stop", "turn-a", ""),
                event("UserPromptSubmit", "turn-b", "正文B"),
                event("Stop", "turn-b", ""),
            ] {
                sender.send(value).expect("生产 Hook 队列");
            }
            let mut report = server.drain_into_runtime(&mut app.runtime);
            assert_eq!(report.events.len(), 4, "四项真实 drain 均已接受");
            assert!(
                app.reconcile_card_instance_hook_report(&mut report),
                "B 精确开始与终态应变更"
            );
            assert_eq!(
                app.card_instance_receipt("b").expect("B").stage,
                CardInstanceStage::Succeeded,
                "B 仅由自己的 turn 成功"
            );
            assert_eq!(
                report.started_tasks.len(),
                1,
                "旧 A start 不能被同 node 的 B 吞掉"
            );
            assert_eq!(
                report.started_tasks[0].identity.turn_id.as_deref(),
                Some("turn-a"),
                "只保留未归属新空间任务的 A start"
            );
            assert_eq!(
                report.terminal_tasks.len(),
                1,
                "旧 A 终态仍须交给兼容协调器"
            );
            assert_eq!(
                report.terminal_tasks[0].identity.turn_id.as_deref(),
                Some("turn-a"),
                "过滤精确 B，保留同批 A"
            );
            sender
                .send(event("StopFailure", "turn-b", ""))
                .expect("重复不同终态");
            let mut repeated = server.drain_into_runtime(&mut app.runtime);
            assert!(
                !app.reconcile_card_instance_hook_report(&mut repeated),
                "重复终态不能改写 B 成功"
            );
            assert!(
                repeated.terminal_tasks.is_empty(),
                "B 已归属本 seam，重复终态也不能误结束旧队列 A"
            );
        });
        assert_eq!(
            writes[1].lock().expect("目标").len(),
            2,
            "批量证据不产生任何补发"
        );
        assert!(
            writes[0].lock().expect("首卡").is_empty(),
            "所有非目标零输入"
        );
    });
}

/// 同批接收原任务终态后再切换 session，不能用批末身份否定此前已通过守卫的准确证据。
/// 数据构造：B 已绑定 session/turn-b，接收 Stop 后在同批接收 session-new 的 SessionStart。
/// 执行过程：生产 drain 应用两项，再按接收时快照核对原 B 和过滤其已归属终态。
/// 预期结果：原 B Succeeded，原冻结会话不变；其 Stop 不回流旧协调器；新 session 仍是 Runtime 当前值。
#[test]
fn accepted_terminal_before_same_batch_session_change_keeps_its_original_identity() {
    use crate::settings::agent_hooks::AgentHookServer;
    use norma::agent_hook::{parse_hook_event, HookSource};
    with_cards(|app, cx, nodes, writes| {
        let now = Instant::now();
        app.update(cx, |app, _| {
            app.runtime.bind_discovered_agent_session(nodes[1], "session");
            accept(app, nodes[1], "b", "正文B", CardInstanceTracking::Managed, None);
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            assert!(observe(app, nodes[1], "UserPromptSubmit", "turn-b", "正文B"), "B 原 turn 绑定");
            let (sender, receiver) = crossbeam_channel::unbounded();
            let server = AgentHookServer::for_test_events(receiver);
            for value in [serde_json::json!({"hook_event_name":"Stop","session_id":"session","turn_id":"turn-b"}),
                serde_json::json!({"hook_event_name":"SessionStart","session_id":"session-new"})] {
                sender.send(parse_hook_event(HookSource::OpenCode, nodes[1], value).expect("原始 Hook")).expect("生产队列");
            }
            let mut report = server.drain_into_runtime(&mut app.runtime);
            assert_eq!(report.events.len(), 2, "原终态和新会话都经真实 drain 接受");
            assert!(app.reconcile_card_instance_hook_report(&mut report), "接收时有效的 B 终态不能被批末 session 否定");
            let receipt = app.card_instance_receipt("b").expect("B 原回执");
            assert_eq!(receipt.stage, CardInstanceStage::Succeeded, "B 保留自己的原终态");
            assert_eq!(receipt.target.execution.session_id.as_deref(), Some("session"), "核对不能刷新原冻结身份");
            assert!(report.terminal_tasks.is_empty(), "B 原 Stop 已归属新 seam，不能误结束旧队列");
            assert_eq!(app.runtime.agent_session_id(nodes[1]).as_deref(), Some("session-new"), "当前 Runtime 正常进入新 session");
        });
        assert_eq!(
            writes[1].lock().expect("目标").len(),
            2,
            "核对和切会话都不重放 B"
        );
        assert!(writes[0].lock().expect("首卡").is_empty(), "其他卡片零输入");
    });
}
