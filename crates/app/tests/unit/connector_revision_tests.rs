use super::*;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
use gpui::{AppContext, TestAppContext};
use storage::{CanvasStore, NodeSession, NodeSessionKind, NodeSessionStore, ProjectStore};

fn restored_app(
    cx: &mut gpui::App,
    root: &Path,
    canvas: &CanvasStore,
    sessions: &NodeSessionStore,
) -> gpui::Entity<crate::app::MultisoulApp> {
    cx.new(|cx| {
        let factory: runtime::PtyFactory =
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
        let mut app = crate::app::MultisoulApp::new_with_pty_factory(cx, factory);
        app.runtime.set_workspace_root(root.to_owned());
        app.runtime.load_canvas_snapshot(canvas).expect("恢复画布");
        app.runtime
            .hydrate_agent_session_bindings(&sessions.load_sessions().expect("恢复会话"));
        app.workspace_route_id =
            Some(crate::norma::endpoint::new_workspace_route_id().expect("新路由代次"));
        app
    })
}

/// 场景：相同SQLite画布与Provider绑定重建App，随机路由和进程run变化不改变revision。
/// 构造：一张Codex卡、定义7、绑定session-saved；640×400卡片、80×24 echo PTY不运行真实CLI。
/// 执行：保存画布/会话、重建两次App，各启动同一会话，比较card_id/revision；改变绑定与路由根再比较。
/// 预期：冷恢复与启动后的revision均稳定；新的Provider session或持久路由根必须改变revision。
#[test]
fn card_revision_survives_persisted_restart_and_changes_with_target() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let database = dir.path().join("canvas.sqlite");
    ProjectStore::new(database.clone())
        .migrate()
        .expect("项目schema");
    let canvas_store = CanvasStore::new(database.clone());
    let session_store = NodeSessionStore::new(database);
    let mut document = canvas::Document::new();
    let node_id = document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: Some(7),
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    canvas_store
        .save_snapshot(&document.to_snapshot())
        .expect("保存画布");
    session_store
        .save_session(&NodeSession {
            node_id,
            kind: NodeSessionKind::Agent,
            cwd: Some(dir.path().to_string_lossy().into_owned()),
            session_id: Some("session-saved".into()),
        })
        .expect("保存会话");
    let identity = CardIdentity {
        workspace_id: "desktop-1".into(),
        area_id: "area-1".into(),
        canvas_id: "persisted-canvas".into(),
        node_id,
        definition_id: 7,
        created_at: 100,
    };
    let signer = CardSigner::new(&[3; 32]);
    let card_id = signer.sign(&identity);
    let test = TestAppContext::single();
    let (first_revision, first_run, first_route) = test.update(|cx| {
        let entity = restored_app(cx, dir.path(), &canvas_store, &session_store);
        entity.update(cx, |app, _| {
            let cold = card_revision(app, &identity);
            app.runtime
                .start_native_agent_terminal(node_id, 80, 24)
                .expect("echo启动");
            let run = app.runtime.mobile_card_snapshots()[0]
                .run_id
                .clone()
                .expect("启动身份");
            let revision = card_revision(app, &identity);
            assert_eq!(revision, cold, "同一会话启动新PTY不应使revision变化");
            assert_eq!(signer.sign(&identity), card_id, "card_id保持持久身份");
            (revision, run, app.workspace_route_id.clone())
        })
    });
    test.update(|cx| {
        let entity = restored_app(cx, dir.path(), &canvas_store, &session_store);
        entity.update(cx, |app, _| {
            assert_ne!(
                app.workspace_route_id, first_route,
                "实际重建产生不同随机路由"
            );
            assert_eq!(
                card_revision(app, &identity),
                first_revision,
                "从SQLite恢复后revision稳定"
            );
            app.runtime
                .start_native_agent_terminal(node_id, 80, 24)
                .expect("重启echo");
            assert_ne!(
                app.runtime.mobile_card_snapshots()[0].run_id.as_deref(),
                Some(first_run.as_str()),
                "模拟重启确实生成不同运行身份"
            );
            assert_eq!(
                card_revision(app, &identity),
                first_revision,
                "运行身份变化不能污染revision"
            );
            app.runtime.kill_all();
        });
    });
    session_store
        .save_session(&NodeSession {
            node_id,
            kind: NodeSessionKind::Agent,
            cwd: Some(dir.path().to_string_lossy().into_owned()),
            session_id: Some("session-replaced".into()),
        })
        .expect("换Provider会话");
    test.update(|cx| {
        let entity = restored_app(cx, dir.path(), &canvas_store, &session_store);
        entity.update(cx, |app, _| {
            let changed = card_revision(app, &identity);
            assert_ne!(
                changed, first_revision,
                "真实Provider会话替换必须使旧revision失效"
            );
            app.runtime
                .set_workspace_root(dir.path().join("other-project"));
            assert_ne!(
                card_revision(app, &identity),
                changed,
                "持久目标目录变化必须更新路由指纹"
            );
            assert_eq!(
                signer.sign(&identity),
                card_id,
                "会话/目录变化只变revision，不变card_id"
            );
        });
    });
}
