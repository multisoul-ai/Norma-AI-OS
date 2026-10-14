use super::test_support::Fixture;
use serde_json::json;
use std::sync::Arc;

/// 场景：Mac 已有区域保存八张 Claude 卡片，再经连接器创建 Codex 卡片。
/// 构造：临时画布及项目定义库；八个原名字，生产 PreparedProject 加载并保留实例。
/// 执行：真实 create_card → directory → 脏保存 → 重新读取画布和定义。
/// 预期：8 + 1 = 9 张卡片全部保留，新名字不重复；保存不能覆盖原八张卡片。
#[test]
fn connector_creation_preserves_eight_persisted_claude_cards() {
    let mut f = Fixture::with_persisted_cards(Arc::new(norma::connector::SystemClock), 8);
    let before = f.call("directory", json!({}));
    assert_eq!(
        before["workspaces"][0]["cards"]
            .as_array()
            .expect("原目录")
            .len(),
        8,
        "加载后原八张可见"
    );
    let created = f.create("ninth", 0, None);
    assert_eq!(created["state"], "loaded", "创建成功：{created}");
    let directory = f.call("directory", json!({}));
    let cards = directory["workspaces"][0]["cards"]
        .as_array()
        .expect("完整目录");
    assert_eq!(cards.len(), 9, "原八张加新 Codex 共九张");
    assert!(
        cards.iter().all(|c| c["role_name_unique"] == true),
        "九个名字必须唯一"
    );
    assert_ne!(created["role_name"], "Alan", "不能占用原卡片名字");
    let store = f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.persistence
                .flush_now(&app.runtime, &app.master_layout)
                .expect("保存全部画布");
            app.persistence.project_store().expect("区域库").clone()
        })
    });
    let reopened = store
        .canvas_store()
        .load_snapshot()
        .expect("重新读取")
        .expect("保存画布");
    assert_eq!(reopened.nodes.len(), 9, "保存和重开必须保留全部九张");
    for name in [
        "Alan", "Ada", "Linus", "Dennis", "Ken", "Donald", "Edsger", "Grace",
    ] {
        assert!(reopened.nodes.iter().any(|n| matches!(&n.kind,canvas::CanvasNodeKind::Agent(s) if s.alias.as_deref()==Some(name))), "重开必须保留 {name}");
    }
}

/// 场景：同一项目两个区域先同时加载；第一区域随后新增共享定义，另一区域的内存定义仍为空。
/// 构造：临时共享项目，先在区域 A 建 Alan，再在区域 B 建卡。
/// 执行：两个真实 create_card 和完整 workspace directory。
/// 预期：第二次成功且分配不同名字，目录不漏任一区域；复制同名卡后两张都标不唯一。
#[test]
fn connector_creation_refreshes_shared_definitions_and_counts_all_areas() {
    let mut f = Fixture::new();
    let first = f.create("first-area", 0, None);
    let second = f.create("second-area", 1, None);
    assert_eq!(
        second["state"], "loaded",
        "区域 B 必须刷新 A 新增的共享定义：{second}"
    );
    assert_ne!(
        first["role_name"], second["role_name"],
        "共享项目自动名字不能重复"
    );
    let first_node = f
        .repo
        .signer
        .verify(first["card_id"].as_str().expect("卡片"))
        .expect("身份")
        .node_id;
    f.test.update(|cx| {
        let kind = f.sessions[0]
            .read(cx)
            .runtime
            .document
            .node(first_node)
            .expect("原卡")
            .kind
            .clone();
        f.sessions[1].update(cx, |app, _| {
            app.runtime
                .document
                .add_node(kind, canvas::CanvasRect::new(0.0, 0.0, 640.0, 400.0));
        });
    });
    let directory = f.call("directory", json!({}));
    let cards = directory["workspaces"][0]["cards"]
        .as_array()
        .expect("所有区域");
    assert_eq!(cards.len(), 3, "跨两区域保留 2 + 1 张");
    let duplicates: Vec<_> = cards
        .iter()
        .filter(|c| c["role_name"] == first["role_name"])
        .collect();
    assert_eq!(duplicates.len(), 2, "跨区域名字相同的两张均可见");
    assert!(
        duplicates.iter().all(|c| c["role_name_unique"] == false),
        "唯一性必须按全 workspace 计数"
    );
    assert_eq!(
        cards
            .iter()
            .find(|c| c["role_name"] == second["role_name"])
            .expect("新卡")["role_name_unique"],
        true,
        "另一名字保持唯一"
    );
}
