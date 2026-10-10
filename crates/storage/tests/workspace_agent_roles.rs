use agent::workspace_dispatch::WorkspaceCardRef;
use storage::{
    StorageError, WorkspaceAgentRoleStore, WorkspaceCallKeyState, WorkspaceCallResolution,
};

fn card(workspace: &str, area: &str, id: &str) -> WorkspaceCardRef {
    WorkspaceCardRef {
        workspace_id: workspace.into(),
        area_id: area.into(),
        node_id: 1,
        card_instance_id: id.into(),
    }
}

/// 同空间的主名与内置语音别名共同查重，不同空间仍可同名。
/// 数据构造：甲空间两个区域，同 node_id=1 但持久卡片 a/b 不同；乙空间卡片 c。
/// 执行过程：甲创建带空白 Alan，尝试另一卡片 alan 和艾伦；再在乙创建 Alan，读取双语地址。
/// 预期结果：甲只有一个角色，两个冲突均拒绝；甲/乙同名分别命中自己的持久卡片。
#[test]
fn workspace_name_and_builtin_alias_are_one_unique_call_namespace() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let store = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let alan = card("alpha", "area-a", "card-a");
    let other = card("alpha", "area-b", "card-b");
    let beta = card("beta", "area-c", "card-c");
    let created = store.create(&alan, "  Alan  ", true).expect("应登记 Alan");
    assert_eq!(created.name, "Alan", "登记应移除首尾空白");
    assert_eq!(
        created.speech_alias.as_deref(),
        Some("艾伦"),
        "内置名应带对应语音别名"
    );
    assert!(
        matches!(
            store.create(&other, "alan", true),
            Err(StorageError::AgentNameConflict { .. })
        ),
        "英文大小写不能绕过同空间唯一性"
    );
    assert!(
        matches!(
            store.create(&other, "艾伦", true),
            Err(StorageError::AgentNameConflict { .. })
        ),
        "主名不能占用另一角色的语音别名"
    );
    store
        .create(&beta, "Alan", true)
        .expect("其他空间可复用 Alan");
    assert_eq!(
        store.list("alpha").expect("应读取甲").len(),
        1,
        "冲突不得残留半登记角色"
    );
    assert_eq!(
        store.get(&other).expect("应查另一卡片"),
        None,
        "失败创建不得保存角色"
    );
    assert_eq!(
        store.resolve("alpha", "ALAN").expect("应解析英文"),
        WorkspaceCallResolution::Active(created.clone()),
        "英文地址应命中甲的原卡片"
    );
    assert_eq!(
        store.resolve("alpha", "艾伦").expect("应解析中文"),
        WorkspaceCallResolution::Active(created),
        "语音别名应命中同一卡片"
    );
    assert_eq!(
        store
            .get(&beta)
            .expect("应查乙")
            .expect("乙角色应存在")
            .card,
        beta,
        "其他空间名称不能串投到甲"
    );
}

/// 连续改名、停用和重启不能使历次旧称呼复活或释放。
/// 数据构造：同一卡片依次使用 Alan/艾伦、Bella/贝拉、Bruno/布鲁诺，共六键。
/// 执行过程：创建、两次改名、停用，再重开 store；解析旧键并尝试自身或其他卡片复用。
/// 预期结果：四旧键 inactive，两新键 active；revision=创建1+改名2+停用1=4，读取不推进版本。
#[test]
fn renamed_call_keys_stay_inactive_and_occupied_across_disable_and_restart() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = WorkspaceAgentRoleStore::new(path.clone());
    let original = card("alpha", "area-a", "card-a");
    let other = card("alpha", "area-b", "card-b");
    store.create(&original, "Alan", true).expect("应创建原角色");
    store.rename(&original, "Bella").expect("第一次改名应成功");
    store.rename(&original, "Bruno").expect("第二次改名应成功");
    store.set_enabled(&original, false).expect("停用应成功");
    let reopened = WorkspaceAgentRoleStore::new(path);
    let role = reopened
        .get(&original)
        .expect("应读取原角色")
        .expect("原角色仍存在");
    assert_eq!(role.name, "Bruno", "停用和重启必须保留最新主名");
    assert!(!role.enabled, "停用状态必须持久保留");
    assert_eq!(role.card, original, "连续改名不改变持久卡片身份");
    for old in ["Alan", "艾伦", "Bella", "贝拉"] {
        assert_eq!(
            reopened.resolve("alpha", old).expect("应查询旧键"),
            WorkspaceCallResolution::Inactive(role.clone()),
            "旧称呼 {old} 只能返回失效占用"
        );
        assert!(
            matches!(
                reopened.create(&other, old, true),
                Err(StorageError::AgentNameConflict { .. })
            ),
            "其他卡片不得复用失效称呼 {old}"
        );
    }
    assert!(
        matches!(
            reopened.rename(&original, "Alan"),
            Err(StorageError::AgentNameConflict { .. })
        ),
        "同一卡片也不能自行复活历次旧称呼"
    );
    assert_eq!(
        reopened.resolve("alpha", "布鲁诺").expect("应查新别名"),
        WorkspaceCallResolution::Active(role),
        "停用仅改变候选资格，不取消新地址登记"
    );
    let snapshot = reopened.snapshot("alpha").expect("应读完整快照");
    assert_eq!(snapshot.revision, 4, "四次实际变更应各推进一次目录版本");
    assert_eq!(snapshot.call_keys.len(), 6, "连续改名应保留三组双语呼叫键");
    assert_eq!(
        snapshot
            .call_keys
            .iter()
            .filter(|key| key.state == WorkspaceCallKeyState::Active)
            .count(),
        2,
        "只有最新主名与别名有效"
    );
    assert_eq!(
        snapshot
            .call_keys
            .iter()
            .filter(|key| key.state == WorkspaceCallKeyState::Inactive)
            .count(),
        4,
        "历次主名和别名均失效占用"
    );
    assert_eq!(
        reopened.snapshot("alpha").expect("再次读取").revision,
        4,
        "只读查询不能改变目录版本"
    );
}

