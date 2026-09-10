use crate::harness::{drain_until, enter_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use runtime::{AgentTaskState, NodeInput};
use std::time::Duration;
use terminal::{Key, KeyInput};

/// 正式 App 中断入口须等待真实 Claude 新增回合证据，原生终端保持可用。
///
/// 数据构造：真实 Claude 卡和一个临时文件的精确 printf 请求；不批准任何写入。
/// 执行过程：等 SessionStart(30s) → 真实任务等审批(120s=Hook TTL) → 正式中断 → 等回执(30s) → 读历史并回收。
/// 预期结果：回执明确 interrupted、状态 Ended、审批撤销且目标文件不存在；保留原生 TUI。
/// 此测试是 headless GPUI + 真实 CLI，不是真实 .app/iPhone；保留用户外部 Hook。
#[test]
#[ignore = "requires real Claude Code and model access"]
fn e2e_mobile_claude_native_interrupt_evidence() {
    run_interrupt("approval");
}

/// 生成中的 Escape 应返回准确回合结果，不能只测审批界面。
///
/// 数据构造：无工具的 3000 词长回答，要求首行由指定的五个字母拼接，提示自身不含完整标记。
/// 执行过程：等待真实回复首行（最多 120s = 本套 Provider 等待上限）→ 正式中断 → 读取回执和历史。
/// 预期结果：回执 interrupted、状态 Ended，保留原生终端；没有工具写入。
/// 使用 headless GPUI 与真实 CLI，保留外部 Hook。
#[test]
#[ignore = "requires real Claude Code, model access and freshly built Hook"]
fn e2e_mobile_claude_interrupt_during_generation() {
    run_interrupt("thinking");
}

/// 实际工具执行开始后中断，必须阻止延迟的第二次临时写入。
///
/// 数据构造：精确命令先写 started、sleep(5s)、后写 output；只批准这一次命令。
/// 执行过程：观察 started 文件 → 正式中断 → 等回执 → 继续观察 6s（5s 等待 + 1s 余量）。
/// 预期结果：started 存在，output 不存在，回执 interrupted；保留外部 Hook 与原生终端。
#[test]
#[ignore = "requires real Claude Code, model access and freshly built Hook"]
fn e2e_mobile_claude_interrupt_during_tool_execution() {
    run_interrupt("execution");
}

/// 原生桌面先取消审批后，手机通过正式 Runtime 入口发来的旧批准必须失败。
///
/// 数据构造：一张真实 Claude 卡、一个指定临时文件的 printf 请求；不批准任何写入。
/// 执行过程：等真实 PermissionRequest → 原生 Escape → 最多 5s 等 transport 撤销 → 手机迟到 Allow。
/// 预期结果：原 Hook 为 Cancelled，迟到 Allow 不被接受，没有响应与磁盘写入；保留外部 Hook。
#[test]
#[ignore = "requires real Claude Code, model access and freshly built Hook"]
fn e2e_mobile_claude_desktop_cancellation_rejects_late_mobile_approval() {
    run_interrupt("desktop");
}

fn run_interrupt(phase: &str) {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test_cx);
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("workspace");
    let directory = tempfile::tempdir().expect("隔离目录");
    let target = directory.path().join("output.txt");
    enter_canvas(&app, cx, workspace);
    let (id, launch_receipt) = app.update(cx, |app, cx| {
        let id = app
            .runtime
            .create_mobile_agent_card(
                AgentProfile::Claude,
                None,
                None,
                CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("空卡");
        let receipt = app.start_mobile_agent(id, cx).expect("启动");
        (id, receipt)
    });
    let ready = drain_until(
        &app,
        cx,
        |r| (r.agent_task_state(id) == Some(AgentTaskState::Ready)).then_some(()),
        Duration::from_secs(30),
    );
    if ready.is_none() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    assert!(
        ready.is_some(),
        "真实会话应就绪，启动回执：{:?}",
        launch_receipt.try_recv()
    );
    let started = directory.path().join("started.txt");
    let command = if phase == "execution" {
        format!(
            "printf started > '{}'; sleep 5; printf hello-from-norma > '{}'",
            started.display(),
            target.display()
        )
    } else {
        format!("printf hello-from-norma > '{}'", target.display())
    };
    let task = if phase == "thinking" {
        "Without tools, begin your reply with the five capital letters R E A D Y joined without spaces on their own line. Then write at least 3000 words explaining database transactions in detail.".to_owned()
    } else {
        format!("Use Bash exactly once to run this exact command: {command}. Do not change the command, retry, or use another tool. After the result, reply briefly and stop.")
    };
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(id, NodeInput::Text(task))
            .expect("真实任务")
    });
    // 100ms = 既有原生终端文本写入到 Enter 的提交间隔，保持 bracketed paste 消费顺序。
    std::thread::sleep(Duration::from_millis(100));
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(
                id,
                NodeInput::Key(KeyInput {
                    key: Key::Enter,
                    ctrl: false,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("提交任务")
    });
    let (run, turn, pending) = if phase == "thinking" {
        let identity = drain_until(
            &app,
            cx,
            |r| {
                if r.agent_task_state(id) != Some(AgentTaskState::Thinking) {
                    return None;
                }
                let run = r.mobile_agent_run_id(id)?.to_owned();
                let turn = r.mobile_claude_turn_id(id, &run)?.to_owned();
                Some((run, turn))
            },
            Duration::from_secs(30),
        );
        if identity.is_none() {
            app.update(cx, |app, _| app.runtime.kill_all());
        }
        let (run, turn) = identity.expect("思考中的真实身份");
        let generating = drain_until(
            &app,
            cx,
            |r| {
                let runtime::NodeView::Term { snapshot, .. } = r.node_view(id)?;
                let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
                (text.contains("READY") && r.mobile_claude_turn_id(id, &run) == Some(&turn))
                    .then_some(())
            },
            Duration::from_secs(120),
        );
        if generating.is_none() {
            app.update(cx, |app, _| app.runtime.kill_all());
        }
        assert!(
            generating.is_some(),
            "必须观察到真实回复首行，不能用 UserPromptSubmit 推断模型已开始生成"
        );
        (run, turn, None)
    } else {
        let pending = drain_until(
            &app,
            cx,
            |r| r.mobile_approvals(id).into_iter().next(),
            Duration::from_secs(120),
        );
        if pending.is_none() {
            app.update(cx, |app, _| app.runtime.kill_all());
        }
        let pending = pending.expect("应有真实审批");
        assert_eq!(
            pending.request().event.raw_json["tool_input"]["command"],
            command,
            "只验证指定临时命令"
        );
        let run = pending.request().run_id.clone();
        let turn = pending
            .request()
            .event
            .turn_id
            .clone()
            .expect("真实回合 ID");
        if phase == "execution" {
            assert!(
                app.update(cx, |app, _| app.runtime.decide_mobile_approval(
                    id,
                    &run,
                    pending.id(),
                    norma::agent_hook::ApprovalDecision::Allow
                )),
                "只批准精确命令一次"
            );
            let executing = drain_until(
                &app,
                cx,
                |_| started.exists().then_some(()),
                Duration::from_secs(10),
            );
            if executing.is_none() {
                app.update(cx, |app, _| app.runtime.kill_all());
            }
            assert!(executing.is_some(), "started 文件证明工具确已开始执行");
            assert!(!target.exists(), "中断应在延迟 output 写入前发起");
        }
        (run, turn, Some(pending))
    };
    if phase == "desktop" {
        let pending = pending.expect("真实审批");
        app.update(cx, |app, _| {
            app.runtime
                .send_node_input(
                    id,
                    NodeInput::Key(KeyInput {
                        key: Key::Escape,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .expect("原生桌面取消")
        });
        let cancelled = drain_until(
            &app,
            cx,
            |_| {
                (pending.status(std::time::Instant::now())
                    == norma::agent_hook::ApprovalStatus::Cancelled)
                    .then_some(())
            },
            Duration::from_secs(5),
        );
        let late = app.update(cx, |app, _| {
            app.runtime.decide_mobile_approval(
                id,
                &run,
                pending.id(),
                norma::agent_hook::ApprovalDecision::Allow,
            )
        });
        let response = pending.take_response(std::time::Instant::now());
        app.update(cx, |app, _| app.runtime.kill_all());
        assert!(cancelled.is_some(), "原生桌面操作应撤销该次 Hook");
        assert!(!late, "手机不能批准电脑端已经撤销的调用");
        assert!(response.is_none(), "撤销后不能补发任何 Hook 响应");
        assert!(!target.exists(), "迟到批准不能造成临时文件写入");
        std::fs::write("/tmp/norma-ios-claude-desktop-competition-v12.json", serde_json::to_vec_pretty(&serde_json::json!({
            "run_id":run,"turn_id":turn,"request_id":pending.id(),"hook_cancelled":cancelled.is_some(),
            "late_mobile_choice_accepted":late,"hook_response":response,"target_exists":target.exists()
        })).expect("证据 JSON")).expect("保存竞争证据");
        return;
    }
    let interrupt = app
        .update(cx, |app, cx| {
            app.interrupt_mobile_terminal(id, &run, &turn, cx)
        })
        .expect("正式中断入口");
    let outcome = drain_until(
        &app,
        cx,
        |_| interrupt.try_recv().ok(),
        Duration::from_secs(30),
    );
    let cancelled = drain_until(
        &app,
        cx,
        |r| r.mobile_approvals(id).is_empty().then_some(()),
        Duration::from_secs(5),
    );
    let receipt = app
        .update(cx, |app, cx| {
            app.read_mobile_claude_history(id, &run, None, cx)
        })
        .expect("完整历史入口");
    let history = drain_until(
        &app,
        cx,
        |_| receipt.try_recv().ok(),
        Duration::from_secs(5),
    )
    .expect("读回执")
    .expect("完整页");
    let (state, screen) = app.read_with(cx, |app, _| {
        let runtime::NodeView::Term { snapshot, .. } = app.runtime.node_view(id).expect("保留 TUI");
        (
            format!("{:?}", app.runtime.agent_task_state(id)),
            snapshot
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.c)
                .collect::<String>(),
        )
    });
    let evidence = serde_json::json!({"state":state,"outcome":outcome,"phase":phase,"transcript_path":pending.as_ref().map(|p| &p.request().event.raw_json["transcript_path"]),"cancelled":cancelled.is_some(),"approval_status":pending.as_ref().map(|p| format!("{:?}",p.status(std::time::Instant::now()))),"history":history.messages,"screen":screen});
    std::fs::write(
        format!("/tmp/norma-ios-claude-native-interrupt-{phase}-v11.json"),
        serde_json::to_vec_pretty(&evidence).expect("证据 JSON"),
    )
    .expect("保存证据");
    if phase == "execution" {
        drain_until(&app, cx, |_| None::<()>, Duration::from_secs(6));
        assert!(started.exists(), "保留实际执行证据");
    }
    app.update(cx, |app, _| app.runtime.kill_all());
    assert!(
        cancelled.is_some(),
        "原生中断应撤销审批；实际状态见本轮证据"
    );
    assert!(!target.exists(), "中断后不能执行后续 output 写入");
    let outcome = outcome
        .expect("应在期限内得到最终回执")
        .expect("真实中断应成功");
    assert_eq!(outcome["status"], "interrupted", "只能根据新增历史报告中断");
    assert_eq!(outcome["turnId"], turn, "回执必须属于准确回合");
    assert_eq!(state, "Some(Ended)", "不能残留 WaitingApproval");
}
