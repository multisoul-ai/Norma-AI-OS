use super::*;
use crate::assets::{APP_ICON_ASSET, APP_LOGO_ASSET};

/// Settings 品牌脚注元数据：必须复用软件图标，并从当前 app crate 的编译版本生成版本号。
///
/// 数据构造（含关键数值的推导过程）：
///   product name = "Norma OS"（Packager.toml 的 product-name）
///   compiled version = env!("CARGO_PKG_VERSION")（当前 app crate 版本）
///   app icon = APP_ICON_ASSET = "icons/app-icon.png"
///   launchpad logo = APP_LOGO_ASSET = "icons/logo.png"（不同用途，不应混用）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 `settings_brand_metadata()` 读取设置侧栏的品牌展示数据。
///   2. 比较产品名与 Norma OS 的正式名称。
///   3. 比较版本与 Cargo 在编译期注入的当前 app 版本。
///   4. 比较图标路径与现有软件 icon，并排除 Launchpad 产品 logo。
///
/// 预期结果：
///   - 正断言：产品名、当前编译版本、软件 icon 均精确匹配单一事实来源。
///   - 负断言：版本不得回退为占位值，图标不得误用 Launchpad logo。
#[test]
fn test_settings_brand_metadata_uses_compiled_app_identity() {
    let brand = settings_brand_metadata();

    assert_eq!(
        brand.product_name, "Norma OS",
        "Settings 左下角应显示正式产品名 Norma OS"
    );
    assert_eq!(
        brand.version,
        env!("CARGO_PKG_VERSION"),
        "Settings 版本号必须等于当前 app crate 的编译版本，不能手工维护"
    );
    assert_eq!(
        brand.icon_asset, APP_ICON_ASSET,
        "Settings 品牌脚注必须复用当前软件 icon"
    );
    assert_ne!(
        brand.version, "0.0.0",
        "Settings 版本号不得显示占位值 0.0.0"
    );
    assert_ne!(
        brand.icon_asset, APP_LOGO_ASSET,
        "Settings 应显示软件 icon，不应误用 Launchpad 的独立产品 logo"
    );
}
