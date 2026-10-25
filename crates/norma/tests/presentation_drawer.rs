use norma::protocol::{CardCommandResult, PresentationLayout};

/// 场景：抽屉展示必须返回可解码的真实布局，而非伪称画布 Master。
/// 数据构造：服务端的 Presented 回执，content_drawer 布局和真实内容表面 ID。
/// 执行过程：通过生产协议类型解码 JSON，再重新编码。
/// 预期结果：新布局被接受，保留 created 语义，不能解码为 Master。
#[test]
fn drawer_presentation_receipt_round_trips() {
    let json = r#"{"Presented":{"node_id":42,"kind":"preview","disposition":"created","layout":"content_drawer"}}"#;
    let result: CardCommandResult = serde_json::from_str(json).expect("协议必须接受抽屉展示回执");
    let CardCommandResult::Presented {
        layout, node_id, ..
    } = result
    else {
        panic!("回执必须为 Presented");
    };
    assert_ne!(
        layout,
        PresentationLayout::Master,
        "抽屉不能谎报画布 Master"
    );
    assert_eq!(node_id, 42, "表面 ID 必须不变");
    assert_eq!(
        serde_json::to_string(&layout).expect("编码布局"),
        "\"content_drawer\"",
        "布局字段须如实保留"
    );
}
