use super::*;

/// 场景：安装完成后的主操作不能继续显示下载，也不能再次触发下载。
/// 构造：成功、已保存、失败、空安装和运行中的五种真实面板状态。
/// 执行：读取同一份驱动按钮文字及启用状态的安装操作决策。
/// 预期：成功明确为已安装且禁用；已保存只提供明确的重新安装，失败可重试。
#[test]
fn voice_a1_completed_installation_is_not_a_download_action() {
    assert_eq!(
        State::Installed.install_label_key(),
        "voice.a1_installed",
        "成功后的按钮必须明确已安装"
    );
    assert!(
        !State::Installed.can_install(),
        "已安装主操作不能再次触发下载"
    );
    assert_eq!(
        State::Saved.install_label_key(),
        "voice.a1_reinstall",
        "已有配置时必须明确是重新安装"
    );
    assert!(State::Saved.can_install(), "显式重装仍应可用以恢复损坏文件");
    assert_eq!(
        State::Failed(A1InstallFailure::Cancelled).install_label_key(),
        "voice.a1_install_retry",
        "取消后必须提供重试"
    );
    assert!(State::Idle.can_install(), "首次安装应可开始");
    assert!(
        !State::Running(A1InstallProgress::Verify).can_install(),
        "校验中不能重复开始安装"
    );
}

/// 场景：重开只发现配置时不虚构已校验，显式文件校验成功后应刷新已安装状态。
/// 构造：Saved与正在安装的两种面板状态；执行外部文件校验成功通知。
/// 预期：Saved只转为Installed一次；迟到通知不能覆盖当前下载进度。
#[test]
fn voice_a1_verified_saved_installation_updates_its_visible_state() {
    let mut saved = State::Saved;
    assert!(saved.files_verified(), "已保存配置在实际校验后应转为已安装");
    assert!(
        matches!(saved, State::Installed),
        "成功后的可见状态必须是已安装"
    );
    assert!(!saved.files_verified(), "重复通知不能持续触发重绘");
    let mut running = State::Running(A1InstallProgress::Runtime);
    assert!(!running.files_verified(), "旧通知不能覆盖新安装过程");
    assert!(
        matches!(running, State::Running(_)),
        "正在安装的状态必须保留"
    );
}

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
