use super::*;

/// B-8: Browser 引擎默认使用 Wry，CEF 只能显式启用。
///
/// 数据构造（含关键数值的推导过程）：
///   env(None)          = 未配置，必须走稳定的 Wry/WebKit；
///   env("cef")         = 用户显式请求 CEF，只有 feature 编译时才可用；
///   env("chromium")    = 用户用产品语义请求 Chromium，等价于 cef；
///   env("wry")         = 用户显式选择系统 WebKit；
///   env("unsupported") = 错误配置，回到稳定默认 Wry。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 BrowserEngineKind::from_env_value(None)
///   2. 调用 BrowserEngineKind::from_env_value(Some("cef"))
///   3. 调用 BrowserEngineKind::from_env_value(Some("chromium"))
///   4. 调用 BrowserEngineKind::from_env_value(Some("wry"))
///   5. 调用 BrowserEngineKind::from_env_value(Some("unsupported"))
///
/// 预期结果：
///   - 正断言：未配置时返回 Wry，避免项目里已有 Browser 节点自动创建 CEF
///   - 正断言：cef/chromium 在 CEF feature 构建中返回 Cef，否则安全回退 Wry
///   - 正断言：wry 显式返回 Wry
///   - 负断言：未知值不应把稳定默认值提升到 CEF
#[test]
fn test_browser_engine_kind_selects_wry_by_default_and_cef_when_requested() {
    #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
    let explicit_cef_engine = BrowserEngineKind::Cef;
    #[cfg(not(all(target_os = "macos", feature = "chromium-browser")))]
    let explicit_cef_engine = BrowserEngineKind::Wry;

    assert_eq!(
        BrowserEngineKind::from_env_value(None),
        BrowserEngineKind::Wry,
        "browser engine should default to Wry so restored Browser cards do not auto-enter CEF"
    );
    assert_eq!(
        BrowserEngineKind::from_env_value(Some("cef")),
        explicit_cef_engine,
        "browser engine should select CEF only when the CEF feature is compiled"
    );
    assert_eq!(
        BrowserEngineKind::from_env_value(Some("chromium")),
        explicit_cef_engine,
        "browser engine should treat chromium as explicit CEF only when the CEF feature is compiled"
    );
    assert_eq!(
        BrowserEngineKind::from_env_value(Some("wry")),
        BrowserEngineKind::Wry,
        "browser engine should allow an explicit Wry fallback for WebKit debugging"
    );
    assert_eq!(
        BrowserEngineKind::from_env_value(Some("unsupported")),
        BrowserEngineKind::Wry,
        "unknown browser engine values should fall back to Wry instead of entering CEF"
    );
}

/// B-9: CEF 代码必须 feature-gated，默认 Browser 路径不能启用 CEF。
///
/// 数据构造（含关键数值的推导过程）：
///   browser.rs       = BrowserStore 生命周期入口，默认构建必须可用；
///   cef_backend.rs   = 唯一允许出现 `cef::` 低层绑定的位置；
///   chromium-browser = Cargo feature，必须显式启用，默认运行使用 Wry；
///   default features = 可以包含 licenses 等无关能力，但不能包含 chromium-browser。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 crates/app/Cargo.toml
///   2. 从 `[features]` 中定位唯一的 default feature 声明
///   3. 读取 app/browser.rs
///   4. 读取 app/browser/cef_backend.rs
///
/// 预期结果：
///   - 正断言：Cargo.toml 定义 chromium-browser feature
///   - 正断言：Cargo.toml 存在 default feature 声明
///   - 正断言：cef_backend.rs 存在并包含 cef backend 类型
///   - 负断言：default feature 不应包含 chromium-browser
///   - 负断言：browser.rs 不应直接引用 `cef::`，避免默认构建被 CEF 污染
#[test]
fn test_browser_cef_backend_is_feature_gated_and_not_imported_by_default_path() {
    let cargo = include_str!("../../Cargo.toml");
    let browser = include_str!("../../src/app/browser.rs");
    let cef_backend = include_str!("../../src/app/browser/cef_backend.rs");
    let default_features = cargo
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("default = ["))
        .expect("app Cargo.toml should declare default features explicitly");

    assert!(
        cargo.contains("chromium-browser = [\"dep:cef\"]"),
        "app Cargo.toml should expose chromium-browser as an explicit CEF dependency feature"
    );
    assert!(
        default_features.starts_with("default = ["),
        "app Cargo.toml should keep an explicit default feature declaration"
    );
    assert!(
        cef_backend.contains("CefBrowserView"),
        "CEF backend should define a dedicated CefBrowserView type behind the backend boundary"
    );
    assert!(
        !default_features.contains("\"chromium-browser\""),
        "app Cargo.toml default features must not include chromium-browser because CEF creation can trap at startup"
    );
    assert!(
        !browser.contains("cef::"),
        "browser.rs must not import cef directly because default builds should not require CEF resources"
    );
}

