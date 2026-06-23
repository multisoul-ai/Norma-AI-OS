use std::path::Path;

use super::is_inside_app_bundle_path;

/// P-1: 只有 .app/Contents/MacOS 内的二进制允许主动请求系统隐私权限。
///
/// 数据构造（含关键数值的推导过程）：
///   packaged = /Applications/Multisoul.app/Contents/MacOS/multisoul
///   cargo    = target/debug/multisoul
///   helper   = /tmp/Multisoul.app-not/Contents/MacOS/multisoul
///
/// 执行过程（逐步说明系统如何处理）：
///   1. packaged 路径包含连续三段 .app、Contents、MacOS
///   2. cargo 路径不包含 .app bundle 结构
///   3. helper 虽包含 ".app" 字符串但不是以 .app 结尾的 bundle 段
///
/// 预期结果：
///   - 正断言：packaged 应允许请求 Speech/Microphone 权限
///   - 负断言：cargo/helper 不应请求权限，避免 macOS framework 裸 abort
#[test]
fn test_permission_prompt_requires_packaged_app_bundle_path() {
    let packaged = Path::new("/Applications/Multisoul.app/Contents/MacOS/multisoul");
    let cargo = Path::new("/Users/alan/project/target/debug/multisoul");
    let helper = Path::new("/tmp/Multisoul.app-not/Contents/MacOS/multisoul");

    assert!(
        is_inside_app_bundle_path(packaged),
        "打包 .app 内的二进制应允许主动请求系统隐私权限"
    );
    assert!(
        !is_inside_app_bundle_path(cargo),
        "cargo run 的裸二进制不应调用 requestAuthorization，否则会 abort"
    );
    assert!(
        !is_inside_app_bundle_path(helper),
        "路径段只包含 .app 字符串但不是 .app bundle 时不应误判为可请求权限"
    );
}
