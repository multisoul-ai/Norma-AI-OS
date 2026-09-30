use serde_json::{json, Value};
use std::{
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/realtime_delivery_crash.rs"]
mod delivery_crash;
#[path = "support/realtime_upstream.rs"]
mod upstream;
#[path = "support/mobile.rs"]
mod wire;

struct TestGrant(storage::MobileAccessStore);
impl Drop for TestGrant {
    fn drop(&mut self) {
        let _ = self.0.revoke("phone-l3");
    }
}

/// 真正安装包关窗后接收实时工具，原PTY执行准确输入，断开和重连不重复派工。
///
/// 数据构造：显式L3启动、独立.app/数据库/Unix socket/ZDOTDIR、一个空项目和控制授权；
///   本机WebSocket模拟模型工具事件，Python只读写本测试项目；不调用付费模型。
/// 执行过程：真实App打开项目并开启手机服务 → Computer Use关窗 → TLS绑定sideband →
///   派工 → 真实PTY写submitted → 断开 → 重绑并重放原function → 核对原回执/节点。
/// 预期结果：只有一个任务节点，route不变，PTY收到原文；挂断不取消任务、不创建第二窗口。
///   UI点击/窗口证据由Computer Use另存；测试不把marker当作自动化窗口观察。
/// 可选NORMA_REALTIME_CRASH_L3=1：精确匹配本隔离包进程后SIGKILL，重启同一包/数据库；
///   Computer Use重开原项目，再以同一function回读。应保留一张卡和原accepted回执，禁止重派。
/// 可选NORMA_REALTIME_DELIVERY_CRASH_L3=1：真实CLI/Stop Hook完成、故障触发器阻止成果commit，
///   SIGKILL后移除触发器并写后续文件；重启仅恢复原completed与不可用成果，不重采后续字节。
#[test]
#[ignore = "explicit real macOS bundle and Computer Use, loopback only"]
fn real_bundle_dispatches_after_windows_close() {
    assert_eq!(
        std::env::var("NORMA_REALTIME_DESKTOP_L3").as_deref(),
        Ok("1"),
        "必须明确启用本机隔离的真实App验收"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace");
    let capture_crash = std::env::var("NORMA_REALTIME_DELIVERY_CRASH_L3").as_deref() == Ok("1");
    let fixture = tempfile::tempdir().expect("isolated provider");
    let project = fixture.path().join(if capture_crash {
        "Realtime Delivery"
    } else {
        "Realtime Background"
    });
    std::fs::create_dir(&project).expect("test project");
    std::fs::write(
        fixture.path().join("provider.py"),
        if capture_crash {
            include_str!("fixtures/realtime_delivery_provider.py")
        } else {
            include_str!("fixtures/native_background_provider.py")
        },
    )
    .expect("reuse real PTY provider boundary");
    if capture_crash {
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&project)
                .status()
                .expect("isolated Git project")
                .success(),
            "成果基线需要真实Git项目"
        );
    }
    let submitted_file = if capture_crash {
        fixture.path().join("submitted")
    } else {
        project.join("submitted")
    };
    std::fs::write(fixture.path().join(".zshrc"),
        "codex() { exec /usr/bin/python3 \"$NORMA_NATIVE_FIXTURE_ROOT/provider.py\" codex \"$@\"; }\n\
         claude() { exec /usr/bin/python3 \"$NORMA_NATIVE_FIXTURE_ROOT/provider.py\" claude \"$@\"; }\n"
    ).expect("isolated CLI functions");
    let listener = TcpListener::bind("127.0.0.1:0").expect("upstream loopback");
    listener
        .set_nonblocking(true)
        .expect("bounded upstream wait");
    let started = Command::new("bash")
        .arg(root.join("scripts/run-norma-mobile-desktop-l3.sh"))
        .env("NORMA_MOBILE_L3_BUNDLE_ID", "com.norma.realtime-validation")
        .env(
            "NORMA_REALTIME_L3_UPSTREAM",
            listener.local_addr().expect("address").to_string(),
        )
        .env("NORMA_NATIVE_FIXTURE_ROOT", fixture.path())
        .output()
        .expect("build and launch isolated real app");
    assert!(
        started.status.success(),
        "真实App启动失败：{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let output = String::from_utf8_lossy(&started.stdout);
    let app_root = PathBuf::from(output.lines().last().expect("isolated app directory"));
    assert!(
        app_root.parent() == Some(Path::new("/tmp"))
            && app_root
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("norma-mobile-desktop-l3.")),
        "必须是本脚本新建的隔离目录"
    );
    if capture_crash {
        delivery_crash::block_capture(&app_root, fixture.path());
    }
    eprintln!("Desktop realtime root: {}", app_root.display());
    eprintln!("Open project: {}", project.display());
    eprintln!("Computer Use: open this project, enable mobile, close every window, then write the displayed loopback address to {}/closed-address", app_root.display());
    // 15min allows real UI open/enable/close and screenshots; network deadlines below remain 5s.
    wait_until(Duration::from_secs(900), || {
        app_root.join("closed-address").is_file()
    });
    let address: SocketAddr = std::fs::read_to_string(app_root.join("closed-address"))
        .expect("CUA marker")
        .trim()
        .parse()
        .expect("observed server address");
    assert!(
        address.ip().is_loopback(),
        "禁止向用户真实Mac服务发测试请求"
    );
    let store = AppStore::new(app_root.join("data/app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .list()
        .expect("real persisted projects")
        .into_iter()
        .find(|p| p.canonical_root == project.canonicalize().expect("canonical project"))
        .expect("CUA must open the exact test project");
    // Only the explicitly isolated App exports its public identity. Pin subsequent TLS to it;
    // the test executable never reads the App's private key or requests Keychain access.
    let public: Value = serde_json::from_slice(
        &std::fs::read(app_root.join("data/realtime-public-identity.json"))
            .expect("isolated App public identity"),
    )
    .expect("public identity JSON");
    let machine = public["machine_id"]
        .as_str()
        .expect("machine identity")
        .to_owned();
    let cert: Vec<u8> =
        serde_json::from_value(public["certificate_der"].clone()).expect("public DER certificate");
    assert!(
        !cert.is_empty(),
        "TLS必须固定到实际App证书，不能跳过证书验证"
    );
    let token = "a".repeat(64);
    store
        .mobile_access_store()
        .register(
            "phone-l3",
            "Realtime Bundle Test",
            &token,
            &[MobileProjectAccess {
                workspace_id: workspace.workspace_id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("test-only control grant");
    let _grant = TestGrant(store.mobile_access_store());
    let mut query = wire::connect(address, cert.clone());
    let snapshot = |query: &mut _| {
        wire::exchange(
            query,
            &machine,
            "snapshot",
            json!({"type":"snapshot","device_id":"phone-l3","token":token,"workspace_id":workspace.workspace_id}),
        )
    };
    let before = snapshot(&mut query);
    let original = &before["result"]["projects"][0];
    assert_eq!(original["state"], "open", "关窗不能关闭原Runtime：{before}");
    assert_eq!(
        original["card_count"], 0,
        "测试项目必须为空，不能碰用户原卡"
    );
    let bind = |generation| {
        let mut phone = wire::connect(address, cert.clone());
        wire::exchange(
            &mut phone,
            &machine,
            "bind",
            json!({"type":"bind_realtime","device_id":"phone-l3","token":token,
                "binding":{"call_id":"rtc_bundle_l3","workspace_id":workspace.workspace_id,"generation":generation}}),
        );
        phone
    };
    let mut phone = bind(1);
    let mut peer = upstream::attach(&listener);
    let task = if capture_crash {
        "创建 result.txt，内容为 realtime complete"
    } else {
        "实现登录校验，保留约束并交付测试证据"
    };
    let utterance = format!("请让 Codex {task}");
    upstream::turn(&mut peer, "utterance-l3", &utterance);
    upstream::call(&mut peer, "response-l3", "function-l3", task);
    let received = upstream::output(&mut peer, "function-l3");
    assert_eq!(
        received["status"], "accepted",
        "真实App必须返回已接收，不能把失败当完成：{received}"
    );
    assert!(received["task_id"].as_u64().is_some(), "必须有持久任务身份");
    upstream::continuation(&mut peer);
    // The original input-only fixture emits no SessionStart Hook: allow the30s startup grace;
    // the completion fixture sends the real Hook and can become ready earlier.
    // then 100ms between paste and Enter, plus <15s local startup/scheduling margin = 45s.
    // A 20s deadline deletes the fixture before the normal task pump can submit its input.
    wait_until(Duration::from_secs(45), || submitted_file.is_file());
    let submitted = std::fs::read_to_string(&submitted_file).expect("actual PTY input");
    assert!(
        submitted.contains(task),
        "真实PTY必须收到准确指令：{submitted}"
    );
    let completed_turn = capture_crash.then(|| {
        delivery_crash::complete(
            &app_root,
            fixture.path(),
            &workspace.workspace_id,
            received["operation_id"].as_str().expect("operation"),
        )
    });
    let _ = phone.close(None);
    drop(phone);
    drop(peer);
    // The read-only query socket can expire while waiting for the 30s CLI startup grace.
    query = wire::connect(address, cert.clone());
    let after = snapshot(&mut query);
    assert_eq!(
        after["result"]["projects"][0]["route_id"], original["route_id"],
        "挂断不能创建另一个Runtime"
    );
    assert_eq!(
        after["result"]["projects"][0]["card_count"], 1,
        "只应创建一个任务节点"
    );
    phone = bind(2);
    peer = upstream::attach(&listener);
    upstream::turn(&mut peer, "utterance-l3", &utterance);
    upstream::call(&mut peer, "response-l3", "function-l3", task);
    let replay = upstream::output(&mut peer, "function-l3");
    assert_eq!(replay, received, "重连只能回读原回执");
    upstream::continuation(&mut peer);
    let mut final_snapshot = snapshot(&mut query);
    assert_eq!(
        final_snapshot["result"]["projects"][0]["card_count"], 1,
        "重复function不能再次创建Agent"
    );
    let operations: Value = wire::exchange(
        &mut query,
        &machine,
        "operations",
        json!({"type":"realtime_operations","device_id":"phone-l3","token":token,"workspace_id":workspace.workspace_id,"after":0}),
    );
    assert_eq!(
        operations["result"]["operations"].as_array().map(Vec::len),
        Some(1),
        "只保留一个持久操作"
    );
    let _ = phone.close(None);
    let mut live_address = address;
    if capture_crash || std::env::var("NORMA_REALTIME_CRASH_L3").as_deref() == Ok("1") {
        eprintln!(
            "Computer Use: reopen and inspect the single live card, then create {}/crash-ready",
            app_root.display()
        );
        // 10min is the same bounded UI allowance as the final Quit phase.
        wait_until(Duration::from_secs(600), || {
            app_root.join("crash-ready").is_file()
        });
        let bundle = app_root.join("Norma Realtime Validation.app");
        let executable = bundle
            .join("Contents/MacOS/norma-os")
            .canonicalize()
            .expect("canonical isolated executable");
        let processes = Command::new("ps")
            .args(["-axo", "pid=,command="])
            .output()
            .expect("isolated App PID");
        assert!(processes.status.success(), "必须读取真实进程表");
        let candidates = String::from_utf8_lossy(&processes.stdout)
            .lines()
            .filter_map(|line| {
                let (pid, command) = line.trim().split_once(' ')?;
                (command.trim() == executable.to_str()?).then(|| pid.parse::<u32>().expect("PID"))
            })
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1, "只能终止精确隔离包的唯一进程");
        let crashed_pid = candidates[0];
        let killed = Command::new("/bin/kill")
            .args(["-KILL", &crashed_pid.to_string()])
            .status()
            .expect("test-only crash");
        assert!(killed.success(), "必须实际中断隔离App，而非仅写marker");
        // 5s bounds process teardown and listener closure; no production timeout is changed.
        wait_until(Duration::from_secs(5), || {
            std::net::TcpStream::connect(address).is_err()
        });
        if capture_crash {
            delivery_crash::after_crash(&app_root, &project);
        }
        let reopened = Command::new("open")
            .arg("-n")
            .arg(&bundle)
            .args([
                "--env",
                &format!("NORMA_TEST_DATA_ROOT={}/data", app_root.display()),
                "--env",
                "NORMA_MOBILE_L3_LOOPBACK=1",
                "--env",
                "NORMA_VISUAL_PREVIEW=launchpad",
                "--env",
                &format!("NORMA_SOCKET={}/norma.sock", app_root.display()),
                "--env",
                &format!(
                    "NORMA_REALTIME_L3_UPSTREAM={}",
                    listener.local_addr().expect("same upstream")
                ),
                "--env",
                &format!("ZDOTDIR={}", fixture.path().display()),
                "--env",
                &format!("NORMA_NATIVE_FIXTURE_ROOT={}", fixture.path().display()),
                "-o",
                &format!("{}/restart-stdout.log", app_root.display()),
                "--stderr",
                &format!("{}/restart-stderr.log", app_root.display()),
            ])
            .status()
            .expect("restart same isolated bundle");
        assert!(reopened.success(), "重启必须保持同一包、证书和测试数据库");
        eprintln!("Crashed PID {crashed_pid}. Computer Use: open the SAME project, inspect Mobile Connection, then write its loopback address to {}/restart-address", app_root.display());
        wait_until(Duration::from_secs(600), || {
            app_root.join("restart-address").is_file()
        });
        live_address = std::fs::read_to_string(app_root.join("restart-address"))
            .expect("restart address")
            .trim()
            .parse()
            .expect("observed loopback");
        assert!(
            live_address.ip().is_loopback(),
            "重启后仍只能访问隔离回环服务"
        );
        query = wire::connect(live_address, cert.clone());
        final_snapshot = snapshot(&mut query);
        assert_eq!(
            final_snapshot["result"]["projects"][0]["card_count"], 1,
            "进程恢复必须加载原卡片，不能丢失或复制"
        );
        assert_ne!(
            final_snapshot["result"]["projects"][0]["route_id"], original["route_id"],
            "必须是新进程的项目路由，不能用媒体重绑冒充重启"
        );
        phone = wire::connect(live_address, cert.clone());
        wire::exchange(
            &mut phone,
            &machine,
            "restart-bind",
            json!({"type":"bind_realtime","device_id":"phone-l3","token":token,
            "binding":{"call_id":"rtc_bundle_l3","workspace_id":workspace.workspace_id,"generation":3}}),
        );
        peer = upstream::attach(&listener);
        upstream::turn(&mut peer, "utterance-l3", &utterance);
        upstream::call(&mut peer, "response-l3", "function-l3", task);
        let recovered = upstream::output(&mut peer, "function-l3");
        assert_eq!(
            recovered, received,
            "进程重启后只能回读原回执，不能重新执行工具"
        );
        assert_eq!(
            recovered["status"], "accepted",
            "旧任务接收不能升级为执行完成"
        );
        upstream::continuation(&mut peer);
        final_snapshot = snapshot(&mut query);
        assert_eq!(
            final_snapshot["result"]["projects"][0]["card_count"], 1,
            "跨进程重放不能创建第二张Agent卡"
        );
        let recovered_operations = wire::exchange(
            &mut query,
            &machine,
            "recovered-operations",
            json!({"type":"realtime_operations","device_id":"phone-l3","token":token,"workspace_id":workspace.workspace_id,"after":0}),
        );
        assert_eq!(
            recovered_operations["result"]["operations"], operations["result"]["operations"],
            "持久操作身份和结果必须逐字保留，不得新增重派记录"
        );
        assert_eq!(
            std::fs::read_to_string(&submitted_file).expect("old PTY evidence"),
            submitted,
            "原输入证据必须保留"
        );
        let tasks = storage::ProjectStore::for_database_paths(
            &storage::DatabasePaths::isolated_test(&app_root.join("data")),
            &project,
        )
        .expect("same test project store")
        .agent_dispatch_task_store()
        .list()
        .expect("durable dispatch tasks");
        assert_eq!(tasks.len(), 1, "进程恢复后不能新增任务记录");
        assert_eq!(
            tasks[0].runtime_task_id,
            received["task_id"].as_u64().expect("original task"),
            "必须保留旧任务身份"
        );
        assert_eq!(
            tasks[0].state,
            if capture_crash {
                storage::AgentDispatchTaskState::Done
            } else {
                storage::AgentDispatchTaskState::Interrupted
            },
            "已完成与未完成任务必须分别保留真实终态"
        );
        assert_eq!(
            tasks[0].result.is_some(),
            capture_crash,
            "只能为有真实Provider完成的任务保留结果"
        );
        let delivery = completed_turn.as_ref().map(|key| {
            delivery_crash::verify(
                &app_root,
                live_address,
                cert.clone(),
                &machine,
                &workspace.workspace_id,
                &final_snapshot["result"]["projects"][0]["route_id"],
                key,
            )
        });
        std::fs::write(app_root.join("crash-verified.json"), serde_json::to_vec_pretty(&json!({
            "crashed_pid":crashed_pid,"recovered_receipt":recovered,"operations":recovered_operations,"snapshot":final_snapshot,"delivery":delivery,
            "task_state":format!("{:?}",tasks[0].state),"task_result":tasks[0].result.as_ref().and_then(|result|result.text.as_ref())
        })).expect("crash evidence")).expect("save crash evidence");
        let _ = phone.close(None);
    }
    std::fs::write(
        app_root.join("dispatch-verified"),
        serde_json::to_vec_pretty(&json!({
            "before": before, "after": final_snapshot, "receipt": received
        }))
        .expect("evidence"),
    )
    .expect("save actual evidence");
    eprintln!("Verified real dispatch; Computer Use should reopen the same project, inspect its single card, quit the isolated app, then create {}/app-quit", app_root.display());
    wait_until(Duration::from_secs(600), || {
        app_root.join("app-quit").is_file()
    });
    assert!(
        std::os::unix::net::UnixStream::connect(app_root.join("norma.sock")).is_err(),
        "主动退出后编排服务必须停止监听，残留 socket 路径不代表服务仍在运行"
    );
    assert!(
        std::net::TcpStream::connect(live_address).is_err(),
        "主动退出后手机监听不能继续"
    );
    store
        .mobile_access_store()
        .revoke("phone-l3")
        .expect("revoke exact test grant");
}

fn wait_until(timeout: Duration, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "real bundle validation timed out after {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
