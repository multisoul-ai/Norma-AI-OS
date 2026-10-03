use storage::{DatabasePaths, MemoryAccess};

/// 读取关闭后立即重开也留下撤销代次，持续会话据此清除旧上下文。
///
/// 数据构造：默认开关开启，初始撤销代次=0；关闭后再开启。
/// 执行过程：读取初始代次→关闭→开启→新服务读取。
/// 预期结果：开关开启且代次=1；不是旧代次 0，重启不丢失撤销事实。
#[test]
fn recall_revocation_survives_disable_then_enable() {
    let temp = tempfile::tempdir().expect("独立根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = MemoryAccess::new(paths.clone(), None);
    assert_eq!(
        access.recall_epoch().expect("初始代次"),
        0,
        "首次默认没有撤销"
    );
    access
        .set_preferences(agent::MemoryPreferences {
            recall: false,
            save: true,
        })
        .expect("关闭");
    access
        .set_preferences(agent::MemoryPreferences {
            recall: true,
            save: true,
        })
        .expect("重开");
    let reopened = MemoryAccess::new(paths, None);
    assert_eq!(
        reopened.recall_epoch().expect("持久代次"),
        1,
        "关闭操作必须留下可识别的上下文失效代次"
    );
    assert!(
        reopened.preferences().expect("配置").recall,
        "读取应已重新开启"
    );
    assert_ne!(
        reopened.recall_epoch().expect("代次"),
        0,
        "不能因重开回退到旧上下文"
    );
}

/// 不同项目的缓存页面切换单项开关，不能回滚另一页已经保存的设置。
///
/// 数据构造：两个服务 A/B 共享同一 app.sqlite；默认 recall/save 都为 true。
/// 执行过程：A 关闭 recall → B 关闭 save → A 重新开启 recall → 新服务读取。
/// 预期结果：recall=true、save=false；撤销代次=1，不能被重开操作清零。
#[test]
fn independent_switch_updates_preserve_other_saved_preference() {
    let temp = tempfile::tempdir().expect("独立根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut a = MemoryAccess::new(paths.clone(), None);
    let mut b = MemoryAccess::new(paths.clone(), None);
    a.set_recall_enabled(false).expect("A 应关闭 recall");
    b.set_save_enabled(false).expect("B 应关闭 save");
    let result = a.set_recall_enabled(true).expect("A 应重开 recall");
    assert!(result.recall, "A 的单项操作应开启 recall");
    assert!(!result.save, "A 不能用陈旧缓存回滚 B 已关闭的 save");
    let reopened = MemoryAccess::new(paths, None);
    assert!(
        !reopened.preferences().expect("读配置").save,
        "关闭 save 的事实必须持久化"
    );
    assert_eq!(
        reopened.recall_epoch().expect("代次"),
        1,
        "仅关闭一次 recall，应保持一次撤销记录"
    );
}
