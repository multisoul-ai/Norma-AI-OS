use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use std::time::Duration;

/// 正式 App 启动入口在后台准备真实 Claude PTY，进入会话时仍未安排任务。
///
/// 数据构造：独立临时 Workspace，一张 Claude 手机卡；使用本机 CLI 与新编译 Hook，不修改全局配置。
/// 超时 = 30 秒，覆盖冷启动；没有模型提示，因此不应出现 Thinking 或工具审批。
/// 执行过程：创建卡片 → 调用 App 启动入口 → 推进 GPUI → 等待准确 session → 关闭 Runtime。
/// 预期结果：返回会话 ID、原生终端存在、会话绑定稳定；创建时无 live，关闭后无待审批。
/// 此测试是 headless GPUI + 真实 CLI，不能作为真实 .app 或 iPhone 验收。
#[test]
#[ignore = "requires installed Claude Code and freshly built multisoul-agent-hook"]
fn e2e_mobile_controlled_claude_native_launch() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test_cx);
    let directory = tempfile::tempdir().expect("应创建隔离项目");
    let _sockets = enter_mobile_canvas(&app, cx, directory.path());
    let (node_id, receipt) = app.update(cx, |app, cx| {
        let node_id = app
            .runtime
            .create_mobile_agent_card(
                AgentProfile::Claude,
                None,
                None,
                CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("应创建卡片");
        assert_eq!(app.runtime.live_len(), 0, "创建卡片不能启动任务");
        let receipt = app
            .start_mobile_agent(node_id, cx)
            .expect("显式启动应被接受");
        (node_id, receipt)
    });
    let session_id = drain_until(
        &app,
        cx,
        |runtime| runtime.agent_session_id(node_id),
        Duration::from_secs(30),
    )
    .expect("后台准备应附着准确会话");
    let result = receipt
        .try_recv()
        .expect("启动应返回可查询的结果")
        .expect("启动应成功");
    assert_eq!(result, session_id, "结果与卡片身份必须一致");
    let cli_screen = drain_until(
        &app,
        cx,
        |runtime| {
            let runtime::NodeView::Term { snapshot, .. } = runtime.node_view(node_id)?;
            let text: String = snapshot.rows.iter().flatten().map(|cell| cell.c).collect();
            (text.contains("Claude") || text.contains("trust this folder")).then_some(text)
        },
        Duration::from_secs(30),
    )
    .expect("真实 CLI 必须出现界面，不能只验证 PTY 分配");
    assert!(
        !cli_screen.contains("unknown option"),
        "受控启动参数不得被 CLI 拒绝"
    );
    app.update(cx, |app, _| {
        assert!(
            app.runtime.node_view(node_id).is_some(),
            "受控会话必须保留原生终端"
        );
        assert_ne!(
            app.runtime.agent_task_state(node_id),
            Some(runtime::AgentTaskState::Thinking),
            "空闲启动不能发送假提示"
        );
        app.runtime.kill_all();
        assert!(
            app.runtime.mobile_approvals(node_id).is_empty(),
            "关窗后不能留下审批"
        );
    });
}