/// 新名称或对应别名冲突时，原角色和完整目录版本均不改变。
/// 数据构造：Alan 与中文主名“贝拉”分属同空间不同区域，Bella 的别名与贝拉相撞。
/// 执行过程：先取快照，再将 Alan 改为 Bella、贝拉，最后重复当前 Alan 和启用状态。
/// 预期结果：两次冲突与两个无效变更均不改变快照；Alan/艾伦仍有效，Bella 不残留。
#[test]
fn rename_conflicts_preserve_both_current_keys_and_revision() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let store = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let original = card("alpha", "area-a", "card-a");
    store.create(&original, "Alan", true).expect("应创建 Alan");
    store
        .create(&card("alpha", "area-b", "card-b"), "贝拉", true)
        .expect("应占用 Bella 别名");
    let before = store.snapshot("alpha").expect("应读改名前状态");
    assert!(
        matches!(
            store.rename(&original, "Bella"),
            Err(StorageError::AgentNameConflict { .. })
        ),
        "新主名的对应别名冲突应整组拒绝"
    );
    assert!(
        matches!(
            store.rename(&original, "贝拉"),
            Err(StorageError::AgentNameConflict { .. })
        ),
        "新主名与已有主名冲突也应拒绝"
    );
    store
        .rename(&original, " Alan ")
        .expect("重复当前规范名称应成功");
    store
        .set_enabled(&original, true)
        .expect("重复当前启用值应成功");
    assert_eq!(
        store.snapshot("alpha").expect("应读失败后状态"),
        before,
        "失败和无变化请求不能退役旧键、增加新键或推进版本"
    );
    assert_eq!(
        store.resolve("alpha", "Bella").expect("应查拒绝名称"),
        WorkspaceCallResolution::Missing,
        "冲突的新名称不能半途生效"
    );
    let role = store
        .get(&original)
        .expect("应查原角色")
        .expect("原角色仍存在");
    assert_eq!(
        store.resolve("alpha", "艾伦").expect("应查原别名"),
        WorkspaceCallResolution::Active(role),
        "原中文别名仍能准确寻址"
    );
}

/// 只有确认原卡片彻底删除成功后才释放其当前与历史呼叫键。
/// 数据构造：Alan 改为 Bella 后共四键；错误 area 的删除请求引用相同持久 card id。
/// 执行过程：错误地址删除失败并取快照，随后按原地址删除、另卡复用四个称呼中的旧主名和新主名。
/// 预期结果：失败不释放；成功后全键消失，新同名角色身份不同且旧精确引用无角色。
#[test]
fn confirmed_deletion_releases_all_keys_without_rebinding_old_identity() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let store = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let original = card("alpha", "area-a", "card-a");
    store.create(&original, "Alan", true).expect("应创建原角色");
    store.rename(&original, "Bella").expect("应改名");
    let before = store.snapshot("alpha").expect("应读删除前快照");
    let mut stale = original.clone();
    stale.area_id = "area-other".into();
    assert!(
        store.delete_after_card_removed(&stale).is_err(),
        "错误持久地址不能删除原登记"
    );
    assert_eq!(
        store.snapshot("alpha").expect("应读失败后快照"),
        before,
        "删除失败应保留角色和所有占用"
    );
    store
        .delete_after_card_removed(&original)
        .expect("已确认原卡片移除，应释放登记");
    let after = store.snapshot("alpha").expect("应读成功后快照");
    assert_eq!(after.revision, 3, "创建、改名、删除共三次变更");
    assert!(after.roles.is_empty(), "删除后原角色不可再寻址");
    assert!(
        after.call_keys.is_empty(),
        "当前主名、别名和历次旧键均须在同一事务释放"
    );
    let replacement = card("alpha", "area-a", "card-new");
    let new_role = store
        .create(&replacement, "Alan", true)
        .expect("彻底删除后可复用旧名");
    store
        .create(&card("alpha", "area-b", "card-b"), "Bella", true)
        .expect("彻底删除后可复用当前名及别名");
    assert_ne!(
        new_role.card.card_instance_id, original.card_instance_id,
        "同名新角色不能继承原身份"
    );
    assert_eq!(
        store.get(&original).expect("应查旧身份"),
        None,
        "旧引用不能重新绑定同 node_id 新卡片"
    );
    assert_eq!(
        store.resolve("alpha", "艾伦").expect("应查复用别名"),
        WorkspaceCallResolution::Active(new_role),
        "新名称只命中新卡片"
    );
}

