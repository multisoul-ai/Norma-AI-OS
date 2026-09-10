use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::json;
use std::{path::Path, time::Duration};

/// 原卡Codex审批必须先观察真实Hook与原生菜单，不能借用另一个执行器的请求。
///
/// 数据构造：原生100×40PTY、read-only+on-request；唯一命令仅向临时allowed.txt写固定文本。
/// 执行过程：原卡空闲 → 精确写入申请 → 记录真实PermissionRequest/菜单 → Ctrl-C取消 → 清理本测试PTY。
/// 预期结果：准确session/turn不变，未批准的目标不存在；记录完整Provider证据后才设计生产手机决策。
#[test]
#[ignore = "requires original Codex and fresh Hook; read-only native permission probe"]
fn original_codex_permission_request_evidence() {
    probe(None, None);
}

/// 原卡批准必须实际执行指定写入，保留唯一PTY和原会话。
///
/// 数据构造：与请求探针相同的只读沙箱，仅允许临时allowed.txt的固定命令。
/// 执行过程：准确请求和菜单 → 原生y快捷键 → 采集终态Hook及原rollout → 回收本测试PTY。
/// 预期结果：allowed.txt内容准确、denied.txt不存在；记录实际回执以建立生产契约。
#[test]
#[ignore = "requires original Codex; native approval protocol probe"]
fn original_codex_allow_evidence() {
    probe(Some(true), None);
}

/// 原卡拒绝必须保持指定目标不存在，不得替换会话或执行器。
///
/// 数据构造：read-only/on-request，仅请求临时denied.txt固定写入。
/// 执行过程：准确请求和菜单 → 原生Escape → 采集终态Hook及原rollout → 回收本测试PTY。
/// 预期结果：denied.txt和allowed.txt都不存在，记录真实拒绝证据后才实现手机回执。
#[test]
#[ignore = "requires original Codex; native rejection protocol probe"]
fn original_codex_deny_evidence() {
    probe(Some(false), None);
}

/// 原卡Codex通过正式Swift TLS/WSS审批，精确回执落盘后才返回成功。
///
/// 数据构造：唯一原PTY、独立AppDB和进程级只读配置；Swift只申请两个临时文件的固定命令。
/// 执行过程：空卡 → control配对 → 拒绝并确认新回合边界 → 同会话批准 → 回查/重放/旧请求拒绝。
/// 预期结果：拒绝文件不存在、批准内容准确、订阅无丢失；两次真实输入保留原run/session且唯一PTY。
#[test]
#[ignore = "requires real Codex, fresh Hook and NORMA_SWIFT_PROBE"]
fn original_codex_approvals_via_swift_transport() {
    probe(None, Some("approvals"));
}

/// 原卡Codex长命令必须先返回批准回执，再保留准确的非零工具退出结果。
///
/// 数据构造：唯一只读PTY；先拒绝临时写入，再批准sleep30+固定写入+exit7；30s超过20s批准预算。
/// 执行过程：Swift TLS/WSS拒绝→批准→工具完成前回查回执→原会话读取exit7→重放/旧目标拒绝。
/// 预期结果：批准回执在20s内且文件尚不存在；最终只有允许的文件存在，原run/session及唯一PTY保留。
#[test]
#[ignore = "requires original Codex and NORMA_SWIFT_PROBE; 30s Bash"]
fn original_codex_long_bash_approval_via_swift_transport() {
    probe(None, Some("bash-edges"));
}

/// 手机停止原Codex审批中的回合后，旧批准必须失效，原卡仍可接收下一项任务。
///
/// 数据构造：原PTY、两个临时写入；首项只停止，次项明确批准，沿用20s回执预算。
/// 执行过程：准确审批→停止/回查/重放→验证审批移除及迟到批准拒绝→同会话续发并批准。
/// 预期结果：首文件不存在、次文件内容准确；只有两次输入，run/session和唯一PTY保留。
#[test]
#[ignore = "requires original Codex and NORMA_SWIFT_PROBE"]
fn original_codex_pending_stop_via_swift_transport() {
    probe(None, Some("approval-stop"));
}