/// 正式 App/Runtime/Hook 链路处理真实 Claude 写入审批，拒绝与批准均核对磁盘效果。
///
/// 数据构造：独立测试数据库与一张 Claude 卡；工作目录是当前已在开发的项目，写入仅限独立临时文件。
/// 每轮最大等待 120s = Hook 审批时限，启动等待 30s；只允许精确的 printf 命令，不扩大权限。
/// 执行过程：后台启动 → 两轮独立任务 → 先拒绝再批准 → 检查文件与审计 → 读取完整对话 → 以旧卡元数据重开并续读。
/// 预期结果：拒绝文件不存在，批准文件内容准确；重复批准失败；每个真实请求均有独立的选择 / 提交 / transport 消费审计；历史保留两轮完整消息。
/// 此处使用真实 Claude Code 与用户现有模型；仍是 headless GPUI，不是真实 .app/iPhone。
#[test]
#[ignore = "requires real Claude Code, model access and freshly built Hook"]
fn e2e_mobile_claude_permission_executes_only_the_approved_request() {
    use norma::agent_hook::ApprovalDecision;
    use runtime::{AgentTaskState, NodeInput};
    use terminal::{Key, KeyInput};
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test_cx);
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("repository");
    let directory = tempfile::tempdir().expect("写入目标必须隔离");
    let _sockets = enter_mobile_canvas(&app, cx, workspace);
    let audit_store = app.read_with(cx, |app, _| {
        let project = app
            .project_store_for_e2e(workspace)
            .expect("隔离测试项目库");
        storage::ApprovalAuditStore::new(project.path().to_path_buf())
    });
    let node_id = app.update(cx, |app, cx| {
        let id = app
            .runtime
            .create_mobile_agent_card(
                AgentProfile::Claude,
                None,
                None,
                CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("应创建卡片");
        app.start_mobile_agent(id, cx).expect("应接受启动");
        id
    });
    let ready = drain_until(
        &app,
        cx,
        |runtime| (runtime.agent_task_state(node_id) == Some(AgentTaskState::Ready)).then_some(()),
        Duration::from_secs(30),
    );
    if ready.is_none() {
        let text = app.read_with(cx, |app, _| {
            app.runtime
                .node_view(node_id)
                .map(|view| {
                    let runtime::NodeView::Term { snapshot, .. } = view;
                    snapshot
                        .rows
                        .iter()
                        .flatten()
                        .map(|cell| cell.c)
                        .collect::<String>()
                })
                .unwrap_or_default()
        });
        app.update(cx, |app, _| app.runtime.kill_all());
        panic!("正式 SessionStart 未到达；当前测试终端：{text}");
    }
    let session = app
        .read_with(cx, |app, _| app.runtime.agent_session_id(node_id))
        .expect("应绑定真实会话");
    let run_id = app
        .read_with(cx, |app, _| {
            app.runtime.mobile_agent_run_id(node_id).map(str::to_owned)
        })
        .expect("受控运行身份");
    let mut cursor = None;
    let mut history = Vec::new();
    for (filename, decision) in [
        ("first.txt", ApprovalDecision::Deny),
        ("second.txt", ApprovalDecision::Allow),
    ] {
        let target = directory.path().join(filename);
        let command = format!("printf hello-from-norma > '{}'", target.display());
        let prompt = format!("This is the I/O integration check in crates/e2e/src/e2e_mobile_controlled_claude.rs. The temporary file contains ordinary greeting text for the test's content assertion. Use Bash exactly once to execute this exact command: {command}\nDo not read or change any other files. Do not use other tools. If permission is denied, do not retry. Reply briefly then stop.");
        app.update(cx, |app, _| {
            app.runtime
                .send_node_input(node_id, NodeInput::Paste(prompt))
                .expect("应输入真实任务");
        });
        // Same paste/Enter separation as the existing session reply queue (100ms).
        std::thread::sleep(Duration::from_millis(100));
        app.update(cx, |app, _| {
            app.runtime
                .send_node_input(
                    node_id,
                    NodeInput::Key(KeyInput {
                        key: Key::Enter,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .expect("应提交任务")
        });
        let mut saw_started = false;
        let approval = drain_until(
            &app,
            cx,
            |runtime| {
                if let Some(approval) = runtime.mobile_approvals(node_id).into_iter().next() {
                    return Some(Ok(approval));
                }
                match runtime.agent_task_state(node_id) {
                    Some(
                        AgentTaskState::Thinking
                        | AgentTaskState::RunningTool { .. }
                        | AgentTaskState::WaitingApproval { .. },
                    ) => saw_started = true,
                    Some(AgentTaskState::Done) if saw_started => return Some(Err(())),
                    Some(AgentTaskState::Failed { .. } | AgentTaskState::Ended) => {
                        return Some(Err(()))
                    }
                    _ => {}
                }
                None
            },
            Duration::from_secs(120),
        );
        let Some(Ok(approval)) = approval else {
            let diagnostic = app.read_with(cx, |app, _| {
                let text = app
                    .runtime
                    .node_view(node_id)
                    .map(|view| {
                        let runtime::NodeView::Term { snapshot, .. } = view;
                        snapshot
                            .rows
                            .iter()
                            .flatten()
                            .map(|cell| cell.c)
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                format!("{:?}: {text}", app.runtime.agent_task_state(node_id))
            });
            app.update(cx, |app, _| app.runtime.kill_all());
            panic!("未收到本轮正式审批；不会绕过 Provider 权限或外部 Hook。测试终端：{diagnostic}");
        };
        let request = approval.request();
        let exact = request.event.tool_name.as_deref() == Some("Bash")
            && request.event.raw_json["tool_input"]["command"] == command;
        if !exact {
            approval.cancel();
            app.update(cx, |app, _| app.runtime.kill_all());
            panic!("Provider 未请求精确的测试命令；已拒绝");
        }
        assert_eq!(
            request.event.session_id.as_deref(),
            Some(session.as_str()),
            "审批必须绑定当前卡片会话"
        );
        assert!(!target.exists(), "审批前不得写文件");
        assert!(
            app.update(cx, |app, _| app.runtime.decide_mobile_approval(
                node_id,
                &request.run_id,
                approval.id(),
                decision
            )),
            "首次决定应被接受"
        );
        assert!(
            !app.update(cx, |app, _| app.runtime.decide_mobile_approval(
                node_id,
                &request.run_id,
                approval.id(),
                ApprovalDecision::Allow
            )),
            "重复决定不能再被接受"
        );
        let done = drain_until(
            &app,
            cx,
            |runtime| {
                (runtime.agent_task_state(node_id) == Some(AgentTaskState::Done)).then_some(())
            },
            Duration::from_secs(120),
        );
        if done.is_none() {
            app.update(cx, |app, _| app.runtime.kill_all());
        }
        assert!(done.is_some(), "本轮任务必须真实结束");
        // 最多 20 条 = 本例两轮 × 每轮四个生命周期阶段，保留余量；IO 位于测试线程。
        let audit_deadline = std::time::Instant::now() + Duration::from_secs(2); // 本地 writer 200×10ms 预算。
        let rows = loop {
            app.update(cx, |app, _| app.runtime.drain_all());
            let rows = audit_store
                .list_run(&request.run_id, 0, 20)
                .expect("读取正式项目审计");
            if rows.iter().any(|row| {
                row.request_id == approval.id() && row.payload["state"].get("responded").is_some()
            }) {
                break rows;
            }
            assert!(
                std::time::Instant::now() < audit_deadline,
                "真实 Hook 消费后应保存审计"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let rows: Vec<_> = rows
            .iter()
            .filter(|row| row.request_id == approval.id())
            .collect();
        let decision_value = serde_json::to_value(decision).expect("选择 JSON");
        assert_eq!(rows.len(), 4, "每次调用应有四个独立阶段");
        assert_eq!(rows[0].payload["state"], "pending", "真实请求先入队");
        assert_eq!(
            rows[1].payload["state"]["chosen"], decision_value,
            "先保存用户选择"
        );
        assert_eq!(
            rows[2].payload["state"]["decided"], decision_value,
            "持久化后才提交"
        );
        assert_eq!(
            rows[3].payload["state"]["responded"], decision_value,
            "记录 transport 消费"
        );
        assert_eq!(
            rows[1].payload["request"]["event"]["session_id"], session,
            "审计绑定准确会话"
        );
        assert!(
            !rows.iter().any(|row| row.provider != "claude"),
            "不能混入 Codex 审批"
        );
        match decision {
            ApprovalDecision::Allow => assert_eq!(
                std::fs::read_to_string(&target).expect("批准后应有文件"),
                "hello-from-norma",
                "批准的写入内容应准确"
            ),
            ApprovalDecision::Deny => assert!(!target.exists(), "拒绝后文件必须仍不存在"),
        }
        if decision == ApprovalDecision::Deny {
            let receipt = app
                .update(cx, |app, cx| {
                    app.read_mobile_claude_history(node_id, &run_id, None, cx)
                })
                .expect("首轮后台读取");
            let first = drain_until(
                &app,
                cx,
                |_| receipt.try_recv().ok(),
                Duration::from_secs(5),
            )
            .expect("首轮应有回执")
            .expect("首轮历史");
            assert!(
                !first.has_more && !first.pending_tail,
                "单次工具测试的首轮必须在 20 条页内完整结束"
            );
            assert!(
                first.messages.iter().any(|m| m.role == "user"
                    && m.content.as_str().is_some_and(|s| s.contains("first.txt"))),
                "第一页应已包含真实第一轮"
            );
            assert!(
                !first
                    .messages
                    .iter()
                    .any(|m| m.content.as_str().is_some_and(|s| s.contains("second.txt"))),
                "第二轮尚未派发，不得预生成内容"
            );
            cursor = Some(first.cursor);
            history = first.messages;
        }
    }
    // 两个真实回合最多读取 10 页 * 20 条 = 200 条消息，避免错误 cursor 造成无限循环。
    let first_len = history.len();
    let mut reached_end = false;
    for _ in 0..10 {
        let receipt = app
            .update(cx, |app, cx| {
                app.read_mobile_claude_history(node_id, &run_id, cursor.take(), cx)
            })
            .expect("应调正式后台历史入口");
        let page = drain_until(
            &app,
            cx,
            |_| receipt.try_recv().ok(),
            Duration::from_secs(5),
        )
        .expect("本地历史读取应有回执")
        .expect("历史读取必须成功");
        let more = page.has_more || page.pending_tail;
        cursor = Some(page.cursor);
        history.extend(page.messages);
        if !more {
            reached_end = true;
            break;
        }
    }
    assert!(reached_end, "分页必须收敛到当前 EOF");
    assert!(
        !history[first_len..].iter().any(
            |m| m.role == "user" && m.content.as_str().is_some_and(|s| s.contains("first.txt"))
        ),
        "增量读取不能重复上一轮用户消息"
    );
    assert!(
        history.iter().any(
            |m| m.role == "user" && m.content.as_str().is_some_and(|s| s.contains("first.txt"))
        ),
        "完整历史必须包含第一次真实用户指令"
    );
    assert!(
        history
            .iter()
            .any(|m| m.role == "user"
                && m.content.as_str().is_some_and(|s| s.contains("second.txt"))),
        "完整历史必须包含第二次真实用户指令"
    );
    assert!(
        history.iter().any(|m| m.role == "assistant"
            && m.content
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "text"
                    && b["text"].as_str().is_some_and(|s| !s.trim().is_empty())))),
        "必须读到真实 assistant 正文，不能以用户提示或工具块充数"
    );
    assert!(
        history.iter().any(|m| m.role == "assistant"
            && m.content
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "tool_use"
                    && b["input"]["command"]
                        .as_str()
                        .is_some_and(|s| s.contains("second.txt"))))),
        "必须完整保留批准轮的真实工具调用"
    );
    app.update(cx, |app, _| app.runtime.kill_all());
    // 标记为已保存旧卡再走正式重开入口；历史来自上方两个真实回合，不发初始化提示。
    let receipt = app.update(cx, |app, cx| {
        let canvas::CanvasNodeKind::Agent(spec) =
            &mut app.runtime.document.node_mut(node_id).expect("原卡").kind
        else {
            panic!("Agent")
        };
        spec.mobile_control = false;
        app.reopen_mobile_agent(node_id, &session, cx)
            .expect("明确重开")
    });
    let restored = drain_until(
        &app,
        cx,
        |_| receipt.try_recv().ok(),
        Duration::from_secs(30),
    )
    .expect("重开回执")
    .expect("应恢复真实 Claude 历史");
    assert_eq!(restored, session, "不能换成新会话");
    assert!(
        drain_until(
            &app,
            cx,
            |runtime| (runtime.agent_task_state(node_id) == Some(AgentTaskState::Ready))
                .then_some(()),
            Duration::from_secs(30)
        )
        .is_some(),
        "恢复后应空闲，不应发出假任务"
    );
    let new_run = app.read_with(cx, |app, _| {
        app.runtime
            .mobile_agent_run_id(node_id)
            .expect("新运行")
            .to_owned()
    });
    assert_ne!(new_run, run_id, "重开必须更新控制身份");
    let receipt = app
        .update(cx, |app, cx| {
            app.read_mobile_claude_history(node_id, &new_run, cursor, cx)
        })
        .expect("从原游标续读");
    let page = drain_until(
        &app,
        cx,
        |_| receipt.try_recv().ok(),
        Duration::from_secs(5),
    )
    .expect("本地读取")
    .expect("应可续读");
    assert!(
        page.messages.is_empty(),
        "重开不应生成额外用户任务或 assistant 回复"
    );
    app.update(cx, |app, _| {
        assert!(
            app.runtime.mobile_claude_history(node_id, &run_id).is_err(),
            "旧控制身份不能读取重开后的会话"
        );
        assert!(
            app.runtime.node_view(node_id).is_some(),
            "Claude 保留原生终端"
        );
        app.runtime.kill_all();
    });
}
