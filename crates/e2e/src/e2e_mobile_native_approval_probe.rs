use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

/// Swift手机经正式TLS/WSS决定原生Claude的两次准确审批，继续使用电脑原有卡片。
///
/// 数据构造：隔离AppDB、一张mobile_control=false的100×40原PTY；仅本测试进程使用default权限。
/// 执行过程：空卡就绪 → Swift两次精确写入 → 拒绝/批准 → durable回查及重放 → 原会话继续。
/// 预期结果：拒绝文件不存在，批准内容准确；旧请求不可重用、run/session/唯一PTY保留。
/// 此为headless GPUI + 真实CLI + macOS Swift网络客户端，不是真机或GUI点击验收。
#[test]
#[ignore = "requires real Claude, fresh Hook and NORMA_SWIFT_PROBE"]
fn original_claude_approvals_via_swift_transport() {
    verify_transport("approvals");
}

/// 长Bash必须在工具结束前经正式Swift传输返回批准回执，工具exit7不改变授权结果。
///
/// 数据构造：原PTY，拒绝一次；允许sleep30+隔离写入+exit7，30s超过原20s回执上限。
/// 执行过程：原卡就绪 → Swift拒绝/批准 → 20s内回执且文件未生成 → 工具结束 → 重放/旧请求检查。
/// 预期结果：精确批准文件生成、拒绝文件不存在；不新建PTY，不因工具失败报授权未知。
#[test]
#[ignore = "requires real Claude and NORMA_SWIFT_PROBE; 30s Bash"]
fn original_claude_long_bash_approval_via_swift_transport() {
    verify_transport("bash-edges");
}

/// 手机停止原Claude审批中的回合后，旧批准必须失效，原卡可继续输入。
///
/// 数据构造：唯一原PTY、两个隔离写入；首项停止，次项批准，不修改全局权限。
/// 执行过程：准确审批→停止/回查/重放→等待审批移除→拒绝迟到批准→原会话续发。
/// 预期结果：首文件不存在，次文件内容准确；停止不是批准，run/session和原生模式保留。
#[test]
#[ignore = "requires original Claude and NORMA_SWIFT_PROBE"]
fn original_claude_pending_stop_via_swift_transport() {
    verify_transport("approval-stop");
}

/// 原Claude的Write审批必须在手机显示完整路径/内容，仍通过原终端决定。
///
/// 数据构造：两个隔离txt目标、固定hello-from-norma内容；首项拒绝、次项单次批准。
/// 执行过程：真实Write请求→手机读详情→拒绝/批准→回查与旧请求拒绝→验证磁盘。
/// 预期结果：拒绝目标不存在、批准内容准确；原run/session和唯一PTY保留。
#[test]
#[ignore = "requires original Claude and NORMA_SWIFT_PROBE"]
fn original_claude_write_approvals_via_swift_transport() {
    verify_transport("write-approvals");
}

fn verify_transport(mode: &str) {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("真实Swift客户端");
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _socket = enter_mobile_canvas(&app, cx, workspace);
    let events = app.update(cx, |app, _| app.observe_agent_hook_events_for_e2e());
    let node = app.update(cx, |app, _| {
        app.runtime
            .set_pty_factory(Box::new(|mut command, cols, rows, tx| {
                command.args = vec![
                    "-lic".into(),
                    "exec claude --permission-mode default".into(),
                ];
                Ok(Box::new(terminal::PtyHandle::spawn_command(
                    command, cols, rows, tx,
                )?))
            }));
        app.runtime
            .spawn_agent(
                AgentProfile::Claude,
                CanvasRect::new(0., 0., 640., 400.),
                100,
                40,
            )
            .expect("唯一原PTY")
    });
    // Existing cold-start budget; no prompt or task is injected on launch.
    let ready = drain_until(
        &app,
        cx,
        |r| (r.agent_task_state(node) == Some(runtime::AgentTaskState::Ready)).then_some(()),
        Duration::from_secs(30),
    );
    if ready.is_none() {
        app.update(cx, |app, _| app.runtime.kill_all());
    }
    assert!(ready.is_some(), "原CLI必须先空闲就绪");
    let (run, session) = app.read_with(cx, |app, _| {
        (
            app.runtime
                .mobile_agent_run_id(node)
                .expect("运行")
                .to_owned(),
            app.runtime.agent_session_id(node).expect("会话"),
        )
    });
    let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, mode);
    }));
    if let Err(failure) = checked {
        for event in events.try_iter() {
            println!("NATIVE_CLAUDE_FAILURE_HOOK {}", event.raw_json);
        }
        println!(
            "NATIVE_CLAUDE_FAILURE_SCREEN {}",
            app.read_with(cx, |app, _| screen(&app.runtime, node))
        );
        app.update(cx, |app, _| app.runtime.kill_all());
        std::panic::resume_unwind(failure);
    }
    app.update(cx, |app, _| {
        assert_eq!(app.runtime.mobile_agent_run_id(node), Some(run.as_str()), "不能重新启动运行");
        assert_eq!(app.runtime.agent_session_id(node).as_deref(), Some(session.as_str()), "两次审批保持原会话");
        assert_eq!(app.runtime.live_len(), 1, "不能另建执行器");
        assert!(matches!(&app.runtime.document.node(node).expect("原卡").kind, canvas::CanvasNodeKind::Agent(spec) if !spec.mobile_control), "不能切换卡片模式");
        app.runtime.kill_all();
    });
}

