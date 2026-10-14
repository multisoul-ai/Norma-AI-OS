use norma::connector::funnel_command;

/// 场景：设置修改本机端口后，复制的Funnel命令必须指向当前端口。
/// 构造：默认47615、改为48000及合法上界65535；逐次调用纯格式函数。
/// 预期：命令遵循官方tailscale funnel [flags] <target>语法，不保留旧端口。
#[test]
fn funnel_command_uses_the_current_local_port() {
    assert_eq!(
        funnel_command(47615),
        "tailscale funnel --bg 47615",
        "默认端口是本机target"
    );
    assert_eq!(
        funnel_command(48000),
        "tailscale funnel --bg 48000",
        "使用修改后的端口"
    );
    assert_ne!(
        funnel_command(48000),
        funnel_command(47615),
        "改端口必须更新命令"
    );
    assert_eq!(
        funnel_command(65535),
        "tailscale funnel --bg 65535",
        "合法端口上界"
    );
}
