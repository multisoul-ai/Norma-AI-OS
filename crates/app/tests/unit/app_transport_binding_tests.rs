use super::{SocketBinding, SocketLease};
use norma::codex::UnixListener;

/// 场景：原socket被移走，原路径出现新文件时，旧binding关闭不得误删新正文。
/// 构造：真实AF_UNIX端点与租约，捕获OS文件身份后关闭listener，再rename并写入不同正文。
/// 执行：释放原binding，分别检查替代文件与被移动端点。
/// 预期：两条路径均保留，不把拥有旧文件身份当作仍拥有原路径。
#[test]
fn replaced_path_keeps_user_file_and_moved_socket() {
    let dir = tempfile::Builder::new()
        .prefix("nab-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("s.sock");
    let moved = dir.path().join("old.sock");
    let lease = SocketLease::acquire(&path).unwrap();
    let listener = UnixListener::bind(&path).unwrap();
    let binding = SocketBinding::capture(path.clone(), lease).unwrap();
    drop(listener);
    std::fs::rename(&path, &moved).expect("共享删除句柄必须允许端点重命名");
    std::fs::write(&path, b"replacement-user-data").unwrap();
    drop(binding);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"replacement-user-data",
        "旧binding不能删除替代正文"
    );
    assert!(
        moved.exists(),
        "旧binding失去路径归属后也不能擅自删除被移动文件"
    );
}

/// 场景：上次进程遗留的非活动socket允许在持有独占租约时重新绑定。
/// 构造：真实端点关闭后保留原socket文件；通过App入口重建listener。
/// 执行：重新绑定并关闭，检查残留清理。
/// 预期：正常接管陈旧端点，最终删除自己拥有的文件，而不是误判仍有活动宿主。
#[test]
fn inactive_socket_is_rebound_and_owned_cleanup_completes() {
    let dir = tempfile::Builder::new()
        .prefix("nas-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("s.sock");
    drop(UnixListener::bind(&path).unwrap());
    assert!(
        path.exists(),
        "必须实际留下陈旧端点，不能把不存在路径当作恢复测试"
    );
    let listener = super::super::NormaSocketListener::bind(path.clone()).unwrap();
    drop(listener);
    assert!(!path.exists(), "恢复后正常关闭必须完成归属文件清理");
}

/// 场景：检查陈旧socket之后、连接探测期间发生路径替换，不能删除新正文。
/// 构造：真实陈旧端点；探测回调精确在身份检查后rename旧文件并写入替代标记。
/// 执行：走实际prepare入口的连接探测边界，再读取替代文件。
/// 预期：替代正文和移动端点均保留，并返回AddrInUse，禁止先验证后裸路径删除。
#[test]
fn stale_probe_path_exchange_never_deletes_replacement() {
    let dir = tempfile::Builder::new()
        .prefix("nae-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("s.sock");
    let moved = dir.path().join("old.sock");
    drop(UnixListener::bind(&path).unwrap());
    let result = super::prepare_socket_path_with(&path, |path| {
        std::fs::rename(path, &moved).unwrap();
        std::fs::write(path, b"new-user-data").unwrap();
        Ok(false)
    });
    assert_eq!(
        std::fs::read(&path).expect("陈旧探测不能删掉替代正文"),
        b"new-user-data",
        "路径交换后必须保留用户正文"
    );
    assert!(moved.exists(), "不能顺带删除已移动的旧文件");
    assert_eq!(
        result.unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse,
        "身份变化必须明确拒绝绑定"
    );
}

/// 场景：连接探测超时只能表示未知，不能当作端点失效而删除文件。
/// 构造：真实socket文件和在网络边界注入的TimedOut；端点身份未发生变化。
/// 执行：走实际prepare路径并观察错误与原文件。
/// 预期：原样传播超时且保留socket，不能用is_ok把所有错误折成陈旧状态。
#[test]
fn uncertain_probe_error_preserves_endpoint() {
    let dir = tempfile::Builder::new()
        .prefix("nat-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("s.sock");
    drop(UnixListener::bind(&path).unwrap());
    let error =
        super::prepare_socket_path_with(&path, |_| Err(std::io::ErrorKind::TimedOut.into()))
            .unwrap_err();
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::TimedOut,
        "探测未知必须保留原错误"
    );
    assert!(path.exists(), "探测失败不能删除仍可能活动的端点");
}
