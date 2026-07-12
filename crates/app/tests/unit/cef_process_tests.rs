/// C-1: CEF feature 构建必须在库启动入口创建 UI 前接管 CEF 子进程。
///
/// 数据构造（含关键数值的推导过程）：
///   lib.rs         = 应用运行时入口，main.rs 只委托给 run_norma_os；
///   cef_process.rs = feature-gated CEF 子进程接管模块；
///   execute_process 返回值 >= 0 表示当前进程是 CEF 子进程，应立即退出。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 lib.rs，确认 run_norma_os 最早调用 exit_if_cef_subprocess()
///   2. 读取 cef_process.rs，确认普通主进程会先通过 is_cef_subprocess_arg() 过滤
///   3. 检查 feature 分支仅在 CEF 子进程参数存在时调用 cef::execute_process
///   4. 检查 execute_process 返回 >= 0 时是否 std::process::exit
///
/// 预期结果：
///   - 正断言：run_norma_os 在 tracing/Application 之前调用子进程接管入口
///   - 正断言：CEF feature 分支调用 cef::execute_process
///   - 负断言：子进程不应继续进入完整 Norma UI 启动路径
#[test]
fn test_cef_subprocess_handoff_runs_before_app_startup() {
    let app_entrypoint = include_str!("../../src/lib.rs");
    let source = include_str!("../../src/app/cef_process.rs");
    let implementation = source
        .split("#[cfg(test)]")
        .next()
        .expect("cef_process source should have an implementation section");

    let handoff = app_entrypoint
        .find("app::cef_process::exit_if_cef_subprocess();")
        .expect("run_norma_os should call CEF subprocess handoff before app startup");
    let tracing = app_entrypoint
        .find("tracing_subscriber::fmt()")
        .expect("main should initialize tracing after early process handoffs");

    assert!(
        handoff < tracing,
        "CEF subprocess handoff must run before tracing and GPUI startup"
    );
    assert!(
        implementation.contains("is_cef_subprocess_arg"),
        "CEF handoff should inspect process arguments before touching CEF APIs"
    );
    assert!(
        implementation.contains("if !std::env::args().any(is_cef_subprocess_arg)"),
        "main app startup must skip cef::execute_process when no CEF subprocess argument is present"
    );
    assert!(
        implementation.contains("cef::execute_process"),
        "CEF subprocess handoff should delegate actual subprocess execution to cef::execute_process"
    );
    assert!(
        implementation.contains("std::process::exit(exit_code)"),
        "CEF subprocesses must exit after execute_process handles them instead of launching Norma UI"
    );
    assert!(
        !implementation.contains("Application::new"),
        "CEF subprocess handoff module must not start the GPUI application"
    );
}

/// C-2: CEF 子进程判定只接受 Chromium 子进程参数，普通 Norma 启动不能触发。
///
/// 数据构造（含关键数值的推导过程）：
///   "norma-os"              = 普通主进程 argv[0]，不是 CEF 子进程；
///   "--type=renderer"       = CEF renderer 子进程；
///   "--type=gpu-process"    = CEF GPU 子进程；
///   "--user-data-dir=/tmp"  = 普通 Chromium 参数但不代表子进程。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 is_cef_subprocess_arg("norma-os")
///   2. 调用 is_cef_subprocess_arg("--type=renderer")
///   3. 调用 is_cef_subprocess_arg("--type=gpu-process")
///   4. 调用 is_cef_subprocess_arg("--user-data-dir=/tmp")
///
/// 预期结果：
///   - 正断言：renderer/gpu-process 会触发 CEF handoff
///   - 负断言：普通 argv 和无关 Chromium 参数不会触发 CEF handoff
#[test]
fn test_cef_subprocess_arg_detection_ignores_main_process_args() {
    assert!(
        !super::is_cef_subprocess_arg("norma-os"),
        "plain app argv[0] must not be treated as a CEF subprocess"
    );
    assert!(
        super::is_cef_subprocess_arg("--type=renderer"),
        "CEF renderer subprocess should be detected by its --type argument"
    );
    assert!(
        super::is_cef_subprocess_arg("--type=gpu-process"),
        "CEF GPU subprocess should be detected by its --type argument"
    );
    assert!(
        !super::is_cef_subprocess_arg("--user-data-dir=/tmp"),
        "unrelated Chromium flags must not trigger CEF subprocess handoff"
    );
}

