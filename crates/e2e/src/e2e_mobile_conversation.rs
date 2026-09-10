use crate::harness::{drain_until, make_app_real};
use app::MultisoulApp;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Swift / iOS 测试 host 通过正式 WSS / GPUI 路由读取真实 Claude 回复，并拒绝旧运行。
///
/// 数据构造：隔离 App 数据库和受控 Claude，工作目录复用已经信任的当前仓库；唯一提示禁止工具、文件读取或修改。
/// 执行过程：后台启动 → Ready → 标准 paste / Enter → 真实 Done → 测试手机只读配对 → 读取正文 / 拒绝旧 run → kill。
/// 预期结果：返回固定真实回复且无模型工具执行；原生客户端读取准确会话，关闭后无 live；不替代真机或真实 .app。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn e2e_mobile_claude_conversation_native_transport() {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式原生客户端");
    let mut test = TestAppContext::single();
    let sockets = tempfile::tempdir().expect("隔离路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(sockets.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("仓库");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(workspace, window, cx)
        })
    });
    let (node, launch_receipt) = app.update(cx, |app, cx| {
        let node = app
            .runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Claude,
                None,
                None,
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("标准占位卡");
        let receipt = app.start_mobile_agent(node, cx).expect("真实启动");
        (node, receipt)
    });
    // 30s cold CLI startup; 100ms paste/Enter separation matches the existing native reply queue.
    drain_until(
        &app,
        cx,
        |_| launch_receipt.try_recv().ok(),
        Duration::from_secs(30),
    )
    .expect("后台启动应及时返回")
    .expect("真实 CLI 启动失败，检查 Hook PATH");
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Ready)).then_some(())
        },
        Duration::from_secs(30),
    )
    .expect("等待正式 Ready");
    app.update(cx, |app, _| app.runtime.send_node_input(node, runtime::NodeInput::Paste("Do not use any tools, read or modify files. Reply exactly NORMA_CLAUDE_CHAT_READY".into())).expect("明确任务输入"));
    std::thread::sleep(Duration::from_millis(100));
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(
                node,
                runtime::NodeInput::Key(terminal::KeyInput {
                    key: terminal::Key::Enter,
                    ctrl: false,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("提交")
    });
    // 120s bounds the real model; no automatic permission decision or retry is provided.
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Done)).then_some(())
        },
        Duration::from_secs(120),
    )
    .expect("真实回复完成");
    verify_native(&app, cx, workspace, &probe, "conversation");
    app.update(cx, |app, _| {
        app.runtime.kill_all();
        assert!(
            app.runtime.mobile_agent_run_id(node).is_none(),
            "关闭后不能保留可控制的运行实例"
        );
        assert!(
            app.runtime.document.node(node).is_some(),
            "关闭进程应保留卡片元数据"
        );
    });
    // 5s allows the PTY exit event to reach the cached terminal view.
    drain_until(
        &app,
        cx,
        |runtime| {
            matches!(
                runtime.node_view(node),
                Some(runtime::NodeView::Term {
                    status: terminal::TerminalStatus::Exited { .. },
                    ..
                })
            )
            .then_some(())
        },
        Duration::from_secs(5),
    )
    .expect("原生终端必须已退出");
}

