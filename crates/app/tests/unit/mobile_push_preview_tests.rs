use super::*;
use storage::MobileTurnKey;

/// 通知预览按UTF-8预算裁剪，项目/卡片/冻结摘要保持可读，无摘要时不借用当前卡片正文。
///
/// 数据构造：70个emoji=280B卡片名；300个“界”=900B摘要；640B上限减省略号3B，
///   floor(637/3)=212个汉字，加省略号共639B；最长u64节点20位。
/// 执行过程：由正式快照和完成事件生成预览 → 检查准确裁剪 → 清空事件摘要再生成。
/// 预期结果：完整Unicode及节点编号保留；下一回合正文和控制字符不出现；缺失摘要明确提示查看输出。
#[test]
fn preview_bounds_text_and_never_borrows_the_next_turn() {
    let project: ProjectSnapshot = serde_json::from_value(json!({
        "workspace_id":"workspace-a", "display_name":"\n Norma\tOS\u{001b}",
        "state":"open", "route_id":"route-a", "card_count":1,
        "cards":[{"node_id":u64::MAX, "profile":"Codex", "title":"😀".repeat(70),
            "state":"thinking", "mobile_control":true, "task_summary":"NEXT_TURN_TEXT"}]
    }))
    .expect("明确的源快照");
    let mut notice = Notice {
        workspace: "workspace-a".into(),
        route: "route-a".into(),
        key: MobileTurnKey {
            node_id: u64::MAX,
            profile: "Codex".into(),
            run_id: "run-a".into(),
            session_id: "session-a".into(),
            turn_id: "turn-a".into(),
        },
        request: None,
        kind: "completed",
        created: 0,
        summary: Some("界".repeat(300)),
    };
    let preview = content(&project, &notice);
    assert_eq!(preview["project"], "Norma OS", "应折叠空白并去除控制符");
    assert_eq!(
        preview["card"],
        format!("{}… · #18446744073709551615", "😀".repeat(49)),
        "卡片名裁剪必须保留完整Unicode与准确编号"
    );
    assert_eq!(
        preview["summary"],
        format!("{}…", "界".repeat(212)),
        "摘要不能拆断UTF-8字符或超出640B预算"
    );
    notice.summary = None;
    let missing = content(&project, &notice);
    assert_eq!(
        missing["summary"], "本次任务已完成，打开卡片查看完整输出。",
        "无摘要时必须如实提示"
    );
    assert!(
        !missing.to_string().contains("NEXT_TURN_TEXT"),
        "下一回合正文不能冒充旧回合结果"
    );
}
