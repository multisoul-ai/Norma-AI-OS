use std::{path::Path, process::Command};
#[path = "support/doubao_voice.rs"]
mod doubao_voice;
#[path = "support/realtime_targets.rs"]
mod realtime_targets;

/// 豆包归一化网关与真实 iOS 音频引擎完成双向通话、插话、静音及立即重拨。
///
/// 数据构造：独立回环端口、临时票据，PCM16 输入 16k/输出 24k；无外部模型调用。
/// 执行过程：启动网关 → 启动模拟器 App 测试 → 对照服务端真实帧数和连接生命周期。
/// 预期结果：原生断言通过、有效 PCM 上行、两次接通均关闭，没有静默丢帧或重叠会话。
/// NORMA_DOUBAO_BACKGROUND_L3=1改为正式UI主动接通→系统Home后台5s→返回→挂断，
///   只建立1个会话并检查后台帧数增加，不能用setForeground调用替代系统生命周期。
/// NORMA_DOUBAO_RECOVERY_L3=1在首个20ms麦克风帧后反复断流：首次+3次重试=最多4个会话，
///   30s原生截止内必须停止恢复；稳定30s（2×15s就绪预算）之后的新断流才获得新预算。
#[test]
#[ignore = "requires explicit iOS simulator; runs real microphone and PCM playback against loopback"]
fn native_doubao_realtime_audio_and_reconnect() {
    let simulator = std::env::var("NORMA_IOS_SIMULATOR").expect("显式指定隔离模拟器");
    assert!(!simulator.is_empty(), "不能隐式选择用户设备");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace");
    let derived = std::env::var("NORMA_IOS_DERIVED_DATA").expect("显式指定独立构建目录");
    let fixture = doubao_voice::Fixture::new();
    let recovery = std::env::var("NORMA_DOUBAO_RECOVERY_L3").as_deref() == Ok("1");
    if recovery {
        fixture.stats.lock().expect("recovery fixture")["disconnect_on_audio"] = true.into();
    }
    let targets = realtime_targets::Targets::new();
    let directory = tempfile::tempdir().expect("voice fixture directory");
    let config = directory.path().join("voice-origin.txt");
    std::fs::write(
        &config,
        serde_json::json!({"origin":fixture.origin,"targets":targets.config}).to_string(),
    )
    .expect("voice config");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600))
        .expect("private fixture pairing offers");
    let background = std::env::var("NORMA_DOUBAO_BACKGROUND_L3").as_deref() == Ok("1");
    if background || std::env::var_os("NORMA_DOUBAO_CUA").is_some() {
        std::fs::write(
            directory.path().join(if background {
                "ui-native-background"
            } else {
                "ui-cua-requested"
            }),
            "",
        )
        .expect("explicit native acceptance");
        std::fs::write(directory.path().join("ui-cua-command"), "").expect("command center entry");
        eprintln!(
            "Native voice acceptance directory: {}",
            directory.path().display()
        );
        let run = |selector: &str| {
            Command::new("xcodebuild")
                .current_dir(root)
                .args([
                    "-project",
                    "ios/Norma.xcodeproj",
                    "-scheme",
                    "Norma",
                    "-destination",
                    &format!("platform=iOS Simulator,id={simulator}"),
                    "-derivedDataPath",
                    &derived,
                    &format!("NORMA_TEST_QR={}", config.display()),
                    "NORMA_TEST_MODE=doubao-loopback",
                    &format!("-only-testing:{selector}"),
                    "test",
                ])
                .output()
                .expect("Computer Use native bridge")
        };
        let prepared = run("NormaDeviceTests/DoubaoCallTests/testPrepareInteractiveVoiceTargets");
        assert!(
            prepared.status.success(),
            "准备隔离配对失败：{}",
            String::from_utf8_lossy(&prepared.stdout)
        );
        let observed = run(if background {
            "NormaUITests/RealtimeBackgroundFlowTests/testAudioContinuesAcrossSystemBackground"
        } else {
            "NormaUITests/ComputerUseAcceptanceTests/testInteractiveSession"
        });
        let cleaned = run("NormaDeviceTests/DoubaoCallTests/testCleanupInteractiveVoiceTargets");
        assert!(
            cleaned.status.success(),
            "清理本次临时配对失败：{}",
            String::from_utf8_lossy(&cleaned.stdout)
        );
        assert!(
            observed.status.success(),
            "原生语音验收未通过：{}",
            String::from_utf8_lossy(&observed.stdout)
        );
        if background {
            eprintln!(
                "System background evidence: {}",
                std::fs::read_to_string(directory.path().join("background-verified.json"))
                    .expect("native lifecycle evidence")
            );
        }
        return;
    }
    let selectors = if recovery {
        vec![
            "-only-testing:NormaDeviceTests/DoubaoCallTests/testRepeatedShortConnectionsStopRecovering",
            "-only-testing:NormaDeviceTests/DoubaoCallTests/testStableReadinessResetsRetryBudget",
        ]
    } else {
        vec![
            "-only-testing:NormaDeviceTests/DoubaoCallTests",
            "-only-testing:NormaDeviceTests/DoubaoAudioTests",
            "-only-testing:NormaDeviceTests/RealtimeDiscussionCallTests",
            "-only-testing:NormaDeviceTests/RealtimeReportCallTests",
            "-only-testing:NormaDeviceTests/RealtimeOperationTests",
            "-only-testing:NormaDeviceTests/RealtimeReportTests",
            "-only-testing:NormaDeviceTests/VoiceSessionTicketTests",
        ]
    };
    let output = Command::new("xcodebuild")
        .current_dir(root)
        .args([
            "-project",
            "ios/Norma.xcodeproj",
            "-scheme",
            "Norma",
            "-destination",
            &format!("platform=iOS Simulator,id={simulator}"),
            "-derivedDataPath",
            &derived,
            &format!("NORMA_TEST_QR={}", config.display()),
            if recovery {
                "NORMA_TEST_MODE=doubao-recovery"
            } else {
                "NORMA_TEST_MODE=doubao-loopback"
            },
        ])
        .args(selectors)
        .arg("test")
        .output()
        .expect("native doubao runner");
    let stats = fixture.stats.lock().expect("voice stats").clone();
    assert!(
        output.status.success(),
        "豆包原生验收失败，网关计数 {stats}：\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stats["frames"].as_u64().unwrap_or(0) > 0,
        "测试必须经过真实麦克风上行"
    );
    assert_eq!(stats["invalid_frames"], 0, "上行格式不能错位");
    if recovery {
        // Initial + three retries = at most four media sessions. A failed HTTP/WS handshake
        // also consumes a retry, so the native test checks exactly four attempts separately.
        let sessions = stats["sessions"].as_u64().expect("真实会话计数");
        assert!(sessions > 0, "必须真实接通过并由首个麦克风帧触发断流");
        assert!(sessions <= 4, "连续短暂断流最多四次连接，不能无限创建会话");
        assert_eq!(
            stats["closed"], stats["sessions"],
            "每个已建立的媒体会话都必须关闭"
        );
    } else {
        // Two explicit connects + one muted network recovery = three closed sessions.
        assert_eq!(
            stats["sessions"], 3,
            "两次手动接通与一次静音重连应建立三个会话"
        );
        assert_eq!(stats["closed"], 3, "三个媒体会话都必须关闭");
    }
    assert_eq!(stats["active"], 0, "验收结束不能留下活动媒体连接");
    assert_eq!(stats["overlap"], 0, "重拨不能与旧付费连接重叠");
}

