use norma::mobile::{WorkspaceAgentTarget, WorkspaceTerminalFrame, WorkspaceTerminalRequest};

/// 场景：富终端分片保持同一画面身份，超长行不会突破中继帧预算。
///
/// 数据构造：120 行 × 512 列 = 61440 格；每格带独立颜色及中文字符。
///   每帧正文上限 200 KiB，低于中继 256 KiB - 24 字节加密开销。
/// 执行过程：构造完整快照 → 从首行分片 → 逐页拼接 → 核对全部格和结束游标。
/// 预期结果：首片不能包含全部行；拼接不丢行；每片低于预算；越界游标被拒绝。
#[test]
fn rich_terminal_fragments_preserve_all_rows_within_relay_budget() {
    let frame: WorkspaceTerminalFrame = serde_json::from_value(serde_json::json!({
        "version":"screen-a", "cols":512, "screen_lines":24, "start_line":0,
        "total_lines":120, "running":true, "alt_screen":false, "cursor":null,
        "background":0, "foreground":16777215,
        "rows": (0..120).map(|_| (0..512).map(|column| serde_json::json!({
            "text":"中", "fg":column, "bg":512-column, "flags":0
        })).collect::<Vec<_>>()).collect::<Vec<_>>()
    }))
    .expect("富终端快照");
    let first = frame.fragment("snapshot-a", 0).expect("首片");
    assert!(
        first.rows.len() < 120,
        "富网格必须分片而不是越过中继大小限制"
    );
    assert!(
        serde_json::to_vec(&first).expect("编码").len() <= 200 * 1024,
        "首片必须保留中继信封余量"
    );
    let mut rows = first.rows.clone();
    let mut next = first.next_offset;
    while let Some(offset) = next {
        let fragment = frame.fragment("snapshot-a", offset).expect("后续片");
        assert_eq!(fragment.snapshot_id, "snapshot-a", "后续片不能混入另一画面");
        assert!(
            serde_json::to_vec(&fragment).expect("编码").len() <= 200 * 1024,
            "每片必须低于中继预算"
        );
        rows.extend(fragment.frame.rows);
        next = fragment.next_offset;
    }
    assert_eq!(rows.len(), 120, "分片拼接后必须有完整120行");
    assert_eq!(rows[119][511].text, "中", "最后一格不能因分片丢失");
    assert!(
        frame.fragment("snapshot-a", 120).is_err(),
        "越界续传不能产生伪造空画面"
    );
}

/// 场景：终端读取要求原运行身份，历史和续传参数不可混用。
/// 数据构造：1 个完整卡片目标；历史 before=20，对应明确的画面版本。
/// 执行过程：验证实时请求 → 清除 run → 验证历史缺少版本 → 验证合法历史请求。
/// 预期结果：原实时请求合法；缺运行或历史版本均拒绝；合法历史保留目标。
#[test]
fn terminal_requests_require_exact_run_and_consistent_history_scope() {
    let mut request = WorkspaceTerminalRequest {
        desktop_workspace_id: "space-a".into(),
        target: WorkspaceAgentTarget {
            area_id: "area-a".into(),
            definition_id: 1,
            node_id: 2,
            run_id: Some("run-a".into()),
            session_id: Some("session-a".into()),
            task_ref: None,
        },
        known_version: None,
        before: None,
        history_version: None,
        snapshot_id: None,
        offset: None,
    };
    assert!(request.validate().is_ok(), "精确实时目标应允许读取");
    request.target.run_id = None;
    assert!(request.validate().is_err(), "缺少run不能读取当前替代会话");
    request.target.run_id = Some("run-a".into());
    request.before = Some(20);
    assert!(request.validate().is_err(), "历史必须明确绑定画面版本");
    request.history_version = Some("screen-a".into());
    assert!(request.validate().is_ok(), "有版本的原目标历史应允许读取");
}
