use super::endpoint::protocol_compatibility_error;
use super::route::{WorkspaceRouteError, WorkspaceRouteIndex};
use super::transport::NormaSocketListener;
use norma::protocol::{AgentErrorCode, CURRENT_PROTOCOL_VERSION};
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;

/// App 全局 Socket 所有权：第二个入口不得删除或替换正在工作的 Norma socket。
///
/// 数据构造（含关键数值的推导过程）：
///   socket paths      = 1 个共享路径
///   listener attempts = first(1) + second(1) = 2 次
///   expected owners   = 1 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一个 listener 获取 lease 并绑定共享路径。
///   2. 第二个 listener 尝试绑定同一路径。
///   3. 再次连接共享路径，验证第一位所有者仍可达。
///   4. 释放第一位所有者，验证它清理自己的 socket。
///
/// 预期结果：
///   - 正断言：第二次绑定明确返回 `AddrInUse`。
///   - 正断言：第二次绑定失败后，原 socket 仍可连接。
///   - 负断言：第一位所有者释放后不得遗留 socket 路径。
#[test]
fn second_listener_cannot_unlink_a_live_norma_socket() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let socket_path = temp.path().join("norma.sock");
    let first = NormaSocketListener::bind(socket_path.clone())
        .expect("第一个 App 全局入口应成功绑定空闲 socket");

    let second = NormaSocketListener::bind(socket_path.clone());
    let second_error = second
        .err()
        .expect("第二个入口绑定活跃 socket 必须失败，不能替换第一位所有者");

    assert_eq!(
        second_error.kind(),
        ErrorKind::AddrInUse,
        "活跃 Norma socket 必须以 AddrInUse 拒绝第二位所有者"
    );
    assert!(
        UnixStream::connect(&socket_path).is_ok(),
        "第二次绑定失败后，第一位所有者的 socket 必须仍可连接"
    );

    drop(first);

    assert!(
        !socket_path.exists(),
        "唯一所有者释放后必须删除自己拥有的 Norma socket"
    );
}

/// App 全局 Socket 路径安全：普通文件不能被当作 stale socket 删除。
///
/// 数据构造（含关键数值的推导过程）：
///   occupied paths = 1 个普通文件
///   file bytes     = len("user-data") = 9 bytes
///   bind attempts  = 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在目标 socket 路径写入 9 字节用户文件。
///   2. 尝试启动 Norma listener。
///   3. 重新读取目标路径内容。
///
/// 预期结果：
///   - 正断言：绑定返回 `AddrInUse`。
///   - 正断言：原文件内容完整保留。
///   - 负断言：路径不得被替换成 Unix socket。
#[test]
fn listener_refuses_to_replace_a_regular_file() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let socket_path = temp.path().join("norma.sock");
    std::fs::write(&socket_path, b"user-data").expect("测试应写入占位用户文件");

    let listener = NormaSocketListener::bind(socket_path.clone());
    let bind_error = listener.err().expect("普通文件占用目标路径时绑定必须失败");
    let preserved = std::fs::read(&socket_path).expect("绑定失败后用户文件必须仍可读取");

    assert_eq!(
        bind_error.kind(),
        ErrorKind::AddrInUse,
        "普通文件占用 socket 路径时必须返回 AddrInUse"
    );
    assert_eq!(
        preserved, b"user-data",
        "Norma listener 不得删除或改写用户文件"
    );
    assert!(
        UnixStream::connect(&socket_path).is_err(),
        "普通文件不得被静默替换成可连接的 Unix socket"
    );
}