/// B-10: BrowserView 应通过 NativeBrowserView 分发后端，而不是直接持有 wry::WebView。
///
/// 数据构造（含关键数值的推导过程）：
///   browser.rs       = BrowserStore 和 BrowserView 生命周期入口；
///   wry_backend.rs   = Wry/WebKit 后端实现文件；
///   NativeBrowserView = 后端枚举，后续承载 Wry 与 CEF 两种 native view。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 browser.rs
///   2. 读取 browser/wry_backend.rs
///   3. 检查 BrowserView 是否使用 NativeBrowserView 字段
///   4. 检查 wry::WebView 是否只出现在 Wry 后端文件中
///
/// 预期结果：
///   - 正断言：browser.rs 包含 NativeBrowserView 字段
///   - 正断言：wry_backend.rs 包含 WryBrowserView 类型
///   - 负断言：browser.rs 不应直接包含 `Option<wry::WebView>`
#[test]
fn test_browser_view_dispatches_through_native_backend_enum() {
    let browser = include_str!("../../src/app/browser.rs");
    let wry_backend = include_str!("../../src/app/browser/wry_backend.rs");

    assert!(
        browser.contains("NativeBrowserView"),
        "BrowserView should store NativeBrowserView so Wry and CEF can share lifecycle dispatch"
    );
    assert!(
        wry_backend.contains("WryBrowserView"),
        "Wry backend should define WryBrowserView as the existing WebKit implementation boundary"
    );
    assert!(
        !browser.contains("Option<wry::WebView>"),
        "BrowserView must not store wry::WebView directly once engine switching is introduced"
    );
}

/// B-11: CEF 后端必须创建原生 child browser，并实现基础浏览器生命周期命令。
///
/// 数据构造（含关键数值的推导过程）：
///   cef_backend.rs = CEF 后端唯一实现文件；
///   create         = Browser 卡片首次同步时调用；
///   lifecycle      = load_url / go_back / go_forward / reload / set_bounds / set_visible。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app/browser/cef_backend.rs
///   2. 检查 create 是否通过 WindowInfo::set_as_child 创建 AppKit child view
///   3. 检查创建是否调用 browser_host_create_browser_sync
///   4. 检查导航和 resize/visible 是否调用 CEF Browser/BrowserHost API
///
/// 预期结果：
///   - 正断言：CEF 创建路径使用 set_as_child 和 browser_host_create_browser_sync
///   - 正断言：load_url / go_back / go_forward / reload 都有实际调用
///   - 正断言：set_bounds 会设置 native NSView frame 并通知 CEF resize
///   - 负断言：CEF 后端不应保留 unavailable_reason 占位实现
#[test]
fn test_browser_cef_backend_creates_child_browser_and_dispatches_lifecycle() {
    let cef_backend = include_str!("../../src/app/browser/cef_backend.rs");

    assert!(
        cef_backend.contains("WindowInfo::default().set_as_child"),
        "CEF backend should create the browser as a child of the GPUI AppKit view"
    );
    assert!(
        cef_backend.contains("browser_host_create_browser_sync"),
        "CEF backend should create a real CEF browser host instead of keeping a placeholder"
    );
    assert!(
        cef_backend.contains("frame.load_url"),
        "CEF backend load_url should delegate to the CEF main frame"
    );
    assert!(
        cef_backend.contains("self.browser.go_back()"),
        "CEF backend go_back should delegate to the CEF browser"
    );
    assert!(
        cef_backend.contains("self.browser.go_forward()"),
        "CEF backend go_forward should delegate to the CEF browser"
    );
    assert!(
        cef_backend.contains("self.browser.reload()"),
        "CEF backend reload should delegate to the CEF browser"
    );
    assert!(
        cef_backend.contains("set_native_view_frame(native_view, bounds)"),
        "CEF backend set_bounds should update the native NSView frame"
    );
    assert!(
        cef_backend.contains("host.was_resized()"),
        "CEF backend set_bounds should notify CEF after the native view size changes"
    );
    assert!(
        !cef_backend.contains("unavailable_reason"),
        "CEF backend should not remain as an unavailable placeholder after engine switching is implemented"
    );
}

