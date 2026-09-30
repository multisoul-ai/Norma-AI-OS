use super::*;

pub(super) fn run_native(
    cx: &mut gpui::VisualTestContext,
    server: &MobileServer,
    directory: &std::path::Path,
    workspace: &str,
) {
    use std::os::unix::fs::PermissionsExt;
    let qr = directory.join("offer.json");
    std::fs::write(
        &qr,
        server
            .begin_pairing("Plan Mac")
            .expect("配对")
            .to_json()
            .expect("JSON"),
    )
    .expect("临时QR");
    std::fs::set_permissions(&qr, std::fs::Permissions::from_mode(0o600)).expect("QR私有");
    let log = directory.join("native.log");
    let output = std::fs::File::create(&log).expect("日志");
    let mut child =
        std::process::Command::new(std::env::var("NORMA_SWIFT_PROBE").expect("原生probe"))
            .arg(&qr)
            .arg("plans")
            .stderr(output.try_clone().expect("错误日志"))
            .stdout(output)
            .spawn()
            .expect("启动probe");
    // 120s = 90s Xcode启动 + 30s规划命令预算；每10ms推进GPUI，不阻塞前台。
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut device = None;
    let mut upgraded = false;
    let status = loop {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        if let Some(status) = child.try_wait().expect("进程") {
            break status;
        }
        if let Some(pending) = server.pending().expect("申请") {
            assert!(device.is_none(), "只能配对一次");
            device = Some(
                server
                    .approve(
                        &pending.request_id,
                        &[MobileProjectAccess {
                            workspace_id: workspace.into(),
                            access: MobilePermission::Read,
                        }],
                    )
                    .expect("只读授权")
                    .device_id,
            );
        }
        if !upgraded && directory.join("enable-control").exists() {
            server
                .replace_permissions(
                    device.as_deref().expect("已配对"),
                    1,
                    &[MobileProjectAccess {
                        workspace_id: workspace.into(),
                        access: MobilePermission::Control,
                    }],
                )
                .expect("升权");
            upgraded = true;
        }
        if Instant::now() >= deadline {
            child.kill().expect("停止超时");
            child.wait().expect("回收");
            panic!(
                "原生规划超时：{}",
                std::fs::read_to_string(&log).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = std::fs::read_to_string(log).expect("原生日志");
    assert!(status.success(), "原生规划失败：{output}");
    assert!(
        output.contains("NATIVE_PLAN_PROPOSAL_OK"),
        "需要完整原生生成证据：{output}"
    );
    assert!(upgraded, "生成前必须核验只读拒绝和升权");
}