/// Workspace 路由隔离：稳定 ID 精确选项目，旧 route 不能命中新窗口代次。
///
/// 数据构造（含关键数值的推导过程）：
///   workspaces = alpha(1) + beta(1) = 2 个
///   shared node IDs = node 7 × 2（刻意碰撞，但不参与路由）
///   alpha routes = old(1) + reopened(1) = 2 个代次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注册 alpha/route-a1 与 beta/route-b1，两个 payload 都含 node 7。
///   2. 按 beta 的稳定 ID 和 route 精确解析。
///   3. 注销 alpha/route-a1，再以 route-a2 重新注册 alpha。
///   4. 分别用旧 route-a1 和新 route-a2 解析 alpha。
///
/// 预期结果：
///   - 正断言：beta 解析只返回 beta payload。
///   - 正断言：重开后的 alpha 只接受 route-a2。
///   - 负断言：旧 route-a1 必须返回 RouteExpired，不能命中新窗口。
#[test]
fn workspace_routes_are_exact_and_reject_closed_generations() {
    let mut routes = WorkspaceRouteIndex::default();
    routes
        .register("workspace-alpha", "route-a1", ("alpha", 7_u64))
        .expect("alpha 首次 route 应注册成功");
    routes
        .register("workspace-beta", "route-b1", ("beta", 7_u64))
        .expect("beta route 应注册成功");

    let beta = routes
        .resolve("workspace-beta", "route-b1")
        .expect("beta 的稳定 ID 与 route 应精确命中");

    assert_eq!(
        beta.0, "beta",
        "node ID 碰撞时仍必须按 Workspace 身份返回 beta"
    );
    assert_ne!(
        beta.0, "alpha",
        "beta 请求不得因 node ID 相同而路由到 alpha"
    );

    assert!(
        routes.unregister("workspace-alpha", "route-a1"),
        "关闭 alpha 窗口必须注销旧 route"
    );
    routes
        .register("workspace-alpha", "route-a2", ("alpha-reopened", 7_u64))
        .expect("重开 alpha 应注册新 route 代次");

    assert_eq!(
        routes.resolve("workspace-alpha", "route-a1"),
        Err(WorkspaceRouteError::RouteExpired),
        "旧 route 不得命中重开的 Workspace 窗口"
    );
    assert_eq!(
        routes
            .resolve("workspace-alpha", "route-a2")
            .expect("新 route 应命中重开的 alpha")
            .0,
        "alpha-reopened",
        "重开的 Workspace 必须只由新 route 访问"
    );
}

/// 协议门禁：受管命令必须在副作用前拒绝旧版或未来版本客户端。
///
/// 数据构造（含关键数值的推导过程）：
///   current protocol = 3
///   tested versions  = old(1) + current(3) + future(4) = 3 个
///   command classes  = managed(true) + neutral(false) = 2 类
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对受管命令检查 v1、v3、v4。
///   2. 对非编排命令检查旧 v1 兼容路径。
///   3. 比较结构化错误码，不执行任何 Runtime 命令。
///
/// 预期结果：
///   - 正断言：v1 返回 client_upgrade_required，v4 返回 protocol_version_mismatch。
///   - 正断言：当前 v3 通过，非编排 v1 保持兼容。
///   - 负断言：不兼容受管版本不得返回 None 后继续产生副作用。
#[test]
fn managed_protocol_versions_fail_before_side_effects() {
    let old = protocol_compatibility_error(1, true);
    let current = protocol_compatibility_error(CURRENT_PROTOCOL_VERSION, true);
    // future = current(3) + 1 = 4
    let future = protocol_compatibility_error(CURRENT_PROTOCOL_VERSION + 1, true);
    let legacy_card = protocol_compatibility_error(1, false);

    assert_eq!(
        old,
        Some(AgentErrorCode::ClientUpgradeRequired),
        "旧编排 CLI 必须要求升级并重启 Agent Session"
    );
    assert_eq!(
        future,
        Some(AgentErrorCode::ProtocolVersionMismatch),
        "未来主协议不得被当前 App 猜测执行"
    );
    assert!(current.is_none(), "当前协议版本必须通过门禁");
    assert!(
        legacy_card.is_none(),
        "非编排 card 命令必须保留既有 v1 兼容策略"
    );
    assert!(
        old.is_some() && future.is_some(),
        "不兼容受管版本不得落入可执行路径"
    );
}
