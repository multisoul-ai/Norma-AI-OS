use std::path::Path;
use workspace::content::Project;

/// 场景：默认Windows构建必须支持中文空格路径编辑，并保留外部并发保存。
/// 构造：独立项目内的中文文本，记录读出的真实版本，再模拟另一个编辑器修改。
/// 执行：先正常保存，再用旧版本保存一次；读取最终磁盘正文。
/// 预期：正常保存成功且版本变化；旧版本被拒绝，外部内容不丢失。
#[test]
fn text_save_preserves_concurrent_content_on_each_platform() {
    let temp = tempfile::tempdir().unwrap();
    let path = Path::new("中文 空格.txt");
    std::fs::write(temp.path().join(path), "before").unwrap();
    let project = Project::open(temp.path()).unwrap();
    let before = project.read_text(path).unwrap();
    let saved = project.save_text(path, &before.version, "draft").unwrap();
    assert_eq!(saved.text, "draft", "正常保存必须保留正文");
    assert_ne!(saved.version, before.version, "正文改变必须更新版本");
    std::fs::write(temp.path().join(path), "external").unwrap();
    assert!(
        project
            .save_text(path, &saved.version, "stale draft")
            .is_err(),
        "旧版本保存不能覆盖外部修改"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join(path)).unwrap(),
        "external",
        "冲突后磁盘必须保留外部版本"
    );
}