fn probe(choice: Option<bool>, swift: Option<&str>) {
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("workspace");
    let _socket = enter_mobile_canvas(&app, cx, workspace);
    let directory = tempfile::tempdir().expect("仅本探针目标");
    let target = directory.path().join(if choice == Some(false) {
        "denied.txt"
    } else {
        "allowed.txt"
    });
    let probe_config = directory.path().join("codex-config");
    std::fs::create_dir(&probe_config).expect("仅本进程配置");
    use std::os::unix::fs::{symlink, PermissionsExt};
    std::fs::set_permissions(&probe_config, std::fs::Permissions::from_mode(0o700))
        .expect("配置限本用户");
    let original_config = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("真实用户目录")).join(".codex")
        });
    // Reuse credentials through a private symlink, never read/print/copy their content.
    for name in ["config.toml", "auth.json"] {
        symlink(original_config.join(name), probe_config.join(name)).expect("复用现有配置与登录");
    }
    let mut hooks = norma::agent_hook::default_install_paths();
    hooks.codex_hooks = probe_config.join("hooks.json");
    norma::agent_hook::install_hook_config(
        norma::agent_hook::HookTarget::Codex,
        &workspace.join("target/debug/multisoul-agent-hook"),
        &hooks,
    )
    .expect("仅Norma观察Hook");
    let events = app.update(cx, |app, _| app.observe_agent_hook_events_for_e2e());
    let node = app.update(cx, |app, _| {
        app.runtime.set_mobile_codex_archive_config_for_e2e(probe_config.clone());
        app.runtime.set_pty_factory(Box::new(move |mut command, cols, rows, tx| {
            // This isolated shell-tool probe does not need Apps/MCP; retain native permission and Norma Hooks.
            // Per-process flags only: no global settings change, no approval bypass.
            // This flag trusts only our just-generated Norma Hook source; tool approvals stay enabled.
            command.args = vec!["-lic".into(), "exec codex --disable apps --dangerously-bypass-hook-trust --sandbox read-only --ask-for-approval on-request -c 'approvals_reviewer=\"user\"'".into()];
            command.env.push(("CODEX_HOME".into(), probe_config.to_string_lossy().into_owned()));
            let pty = terminal::PtyHandle::spawn_command(command, cols, rows, tx)?;
            println!("NATIVE_CODEX_PTY_ROOT {:?}", terminal::PtyBackend::child_pid(&pty));
            Ok(Box::new(pty))
        }));
        app.runtime.spawn_agent(AgentProfile::Codex, CanvasRect::new(0.,0.,640.,400.),100,40).expect("原卡唯一PTY")
    });
    let ready = drain_until(
        &app,
        cx,
        |r| {
            r.mobile_card_snapshots()
                .iter()
                .any(|card| card.node_id == node && card.state == "ready")
                .then_some(())
        },
        Duration::from_secs(30),
    );
    if ready.is_none() {
        let display = app.read_with(cx, |app, _| screen(&app.runtime, node));
        app.update(cx, |app, _| app.runtime.kill_all());
        panic!("原卡未就绪：{display}");
    }
    let run = app.read_with(cx, |app, _| {
        app.runtime
            .mobile_agent_run_id(node)
            .expect("原运行")
            .to_owned()
    });
    if let Some(mode) = swift {
        let client = std::env::var("NORMA_SWIFT_PROBE").expect("正式Swift客户端");
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &client, mode);
        }));
        if let Err(failure) = checked {
            for event in events.try_iter() {
                println!("NATIVE_CODEX_FAILURE_HOOK {}", event.raw_json);
                if event.provider_event == "PermissionRequest" {
                    if let Some(path) = event.raw_json["transcript_path"].as_str() {
                        rollout_evidence(Path::new(path));
                    }
                }
            }
            app.update(cx, |app, _| app.runtime.kill_all());
            std::panic::resume_unwind(failure);
        }
        let prompts: Vec<_> = events
            .try_iter()
            .filter(|e| e.provider_event == "UserPromptSubmit")
            .collect();
        app.update(cx, |app, _| {
            let session = app.runtime.agent_session_id(node).expect("首条输入建立原会话");
            assert_eq!(prompts.len(), 2, "拒绝后只能续发一次，不能自动重试");
            assert_eq!(prompts[0].session_id.as_deref(), Some(session.as_str()), "拒绝必须在原会话");
            assert_eq!(prompts[1].session_id.as_deref(), Some(session.as_str()), "批准必须复用同会话");
            assert_ne!(prompts[0].turn_id, prompts[1].turn_id, "两次决定必须属于不同真实回合");
            assert_eq!(app.runtime.mobile_agent_run_id(node), Some(run.as_str()), "不能更换运行");
            assert_eq!(app.runtime.live_len(), 1, "不能另建PTY执行器");
            assert!(matches!(&app.runtime.document.node(node).expect("原卡").kind, canvas::CanvasNodeKind::Agent(spec) if !spec.mobile_control), "不能切换卡片模式");
            app.runtime.kill_all();
        });
        return;
    }
    let command = format!("printf norma-native-codex > '{}'", target.display());
    let prompt = format!("This is an isolated native permission integration test. Use exec_command exactly once with cmd exactly: {command}\nSet sandbox_permissions to require_escalated and justification to 'Write the isolated Norma test marker?'. Do not execute anything before approval. Do not use other tools, read other files, delegate or change this command. If denied, stop and do not retry.");
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(node, runtime::NodeInput::Text(prompt))
            .expect("原PTY正文")
    });
    //100ms is the existing paste-to-Enter interval used by both native desktop providers.
    std::thread::sleep(Duration::from_millis(100));
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(node, key(terminal::Key::Enter))
            .expect("原PTY提交")
    });
    let mut observed = Vec::new();
    //120s bounds model+external Hook latency for evidence only; never a production request timeout.
    let permission = drain_until(
        &app,
        cx,
        |_| {
            for event in events.try_iter() {
                println!("CODEX_NATIVE_HOOK {}", event.raw_json);
                let permission = event.provider_event == "PermissionRequest";
                observed.push(event.clone());
                if permission {
                    return Some(event);
                }
            }
            None
        },
        Duration::from_secs(120),
    );
    //10s lets informational Hooks finish rendering their native menu; no key is sent in this step.
    let display = drain_until(
        &app,
        cx,
        |r| {
            let text = screen(r, node);
            (text.contains("Would you like to run")
                || text.contains("Do you want")
                || text.contains("Esc to"))
            .then_some(text)
        },
        Duration::from_secs(10),
    );
    println!(
        "CODEX_NATIVE_PERMISSION {}",
        json!(permission.as_ref().map(|p| &p.raw_json))
    );
    let display = display.expect("真实菜单必须可见，只有Hook不算审批可操作");
    println!("CODEX_NATIVE_SCREEN {}", display);
    let permission = permission.expect("必须记录真实审批事件");
    assert_eq!(
        permission.raw_json["tool_input"]["command"], command,
        "只能操作指定隔离写入"
    );
    assert!(!target.exists(), "决定前不允许执行写入");
    if let Some(allow) = choice {
        assert!(
            display.contains("1. Yes, proceed (y)"),
            "必须是真实原生批准快捷键"
        );
        assert!(
            display.contains("No, and tell Codex what to do differently (esc)"),
            "必须是真实原生拒绝快捷键"
        );
        app.update(cx, |app, _| {
            app.runtime
                .send_node_input(
                    node,
                    key(if allow {
                        terminal::Key::Char('y')
                    } else {
                        terminal::Key::Escape
                    }),
                )
                .expect("仅操作原PTY当前菜单");
        });
        // 30s = 本次短命令的工具执行和模型终态证据上限，不作为手机API延迟。
        let stopped = drain_until(
            &app,
            cx,
            |_| {
                let mut stopped = false;
                for event in events.try_iter() {
                    println!("CODEX_NATIVE_AFTER_HOOK {}", event.raw_json);
                    stopped |= event.provider_event == "Stop";
                    observed.push(event);
                }
                stopped.then_some(())
            },
            Duration::from_secs(30),
        );
        println!("CODEX_NATIVE_STOPPED {}", stopped.is_some());
        if let Some(path) = permission.raw_json["transcript_path"].as_str() {
            rollout_evidence(Path::new(path));
        }
        println!(
            "CODEX_NATIVE_AFTER_SCREEN {}",
            app.read_with(cx, |app, _| screen(&app.runtime, node))
        );
    }
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(
                node,
                runtime::NodeInput::Key(terminal::KeyInput {
                    key: terminal::Key::Char('c'),
                    ctrl: true,
                    alt: false,
                    shift: false,
                }),
            )
            .expect("只取消本探针原PTY")
    });
    let session = observed
        .iter()
        .find(|e| e.provider_event == "UserPromptSubmit")
        .and_then(|e| e.session_id.clone());
    app.update(cx, |app, _| {
        assert_eq!(
            app.runtime.mobile_agent_run_id(node),
            Some(run.as_str()),
            "不能替换原运行"
        );
        assert_eq!(
            app.runtime.agent_session_id(node),
            session,
            "只能使用首条原生输入建立的会话"
        );
        assert_eq!(app.runtime.live_len(), 1, "不能另建执行器");
        app.runtime.kill_all();
    });
    if choice == Some(true) {
        assert_eq!(
            std::fs::read_to_string(&target).expect("批准后的准确文件"),
            "norma-native-codex",
            "批准必须真实执行指定命令"
        );
        assert!(
            !directory.path().join("denied.txt").exists(),
            "批准不能操作拒绝目标"
        );
    } else {
        assert!(!target.exists(), "未批准不能执行隔离写入");
    }
    assert_eq!(permission.session_id, session, "请求必须属于原会话");
    assert!(permission.turn_id.is_some(), "审批必须携带真实回合身份");
    assert!(
        choice == Some(true) || !observed.iter().any(|e| e.provider_event == "PostToolUse"),
        "未批准不能收到工具成功Hook"
    );
}

fn rollout_evidence(path: &Path) {
    for line in std::fs::read_to_string(path)
        .expect("仅本探针原会话rollout")
        .lines()
    {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // Only this isolated session's tool records; never configuration or authentication.
        if (value["type"] == "response_item"
            && matches!(
                value["payload"]["type"].as_str(),
                Some(
                    "function_call"
                        | "function_call_output"
                        | "custom_tool_call"
                        | "custom_tool_call_output"
                )
            ))
            || (value["type"] == "event_msg"
                && matches!(
                    value["payload"]["type"].as_str(),
                    Some(
                        "item_completed"
                            | "exec_command_begin"
                            | "exec_command_end"
                            | "turn_aborted"
                            | "task_complete"
                    )
                ))
        {
            println!("CODEX_NATIVE_ROLLOUT {value}");
        }
    }
}

fn key(key: terminal::Key) -> runtime::NodeInput {
    runtime::NodeInput::Key(terminal::KeyInput {
        key,
        ctrl: false,
        alt: false,
        shift: false,
    })
}

fn screen(runtime: &runtime::Runtime, node: canvas::NodeId) -> String {
    let Some(runtime::NodeView::Term { snapshot, .. }) = runtime.node_view(node) else {
        return String::new();
    };
    snapshot
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
