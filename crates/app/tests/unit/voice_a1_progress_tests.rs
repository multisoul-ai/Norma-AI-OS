use super::*;

/// 场景：后台已完成后才排到前台的旧进度不能把安装终态覆盖回运行态。
/// 构造：分别创建已成功、已失败及仍在运行的安装状态，使用真实Verify/Download事件。
/// 执行：给三个状态投递一次迟到进度。
/// 预期：成功/失败终态保持，当前运行态仍正常更新；不能出现按钮可重试但状态永久运行。
#[test]
fn voice_a1_late_progress_preserves_installation_terminal_state() {
    let mut installed = State::Installed;
    installed.progress(A1InstallProgress::Verify);
    assert!(
        matches!(installed, State::Installed),
        "迟到进度不能撤销安装成功"
    );
    let mut failed = State::Failed(A1InstallFailure::Python);
    failed.progress(A1InstallProgress::Verify);
    assert!(
        matches!(failed, State::Failed(A1InstallFailure::Python)),
        "迟到进度不能遮住明确失败原因"
    );
    let mut running = State::Running(A1InstallProgress::Runtime);
    running.progress(A1InstallProgress::Download {
        file: "weights".into(),
        completed: 3,
        total: 3,
    });
    assert!(
        matches!(
            running,
            State::Running(A1InstallProgress::Download {
                completed: 3,
                total: 3,
                ..
            })
        ),
        "仍在运行的同一安装必须继续更新实际进度"
    );
}