/// 原生Claude确认窗口必须仍能通过原PTY操作；观察Hook不能抢占或替代原生审批。
///
/// 数据构造：一张mobile_control=false原卡、100×40真实PTY；隔离进程改为default权限，
///   附加只记录事件、不返回决定的Hook；写入只限本测试临时allowed.txt/denied.txt。
/// 执行过程：原卡启动 → 请求精确Bash写入 → 记录PermissionRequest和原生屏幕 → 原PTY拒绝/批准 → 核对工具结果。
/// 预期结果：同一session与唯一PTY保留；拒绝不写文件，批准只写固定内容；本测试不是手机审批实现或真实App验收。
#[test]
#[ignore = "requires real Claude, model access and native permission prompt; isolated probe"]
fn original_claude_permission_prompt_accept_and_deny_evidence() {
    verify(false, false, false);
    verify(true, false, false);
}

/// 原生Bash长命令的开始证据必须来自Provider记录，不能用按键发送代替确认。
///
/// 数据构造：原卡默认权限、唯一Bash，sleep30秒后只向隔离allowed.txt写固定内容；timeout60000ms=30s+30s余量。
/// 执行过程：精确PermissionRequest → 原PTY数字1 → 记录新增progress及tool结果的时间 → 核对磁盘。
/// 预期结果：执行前文件不存在；完成后内容准确；保留原PTY/session；原始证据用于决定生产回执入口。
#[test]
#[ignore = "requires real Claude and 30s foreground Bash; isolated protocol probe"]
fn original_claude_long_bash_execution_evidence() {
    verify(true, true, false);
}

/// 已批准但exit7的Bash必须留下真实执行结果，不能被误判为拒绝。
///
/// 数据构造：只写隔离allowed.txt后exit7；7是刻意的非零退出码，不是超时预算。
/// 执行过程：原卡默认权限 → 精确请求 → 原PTY批准 → 读取准确工具错误与磁盘写入。
/// 预期结果：is_error为true且文件存在；拒绝目标不存在，原PTY保持不变。
#[test]
#[ignore = "requires real Claude and native Bash failure; isolated protocol probe"]
fn original_claude_failed_bash_execution_evidence() {
    verify(true, false, true);
}

