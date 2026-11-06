use super::*;

/// 场景：灵动岛设置全局保存，重启保留最后一次选择，关闭一类提醒不改变其他类别。
/// 数据：隔离 SQLite；初始隐藏=false、三类主动弹出=true；连续保存隐藏和审批关闭，再关闭提问。
/// 执行：通过生产准备/后台写入接口保存并等待屏障，再从相同数据库重新准备。
/// 预期：重启保持隐藏、审批/提问关闭、完成开启；中间快照不能覆盖最后的选择。
#[tokio::test]
async fn island_preferences_restore_latest_background_write() {
    let dir = tempfile::tempdir().expect("创建隔离设置目录");
    let store = storage::SettingsStore::new(dir.path().join("settings.sqlite"));
    let loaded = Prepared::load(store.clone());
    assert_eq!(
        loaded.preferences,
        IslandPreferences::default(),
        "缺省展示岛并允许三类主动弹出"
    );
    assert!(loaded.error.is_none(), "空数据库不是加载失败");
    let writer = Persistence::start(store.clone());
    let mut latest = IslandPreferences {
        hidden: true,
        approvals: false,
        ..Default::default()
    };
    writer.save(latest).expect("后台接受第一份设置");
    latest.questions = false;
    writer.save(latest).expect("后台接受最新设置");
    writer.flush().await.expect("等待全部设置写入");
    let restored = Prepared::load(store);
    assert_eq!(restored.preferences, latest, "重启恢复最终偏好而非旧快照");
    assert!(
        restored.preferences.completions,
        "关闭审批/提问不能连带关闭完成通知"
    );
    assert!(
        !restored.preferences.questions,
        "第二次关闭提问不能被旧快照覆盖"
    );
}

/// 场景：旧版部分设置升级时保留显式关闭值，并为新字段使用产品默认值。
/// 数据：只保存 approvals=false；随后写入错误类型 hidden="yes"。
/// 执行：分别通过后台准备接口读取部分设置和损坏设置。
/// 预期：部分设置仍显示岛且提问/完成开启；损坏配置显式报错，不默默覆盖数据库。
#[test]
fn island_preferences_upgrade_partial_settings_and_report_corruption() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let store = storage::SettingsStore::new(dir.path().join("settings.sqlite"));
    store
        .set_string(KEY, r#"{"approvals":false}"#)
        .expect("构造部分设置");
    let loaded = Prepared::load(store.clone());
    assert_eq!(
        loaded.preferences,
        IslandPreferences {
            approvals: false,
            ..Default::default()
        },
        "省略字段沿用产品默认值"
    );
    assert!(loaded.error.is_none(), "合法部分设置不应报错");
    let invalid = r#"{"hidden":"yes"}"#;
    store.set_string(KEY, invalid).expect("构造损坏设置");
    assert!(
        Prepared::load(store.clone()).error.is_some(),
        "错误类型必须显示加载失败"
    );
    assert_eq!(
        store.get_string(KEY).unwrap().as_deref(),
        Some(invalid),
        "读取失败不能悄悄覆盖原设置"
    );
}

/// 场景：后台写入失败时不能把未保存的偏好作为成功回执。
/// 数据：SettingsStore 指向目录，SQLite 无法打开；提交隐藏设置。
/// 执行：保存请求并等待后台结果与退出屏障。
/// 预期：两处都返回错误，结果仍关联本次设置，方便界面显示准确失败状态。
#[tokio::test]
async fn island_preferences_write_failure_remains_visible() {
    let dir = tempfile::tempdir().expect("创建不可写为SQLite的目录");
    let writer = Persistence::start(storage::SettingsStore::new(dir.path().to_owned()));
    let preference = IslandPreferences {
        hidden: true,
        ..Default::default()
    };
    let revision = writer.save(preference).expect("队列仍可接受请求");
    let result = writer.results.recv().await.expect("必须回传写入结果");
    assert_eq!(result.revision, revision, "失败必须关联本次写入");
    assert!(result.result.is_err(), "目录不能写成SQLite，不能报告成功");
    assert!(writer.flush().await.is_err(), "退出屏障保留保存失败");
}

/// 场景：所有屏幕共用的控制器必须读取已恢复设置，并即时响应设置页修改。
/// 数据：在创建控制器前恢复隐藏=true；空模型，不访问用户数据库或真实窗口。
/// 执行：尝试展开隐藏岛，取消隐藏后展开，再通过设置页入口隐藏。
/// 预期：初始与再次隐藏均保持收起且没有截止时间；取消隐藏后可展开。
#[test]
fn island_controller_consumes_restored_and_changed_preferences() {
    use gpui::AppContext;
    use runtime::island::{IslandEvent, IslandMode};
    let mut cx = gpui::TestAppContext::single();
    cx.update(|cx| {
        install_prepared(
            Prepared {
                preferences: IslandPreferences {
                    hidden: true,
                    ..Default::default()
                },
                store: None,
                error: None,
            },
            cx,
        )
    });
    let model = cx.new(|_| super::super::IslandModel::default());
    let controller = cx.new(|cx| super::super::controller::IslandController::new(model, cx));
    controller.update(&mut cx, |controller, cx| {
        controller.event(1, IslandEvent::Expand, cx);
        assert_eq!(
            controller.displays.state.mode(),
            IslandMode::Compact,
            "恢复隐藏偏好后不能闪现展开岛"
        );
    });
    cx.update(|cx| select(Setting::Hidden, false, cx));
    controller.update(&mut cx, |controller, cx| {
        controller.event(1, IslandEvent::Expand, cx);
        assert_eq!(
            controller.displays.state.mode(),
            IslandMode::Expanded,
            "取消隐藏后可正常展开"
        );
    });
    cx.update(|cx| select(Setting::Hidden, true, cx));
    controller.read_with(&cx, |controller, _| {
        assert_eq!(
            controller.displays.state.mode(),
            IslandMode::Compact,
            "设置页隐藏立即收回所有屏幕的共享状态"
        );
        assert_eq!(
            controller.displays.state.deadline_ms(),
            None,
            "隐藏取消悬停或收回计时"
        );
    });
}
