use super::*;
use norma::mobile::{WorkspaceAgentTarget, WorkspaceTerminalCell};

fn request() -> WorkspaceTerminalRequest {
    WorkspaceTerminalRequest {
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
    }
}
fn frame() -> WorkspaceTerminalFrame {
    let cell = WorkspaceTerminalCell {
        text: "中".into(),
        fg: 0xFF0000,
        bg: 0,
        flags: 1,
    };
    // 120 * 512 * ~45 bytes = ~2.6 MiB, so a 200 KiB reply must be fragmented.
    WorkspaceTerminalFrame {
        version: "version-a".into(),
        cols: 512,
        screen_lines: 24,
        start_line: 0,
        total_lines: 120,
        running: true,
        alt_screen: false,
        cursor: None,
        background: 0,
        foreground: 0xFFFFFF,
        rows: vec![vec![cell; 512]; 120],
    }
}

/// 场景：续传只能读取原设备、空间、区域、卡片、运行和会话的不可变快照。
/// 数据构造：120×512中文格超过200KiB预算；两台手机，两个区域，同名卡片不参与身份判断。
/// 执行过程：首片写入缓存 → 篡改每种身份续传 → 原身份续传。
/// 预期结果：跨身份全部拒绝；原捕获仍存在，原颜色不被后续输出替换。
#[test]
fn terminal_cache_isolates_every_identity_and_preserves_fragments() {
    let mut cache = CaptureCache::default();
    let mut request = request();
    let first = cache
        .update("phone-a", &request, frame())
        .expect("首片")
        .fragment
        .expect("分片");
    request.snapshot_id = Some(first.snapshot_id);
    request.offset = first.next_offset;
    assert!(
        cache.continuation("phone-b", &request).is_err(),
        "其他手机不能窃取捕获"
    );
    let mut wrong = request.clone();
    wrong.desktop_workspace_id = "space-b".into();
    assert!(
        cache.continuation("phone-a", &wrong).is_err(),
        "其他空间不能续传"
    );
    wrong = request.clone();
    wrong.target.area_id = "area-b".into();
    assert!(
        cache.continuation("phone-a", &wrong).is_err(),
        "同名项目区域不能复用捕获"
    );
    wrong = request.clone();
    wrong.target.node_id = 3;
    assert!(
        cache.continuation("phone-a", &wrong).is_err(),
        "同名卡片不能复用捕获"
    );
    wrong = request.clone();
    wrong.target.definition_id = 2;
    assert!(
        cache.continuation("phone-a", &wrong).is_err(),
        "其他Agent定义不能复用捕获"
    );
    wrong = request.clone();
    wrong.target.run_id = Some("run-b".into());
    assert!(
        cache.continuation("phone-a", &wrong).is_err(),
        "重建运行不能读取旧捕获"
    );
    wrong = request.clone();
    wrong.target.session_id = Some("session-b".into());
    assert!(
        cache.continuation("phone-a", &wrong).is_err(),
        "更换会话不能读取旧捕获"
    );
    let next = cache.continuation("phone-a", &request).expect("原身份续传");
    assert_eq!(next.version, "version-a", "续传必须保持原捕获版本");
    assert_eq!(
        next.fragment.expect("后续片").rows[0][0].fg,
        0xFF0000,
        "原红色格必须保留"
    );
}

/// 场景：无变化查询只返回版本，缓存过期时不能伪造续传。
/// 数据构造：known_version等于当前版本；过期时间31秒=30秒TTL+1秒。
/// 执行过程：无变化查询 → 建立分片 → 标记过期 → 尝试续传。
/// 预期结果：不返回重复网格；过期捕获被拒绝，重连需要从首片开始。
#[test]
fn terminal_cache_versions_and_expiry_are_explicit() {
    let mut cache = CaptureCache::default();
    let mut request = request();
    request.known_version = Some("version-a".into());
    let unchanged = cache.update("phone-a", &request, frame()).expect("无变化");
    assert!(unchanged.fragment.is_none(), "无变化不能重复发送富网格");
    assert_eq!(unchanged.version, "version-a", "版本必须明确返回");
    request.known_version = None;
    let first = cache
        .update("phone-a", &request, frame())
        .expect("首片")
        .fragment
        .expect("分片");
    request.snapshot_id = Some(first.snapshot_id);
    request.offset = first.next_offset;
    cache.entries[0].created = Instant::now() - Duration::from_secs(31);
    assert!(
        cache.continuation("phone-a", &request).is_err(),
        "30秒后捕获不能续传"
    );
    assert!(cache.entries.is_empty(), "过期捕获不得留在RAM中");
}
