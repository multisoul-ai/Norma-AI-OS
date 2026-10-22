/// DBI-0C：cargo-packager 必须显式为主 App 开启唯一的 Production 数据标记。
///
/// 数据构造（含关键数值的推导过程）：
///   Packager 配置文件数 = 1
///   主 App release build 命令数 = 1
///   必需 feature = production-data
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编译期读取仓库根 `Packager.toml`。
///   2. 归一化可选的 --no-default-features 参数，检查主 App 的显式 production-data feature。
///   3. 同时检查旧的无 feature 主 App 构建片段已经消失。
///
/// 预期结果：
///   - 正断言：打包命令包含 `--features production-data`。
///   - 负断言：不得保留无 Production 标记的主 App release 构建。
#[test]
fn test_packager_enables_the_production_data_feature_for_the_main_app() {
    // --no-default-features 只关闭默认 feature，不改变显式 production-data 的含义。
    let packager = include_str!("../../../../Packager.toml").replace("--no-default-features ", "");

    assert!(
        packager.contains("cargo build --release -p app --features production-data"),
        "Packager 主 App 构建必须显式开启 production-data"
    );
    assert!(
        !packager.contains("cargo build --release -p app &&"),
        "Packager 不得继续使用缺少 production-data 的主 App 构建命令"
    );
}

/// DBI-0D：ad-hoc 本地包同样必须以 Production 数据标记重新构建主 App。
///
/// 数据构造（含关键数值的推导过程）：
///   Packager 配置文件数 = 1（Packager.dev.toml）
///   必需 feature = production-data，且构建发生在打包前置命令内，避免复用旧的 Test 二进制
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编译期读取 `Packager.dev.toml`。
///   2. 归一化 --no-default-features 后检查 production-data 构建命令。
///
/// 预期结果：
///   - 正断言：before-packaging-command 含 `cargo build --release -p app --features production-data`。
#[test]
fn test_dev_packager_also_builds_the_main_app_with_production_data() {
    let packager =
        include_str!("../../../../Packager.dev.toml").replace("--no-default-features ", "");

    assert!(
        packager.contains("cargo build --release -p app --features production-data"),
        "ad-hoc 本地包必须在打包前以 production-data 重新构建主 App"
    );
}