fn verify(allow: bool, slow: bool, fails: bool) {
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _socket = enter_mobile_canvas(&app, cx, workspace);
    let directory = tempfile::tempdir().expect("隔离写入与Hook日志");
    let log = directory.path().join("events.jsonl");
    let recorder = directory.path().join("record.py");
    fs::write(&recorder, format!(
        "import json,sys\nwith open({},'a') as f: f.write(json.dumps(json.load(sys.stdin))+'\\n')\n",
        serde_json::to_string(&log.to_string_lossy()).expect("Python字符串")
    )).expect("仅记录当前进程事件");
    let mut hooks = serde_json::Map::new();
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PermissionRequest",
        "PostToolUse",
        "PostToolUseFailure",
        "Stop",
    ] {
        hooks.insert(
            event.into(),
            json!([{"matcher":"*","hooks":[{"type":"command",
            "command":format!("python3 '{}'",recorder.display()),"timeout":5}]}]),
        );
    }
    let settings = directory.path().join("settings.json");
    fs::write(&settings, json!({"hooks":hooks}).to_string()).expect("进程级只读Hook设置");
    let root_pid = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let captured_pid = root_pid.clone();
    let node = app.update(cx, |app, _| {
        app.runtime.set_pty_factory(Box::new(move |mut command, cols, rows, tx| {
            // Only this test's new process uses default permission; no user process/config is changed.
            command.args = vec!["-lic".into(), "exec claude --permission-mode default --settings \"$NORMA_NATIVE_PROBE_SETTINGS\"".into()];
            command.env.push(("NORMA_NATIVE_PROBE_SETTINGS".into(), settings.to_string_lossy().into_owned()));
            let pty = terminal::PtyHandle::spawn_command(command, cols, rows, tx)?;
            captured_pid.store(terminal::PtyBackend::child_pid(&pty).unwrap_or(0), std::sync::atomic::Ordering::Release);
            Ok(Box::new(pty))
        }));
        app.runtime.spawn_agent(AgentProfile::Claude, CanvasRect::new(0.,0.,640.,400.),100,40)
            .expect("原生卡和唯一真实PTY")
    });
    // 30s是现有CLI冷启动预算；120s是模型和审批各自的有界验收预算，不是网络延迟指标。
    let ready = drain_until(
        &app,
        cx,
        |r| (r.agent_task_state(node) == Some(runtime::AgentTaskState::Ready)).then_some(()),
        Duration::from_secs(30),
    );
    if ready.is_none() {
        let terminal = app.read_with(cx, |app, _| screen(&app.runtime, node));
        app.update(cx, |app, _| app.runtime.kill_all());
        panic!("原生Claude未就绪：{terminal}");
    }
    let session = app.read_with(cx, |app, _| {
        app.runtime.agent_session_id(node).expect("原会话")
    });
    let target = directory
        .path()
        .join(if allow { "allowed.txt" } else { "denied.txt" });
    let prefix = if slow { "sleep 30; " } else { "" };
    let suffix = if fails { "; exit 7" } else { "" };
    let command = format!(
        "{prefix}printf norma-native-permission > '{}'{suffix}",
        target.display()
    );
    let mode = if slow {
        " Set Bash timeout to 60000 milliseconds and run_in_background to false."
    } else {
        ""
    };
    let prompt = format!("Use Bash exactly once to run this exact command: {command}. Request permission for this command.{mode} Do not modify the command, read files, use other tools or delegate. If denied, do not retry. Then reply briefly and stop.");
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(node, runtime::NodeInput::Text(prompt))
            .expect("原PTY输入")
    });
    // 100ms沿用原桌面paste-to-Enter间隔，避免TUI尚未消费粘贴正文。
    std::thread::sleep(Duration::from_millis(100));
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(node, key(terminal::Key::Enter))
            .expect("原PTY提交")
    });
    let permission = drain_until(
        &app,
        cx,
        |_| {
            events(&log)
                .into_iter()
                .find(|e| e["hook_event_name"] == "PermissionRequest")
        },
        Duration::from_secs(120),
    );
    if permission.is_none() {
        let terminal = app.read_with(cx, |app, _| screen(&app.runtime, node));
        app.update(cx, |app, _| app.runtime.kill_all());
        panic!(
            "没有原生PermissionRequest：{terminal}; events={}",
            json!(events(&log))
        );
    }
    let permission = permission.expect("准确请求");
    assert_eq!(permission["session_id"], session, "Hook必须属于原卡会话");
    assert_eq!(
        permission["tool_input"]["command"], command,
        "只能操作指定临时写入"
    );
    println!("NATIVE_CLAUDE_PERMISSION {}", permission);
    let tool_id = events(&log)
        .into_iter()
        .find(|event| {
            event["hook_event_name"] == "PreToolUse"
                && event["tool_input"] == permission["tool_input"]
        })
        .and_then(|event| event["tool_use_id"].as_str().map(str::to_owned))
        .expect("同一工具的准确ID");
    let transcript = Path::new(permission["transcript_path"].as_str().expect("会话历史"));
    let projects = Path::new(&std::env::var("HOME").expect("home")).join(".claude/projects");
    let mut cursor = norma::claude::transcript_end(transcript, &projects, &session, workspace)
        .expect("决定前完整EOF");
    let dialog = drain_until(
        &app,
        cx,
        |r| {
            let text = screen(r, node);
            (text.contains("Yes") && text.contains("Esc")).then_some(text)
        },
        Duration::from_secs(30),
    );
    if dialog.is_none() {
        let terminal = app.read_with(cx, |app, _| screen(&app.runtime, node));
        app.update(cx, |app, _| app.runtime.kill_all());
        panic!("Hook退出后原生确认界面未出现：{terminal}");
    }
    println!("NATIVE_CLAUDE_DIALOG {}", dialog.expect("原生确认屏幕"));
    assert!(!target.exists(), "决定前不得执行写入");
    let submitted = std::time::Instant::now();
    let progress_offset = fs::metadata(transcript).expect("原历史长度").len() as usize;
    let mut seen_progress = std::collections::HashSet::new();
    let mut inspected_process = false;
    app.update(cx, |app, _| {
        app.runtime
            .send_node_input(
                node,
                key(terminal::Key::Char(if allow { '1' } else { '3' })),
            )
            .expect("原生数字快捷键选择");
    });
    let outcome = drain_until(
        &app,
        cx,
        |_| {
            if slow {
                // Observe only this test CLI's descendants, and print only our exact target command.
                if !inspected_process && submitted.elapsed() >= Duration::from_secs(3) {
                    inspected_process = true;
                    process_evidence(root_pid.load(std::sync::atomic::Ordering::Acquire), &target);
                }
                // Probe-only raw reads expose unknown Provider events; production must remain bounded/incremental.
                for line in
                    fs::read_to_string(transcript).expect("原历史")[progress_offset..].lines()
                {
                    let Ok(event) = serde_json::from_str::<Value>(line) else {
                        continue;
                    };
                    if event["type"] == "progress"
                        && seen_progress.insert(event["uuid"].to_string())
                    {
                        println!(
                            "NATIVE_CLAUDE_PROGRESS elapsed_ms={} {}",
                            submitted.elapsed().as_millis(),
                            event
                        );
                    }
                }
            }
            // Native No does not emit Stop/PostToolUseFailure in Claude 2.1.202.
            // The actual tool_result, bound to prompt and tool ID, is the receipt.
            let page = norma::claude::read_transcript(
                transcript,
                &projects,
                &session,
                workspace,
                Some(cursor.clone()),
                100,
            )
            .expect("读取精确原会话");
            cursor = page.cursor;
            page.messages.into_iter().find_map(|message| {
                if message.role != "user"
                    || message.prompt_id.as_deref() != permission["prompt_id"].as_str()
                {
                    return None;
                }
                message
                    .content
                    .as_array()?
                    .iter()
                    .find(|block| block["type"] == "tool_result" && block["tool_use_id"] == tool_id)
                    .cloned()
            })
        },
        // Slow foreground command runs 30s; 15s extra covers Provider result append, not a production API timeout.
        Duration::from_secs(if slow { 45 } else { 30 }),
    );
    println!(
        "NATIVE_CLAUDE_TOOL_RESULT elapsed_ms={} {}",
        submitted.elapsed().as_millis(),
        json!(outcome)
    );
    println!("NATIVE_CLAUDE_AFTER {}", json!(events(&log)));
    println!(
        "NATIVE_CLAUDE_AFTER_SCREEN {}",
        app.read_with(cx, |app, _| screen(&app.runtime, node))
    );
    app.update(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "不能建立第二执行器");
        assert_eq!(
            app.runtime.agent_session_id(node).as_deref(),
            Some(session.as_str()),
            "决定必须留在原会话"
        );
        app.runtime.kill_all();
    });
    assert!(
        outcome.is_some(),
        "必须找到准确工具终态；按键写入不等于完成"
    );
    if allow {
        assert_eq!(
            outcome.expect("批准执行证据")["is_error"] == true,
            fails,
            "准确结果必须反映指定命令的退出状态"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("批准后文件"),
            "norma-native-permission",
            "批准只执行指定写入"
        );
        assert!(
            !directory.path().join("denied.txt").exists(),
            "不能执行拒绝目标"
        );
    } else {
        assert!(!target.exists(), "拒绝后不能写入");
        let result = outcome.expect("拒绝证据");
        assert_eq!(result["is_error"], true, "拒绝不得被视为成功执行");
        assert!(
            result["content"].as_str().is_some_and(
                |text| text.starts_with("The user doesn't want to proceed with this tool use.")
            ),
            "必须是Provider的明确拒绝结果"
        );
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

fn process_evidence(root: u32, target: &Path) {
    let mut pending = vec![root];
    let mut seen = std::collections::HashSet::new();
    // 64 processes bounds this read-only probe; ordinary CLI tool execution is a shallow tree.
    while let Some(parent) = pending.pop() {
        if parent <= 1 || !seen.insert(parent) || seen.len() > 64 {
            continue;
        }
        let output = std::process::Command::new("/usr/bin/pgrep")
            .args(["-P", &parent.to_string()])
            .output()
            .expect("本CLI子进程");
        for pid in String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|s| s.parse::<u32>().ok())
        {
            let record = std::process::Command::new("/bin/ps")
                .args(["-p", &pid.to_string(), "-o", "ppid=,command="])
                .output()
                .expect("候选子进程");
            let record = String::from_utf8_lossy(&record.stdout);
            let Some((ppid, command)) = record.trim().split_once(char::is_whitespace) else {
                continue;
            };
            if ppid.parse::<u32>().ok() != Some(parent) {
                continue;
            }
            if command.contains(target.to_string_lossy().as_ref()) {
                println!("NATIVE_CLAUDE_PROCESS root={root} parent={parent} pid={pid} {command}");
            }
            pending.push(pid);
        }
    }
}

fn events(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
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