/// 桌面和手机等两个独立写入者同时抢同名时只能一方成功。
/// 数据构造：先用乙空间 Nora 初始化 AppDB；甲空间两线程各持独立 Store、area/card，争抢 Alan。
/// 执行过程：Barrier 同步开始创建，join 两个结果后只通过公共快照读取甲空间。
/// 预期结果：成功数=1，明确冲突数=1，甲有一角色和两有效键、revision=1，没有半登记。
#[test]
fn concurrent_name_claims_have_one_winner_and_one_explicit_conflict() {
    use std::sync::{Arc, Barrier};
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = WorkspaceAgentRoleStore::new(path.clone());
    store
        .create(&card("beta", "area-c", "card-c"), "Nora", true)
        .expect("应初始化测试 AppDB");
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [
        card("alpha", "area-a", "card-a"),
        card("alpha", "area-b", "card-b"),
    ]
    .into_iter()
    .map(|target| {
        let path = path.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let store = WorkspaceAgentRoleStore::new(path);
            barrier.wait();
            store.create(&target, "Alan", true)
        })
    })
    .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("创建线程不应 panic"))
        .collect();
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "同空间同呼叫键只能一个创建成功"
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(StorageError::AgentNameConflict { .. })))
            .count(),
        1,
        "另一方必须收到可解释的名称冲突"
    );
    let snapshot = store.snapshot("alpha").expect("应读完整并发结果");
    assert_eq!(snapshot.roles.len(), 1, "冲突方不能留下孤立角色");
    assert_eq!(
        snapshot.call_keys.len(),
        2,
        "主名与别名必须作为整组原子登记"
    );
    assert_eq!(snapshot.revision, 1, "只有成功写入应推进版本");
    assert_eq!(
        snapshot.call_keys[0].card_instance_id, snapshot.call_keys[1].card_instance_id,
        "两个有效键必须命中同一成功卡片"
    );
}

/// 非法名称与重复卡片身份不能改变已有登记。
/// 数据构造：Alan 合法；Norma/Alan2/Alan-Web/一 为保留或格式非法，另一身份复用 area/node。
/// 执行过程：取原快照，尝试非法创建、同卡重复登记、相同位置新卡登记，再对比完整快照。
/// 预期结果：所有尝试失败，原角色与双语键和 revision 均不变。
#[test]
fn invalid_names_and_duplicate_card_addresses_leave_registration_unchanged() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let store = WorkspaceAgentRoleStore::new(temp.path().join("app.sqlite"));
    let original = card("alpha", "area-a", "card-a");
    store.create(&original, "Alan", true).expect("应创建 Alan");
    let before = store.snapshot("alpha").expect("应读基线");
    for name in [" Norma ", "Alan2", "Alan-Web", "一"] {
        assert!(
            matches!(
                store.create(&card("alpha", "area-b", "card-b"), name, true),
                Err(StorageError::InvalidAgentName(_))
            ),
            "名称 {name:?} 应由现行合法格式/保留名规则拒绝"
        );
    }
    assert!(
        matches!(
            store.create(&original, "Bella", true),
            Err(StorageError::WorkspaceAgentCardConflict(_))
        ),
        "同一持久卡片不能注册第二角色"
    );
    assert!(
        matches!(
            store.create(&card("alpha", "area-a", "card-new"), "Bella", true),
            Err(StorageError::WorkspaceAgentCardConflict(_))
        ),
        "同一空间局部位置不能同时挂两张持久卡片"
    );
    assert_eq!(
        store.snapshot("alpha").expect("应读拒绝后状态"),
        before,
        "非法请求不能影响原登记或目录版本"
    );
}
