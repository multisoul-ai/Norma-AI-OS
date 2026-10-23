use super::*;
use gpui::TestAppContext;
use std::sync::Mutex;

/// 场景：默认 INFO 日志能区分 /help 未执行、被错误提示阻塞及恢复显示。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离存储、0 个项目；/help 对应 1 个帮助候选。
///   先输入含 PRIVATE-CANARY 的草稿，再输入 /help；错误提示含 ERROR-CANARY。
///   延迟快照等待 300ms = 抽屉展开 220ms + 调度余量 80ms，只应记录 1 次。
///   阻塞解除后同步 2 次 = 1 次状态转换 + 1 次无变化，故转换日志只能有 1 条。
/// 执行过程（逐步说明系统如何处理）：
///   1. 聚焦并通过真实 InputEvent 输入草稿，推进快照定时器，再隐藏候选后按 Enter。
///   2. 恢复候选、设置工作空间错误，再执行 /help → 引导暂停。
///   3. 清除错误、同步两次 → 引导显示；最后关闭引导。
/// 预期结果：
///   - 正断言：INFO 包含输入、执行拒绝、阻塞原因和恢复状态。
///   - 负断言：不能泄露草稿或错误正文，也不能每次同步重复写日志。
#[test]
fn help_diagnostics_explain_blockers_without_logging_user_text() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let log_path = temp.path().join("help.log");
    let file = std::fs::File::create(&log_path).expect("应创建日志文件");
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .with_writer(Mutex::new(file))
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
        let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
        let mut test = TestAppContext::single();
        let mut app = None;
        let (_, cx) = test.add_window_view(|window, cx| {
            let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
            app = Some(entity.clone());
            ui::component::Root::new(entity, window, cx)
        });
        let app = app.expect("应创建 App");
        app.update_in(cx, |app, window, cx| app.focus_command_input(window, cx));
        cx.simulate_input("/private/PRIVATE-CANARY");
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("/help");
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        cx.run_until_parked();
        app.update(cx, |app, _| app.hide_command_suggestions());
        cx.simulate_keystrokes("enter");
        app.update_in(cx, |app, window, cx| {
            assert!(!app.feature_guide.is_visible(), "隐藏候选时不应执行帮助");
            app.set_command_query("/help".into(), true, window, cx);
            app.workspace_state.error = Some(ui::i18n::LocalizedText::new("ERROR-CANARY"));
            app.execute_dock_suggestion(window, cx);
            assert!(!app.feature_guide.is_visible(), "错误提示应阻塞引导");
            app.workspace_state.error = None;
            app.sync_feature_guide_blockers(cx);
            app.sync_feature_guide_blockers(cx);
            assert!(app.feature_guide.is_visible(), "阻塞解除后引导应显示");
            app.close_feature_guide_for_session(cx);
        });
    });
    let logs = std::fs::read_to_string(log_path).expect("应读取日志");
    assert!(
        logs.contains("stage=\"input_changed\""),
        "默认级别必须记录指令输入"
    );
    assert!(
        logs.contains("stage=\"execute_hidden\""),
        "必须解释候选隐藏导致未执行"
    );
    assert!(
        logs.contains("help_selected=true"),
        "必须记录是否命中帮助候选"
    );
    assert_eq!(
        logs.matches("stage=\"help_after_300ms\"").count(),
        1,
        "完整 /help 应只追加一次延迟快照，其他草稿不应启动快照"
    );
    let blocked = logs
        .lines()
        .find(|line| line.contains("stage=\"guide_open_result\""))
        .expect("必须记录引导打开结果");
    assert!(
        blocked.contains("presentation=Suspended"),
        "必须记录引导实际暂停"
    );
    assert!(
        blocked.contains("workspace_error_visible=true"),
        "必须指出工作空间错误阻塞"
    );
    assert!(
        blocked.contains("settings_open=false"),
        "不能把错误提示误报成设置面板"
    );
    assert_eq!(
        logs.matches("stage=\"guide_visibility_changed\"").count(),
        1,
        "无状态变化的重复同步不能刷日志"
    );
    assert!(!logs.contains("PRIVATE-CANARY"), "日志不能泄露输入正文");
    assert!(!logs.contains("ERROR-CANARY"), "日志不能泄露错误正文");
}