/// C-3: CEF 动态库必须早于任何 CEF API 调用被加载。
///
/// 数据构造（含关键数值的推导过程）：
///   cef_process.rs = 子进程入口，首次危险 API 是 cef::execute_process；
///   cef_backend.rs = Browser 卡片入口，首次危险 API 是 CefString::from / cef::initialize；
///   PC=0 崩溃    = 未加载 framework 时调用 CEF 动态函数指针。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 cef_process.rs 的生产实现片段
///   2. 确认 load_cef_library() 出现在 cef::execute_process 之前
///   3. 读取 browser/cef_backend.rs 的生产实现片段
///   4. 确认 load_cef_library()? 出现在 Settings / CefString / initialize 之前
///
/// 预期结果：
///   - 正断言：子进程 handoff 先加载 CEF framework
///   - 正断言：Browser backend 初始化先加载 CEF framework
///   - 负断言：不允许在 load_cef_library()? 之前调用 cef::execute_process 或 initialize
#[test]
fn test_cef_framework_load_precedes_cef_api_calls() {
    let process_source = include_str!("../../src/app/cef_process.rs");
    let process_impl = process_source
        .split("#[cfg(test)]")
        .next()
        .expect("cef_process source should have an implementation section");
    let backend_source = include_str!("../../src/app/browser/cef_backend.rs");
    let backend_impl = backend_source
        .split("#[cfg(test)]")
        .next()
        .expect("cef_backend source should have an implementation section");
    let backend_initialize = backend_impl
        .split("fn initialize_cef")
        .nth(1)
        .expect("cef_backend should have an initialize_cef function");

    let process_load = process_impl
        .find("load_cef_library()")
        .expect("CEF subprocess handoff must load the framework before CEF APIs");
    let execute_process = process_impl
        .find("cef::execute_process")
        .expect("CEF subprocess handoff should call cef::execute_process");
    assert!(
        process_load < execute_process,
        "CEF subprocess handoff must load the framework before cef::execute_process"
    );

    let backend_load = backend_initialize
        .find("load_cef_library()?")
        .expect("CEF Browser backend must load the framework before initialization");
    let settings = backend_initialize
        .find("Settings {")
        .expect("CEF Browser backend should build Settings for cef::initialize");
    let initialize = backend_initialize
        .find("initialize(")
        .expect("CEF Browser backend should call cef::initialize");
    assert!(
        backend_load < settings,
        "CEF Browser backend must load the framework before constructing CefString-backed Settings"
    );
    assert!(
        backend_load < initialize,
        "CEF Browser backend must load the framework before cef::initialize"
    );
    assert!(
        !backend_initialize[..backend_load].contains("CefString::from"),
        "CEF Browser backend must not construct CefString before the framework is loaded"
    );
}

/// C-4: cargo run 裸二进制启动前必须准备 CEF GPU 运行时 dylib。
///
/// 数据构造（含关键数值的推导过程）：
///   target/debug/norma-os       = cargo run 使用的裸二进制，不在 .app bundle 内；
///   CEF Libraries/libEGL.dylib  = CEF GPU 进程依赖的 ANGLE EGL 动态库；
///   CEF Libraries/libGLESv2.dylib = 崩溃日志中缺失的 ANGLE GLES 动态库。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 cef_process.rs 的生产实现片段
///   2. 确认 load_cef_library 在 cef::load_library 前调用 prepare_cef_runtime_libraries
///   3. 确认准备列表包含 libEGL.dylib
///   4. 确认准备列表包含 libGLESv2.dylib
///
/// 预期结果：
///   - 正断言：GPU dylib 准备发生在 CEF framework 加载之前
///   - 正断言：libEGL.dylib 和 libGLESv2.dylib 都被纳入准备列表
///   - 负断言：不应只配置 resources_dir_path 而遗漏 GPU 子进程 dylib
#[test]
fn test_cef_load_prepares_gpu_runtime_libraries_for_cargo_run() {
    let process_source = include_str!("../../src/app/cef_process.rs");
    let process_impl = process_source
        .split("#[cfg(test)]")
        .next()
        .expect("cef_process source should have an implementation section");
    let load_impl = process_impl
        .split("pub(crate) fn load_cef_library")
        .nth(1)
        .expect("cef_process should have a load_cef_library function");

    let prepare = load_impl
        .find("prepare_cef_runtime_libraries(&paths)?")
        .expect("load_cef_library should stage CEF GPU runtime libraries before loading CEF");
    let load = load_impl
        .find("cef::load_library")
        .expect("load_cef_library should call cef::load_library");

    assert!(
        prepare < load,
        "CEF GPU runtime dylibs must be prepared before cef::load_library or subprocess startup can fail"
    );
    assert!(
        process_impl.contains("\"libEGL.dylib\""),
        "CEF runtime library list should include libEGL.dylib for ANGLE EGL"
    );
    assert!(
        process_impl.contains("\"libGLESv2.dylib\""),
        "CEF runtime library list should include libGLESv2.dylib because the GPU process loads it from the executable directory under cargo run"
    );
    assert!(
        process_impl.contains("prepare_cef_runtime_libraries"),
        "CEF process setup should have an explicit runtime dylib preparation boundary"
    );
}
