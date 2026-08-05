use super::{validate_clone_destination, CloneDestinationError};

/// GC-D1：目标目录必须是现有父目录下尚不存在的直接子目录。
///
/// 数据构造（含关键数值的推导过程）：
///   临时父目录 = 1 个；候选项目目录 = parent/codex，初始不存在。
///   路径层级增量 = candidate.components - parent.components = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. canonicalize 现有父目录。
///   2. 校验 folder_name="codex" 是单个安全路径段。
///   3. 拼接但不创建目标目录。
///
/// 预期结果：
///   - 正断言：返回 canonical parent/codex。
///   - 负断言：校验过程不得提前创建目标目录。
#[test]
fn clone_destination_accepts_new_direct_child_without_creating_it() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let expected_parent = temp
        .path()
        .canonicalize()
        .expect("临时父目录应可 canonicalize");
    let expected_target = expected_parent.join("codex");

    let destination = validate_clone_destination(temp.path(), "codex")
        .expect("现有父目录下的新直接子目录应通过校验");

    assert_eq!(
        destination.parent(),
        expected_parent.as_path(),
        "destination parent 应是 canonical 临时父目录"
    );
    assert_eq!(
        destination.target(),
        expected_target.as_path(),
        "destination target 应精确为 parent/codex"
    );
    assert!(
        !destination.target().exists(),
        "目标校验不得提前创建 codex 目录，创建权应属于 Git clone 任务"
    );
    assert_ne!(
        destination.target(),
        destination.parent(),
        "目标必须是父目录的子项，不能覆盖父目录本身"
    );
}

/// GC-D2：已有目标和无效父目录应在启动 Git 前被分类拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   现有父目录 = 1；现有目标目录 = parent/existing；缺失父目录 = parent/missing。
///   Git 子进程启动数 = 0（本测试只验证前置校验）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 校验已存在的 parent/existing。
///   2. 校验尚不存在的 parent/missing 作为父目录。
///   3. 比较两条失败路径的错误分类。
///
/// 预期结果：
///   - 正断言：已有目标返回 TargetAlreadyExists。
///   - 正断言：缺失父目录返回 ParentDoesNotExist。
///   - 负断言：两类错误不得相互混淆。
#[test]
fn clone_destination_rejects_existing_target_and_missing_parent() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let existing = temp.path().join("existing");
    std::fs::create_dir(&existing).expect("测试应创建现有目标目录");

    let existing_error = validate_clone_destination(temp.path(), "existing")
        .expect_err("已有目标目录必须在运行 Git 前被拒绝");
    let missing_parent_error = validate_clone_destination(&temp.path().join("missing"), "repo")
        .expect_err("缺失父目录必须在运行 Git 前被拒绝");

    assert_eq!(
        existing_error,
        CloneDestinationError::TargetAlreadyExists,
        "已有目标应返回 TargetAlreadyExists"
    );
    assert_eq!(
        missing_parent_error,
        CloneDestinationError::ParentDoesNotExist,
        "缺失父目录应返回 ParentDoesNotExist"
    );
    assert_ne!(
        existing_error, missing_parent_error,
        "已有目标与缺失父目录是不同恢复动作，不得共用错误分类"
    );
}

/// GC-D3：目录名必须是安全的单一路径段，不能逃逸用户选择的父目录。
///
/// 数据构造（含关键数值的推导过程）：
///   父级逃逸(1) + 当前目录(1) + 绝对路径(1) + 含分隔符(1) + 空字符串(1)
///   = 5 个不安全目录名。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对五个候选名执行相同的路径段校验。
///   2. 在任何路径拼接或目录创建前拒绝不安全输入。
///   3. 检查统一的 InvalidFolderName 分类。
///
/// 预期结果：
///   - 正断言：父级逃逸与绝对路径均被拒绝。
///   - 负断言：任何输入都不得解析成父目录外的目标。
#[test]
fn clone_destination_rejects_path_traversal_and_non_segment_names() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");

    let parent_escape =
        validate_clone_destination(temp.path(), "../escape").expect_err("父级逃逸目录名必须被拒绝");
    let current_directory =
        validate_clone_destination(temp.path(), ".").expect_err("当前目录标记必须被拒绝");
    let absolute =
        validate_clone_destination(temp.path(), "/tmp/escape").expect_err("绝对目标路径必须被拒绝");
    let nested = validate_clone_destination(temp.path(), "owner/repo")
        .expect_err("含路径分隔符的嵌套目录名必须被拒绝");
    let empty = validate_clone_destination(temp.path(), "").expect_err("空目录名必须被拒绝");

    assert_eq!(
        parent_escape,
        CloneDestinationError::InvalidFolderName,
        "../escape 应返回 InvalidFolderName"
    );
    assert_eq!(
        current_directory,
        CloneDestinationError::InvalidFolderName,
        ". 应返回 InvalidFolderName"
    );
    assert_eq!(
        absolute,
        CloneDestinationError::InvalidFolderName,
        "绝对路径应返回 InvalidFolderName"
    );
    assert_eq!(
        nested,
        CloneDestinationError::InvalidFolderName,
        "嵌套路径应返回 InvalidFolderName"
    );
    assert_eq!(
        empty,
        CloneDestinationError::InvalidFolderName,
        "空目录名应返回 InvalidFolderName"
    );
    assert!(
        !temp.path().join("../escape").exists(),
        "无效输入不得创建父目录外的 escape 目标"
    );
}
