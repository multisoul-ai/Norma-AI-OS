use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};
use storage::{
    AppStore, MobilePermission, MobileProjectAccess, MobileTaskBaselineStore,
    MobileTaskDeliveryStore, TaskBaseline, TaskDeliveryContent,
};

#[path = "support/doubao_cloud.rs"]
mod cloud;

struct Gateway(Child);
impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// 完整Node网关、真实Mac App和原生iOS之间的任务/成果闭环。
///
/// 数据构造：显式隔离App根目录及模拟器，一个空Git项目；仅ASR/Chat/TTS和CLI为替身。
/// 执行过程：读取真实App公证书 → 启生产网关 → 原生测试生成设备身份 → 仅授权此项目 →
///   iOS经网关派工A，排队B/C并取消C，挂断重拨后完成A/B，正式feed和PCM确认后返回。
/// 预期结果：A只有首行，B基线含A且只追加第二行；C无文件/成果，挂断不删原卡。
/// 本用例不自行注入成果或工具回执；真实UI由Computer Use打开项目/手机服务，结束后退出。
/// NORMA_DOUBAO_CLOUD_L3=1时真实调用云端ASR/Chat/TTS；原生发送预生成PCM且不上传现场音频，
///   单任务产生云端验证完成文件，原回执/成果/真实播放均需通过。
/// 再设置NORMA_DOUBAO_REAL_CLI=1时使用预先经正常信任审查的真实CLI；
///   不释放替身完成门，必须保留真实session/turn与Stop证据，原生暂停/恢复实际汇报。
#[test]
#[ignore = "explicit isolated real macOS app and iOS simulator with local production Node gateway"]
fn actual_gateway_desktop_and_native_report() {
    let cloud = std::env::var("NORMA_DOUBAO_CLOUD_L3").as_deref() == Ok("1");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace");
    let app_root =
        PathBuf::from(std::env::var("NORMA_DOUBAO_DESKTOP_ROOT").expect("explicit real app root"));
    assert!(
        app_root.parent() == Some(Path::new("/tmp"))
            && app_root
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("norma-mobile-desktop-l3.")),
        "只能访问隔离L3 App目录"
    );
    let simulator = std::env::var("NORMA_IOS_SIMULATOR").expect("explicit isolated simulator");
    let derived = std::env::var("NORMA_IOS_DERIVED_DATA").expect("isolated derived data");
    let project =
        PathBuf::from(std::env::var("NORMA_NATIVE_FIXTURE_ROOT").expect("isolated provider"))
            .join("Realtime Delivery")
            .canonicalize()
            .expect("test project");
    let address: std::net::SocketAddr = std::fs::read_to_string(app_root.join("mobile-address"))
        .expect("Computer Use observed mobile address")
        .trim()
        .parse()
        .expect("address");
    assert!(address.ip().is_loopback(), "不能使用真实外部服务");
    let store = AppStore::new(app_root.join("data/app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .list()
        .expect("projects")
        .into_iter()
        .find(|p| p.canonical_root == project)
        .expect("CUA opened exact project")
        .workspace_id;
    let public: Value = serde_json::from_slice(
        &std::fs::read(app_root.join("data/realtime-public-identity.json"))
            .expect("real App public identity"),
    )
    .expect("public identity");
    let machine = public["machine_id"].as_str().expect("machine");
    let directory = tempfile::tempdir().expect("private L3 config");
    let config = directory.path().join("gateway.json");
    let mut gateway_command = Command::new("node");
    if cloud {
        gateway_command.arg(format!(
            "--env-file={}",
            std::env::var("NORMA_DOUBAO_ENV_FILE").expect("explicit local Plan credential file")
        ));
    }
    let script = if cloud {
        "services/norma-cloud/check-cloud-native.mjs"
    } else {
        "services/norma-cloud/check-native.mjs"
    };
    let mut gateway = Gateway(
        gateway_command
            .arg(root.join(script))
            .arg(&config)
            .spawn()
            .expect("production Node gateway"),
    );
    wait(Duration::from_secs(5), || {
        config.is_file() || gateway.0.try_wait().expect("gateway process").is_some()
    });
    let mut value: Value =
        serde_json::from_slice(&std::fs::read(&config).expect("gateway config")).expect("JSON");
    value["machine"] = json!({"id":machine,"name":"Realtime L3 Mac","certificateSHA256":machine.strip_prefix("mac-").expect("certificate digest"),"endpoints":[format!("wss://{address}/mobile")]});
    value["workspace"] = json!(workspace);
    std::fs::write(&config, serde_json::to_vec(&value).expect("config JSON"))
        .expect("private config");
    let config_path = config.clone();
    let test = std::thread::spawn(move || {
        Command::new("xcodebuild").current_dir(root).args([
        "-project","ios/Norma.xcodeproj","-scheme","Norma","-destination",&format!("platform=iOS Simulator,id={simulator}"),
        "-derivedDataPath",&derived,&format!("NORMA_TEST_QR={}",config_path.display()),if cloud { "NORMA_TEST_MODE=doubao-cloud-desktop" } else { "NORMA_TEST_MODE=doubao-desktop" },
        if cloud { "-only-testing:NormaDeviceTests/DoubaoCloudDesktopTests/testCloudTaskAndNativeReport" } else { "-only-testing:NormaDeviceTests/DoubaoDesktopTests/testActualGatewayDesktopTaskAndReport" },"test"
    ]).output().expect("native L3")
    });
    let auth_path = directory.path().join("phone-auth.json");
    // 180s = up to 120s incremental Xcode build + 60s simulator boot/launch.
    wait(Duration::from_secs(180), || {
        auth_path.is_file() || test.is_finished()
    });
    if !auth_path.is_file() {
        let output = test.join().expect("native thread");
        std::fs::write(app_root.join("doubao-native.log"), &output.stdout)
            .expect("failure evidence");
        panic!(
            "原生测试未创建身份：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let credential: Value =
        serde_json::from_slice(&std::fs::read(&auth_path).expect("private test identity"))
            .expect("credential JSON");
    let device = credential["deviceID"].as_str().expect("test device");
    store
        .mobile_access_store()
        .register(
            device,
            "Realtime Native L3",
            credential["token"].as_str().expect("test token"),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("only test project control");
    let output = test.join().expect("native test complete");
    store
        .mobile_access_store()
        .revoke(device)
        .expect("revoke exact temporary device");
    std::fs::write(app_root.join("doubao-native.log"), &output.stdout).expect("native evidence");
    assert!(
        output.status.success(),
        "完整链路失败，详情见 {}/doubao-native.log：{}",
        app_root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    if cloud {
        cloud::verify(&app_root, directory.path(), &project, &workspace);
        return;
    }
    assert_eq!(
        std::fs::read_to_string(project.join("result.txt")).expect("actual PTY file"),
        "realtime complete\nqueue second\n",
        "不能用回执替代实际产物"
    );
    assert!(
        !project.join("cancelled.txt").exists(),
        "取消任务C不得执行或创建文件"
    );
    let evidence: Value = serde_json::from_slice(
        &std::fs::read(directory.path().join("native-verified.json")).expect("native evidence"),
    )
    .expect("evidence JSON");
    let deliveries = MobileTaskDeliveryStore::new(app_root.join("data/app.sqlite"));
    let list = deliveries.list(&workspace, None, None).expect("deliveries");
    assert_eq!(list.len(), 2, "A/B各一份成果，C不能生成伪成果");
    let first = list
        .iter()
        .find(|entry| entry.outcome.key.turn_id == evidence["turns"][0]["turn"])
        .expect("A exact Provider turn");
    let second = list
        .iter()
        .find(|entry| entry.outcome.key.turn_id == evidence["turns"][1]["turn"])
        .expect("B exact Provider turn");
    let first = deliveries
        .get(&workspace, &first.outcome.key)
        .expect("A frozen delivery")
        .expect("A exists");
    let second = deliveries
        .get(&workspace, &second.outcome.key)
        .expect("B frozen delivery")
        .expect("B exists");
    let TaskDeliveryContent::Available {
        review: first_review,
    } = first.content
    else {
        panic!("A必须含可验收差异");
    };
    let TaskDeliveryContent::Available {
        review: second_review,
    } = second.content
    else {
        panic!("B必须含可验收差异");
    };
    assert_eq!(first_review.files.len(), 1, "A只创建result.txt");
    let a = &first_review.files[0];
    assert_eq!(a.path, "result.txt", "A必须准确归属首个文件");
    assert!(a.before.is_none(), "A前文件不存在，不能混入B基线");
    assert_eq!(
        a.after.as_ref().expect("A after").bytes,
        b"realtime complete\n",
        "A成果必须冻结在B执行前，不得混入第二行"
    );
    assert_eq!(second_review.files.len(), 1, "B只修改result.txt");
    let b = &second_review.files[0];
    assert_eq!(b.path, "result.txt", "B必须修改原文件");
    assert_eq!(
        b.before.as_ref().expect("B before").bytes,
        b"realtime complete\n",
        "B差异的原文必须是A的完成结果"
    );
    assert_eq!(
        b.after.as_ref().expect("B after").bytes,
        b"realtime complete\nqueue second\n",
        "B只能追加自己的第二行"
    );
    let baselines = MobileTaskBaselineStore::new(app_root.join("data/app.sqlite"));
    let a = baselines
        .get(&workspace, &first.outcome.key)
        .expect("A baseline")
        .expect("A source");
    let b = baselines
        .get(&workspace, &second.outcome.key)
        .expect("B baseline")
        .expect("B source");
    assert_eq!(
        a.operation_id, evidence["receipts"][0]["operation_id"],
        "A成果必须属于首次派工回执"
    );
    assert_eq!(
        b.operation_id, evidence["receipts"][1]["operation_id"],
        "B成果必须属于排队回执"
    );
    let TaskBaseline::Available { review } = a.baseline else {
        panic!("A基线必须可用")
    };
    assert!(review.files.is_empty(), "A从空项目开始，不能使用B的基线");
    std::fs::copy(
        directory.path().join("native-verified.json"),
        app_root.join("doubao-verified.json"),
    )
    .expect("full-chain evidence");
    assert!(
        !directory.path().join("phone-auth.json").exists(),
        "原生测试必须回收精确凭据"
    );
}

fn wait(timeout: Duration, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "L3 fixture timed out after {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
