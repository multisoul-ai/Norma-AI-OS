use norma::mobile::{ConversationPage, ConversationProjection, ConversationRequest};
use serde_json::json;

/// 对话事件只在页变化时推进序号，心跳不能伪造新的历史更新。
///
/// 数据构造：固定run/session、Codex最近页一条回复由“中”增长到“中文”；序号从0开始。
/// 执行过程：首个完整页 → 未变心跳 → 同ID正文增长 → 同页心跳。
/// 预期结果：首屏sequence0且有正文；心跳无page且序号不变；增长完整保留，旧短文本不再输出。
#[test]
fn conversation_projection_tracks_changes_without_duplicate_heartbeats() {
    let request = ConversationRequest {
        workspace_id: "p".into(),
        route_id: "r".into(),
        node_id: 1,
        run_id: "run".into(),
        session_id: "session".into(),
        cursor: None,
    };
    let page = |text| {
        ConversationPage::from_codex(json!({"data":[{"id":"t","items":[{"id":"a","type":"agentMessage","text":text}]}],"nextCursor":null})).expect("完整Provider页")
    };
    let mut projection = ConversationProjection::new("stream-a".into(), request.clone());
    let initial = projection.update(page("中")).expect("基线");
    assert_eq!(initial.sequence, 0, "每连接有新基线");
    assert_eq!(initial.request, request, "事件绑定完整会话范围");
    assert_eq!(
        initial.page.expect("初始页").items[0].text,
        "中",
        "首屏有真实正文"
    );
    let heartbeat = projection.update(page("中")).expect("心跳");
    assert_eq!(heartbeat.sequence, 0, "心跳不能推进历史序号");
    assert!(heartbeat.page.is_none(), "心跳不重复传完整页");
    let changed = projection.update(page("中文")).expect("新正文");
    assert_eq!(changed.sequence, 1, "正文增长推进一次");
    assert_eq!(
        changed.page.expect("变化页").items[0].text,
        "中文",
        "更新不可保留旧短文本"
    );
    assert!(
        projection
            .update(page("中文"))
            .expect("第二次心跳")
            .page
            .is_none(),
        "已同步内容不能再次作为变化发布"
    );
}
