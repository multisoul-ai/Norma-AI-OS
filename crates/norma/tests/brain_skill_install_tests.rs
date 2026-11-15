use norma::orchestration::{
    ensure_brain_skill, remove_brain_skill, AuthorizationRegistry, InstallPaths,
    OrchestrationInstaller, OrchestrationProvider,
};
use std::path::Path;

fn paths(root: &Path) -> InstallPaths {
    InstallPaths::for_home_and_bundle_bin(root, &root.join("bundle"))
}

fn skill_file(paths: &InstallPaths) -> std::path::PathBuf {
    paths
        .skill_path(OrchestrationProvider::Claude)
        .expect("claude skill path")
        .parent()
        .and_then(Path::parent)
        .expect("skills root")
        .join("norma-brain")
        .join("SKILL.md")
}

/// 场景：启动大脑前确保 norma-brain skill 已安装；重复调用不重写文件。
/// 数据构造：隔离的 home；Claude 作为当前后端；norma 路径为 /app/norma。
/// 预期结果：写出带托管标记和命令路径的 SKILL.md；第二次调用文件内容完全不变。
#[test]
fn ensure_installs_the_managed_skill_once() {
    let root = tempfile::tempdir().expect("root");
    let paths = paths(root.path());
    ensure_brain_skill(
        &paths,
        OrchestrationProvider::Claude,
        Path::new("/app/norma"),
    )
    .expect("install");
    let file = skill_file(&paths);
    let first = std::fs::read_to_string(&file).expect("skill file");
    assert!(
        first.contains("norma-brain-managed") && first.contains("name: norma-brain"),
        "check: first.contains('norma-brain-managed') && first.con"
    );
    assert!(first.contains("/app/norma"), "命令路径写进 skill");
    ensure_brain_skill(
        &paths,
        OrchestrationProvider::Claude,
        Path::new("/app/norma"),
    )
    .expect("again");
    assert_eq!(
        std::fs::read_to_string(&file).expect("skill file"),
        first,
        "check: std::fs::read_to_string(&file).expect('skill file'"
    );
}

/// 场景：App 移动或升级后 norma 路径变了，skill 要重装成新路径。
/// 数据构造：先以 /old/norma 安装，再以 /new/norma 调用。
/// 预期结果：文件含新路径，不含旧路径。
#[test]
fn ensure_reinstalls_when_the_command_path_changes() {
    let root = tempfile::tempdir().expect("root");
    let paths = paths(root.path());
    ensure_brain_skill(
        &paths,
        OrchestrationProvider::Claude,
        Path::new("/old/norma"),
    )
    .expect("old");
    ensure_brain_skill(
        &paths,
        OrchestrationProvider::Claude,
        Path::new("/new/norma"),
    )
    .expect("new");
    let text = std::fs::read_to_string(skill_file(&paths)).expect("skill file");
    assert!(
        text.contains("/new/norma") && !text.contains("/old/norma"),
        "check: text.contains('/new/norma') && !text.contains('/ol"
    );
}

/// 场景：用户在同名位置放了自己的文件，不能被覆盖；卸载也只删托管文件。
/// 数据构造：先写一个没有托管标记的 SKILL.md。
/// 预期结果：ensure 返回错误且文件原样；remove 不删它；对托管文件 remove 会删除。
#[test]
fn foreign_files_are_never_overwritten_or_removed() {
    let root = tempfile::tempdir().expect("root");
    let paths = paths(root.path());
    let file = skill_file(&paths);
    std::fs::create_dir_all(file.parent().expect("dir")).expect("mkdir");
    std::fs::write(&file, "my own skill").expect("write");
    assert!(
        ensure_brain_skill(
            &paths,
            OrchestrationProvider::Claude,
            Path::new("/app/norma")
        )
        .is_err(),
        "check: ensure_brain_skill( &paths, OrchestrationProvider:"
    );
    remove_brain_skill(&paths, OrchestrationProvider::Claude).expect("remove foreign is a no-op");
    assert_eq!(
        std::fs::read_to_string(&file).expect("kept"),
        "my own skill",
        "check: std::fs::read_to_string(&file).expect('kept')"
    );
    std::fs::remove_file(&file).expect("clear");
    ensure_brain_skill(
        &paths,
        OrchestrationProvider::Claude,
        Path::new("/app/norma"),
    )
    .expect("install");
    remove_brain_skill(&paths, OrchestrationProvider::Claude).expect("remove managed");
    assert!(!file.exists(), "check: !file.exists()");
}

/// 场景：用户自己的文件里只是提到了标记字样，不等于它是托管文件。
/// 数据构造：一个 SKILL.md 正文里写着 norma-brain-managed，但没有独占一行的标记。
/// 预期结果：ensure 拒绝覆盖，remove 不删除。
#[test]
fn a_file_that_merely_mentions_the_marker_is_not_managed() {
    let root = tempfile::tempdir().expect("root");
    let paths = paths(root.path());
    let file = skill_file(&paths);
    std::fs::create_dir_all(file.parent().expect("dir")).expect("mkdir");
    std::fs::write(&file, "notes about norma-brain-managed skills").expect("write");
    assert!(
        ensure_brain_skill(
            &paths,
            OrchestrationProvider::Claude,
            Path::new("/app/norma")
        )
        .is_err(),
        "must not overwrite"
    );
    remove_brain_skill(&paths, OrchestrationProvider::Claude).expect("no-op");
    assert!(file.exists(), "must not delete a user's file");
}

#[derive(Default)]
struct NoopRegistry;

impl AuthorizationRegistry for NoopRegistry {
    type Error = String;

    fn authorize(&mut self, _: OrchestrationProvider, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn revoke(&mut self, _: OrchestrationProvider) -> Result<(), String> {
        Ok(())
    }

    fn is_authorized(&self, _: OrchestrationProvider, _: &str) -> Result<bool, String> {
        Ok(false)
    }
}

/// 场景：用户卸载 Norma 的编排集成时，norma-brain skill 不能残留在 CLI 的 skills 目录里。
/// 数据构造：先给 Claude 装好 norma-brain skill，再走安装器的 uninstall。
/// 预期结果：uninstall 成功，skill 文件被删除。
#[test]
fn uninstalling_the_integration_removes_the_brain_skill() {
    let root = tempfile::tempdir().expect("root");
    let paths = paths(root.path());
    ensure_brain_skill(
        &paths,
        OrchestrationProvider::Claude,
        Path::new("/app/norma"),
    )
    .expect("install");
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    installer
        .uninstall(OrchestrationProvider::Claude, &mut NoopRegistry)
        .expect("uninstall");
    assert!(
        !skill_file(&paths).exists(),
        "brain skill must not linger after uninstall"
    );
}