pub(crate) fn verify_native(
    app: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    workspace: &Path,
    probe: &str,
    mode: &str,
) {
    let directory = tempfile::tempdir().expect("隔离手机文件");
    let approvals = matches!(
        mode,
        "approvals" | "bash-edges" | "approval-stop" | "write-approvals"
    );
    // Reuse actual native actions; ordinary regression runs do not collect extra samples.
    let measuring = std::env::var("NORMA_MOBILE_MEASURE").as_deref() == Ok("1")
        && (approvals || matches!(mode, "messages" | "native-card"));
    if measuring {
        std::fs::write(directory.path().join("measure-latency"), "").expect("隔离测量标记");
    }
    let store = storage::AppStore::new(app.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(workspace, "Native Chat")
        .expect("项目");
    let server = app::MobileServer::start_with_mobile_sources(
        app::MobileAccess::with_identity_for_e2e(
            store,
            norma::mobile::ServerIdentity::generate().expect("证书"),
        ),
        vec!["127.0.0.1:0".parse().expect("回环")],
        app::MobileSources {
            terminals: app.read_with(cx, |app, cx| app.mobile_terminal_source_for_e2e(cx)),
            snapshots: app.read_with(cx, |app, cx| app.mobile_snapshot_source_for_e2e(cx)),
            commands: app.read_with(cx, |app, cx| app.mobile_command_source_for_e2e(cx)),
            conversations: app.read_with(cx, |app, cx| app.mobile_conversation_source_for_e2e(cx)),
            approvals: app.read_with(cx, |app, cx| app.mobile_approval_source_for_e2e(cx)),
            reviews: app.read_with(cx, |app, cx| app.mobile_review_source_for_e2e(cx)),
        },
    )
    .expect("正式 WSS");
    let qr = directory.path().join("offer.json");
    std::fs::write(
        &qr,
        server
            .begin_pairing("Native Chat Mac")
            .expect("测试扫码")
            .to_json()
            .expect("编码"),
    )
    .expect("临时 QR");
    if approvals || mode == "external" {
        std::fs::write(
            directory.path().join("approval-targets.json"),
            serde_json::to_vec(&[
                directory.path().join("denied.txt"),
                directory.path().join("allowed.txt"),
            ])
            .expect("精确临时路径"),
        )
        .expect("测试手机读取指定目标");
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&qr, std::fs::Permissions::from_mode(0o600)).expect("二维码限本用户");
    let log_path = directory.path().join("native.log");
    let log = std::fs::File::create(&log_path).expect("日志");
    let mut child = std::process::Command::new(probe)
        .arg(qr)
        .arg(mode)
        .stdout(log.try_clone().expect("日志副本"))
        .stderr(log)
        .spawn()
        .expect("原生手机测试进程");
    // 120s = Xcode launch allowance 90s + mobile pairing 30s. Test-only approval of this exact project.
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut approved = false;
    let mut downgraded = false;
    let mut feedback_closed = false;
    let status = loop {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        if let Some(status) = child.try_wait().expect("进程状态") {
            break status;
        }
        if let Some(pending) = server.pending().expect("配对申请") {
            assert!(!approved, "只允许一台测试手机申请一次");
            server
                .approve(
                    &pending.request_id,
                    &[storage::MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: if matches!(
                            mode,
                            "terminal"
                                | "feedback"
                                | "reviews"
                                | "messages"
                                | "native-card"
                                | "todos"
                                | "interrupts"
                                | "queues"
                                | "approvals"
                                | "bash-edges"
                                | "approval-stop"
                                | "write-approvals"
                                | "external"
                                | "events"
                        ) {
                            storage::MobilePermission::Control
                        } else {
                            storage::MobilePermission::Read
                        },
                    }],
                )
                .expect("按测试场景批准项目权限");
            approved = true;
        }
        if mode == "feedback"
            && !feedback_closed
            && directory.path().join("feedback-close").exists()
        {
            app.update(cx, |app, _| app.runtime.kill_all());
            std::fs::write(directory.path().join("feedback-closed"), "").expect("确认隔离会话关闭");
            feedback_closed = true;
        }
        if mode == "events" && !downgraded && directory.path().join("stream-read-only").exists() {
            let device = server.devices().expect("已配对手机").remove(0);
            server
                .replace_permissions(
                    &device.device_id,
                    1,
                    &[storage::MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: storage::MobilePermission::Read,
                    }],
                )
                .expect("测试显式降级");
            downgraded = true;
        }
        if Instant::now() >= deadline {
            child.kill().expect("停止超时验证");
            child.wait().expect("回收");
            panic!("原生历史检查超时");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let output = std::fs::read_to_string(&log_path).expect("结果");
    let provider = app.read_with(cx, |app, _| {
        if matches!(mode, "reviews" | "archive") {
            "Project".into()
        } else {
            app.runtime.mobile_card_snapshots()[0].profile.clone()
        }
    });
    let evidence = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/norma-ios-evidence");
    std::fs::create_dir_all(&evidence).expect("工作区验收日志目录");
    std::fs::copy(
        &log_path,
        evidence.join(format!(
            "norma-ios-{mode}-native-{provider}-{}.log",
            if mode == "messages" {
                "v61"
            } else if mode == "events" {
                "v64"
            } else if measuring || (approvals || mode == "external") {
                "v59"
            } else if mode == "archive" {
                "v55"
            } else if mode == "feedback" {
                "v37"
            } else if matches!(mode, "reviews" | "messages") {
                "v35"
            } else if mode == "todos" {
                "v66"
            } else if mode == "queues" {
                "v24"
            } else if mode == "interrupts" {
                "v22"
            } else {
                "v20"
            }
        )),
    )
    .expect("保存证据");
    if approvals && status.success() {
        assert!(
            output.contains("NATIVE_REAL_APPROVAL_FEED_OK"),
            "两次真实决定必须在同一原生实时订阅中出现并被移除，不能回退轮询"
        );
    }
    if mode == "bash-edges" && status.success() {
        assert!(
            output.contains("NATIVE_LONG_BASH_RECEIPT_BEFORE_COMPLETION"),
            "长工具必须在完成之前得到持久化批准回执"
        );
        if provider == "Codex" {
            assert!(
                output.contains("NATIVE_LONG_BASH_EXIT_CODE_OK 7"),
                "手机完整历史必须保留真实非零工具退出码"
            );
        }
    }
    if measuring && status.success() {
        let reports: Vec<serde_json::Value> = output
            .lines()
            .filter_map(|line| line.find("NATIVE_LATENCY=").map(|i| &line[i + 15..]))
            .map(|text| serde_json::from_str(text).expect("原生延迟JSON"))
            .collect();
        let name = if matches!(mode, "messages" | "native-card") {
            "chat_page_read"
        } else {
            "approval_list_read"
        };
        let read = reports
            .iter()
            .find(|r| r["name"] == name)
            .expect("必须有真实原生读取采样");
        assert_eq!(
            read["samples_ms"].as_array().expect("原始数据").len(),
            20,
            "20次有效原生读取都必须保留，不能丢弃慢样本"
        );
        assert!(
            reports.iter().all(|r| r["provider"] == provider),
            "不能混合不同Provider的样本"
        );
        assert!(
            reports.iter().all(|r| r.get("token").is_none()),
            "性能日志不能包含配对凭据"
        );
    }
    if !status.success() && matches!(mode, "messages" | "feedback" | "native-card" | "bash-edges") {
        app.read_with(cx, |app, _| {
            let card = app.runtime.mobile_card_snapshots().remove(0);
            eprintln!(
                "MESSAGE_FAILURE_STATE {} latest={:?}",
                card.state,
                app.runtime.mobile_latest_turn_key(card.node_id)
            );
            if let Some(runtime::NodeView::Term { snapshot, .. }) =
                app.runtime.node_view(card.node_id)
            {
                let text = snapshot
                    .rows
                    .iter()
                    .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n");
                eprintln!("MESSAGE_FAILURE_TERMINAL\n{text}");
            }
        });
    }
    assert!(status.success(), "原生客户端必须读取真实历史：{output}");
    assert!(
        output.contains(if mode == "terminal" {
            "NATIVE_REAL_TERMINAL_OK"
        } else if mode == "archive" {
            "NATIVE_REAL_ARCHIVE_OK"
        } else if mode == "feedback" {
            "NATIVE_REAL_DELIVERY_FEEDBACK_OK"
        } else if mode == "reviews" {
            "NATIVE_ACCEPTANCE_RECEIPT_OK"
        } else if mode == "todos" {
            "NATIVE_REAL_TODO_RETRY_REJECTED"
        } else if mode == "events" {
            "NATIVE_STREAM_SEARCH_DELTA_OK"
        } else if mode == "external" {
            "NATIVE_REAL_EXTERNAL_NOTICE_OK"
        } else if approvals {
            "NATIVE_REAL_APPROVALS_OK"
        } else if mode == "queues" {
            "NATIVE_REAL_QUEUES_OK"
        } else if mode == "interrupts" {
            "NATIVE_REAL_INTERRUPTS_OK"
        } else if matches!(mode, "messages" | "native-card") {
            "NATIVE_REAL_MESSAGES_OK"
        } else {
            "NATIVE_REAL_CONVERSATION_OK"
        }),
        "必须验证正文和旧 run 拒绝"
    );
    if matches!(mode, "messages" | "native-card") {
        assert!(
            output.contains("NATIVE_REAL_CHAT_EVENTS_OK"),
            "两条真实消息必须通过持续订阅更新且无重复"
        );
    }
    if mode == "todos" {
        assert!(
            output.contains("NATIVE_PLAN_STREAM_OUTCOMES_OK")
                && output.contains("NATIVE_PLAN_STREAM_CANCEL_OK"),
            "原生客户端必须通过持续订阅获得准确终态并取消订阅"
        );
        assert!(
            output.contains("NATIVE_REAL_TODO_CANCEL_RETRY"),
            "原决定必须通过手机真实入队/撤回产生，不能预写未发送结果"
        );
        assert!(
            output.contains("NATIVE_REAL_TODO_RETRY_SENT_ONCE"),
            "必须验证重排到真实Provider且只有一次，不能只检查已发送拒绝"
        );
    }
    if mode == "external" {
        assert!(
            !directory.path().join("denied.txt").exists(),
            "外部等待不能绕过审批写入"
        );
    }
    if approvals {
        assert!(
            !directory.path().join("denied.txt").exists(),
            "拒绝的写入不得执行"
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join("allowed.txt")).expect("批准后写入"),
            "hello-from-norma",
            "实际磁盘效果必须与精确命令一致"
        );
    }
}

