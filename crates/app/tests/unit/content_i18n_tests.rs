use super::{change_status, content_error};
use ui::i18n::Locale;
use workspace::content::{ChangeStatus, ContentError, Project};

/// 场景：底层文件错误展示为双语提示，缓存后切换语言仍更新。
/// 数据构造：真实隔离文件包含 NUL；恢复路径和 Git 诊断包含中文。
/// 执行过程：读取失败后只转换一次，再分别读取中英文；检查变更状态的展示。
/// 预期结果：产品文案切换，恢复路径/诊断不变；中文不会泄漏到英文状态标签。
#[test]
fn drawer_i18n_workbench_errors_preserve_raw_context_and_follow_locale() {
    let temp = tempfile::tempdir().expect("隔离项目");
    std::fs::write(temp.path().join("文件.bin"), [0, 1]).expect("二进制内容");
    let project = Project::open(temp.path()).expect("项目");
    let error = project
        .read_text(std::path::Path::new("文件.bin"))
        .expect_err("拒绝二进制编辑");
    let message = content_error(error);
    assert_eq!(
        message.resolve_for(Locale::En),
        "Binary files cannot be edited as text",
        "英文错误"
    );
    assert_eq!(
        message.resolve_for(Locale::ZhCn),
        "二进制文件不支持文本编辑",
        "已有错误切中文"
    );
    let restored = content_error(ContentError::SaveConflictRestored {
        recovery: "用户目录/草稿.txt".into(),
    });
    assert_eq!(restored.resolve_for(Locale::En), "The file changed while saving. The disk version was restored; recover your draft from 用户目录/草稿.txt", "恢复路径保持原文");
    let failed = content_error(ContentError::GitFailed("用户 hook: REJECTED".into()));
    assert_eq!(
        failed.resolve_for(Locale::En),
        "Git operation failed: 用户 hook: REJECTED",
        "诊断不能翻译"
    );
    assert_eq!(
        failed.resolve_for(Locale::ZhCn),
        "Git 操作失败：用户 hook: REJECTED",
        "产品前缀按语言变化"
    );
    let previous = Locale::current();
    Locale::En.apply();
    assert_eq!(
        change_status(ChangeStatus::Modified),
        "Modified",
        "状态在展示边界翻译"
    );
    assert_ne!(
        change_status(ChangeStatus::Added),
        "新增",
        "不能残留中文状态"
    );
    previous.apply();
}
