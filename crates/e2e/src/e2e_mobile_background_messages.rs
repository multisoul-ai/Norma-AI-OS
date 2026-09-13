use crate::{
    e2e_mobile_snapshot::fetch_action,
    harness::{echo_factory, make_app, project_store},
};
use app::{MobileAccess, MobileServer};
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
use gpui::TestAppContext;
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 未开窗原卡的首条手机消息走真实PTY启动、粘贴、Enter和准确Hook回执。
///
/// 数据构造：子测试进程独占临时HOME/ZDOTDIR；Codex和Claude均替换为本测试脚本，
///   每个项目含目标原卡及1张未启动旁卡；25s为生产确认预算，测试20s内完成。
/// 执行过程：保存旧卡 → WSS后台加载 → 发消息 → 脚本记录实际Enter → 投递对应Provider Hook → 回查。
/// 预期结果：只有目标PTY启动；Codex可在首条消息后获得session；Claude等待SessionStart；
///   无窗口、无第二执行器、重复操作不再输入。脚本模拟Provider边界，不声称真实模型验证。
#[test]
fn cold_original_cards_receive_first_message_without_project_window() {
    if let Ok(root) = std::env::var("NORMA_NATIVE_FIXTURE_ROOT") {
        verify(AgentProfile::Codex, Path::new(&root));
        verify(AgentProfile::Claude, Path::new(&root));
        return;
    }
    let directory = tempfile::tempdir().expect("隔离进程目录");
    std::fs::write(directory.path().join(".zshrc"),
        "codex() { exec /usr/bin/python3 \"$NORMA_NATIVE_FIXTURE_ROOT/provider.py\" codex \"$@\"; }\nclaude() { exec /usr/bin/python3 \"$NORMA_NATIVE_FIXTURE_ROOT/provider.py\" claude \"$@\"; }\n").expect("隔离CLI入口");
    std::fs::write(
        directory.path().join("provider.py"),
        include_str!("../tests/fixtures/native_background_provider.py"),
    )
    .expect("模拟Provider");
    let result = std::process::Command::new(std::env::current_exe().expect("测试程序"))
        .args(["--exact", "e2e_mobile_background_messages::cold_original_cards_receive_first_message_without_project_window", "--nocapture"])
        .env("HOME", directory.path()).env("ZDOTDIR", directory.path())
        .env("NORMA_NATIVE_FIXTURE_ROOT", directory.path()).output().expect("独立测试进程");
    assert!(
        result.status.success(),
        "原生冷启动闭环失败: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn verify(profile: AgentProfile, fixture: &Path) {
    let root = fixture.join(profile.display_title());
    std::fs::create_dir(&root).expect("项目目录");
    let mut owner = TestAppContext::single();
    owner.update(|cx| {
        app::install_norma_endpoint_for_e2e(root.join("norma.sock"), cx).expect("隔离路由")
    });
    let (launcher, cx) = make_app(&mut owner, echo_factory());
    let store = AppStore::new(launcher.read_with(cx, |app, _| app.app_database_path_for_e2e()));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(&root, "Background")
        .expect("项目身份");
    let mut runtime = runtime::Runtime::new();
    let node = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0., 0., 640., 400.),
    );
    let other = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: Some("Unstarted".into()),
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(640., 0., 640., 400.),
    );
    runtime
        .save_canvas_snapshot(&project_store(&launcher, cx, &root).canvas_store())
        .expect("原卡存储");
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (snapshots, commands) = launcher.read_with(cx, |app, cx| {
        (
            app.mobile_snapshot_source_for_e2e(cx),
            app.mobile_command_source_for_e2e(cx),
        )
    });
    let server = MobileServer::start_with_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        snapshots,
        commands,
    )
    .expect("WSS");
    let snapshot = fetch_action(
        cx,
        &server,
        &cert,
        &machine,
        json!({"type":"snapshot","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project.workspace_id}),
    );
    let background = launcher
        .read_with(cx, |app, cx| {
            app.loaded_project_for_e2e(&project.workspace_id, cx)
        })
        .expect("后台项目");
    let expected = snapshot["result"]["projects"][0]["cards"][0].clone();
    assert!(expected["run_id"].is_null(), "查看项目不能提前启动Agent");
    let action = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"first-message","workspace_id":project.workspace_id,"route_id":snapshot["result"]["projects"][0]["route_id"],"grant_revision":1,
        "command":{"type":"send_message","expected":expected,"text":"hello from phone"}
    }});
    let accepted = fetch_action(cx, &server, &cert, &machine, action.clone());
    assert_eq!(
        accepted["result"]["status"], "receipt",
        "真实发送应有可恢复回执: {accepted}"
    );
    let query = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project.workspace_id,"operation_id":"first-message"});
    let source = if profile == AgentProfile::Codex {
        norma::agent_hook::HookSource::Codex
    } else {
        norma::agent_hook::HookSource::Claude
    };
    let mut started = false;
    let mut submitted = false;
    // 20s < 25s 生产确认期限；本地脚本不调用模型，仅需启动+100ms粘贴绘制+Hook调度。
    let deadline = Instant::now() + Duration::from_secs(20);
    let receipt = loop {
        if !started && profile == AgentProfile::Claude && root.join("started").exists() {
            let event = norma::agent_hook::parse_hook_event(
                source,
                node,
                json!({"hook_event_name":"SessionStart","session_id":"fixture-session"}),
            )
            .expect("准确启动Hook");
            background.update(cx, |app, _| {
                app.enqueue_agent_hook_event_for_e2e(event)
                    .expect("启动入队")
            });
            started = true;
        }
        if !submitted && root.join("submitted").exists() {
            let text = std::fs::read_to_string(root.join("submitted")).expect("读取实际PTY输入");
            assert_eq!(text, "hello from phone", "实际Enter前的输入必须准确");
            let event = norma::agent_hook::parse_hook_event(source, node, json!({"hook_event_name":"UserPromptSubmit","session_id":"fixture-session","turn_id":"fixture-turn","prompt":text})).expect("准确提交Hook");
            background.update(cx, |app, _| {
                app.enqueue_agent_hook_event_for_e2e(event)
                    .expect("提交入队")
            });
            submitted = true;
        }
        let value = fetch_action(cx, &server, &cert, &machine, query.clone());
        if value["result"]["receipt"]["state"] != "accepted" {
            break value;
        }
        assert!(Instant::now() < deadline, "首条消息必须有界完成: {value}");
    };
    assert_eq!(
        receipt["result"]["receipt"]["state"], "succeeded",
        "必须收到真实PTY提交后的准确回执: {receipt}"
    );
    assert_eq!(
        receipt["result"]["receipt"]["result"]["turn_id"], "fixture-turn",
        "不能编造回合身份"
    );
    assert!(submitted, "未收到Enter不能报告成功");
    assert_eq!(
        background.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "只启动目标原卡"
    );
    assert!(
        background
            .read_with(cx, |app, _| app.runtime.node_view(other))
            .is_none(),
        "旁卡不能批量启动"
    );
    assert_eq!(
        cx.cx.update(|cx| cx.windows().len()),
        1,
        "后台不能额外创建项目窗口"
    );
    let replay = fetch_action(cx, &server, &cert, &machine, action);
    assert_eq!(
        replay["result"]["receipt"]["result"]["turn_id"], "fixture-turn",
        "重复操作只回查原回执"
    );
    background.update(cx, |app, _| app.runtime.kill_all());
}