/// B-12: macOS CEF 必须接入 GPUI 前台消息泵，并暴露创建失败原因。
///
/// 数据构造（含关键数值的推导过程）：
///   cef_backend.rs = CEF 初始化配置与 do_message_loop_work 封装；
///   browser/store/view.rs = BrowserView::sync 创建 native view 的错误边界；
///   windowing.rs   = 每个 GPUI 窗口创建 MultisoulApp 后启动前台循环；
///   macOS CEF      = multi_threaded_message_loop 只支持 Windows/Linux，macOS 需 external pump。
///   root_cache_path = CEF 120+ 进程单例锁根目录，默认值会和其它 CEF app 共享。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 cef_backend.rs，确认 Settings 使用 external_message_pump
///   2. 读取 cef_backend.rs，确认初始化路径不再设置 multi_threaded_message_loop: 1
///   3. 读取 cef_backend.rs，确认 dev build 明确传入 CEF Resources 目录
///   4. 读取 cef_backend.rs，确认 root_cache_path 使用应用专属目录
///   5. 读取 cef_backend.rs，确认有 pump_cef_message_loop_if_initialized 包装 do_message_loop_work
///   6. 读取 windowing.rs，确认窗口创建时启动 CEF message pump loop
///   7. 读取 browser/store/view.rs，确认 NativeBrowserView::create 的 Err 会被记录而不是 `.ok()` 吞掉
///
/// 预期结果：
///   - 正断言：CEF settings 启用 external_message_pump
///   - 正断言：App 窗口启动 CEF message pump loop
///   - 正断言：Browser 创建失败会记录 tracing::warn
///   - 负断言：macOS CEF 初始化不应继续使用 unsupported multi_threaded_message_loop
#[test]
fn test_browser_cef_backend_uses_gpui_message_pump_and_logs_create_errors() {
    let cef_backend = include_str!("../../src/app/browser/cef_backend.rs");
    let browser = include_str!("../../src/app/browser/store/view.rs");
    let windowing = include_str!("../../src/workspace/windowing.rs");

    assert!(
        cef_backend.contains("external_message_pump: 1"),
        "CEF settings should enable external_message_pump so GPUI can drive Chromium work"
    );
    assert!(
        !cef_backend.contains("multi_threaded_message_loop: 1"),
        "macOS CEF must not use multi_threaded_message_loop because CEF only supports it on Windows/Linux"
    );
    assert!(
        cef_backend.contains("resources_dir_path: cef_resources_dir_cef_string()?"),
        "CEF settings should pass the framework Resources directory so dev builds can find CEF pak assets"
    );
    assert!(
        cef_backend.contains("root_cache_path: cef_root_cache_path_cef_string()?"),
        "CEF settings should customize root_cache_path so Norma OS does not share the default CEF process singleton"
    );
    assert!(
        cef_backend.contains("pump_cef_message_loop_if_initialized")
            && cef_backend.contains("cef::do_message_loop_work()"),
        "CEF backend should expose a guarded message pump wrapper around cef::do_message_loop_work"
    );
    assert!(
        windowing.contains("app.start_cef_message_pump_loop(window, cx)"),
        "each GPUI app window should start a foreground loop that pumps CEF work"
    );
    assert!(
        browser.contains("tracing::warn!")
            && browser.contains("failed to create native Browser view"),
        "BrowserView::sync should log native browser creation errors instead of silently dropping them"
    );
    assert!(
        !browser.contains("NativeBrowserView::create(engine, node_id, url, bounds, window, event_sender).ok()"),
        "BrowserView::sync must not convert create errors to None with .ok(), because that leaves only Loading browser visible"
    );
}

/// B-13: CEF 创建路径不得把当前 Rust Client wrapper 传入 browser host create。
///
/// 数据构造（含关键数值的推导过程）：
///   cef_backend.rs       = CEF Browser 卡片唯一 native 创建路径；
///   `Some(&mut client)`  = 触发崩溃报告中 CefClient_0_CToCpp invalid version -1 的实参；
///   `None` client        = 保留 CEF child browser 创建，但暂时不注册标题回调。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app/browser/cef_backend.rs
///   2. 确认 create 仍调用 browser_host_create_browser_sync 创建真实 CEF browser
///   3. 确认第二个实参传 None，不再把 Rust wrap_client 产物传给 CEF C++ wrapper
///   4. 确认后端不再持有 Client 字段，也不再声明 wrap_client! 宏块
///
/// 预期结果：
///   - 正断言：CEF 后端仍创建 native browser host
///   - 正断言：browser_host_create_browser_sync 的 client 参数为 None
///   - 负断言：创建路径不应出现 Some(&mut client)
///   - 负断言：后端不应保留 Rust Client wrapper，避免启动打开项目时再次崩溃
#[test]
fn test_browser_cef_backend_avoids_rust_client_wrapper_version_trap() {
    let cef_backend = include_str!("../../src/app/browser/cef_backend.rs");
    let compact_backend = cef_backend.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        cef_backend.contains("browser_host_create_browser_sync"),
        "CEF backend should still create a real browser host after removing the custom client"
    );
    assert!(
        compact_backend.contains(
            "browser_host_create_browser_sync( Some(&window_info), None, Some(&url), Some(&BrowserSettings::default()), None, None,"
        ),
        "CEF backend should pass None for the client parameter until a compatible Client shim is implemented"
    );
    assert!(
        !cef_backend.contains("Some(&mut client)"),
        "CEF create path must not pass the Rust Client wrapper that triggered CefClient_0_CToCpp invalid version -1"
    );
    assert!(
        !cef_backend.contains("wrap_client!"),
        "CEF backend should not keep the Rust Client wrapper in this crash-safe creation path"
    );
    assert!(
        !cef_backend.contains("_client: Client"),
        "CefBrowserView should not retain a Client field when no client is passed to CEF"
    );
}
