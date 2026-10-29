use std::path::Path;
use workspace::content::{ChangeStatus, Comparison, ContentError, Project};

/// 场景：右侧工作台不能收到底层提前写死的中文产品提示。
/// 数据构造：隔离项目包含带 NUL 的二进制文件，并初始化尚无提交的 Git 仓库。
/// 执行过程：读取文本、获取本地 Git 快照。
/// 预期结果：错误和变更状态保留语义类型；无提交分支使用 None，不返回中文占位文案。
#[test]
fn content_results_preserve_semantics_for_localized_presentation() {
    let temp = tempfile::tempdir().expect("隔离项目");
    std::fs::write(temp.path().join("binary.bin"), [0, 1, 2]).expect("二进制文件");
    git2::Repository::init(temp.path()).expect("初始化 Git");
    let project = Project::open(temp.path()).expect("打开项目");
    let error = project
        .read_text(Path::new("binary.bin"))
        .expect_err("二进制不能按文本读取");
    assert_eq!(
        error,
        ContentError::BinaryFile,
        "领域错误应是可翻译的语义值"
    );
    let snapshot = project.git_snapshot(&Comparison::Local).expect("读取快照");
    assert_eq!(snapshot.branch, None, "没有提交时不写入中文分支名");
    let change = snapshot.changes.first().expect("二进制文件是未跟踪的变更");
    assert_eq!(change.status, ChangeStatus::Added, "新增状态应是语义值");
    assert!(!snapshot.changes.is_empty(), "状态不能因为未提交而丢失");
}
