use super::*;

fn device(id: &str, name: &str, is_default: bool) -> AudioInputDevice {
    AudioInputDevice {
        id: id.to_string(),
        name: name.to_string(),
        is_default,
    }
}

/// D-1: 未指定输入源时应选择系统默认麦克风。
///
/// 数据构造（含关键数值的推导过程）：
///   devices.len() = 2
///   builtin.is_default = true
///   headset.is_default = false
///   selected_id = None，表示 Settings 中选择 System Default
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造包含 builtin 与 headset 的设备列表
///   2. 调用 resolve_audio_input_device_selection(None, devices)
///   3. resolver 查找 is_default=true 的设备并返回该设备
///
/// 预期结果：
///   - 正断言：返回 Some(builtin)
///   - 负断言：不应返回 headset，避免 System Default 被误解析成非默认设备
#[test]
fn test_resolve_audio_input_device_none_selects_default_device() {
    let devices = vec![
        device("builtin", "MacBook Pro Microphone", true),
        device("headset", "AirPods Pro", false),
    ];

    let selected = resolve_audio_input_device_selection(None, &devices)
        .expect("System Default selection should resolve without error");

    assert_eq!(
        selected.as_ref().map(|device| device.id.as_str()),
        Some("builtin"),
        "System Default should resolve to the device marked is_default=true"
    );
    assert_ne!(
        selected.as_ref().map(|device| device.id.as_str()),
        Some("headset"),
        "System Default must not resolve to a non-default headset"
    );
}

/// D-2: 已保存的输入源 ID 存在时应选择该具体麦克风。
///
/// 数据构造（含关键数值的推导过程）：
///   devices.len() = 2
///   builtin.id = "builtin"，is_default = true
///   headset.id = "headset"，is_default = false
///   selected_id = Some("headset")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认内置麦克风与耳机麦克风
///   2. 调用 resolve_audio_input_device_selection(Some("headset"), devices)
///   3. resolver 优先按 selected_id 精确匹配，而不是使用默认设备
///
/// 预期结果：
///   - 正断言：返回 headset
///   - 负断言：不应返回 builtin，避免保存设置被系统默认覆盖
#[test]
fn test_resolve_audio_input_device_saved_id_selects_matching_device() {
    let devices = vec![
        device("builtin", "MacBook Pro Microphone", true),
        device("headset", "AirPods Pro", false),
    ];

    let selected = resolve_audio_input_device_selection(Some("headset"), &devices)
        .expect("saved headset id should resolve to the matching device");

    assert_eq!(
        selected.as_ref().map(|device| device.id.as_str()),
        Some("headset"),
        "saved input device id should select the matching headset"
    );
    assert_ne!(
        selected.as_ref().map(|device| device.id.as_str()),
        Some("builtin"),
        "saved input device id must not be ignored in favor of the default microphone"
    );
}

/// D-3: 已保存的输入源 ID 不存在时应返回明确错误，不能静默降级。
///
/// 数据构造（含关键数值的推导过程）：
///   devices.len() = 1
///   builtin.id = "builtin"，is_default = true
///   selected_id = Some("missing-headset")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造只有内置麦克风的设备列表，模拟耳机已断开
///   2. 调用 resolve_audio_input_device_selection(Some("missing-headset"), devices)
///   3. resolver 找不到 selected_id，应返回 VoiceInputError::Audio
///
/// 预期结果：
///   - 正断言：错误消息包含 missing-headset，便于用户排障
///   - 负断言：不应返回 Ok，避免用户误以为仍在使用耳机麦克风
#[test]
fn test_resolve_audio_input_device_missing_saved_id_returns_error() {
    let devices = vec![device("builtin", "MacBook Pro Microphone", true)];

    let error = resolve_audio_input_device_selection(Some("missing-headset"), &devices)
        .expect_err("missing saved input device id should return an explicit error");

    assert!(
        error.to_string().contains("missing-headset"),
        "missing-device error should include the selected device id for troubleshooting"
    );
    assert_ne!(
        error,
        VoiceInputError::Audio("".to_string()),
        "missing-device error should not be an empty generic audio error"
    );
}

