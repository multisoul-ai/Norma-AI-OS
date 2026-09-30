use crate::harness::{drain_until, enter_mobile_canvas, make_app_real};
use canvas::{AgentProfile, CanvasRect};
use gpui::TestAppContext;
use std::{path::Path, time::Duration};

/// 原生Codex卡使用手机上传的图片回答图中内容，原PTY不被替换。
/// 数据构造：独立AppDB和真实Codex卡；原生Swift上传明确指定的测试图，图中验证码只出现在像素里。
/// 执行过程：等待原卡Ready→Swift配对与上传→原会话发送文件引用→核对视觉回复→回收PTY。
/// 预期结果：答案含图中验证码；原节点、运行和终端保留，不通过文字提示泄漏预期答案。
#[test]
#[ignore = "requires real Codex, NORMA_SWIFT_PROBE and NORMA_TEST_IMAGE"]
fn e2e_original_codex_reads_mobile_image() {
    verify(AgentProfile::Codex);
}

/// 原生Claude卡沿用同一手机图片协议，读取上传文件后回答图中验证码。
/// 数据构造：相同测试图、独立原生Claude卡，无任务文件写入；30s为CLI冷启动/Hook预算。
/// 执行过程：原卡Ready→真实TLS上传→同一PTY输入→Provider读图→核对视觉回复并关闭。
/// 预期结果：图片确实可读；手机不创建第二个执行器，关闭后不遗留可控制运行。
#[test]
#[ignore = "requires real Claude, NORMA_SWIFT_PROBE and NORMA_TEST_IMAGE"]
fn e2e_original_claude_reads_mobile_image() {
    verify(AgentProfile::Claude);
}

fn verify(profile: AgentProfile) {
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式Swift原生客户端");
    let mut test = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test);
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("已信任仓库");
    let _sockets = enter_mobile_canvas(&app, cx, workspace);
    let node = app.update(cx, |app, _| {
        app.runtime
            .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 80, 24)
            .expect("原生卡片")
    });
    //30s covers actual CLI cold start and SessionStart, not a fake ready state.
    let before = drain_until(
        &app,
        cx,
        |runtime| {
            runtime
                .mobile_card_snapshots()
                .into_iter()
                .find(|card| card.node_id == node && card.state == "ready")
        },
        Duration::from_secs(30),
    )
    .expect("原卡Ready");
    crate::e2e_mobile_conversation::verify_native(&app, cx, workspace, &probe, "images");
    app.update(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "只能使用原有PTY");
        assert_eq!(app.runtime.mobile_agent_run_id(node), before.run_id.as_deref(), "发送图片不能更换运行");
        assert!(matches!(&app.runtime.document.node(node).expect("原卡").kind, canvas::CanvasNodeKind::Agent(spec) if !spec.mobile_control), "不能迁移原卡模式");
        app.runtime.kill_all();
        assert!(app.runtime.mobile_agent_run_id(node).is_none(), "测试关闭后不能留下运行");
    });
}