/// 原生Swift经正式GPUI/WSS读取增量、创建删除与权限变化后的新基线。
///
/// 数据构造：隔离数据库/目录与Codex空卡；原生消费者暂停4.5s=两次2s心跳+0.5s余量，不启动CLI。
/// 执行过程：首帧后暂停消费 → 恢复最新快照 → 创建Claude并按任务/Agent/状态筛选 → 删除后筛选变空 → 降权 → 取消。
/// 预期结果：准确增删且筛选不残留旧卡或混入Codex，清除后保留Codex；新授权不可写；取消无后续更新，live为0。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE"]
fn e2e_mobile_project_events_native_transport() {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式原生客户端");
    let mut test = TestAppContext::single();
    let directory = tempfile::tempdir().expect("隔离项目与路由");
    test.update(|cx| {
        app::install_norma_endpoint_for_e2e(directory.path().join("norma.sock"), cx).expect("路由")
    });
    let (app, cx) = make_app_real(&mut test);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(directory.path(), window, cx)
        })
    });
    let retained = app.update(cx, |app, _| {
        app.runtime
            .create_mobile_agent_card(
                canvas::AgentProfile::Codex,
                None,
                Some("Retained".into()),
                canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            )
            .expect("空卡")
    });
    verify_native(&app, cx, directory.path(), &probe, "events");
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.mobile_card_snapshots().len(),
            1,
            "新建卡已被准确删除"
        );
        assert!(
            app.runtime.document.node(retained).is_some(),
            "原始Codex卡必须保留"
        );
        assert_eq!(app.runtime.live_len(), 0, "创建与订阅不能隐式启动Agent");
    });
}