/// 实时 Adapter 命令只能由 Mac 内部构造，手机不能绕过 sideband 原始输入校验。
///
/// 数据构造：合法 find_agent_tasks 内部命令、对应 JSON、既有 create_card JSON。
/// 执行过程：序列化内部命令供持久回执计算哈希，再尝试从不可信网络 JSON 反序列化。
/// 预期结果：内部命令可记录；网络不能构造内部执行入口；既有手机创建协议继续有效。
#[test]
fn realtime_adapter_command_cannot_be_injected_by_a_phone() {
    let internal = app::MobileCommand::RealtimeTool {
        command: agent::AdapterCommand::FindAgentTasks {
            task_id: None,
            agent: None,
            states: vec![],
        },
    };
    let wire = serde_json::to_value(&internal).expect("内部操作必须可计算持久请求哈希");
    assert_eq!(
        wire["type"], "realtime_tool",
        "内部命令应有独立类型，不能冒充手机命令"
    );
    assert!(
        serde_json::from_value::<app::MobileCommand>(wire).is_err(),
        "手机不能从网络构造只属于 Mac sideband 的 Adapter 命令"
    );
    assert!(
        serde_json::from_value::<app::MobileCommand>(serde_json::json!({
            "type":"create_card", "profile":"Codex", "title":"原手机协议"
        }))
        .is_ok(),
        "禁止内部命令不能破坏既有手机创建协议"
    );
}

