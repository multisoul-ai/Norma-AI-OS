use super::resources_for_executable;
use std::path::Path;

/// 场景：发布包与源码checkout位于不同目录；按平台构造带中文空格的绝对exe路径。
/// 执行：只用运行中exe推导Resources；预期精确指向交付目录，不能回到构建机源码。
#[test]
fn resources_follow_the_native_package_layout() {
    let (executable, expected) = if cfg!(windows) {
        (
            r"C:\验收 包\Norma OS\norma-os.exe",
            r"C:\验收 包\Norma OS\Resources",
        )
    } else {
        (
            "/Applications/验收 包.app/Contents/MacOS/norma-os",
            "/Applications/验收 包.app/Contents/Resources",
        )
    };
    assert_eq!(
        resources_for_executable(Path::new(executable)).as_deref(),
        Some(Path::new(expected)),
        "资源目录必须服从当前平台的真实发布布局"
    );
    assert_ne!(expected, executable, "Resources不能误指向主程序本身");
}

/// 场景：调用者传入相对路径；构造裸exe及相对包路径并执行解析。
/// 预期：拒绝依赖当前工作目录的推导，不能把源码目录当成已打包资源。
#[test]
fn relative_executables_cannot_claim_packaged_resources() {
    for executable in ["norma-os", "Norma.app/Contents/MacOS/norma-os"] {
        assert!(
            resources_for_executable(Path::new(executable)).is_none(),
            "相对路径不能冒充当前exe的绝对路径：{executable}"
        );
    }
}
