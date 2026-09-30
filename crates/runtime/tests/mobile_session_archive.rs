use runtime::Runtime;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn saved() -> storage::RecordedMobileSession {
    storage::RecordedMobileSession {
        sequence: 1,
        recorded_at_unix: 1,
        transcript_path: None,
        session: storage::MobileSession {
            key: storage::MobileSessionKey {
                node_id: 7,
                profile: "Claude".into(),
                run_id: "run-a".into(),
                session_id: "session-a".into(),
            },
            workspace_root: PathBuf::from("/tmp"),
            title: "原会话".into(),
        },
    }
}

/// 项目Runtime释放后，排队的归档读取必须先被撤销，不能再访问Provider文件或启动只读host。
///
/// 数据构造：原项目/tmp，一份准确Claude身份但尚无文件引用；12s=客户端首帧15s−网络/调度余量3s，为归档启动和读取总预算。
/// 执行过程：获取后台读取对象 → 释放Runtime → 调用read。
/// 预期结果：收到取消错误而非缺少文件错误，说明尚未接触Provider；没有卡片或任务副作用。
#[test]
fn dropping_runtime_revokes_queued_archive_read() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let reader = runtime.mobile_archive_read().expect("后台读取能力");
    assert_eq!(runtime.live_len(), 0, "预约只读能力不能创建Agent");
    drop(runtime);
    let error = reader
        .read(saved(), None, Instant::now() + Duration::from_secs(12))
        .expect_err("已关闭必须失败");
    assert_eq!(
        error, "archive read cancelled or expired",
        "Runtime释放必须撤销尚未开始的读取"
    );
    assert_ne!(
        error, "Claude has no saved transcript",
        "不能在释放后进入Provider读取分支"
    );
}
