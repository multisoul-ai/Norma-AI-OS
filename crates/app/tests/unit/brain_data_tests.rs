use super::*;
use gpui::{AppContext, TestAppContext};
use serde_json::{json, Value};
use std::path::Path;

fn write(path: &Path, value: Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn make_app(
    test: &mut TestAppContext,
    paths: storage::DatabasePaths,
) -> gpui::Entity<MultisoulApp> {
    test.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    })
}

/// 即使配置先到达，数据迁移完成前也不能启动服务。
/// 数据：启用设置和已注册项目，准备阶段先失败再重试成功。
/// 执行：在未准备、准备中、准备失败、准备成功四个时刻申请启动。
/// 预期：前三阶段均无ticket，成功后只允许一个启动；准备不能并发执行。
#[test]
fn brain_data_preparation_gates_service_start_and_retries_failure() {
    let owner = owner::SharedBrain::default();
    let settings = config::BrainSettings {
        enabled: true,
        ..Default::default()
    };
    owner.configure(settings.clone(), false);
    let _client = owner.register(
        norma::protocol::BrainScope {
            workspace_id: "alpha".into(),
            route_id: "route".into(),
        },
        "/alpha".into(),
    );
    let now = Instant::now();
    assert!(owner.begin_start(now).is_none(), "未准备数据时不能启动");
    assert!(owner.begin_prepare(now), "第一次准备可开始");
    assert!(!owner.begin_prepare(now), "多个项目不能同时准备");
    assert!(owner.begin_start(now).is_none(), "迁移中仍不启动");
    owner.finish_prepare(Err("fixture migration failure".into()));
    assert!(owner.begin_start(now).is_none(), "迁移失败不能用空状态启动");
    assert!(!owner.begin_prepare(now), "失败后避免逐帧重试IO");
    assert!(
        owner.begin_prepare(now + START_RETRY + Duration::from_secs(1)),
        "截止后允许重试"
    );
    owner.finish_prepare(Ok(data::Prepared {
        directory: "/global/current".into(),
        settings,
    }));
    assert!(owner.begin_start(now).is_some(), "准备成功才允许启动");
    assert!(
        owner.begin_start(now).is_none(),
        "准备成功后也只能一个启动任务"
    );
}

/// 真实App后台迁移失败时，不能启用服务或用默认设置覆盖旧配置。
/// 数据：有效启用配置与损坏的旧目标文件，隔离数据库和brain目录。
/// 执行：调用生产刷新入口等待失败，再尝试修改设置。
/// 预期：无全局入口、无启动资格；原文件字节保持，错误支持中英文。
#[test]
fn brain_data_failed_preparation_preserves_originals_and_rejects_writes() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("brain");
    let old = base
        .join("projects")
        .join(brain::migration::workspace_hash("alpha"));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("goals.json"), "broken goals").unwrap();
    write(
        &base.join("config.json"),
        json!({"enabled":true,"speak":false}),
    );
    let original = std::fs::read(base.join("config.json")).unwrap();
    let mut test = TestAppContext::single();
    let app = make_app(
        &mut test,
        storage::DatabasePaths::isolated_test(&dir.path().join("data")),
    );
    app.update(&mut test, |app, cx| {
        app.refresh_brain_settings_from(base.clone(), cx)
    });
    test.run_until_parked();
    app.update(&mut test, |app, cx| {
        let owner = owner::shared(cx);
        assert!(owner.directory().is_none(), "失败没有就绪目录");
        assert!(owner.settings().is_none(), "不安装默认空配置");
        assert!(
            owner.begin_start(Instant::now()).is_none(),
            "数据失败后不能启动"
        );
        let error = app
            .desktop_workspace
            .error
            .as_ref()
            .expect("UI展示迁移失败");
        assert!(
            error
                .resolve_for(ui::i18n::Locale::ZhCn)
                .contains("原文件已保留"),
            "中文提示保留原数据"
        );
        assert!(
            error
                .resolve_for(ui::i18n::Locale::En)
                .contains("Original files were preserved"),
            "英文提示同义"
        );
        app.set_brain_speaking(true, cx);
    });
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .unwrap()
        .unwrap();
    assert!(
        !brain::migration::active_dir(&base).exists(),
        "失败不发布全局入口"
    );
    assert_eq!(
        std::fs::read(base.join("config.json")).unwrap(),
        original,
        "禁止设置操作覆盖旧配置"
    );
    assert_eq!(
        std::fs::read_to_string(old.join("goals.json")).unwrap(),
        "broken goals",
        "保留损坏原文供修复"
    );
}