/// 原生陪伴指挥台使用正式 bundle，入口与共享导航可用，未接通不能伪造媒体状态。
///
/// 数据构造：显式指定的 iOS 模拟器、真实角色/动作资源；不创建任务或连接付费模型。
/// 执行过程：xcodebuild 启动正式 App → 角色/凭据检查 → 标准/最大辅助字号布局 → 设置、审批记录与三个 Tab。
///   标准竖屏人物区域至少占屏宽95%、屏高44%（874pt屏约385pt）；44pt触控目标遵循iOS基线。
/// 预期结果：人物不被文字栏边距压缩，字幕/底栏不遮挡，辅助字号可滚动；缺少凭据不建立媒体连接。
#[test]
#[ignore = "requires explicit iOS simulator and Xcode; launches the native app"]
fn native_realtime_companion_entry_and_resources() {
    let simulator = std::env::var("NORMA_IOS_SIMULATOR").expect("显式指定隔离 iOS 模拟器");
    assert!(
        !simulator.is_empty(),
        "模拟器标识不能为空，不能隐式选择用户设备"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let derived = std::env::var("NORMA_IOS_DERIVED_DATA")
        .unwrap_or_else(|_| root.join("ios/DerivedData").display().to_string());
    let output = Command::new("xcodebuild")
        .current_dir(root)
        .args([
            "-project",
            "ios/Norma.xcodeproj",
            "-scheme",
            "Norma",
            "-destination",
            &format!("platform=iOS Simulator,id={simulator}"),
            "-derivedDataPath",
            &derived,
            "-only-testing:NormaUITests/RealtimeCommandFlowTests",
            "-only-testing:NormaDeviceTests/RealtimeCompanionTests",
            "-only-testing:NormaDeviceTests/RealtimeCallTests",
            "test",
        ])
        .output()
        .expect("启动原生 iOS 验收");
    assert!(
        output.status.success(),
        "原生指挥台验收失败：\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// 已配对原生手机自动领取Mac签名语音授权，无需Key且不启动公网媒体。
/// 数据构造：独立TLS目标与一次性配对QR；使用已有回环fixture自动批准控制项目。
/// 执行过程：启动真实模拟器App → 正式配对 → 空口令自动领取 → 缓存复用 → 清理配对。
/// 预期结果：XCTest通过且不伪造通话；本用例单独创建QR，不能重用已被音频测试消费的QR。
/// 显式 NORMA_VOICE_REGISTERED_DATABASE 进一步验证正式网关换票与 ASR 连接，不采集麦克风。
#[test]
#[ignore = "requires explicit iOS simulator; pinned TLS and native Keychain authorization"]
#[cfg(target_os = "macos")]
fn native_voice_authorization_without_keys() {
    let simulator = std::env::var("NORMA_IOS_SIMULATOR").expect("显式隔离模拟器");
    let derived = std::env::var("NORMA_IOS_DERIVED_DATA").expect("显式构建目录");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace");
    let registered = std::env::var("NORMA_VOICE_REGISTERED_DATABASE")
        .ok()
        .map(|database| {
            let expected = std::env::var("NORMA_VOICE_REGISTERED_FINGERPRINT")
                .expect("显式提供已登记公开证书指纹");
            let record = storage::SettingsStore::new(database.into())
                .get_string("mobile.server_identity")
                .expect("读取已登记安装身份")
                .expect("数据库必须包含安装身份");
            let identity = norma::mobile::ServerIdentity::from_storage_json(&record)
                .expect("安装身份记录必须有效");
            assert_eq!(
                identity.certificate_sha256(),
                expected,
                "只允许使用生产网关已登记的安装身份"
            );
            identity
        });
    let production = registered.is_some();
    let targets = realtime_targets::Targets::with_primary_identity(registered);
    let directory = tempfile::tempdir().expect("独立授权fixture");
    let config = directory.path().join("voice.json");
    std::fs::write(
        &config,
        serde_json::json!({"targets":targets.config,"production":production}).to_string(),
    )
    .expect("配对配置");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).expect("私有QR");
    let output = Command::new("xcodebuild").current_dir(root).args([
        "-project", "ios/Norma.xcodeproj", "-scheme", "Norma",
        "-destination", &format!("platform=iOS Simulator,id={simulator}"),
        "-derivedDataPath", &derived,
        &format!("NORMA_TEST_QR={}", config.display()), "NORMA_TEST_MODE=doubao-loopback",
        "-only-testing:NormaDeviceTests/RealtimeCompanionTests/testPairedMacAutomaticallyProvidesVoiceAuthorization", "test",
    ]).output().expect("原生自动授权");
    assert!(
        output.status.success(),
        "原生免Key授权失败：{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if production {
        let proof = "PASS: native automatic Mac signature -> production HTTP 200 -> Doubao WSS ready -> close ACK; no provider key or microphone";
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(proof),
            "生产授权必须真实完成，不能将跳过的原生用例当作成功"
        );
        eprintln!("{proof}");
    }
}
