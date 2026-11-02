#![cfg(windows)]
use std::collections::HashSet;
use std::sync::Arc;
use voice_input::{AudioCapture, AudioQueueStats, VoiceInputError};

/// 场景：Windows默认产品必须调用原生设备枚举，不能返回固定“不支持”。
/// 构造：使用runner真实音频服务，不添加虚拟麦克风；无物理端点允许为空。
/// 执行：枚举端点并检查非空稳定ID、名称、唯一性及至多一个默认设备。
/// 预期：软件API成功；空列表单独记录设备ENV，不冒充录音验收。
#[test]
fn windows_microphone_enumeration_uses_native_endpoints() {
    let devices = voice_input::list_audio_input_devices()
        .expect("Windows必须完成原生设备枚举；缺物理麦克风应为空列表，不是固定unsupported");
    let mut ids = HashSet::new();
    for device in &devices {
        assert!(!device.id.is_empty(), "端点必须有可持久化的真实设备ID");
        assert!(!device.name.is_empty(), "端点必须有可展示的设备名称");
        assert!(
            ids.insert(&device.id),
            "不能用重复名称代替稳定端点ID：{}",
            device.id
        );
    }
    assert!(
        devices.iter().filter(|device| device.is_default).count() <= 1,
        "默认输入设备最多一个"
    );
    if devices.is_empty() {
        eprintln!("ENV_UNSUPPORTED: physical microphone capture; native enumeration succeeded with no endpoints");
    }
}

/// 场景：Windows桌面隐私权限必须有原生状态查询；构造真实runner用户和系统策略。
/// 执行：查询公开的权限状态，保留返回值；预期不是固定平台不支持错误。
/// 该检查不把权限状态等同于存在物理麦克风或成功录音。
#[test]
fn windows_microphone_permission_is_a_native_status() {
    let status = voice_input::microphone_authorization_status()
        .expect("Windows必须提供原生麦克风权限状态，而不是unsupported占位");
    eprintln!("Native Windows microphone permission: {status:?}");
}

/// 场景：已保存的输入端点已移除时，不得静默回退到另一支麦克风。
/// 构造：不可能存在的端点ID、16kHz目标PCM、真实公开capture入口；不需要任何物理设备。
/// 执行：启动指定端点；预期精确拒绝该选择，不打开系统默认输入或用unsupported掩盖选择错误。
#[test]
fn a_missing_windows_endpoint_cannot_fall_back_to_another_microphone() {
    let missing = "norma-test-missing-microphone-endpoint";
    let (sender, _receiver) = crossbeam_channel::bounded(voice_input::PCM_QUEUE_CHUNKS);
    let result = AudioCapture::start(
        sender,
        Arc::new(AudioQueueStats::default()),
        Some(missing),
        16_000,
    );
    let error = match result {
        Err(error) => error,
        Ok(capture) => {
            capture.stop();
            panic!("不存在的端点不能打开另一个麦克风");
        }
    };
    assert_eq!(
        error,
        VoiceInputError::Audio(format!("selected microphone is unavailable: {missing}")),
        "必须在打开设备前拒绝失效的显式输入选择"
    );
}
