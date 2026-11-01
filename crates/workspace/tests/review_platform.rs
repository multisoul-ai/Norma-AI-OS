use std::{
    process::Command,
    time::{Duration, Instant},
};
use workspace::review::{capture, ContentKind};

/// 场景：默认平台可捕获未跟踪工作副本；数据为真实临时 Git 仓库和中文文件。
/// 执行：初始化仓库后通过公共 capture 读取快照。
/// 预期：返回精确中文正文，不产生虚构基准版本，也不将运行数据混入变更。
#[test]
fn native_review_captures_current_bytes() {
    let temp = tempfile::tempdir().expect("隔离工作区");
    let output = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(temp.path())
        .output()
        .expect("真实 Git");
    assert!(
        output.status.success(),
        "临时仓库应初始化成功: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(temp.path().join("中文.txt"), "实际工作副本\n").expect("当前正文");
    let review = capture(temp.path(), Instant::now() + Duration::from_secs(10))
        .expect("默认平台 Review 应可用");
    assert_eq!(review.files.len(), 1, "仅包含一个当前文件");
    assert!(review.base_commit.is_none(), "未提交仓库不能伪造 HEAD");
    let change = &review.files[0];
    assert_eq!(change.path, "中文.txt", "保留 Unicode 相对路径");
    assert!(change.before.is_none(), "未跟踪文件没有旧正文");
    let after = change.after.as_ref().expect("当前文件");
    assert_eq!(after.kind, ContentKind::Text, "中文应为文本");
    assert_eq!(
        after.bytes,
        "实际工作副本\n".as_bytes(),
        "正文须与实际文件一致"
    );
}
