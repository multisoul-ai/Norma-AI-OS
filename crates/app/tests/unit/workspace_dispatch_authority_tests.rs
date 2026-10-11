use super::{register, set_role_enabled};
use norma::workspace_dispatch::WorkspaceCardRef;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

/// 场景：同卡先停用、再启用，SQLite 等待不能逆转权限提交顺序。
/// 构造：真实角色和独立 SQLite 写锁；旧 lease 变 false 证明停用已经进入撤权阶段。
/// 执行：停用阻塞时启动启用，再释放写锁，等待两个生产事务并读取真实角色。
/// 预期：最后启用有效，新 lease 与持久角色一致；旧 lease 永不复活。
#[test]
fn role_mutations_serialize_before_database_wait_without_reviving_old_leases() {
    for attempt in 0..6 {
        let temp = tempfile::tempdir().expect("隔离权限");
        let database = temp.path().join("app.sqlite");
        let card = WorkspaceCardRef {
            workspace_id: "workspace".into(),
            area_id: "area".into(),
            node_id: 1,
            card_instance_id: canvas::new_card_instance_id(),
        };
        let store = storage::WorkspaceAgentRoleStore::new(database.clone());
        store.create(&card, "Betty", true).expect("登记真实角色");
        let old = register(&database, &card);
        let blocker = rusqlite::Connection::open(&database).expect("写锁连接");
        blocker
            .execute_batch("BEGIN IMMEDIATE")
            .expect("阻塞角色事务");
        let path = database.clone();
        let target = card.clone();
        let disable = std::thread::spawn(move || set_role_enabled(&path, &target, false));
        let deadline = Instant::now() + Duration::from_secs(2);
        while old.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            !old.load(Ordering::Acquire),
            "第 {attempt} 次停用先关闭旧权限"
        );
        let during = register(&database, &card);
        assert!(
            !during.load(Ordering::Acquire),
            "停用事务窗口中新 lease 也必须关闭"
        );
        let path = database.clone();
        let target = card.clone();
        let enable = std::thread::spawn(move || set_role_enabled(&path, &target, true));
        std::thread::sleep(Duration::from_millis(15));
        blocker.execute_batch("ROLLBACK").expect("释放写锁");
        disable.join().expect("停用线程").expect("停用事务");
        enable.join().expect("启用线程").expect("启用事务");
        let role = store
            .snapshot("workspace")
            .expect("读取持久角色")
            .roles
            .remove(0);
        assert!(
            role.enabled,
            "第 {attempt} 次最终状态应为后发启用，不能被等待中的停用覆盖"
        );
        assert!(
            register(&database, &card).load(Ordering::Acquire),
            "新权限与已启用角色一致"
        );
        assert!(!old.load(Ordering::Acquire), "旧输入权限不能复活");
        assert!(
            !during.load(Ordering::Acquire),
            "事务窗口内被关闭的权限也不能复活"
        );
    }
}