/// D-4: 麦克风枚举必须使用 macOS 12+ 可用的 AVFoundation 符号。
///
/// 数据构造（含关键数值的推导过程）：
///   Packager minimum-system-version = 12.0
///   AVCaptureDeviceTypeMicrophone 仅在 macOS 14+ 的 AVFoundation 中导出
///   BuiltInMicrophone 字符串值在 Ventura 及更早版本可用
///   两个 AVFoundation 静态符号引用数 = 0，避免新符号缺失与旧符号弃用
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 devices.rs 源码
///   2. 确认 discovery session 通过 NSString 运行时构造 BuiltInMicrophone 类型
///   3. 确认未静态引用旧 BuiltInMicrophone 或 macOS 14+ Microphone 符号
///   4. macOS 上实际调用设备枚举，确保生产函数进入并执行于测试产物
///   5. 用 nm 扫描当前测试二进制的未解析符号，验证链接产物没有这两个静态符号
///
/// 预期结果：
///   - 正断言：源码通过 NSString::from_str 构造 BuiltInMicrophone 类型
///   - 正断言：AVFoundation discovery 在当前受支持 macOS 上成功返回（允许设备列表为空）
///   - 正断言：nm 成功读取实际测试产物的未解析符号表
///   - 负断言：源码和链接产物均不引用旧弃用符号或 macOS 14+ discovery 符号
#[test]
fn test_list_audio_input_devices_uses_macos_12_compatible_capture_device_type() {
    let source = include_str!("../../src/devices.rs");

    assert!(
        source.contains("NSString::from_str(\"AVCaptureDeviceTypeBuiltInMicrophone\")"),
        "microphone discovery should construct the macOS 12-compatible device type at runtime"
    );
    assert!(
        !source.contains("AVCaptureDeviceTypeBuiltInMicrophone, AVMediaTypeAudio")
            && !source.contains("unsafe { AVCaptureDeviceTypeBuiltInMicrophone }"),
        "the deprecated BuiltInMicrophone static must not be linked or Clippy will fail"
    );
    assert!(
        !source.contains("AVCaptureDeviceTypeMicrophone,")
            && !source.contains("AVCaptureDeviceTypeMicrophone }"),
        "macOS 14+ microphone discovery symbol breaks startup on Ventura"
    );

    #[cfg(target_os = "macos")]
    {
        let devices = list_audio_input_devices();
        assert!(
            devices.is_ok(),
            "受支持 macOS 上的 AVFoundation microphone discovery 应成功返回，允许列表为空；实际错误={devices:?}"
        );
        let output = std::process::Command::new("nm")
            .arg("-u")
            .arg(std::env::current_exe().expect("测试应能定位当前 voice_sherpa 测试二进制"))
            .output()
            .expect("macOS 测试环境应提供 nm 以扫描最终链接产物");
        assert!(
            output.status.success(),
            "nm 必须成功读取 voice_sherpa 测试二进制，stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let undefined_symbols = String::from_utf8_lossy(&output.stdout);
        assert!(
            !undefined_symbols
                .lines()
                .any(|line| line.split_whitespace().last()
                    == Some("_AVCaptureDeviceTypeMicrophone")),
            "链接产物不得依赖 macOS 14+ _AVCaptureDeviceTypeMicrophone，否则 Ventura 会在启动时 dyld 失败"
        );
        assert!(
            !undefined_symbols
                .lines()
                .any(|line| line.split_whitespace().last()
                    == Some("_AVCaptureDeviceTypeBuiltInMicrophone")),
            "链接产物不得依赖已弃用的 _AVCaptureDeviceTypeBuiltInMicrophone 静态符号"
        );
    }
}

/// D-5: 空白输入源 ID 应等价于 System Default。
///
/// 数据构造（含关键数值的推导过程）：
///   devices.len() = 2
///   builtin.is_default = true
///   headset.is_default = false
///   selected_id = Some("   ")，保存值仅包含空格
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认内置麦克风与耳机麦克风
///   2. 调用 resolve_audio_input_device_selection(Some("   "), devices)
///   3. resolver trim 后发现 ID 为空，按 System Default 处理
///
/// 预期结果：
///   - 正断言：返回 builtin
///   - 负断言：不应返回 headset，避免空白配置误选非默认设备
#[test]
fn test_resolve_audio_input_device_blank_id_uses_default_device() {
    let devices = vec![
        device("builtin", "MacBook Pro Microphone", true),
        device("headset", "AirPods Pro", false),
    ];

    let selected = resolve_audio_input_device_selection(Some("   "), &devices)
        .expect("blank saved input device id should resolve as System Default");

    assert_eq!(
        selected.as_ref().map(|device| device.id.as_str()),
        Some("builtin"),
        "blank saved input device id should resolve to the default microphone"
    );
    assert_ne!(
        selected.as_ref().map(|device| device.id.as_str()),
        Some("headset"),
        "blank saved input device id must not select a non-default microphone"
    );
}
