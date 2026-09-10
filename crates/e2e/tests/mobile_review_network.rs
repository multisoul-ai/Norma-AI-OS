use app::{LiveReviewRequest, LiveSnapshotRequest, MobileAccess, MobileServer, MobileSources};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile.rs"]
mod support;

/// 真实WSS只读预览完整工作区变更，超过500KiB时分片；越权和过期窗口都不能回传正文。
///
/// 数据构造：隔离Git项目里200000个汉字=600000字节，加69字节1×1红色PNG；
///   read手机仅授权A；8容量通道对应服务连接上限。
/// 执行过程：后台真实捕获 → 分片重组 → 越权B → 携带路径注入 → 切换窗口route → 撤销权限。
/// 预期结果：正文和图片字节完整，PNG不伪造文本diff；无验收/执行回执；
///   B和额外path被拒；旧窗口与撤销后的读取无成果内容。
#[test]
fn review_transfer_is_scoped_complete_and_read_only() {
    let dir = tempfile::tempdir().expect("隔离数据");
    let root = dir.path().join("project");
    std::fs::create_dir(&root).expect("项目");
    let status = std::process::Command::new("/usr/bin/git")
        .current_dir(&root)
        .args(["init", "--quiet"])
        .status()
        .expect("Git fixture");
    assert!(status.success(), "隔离Git初始化必须成功");
    let body = "变".repeat(200_000);
    std::fs::write(root.join("result.txt"), &body).expect("真实文件");
    // PNG=8字节签名+25字节IHDR+24字节IDAT+12字节IEND=69字节，独立测试数据。
    let image: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    std::fs::write(root.join("z-preview.png"), image).expect("真实图片");
    let store = AppStore::new(dir.path().join("app.db"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(&root, "A")
        .expect("项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "Phone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (reviews, requests) = async_channel::bounded::<LiveReviewRequest>(8);
    let hold = Arc::new(AtomicBool::new(false));
    let holding = hold.clone();
    let (arrived, captured) = std::sync::mpsc::sync_channel(1);
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        while let Ok(call) = requests.recv_blocking() {
            let value =
                workspace::review::capture(&root, call.deadline).map_err(|_| "review_unavailable");
            if holding.load(Ordering::SeqCst) {
                arrived.send(()).expect("捕获完成后拦住响应");
                released
                    .recv_timeout(Duration::from_secs(5))
                    .expect("撤销后释放；5s覆盖本地授权写入");
            }
            let _ = call.reply.send(value);
        }
    });
    let (snapshots, requests) = async_channel::bounded::<LiveSnapshotRequest>(8);
    let changed = Arc::new(AtomicBool::new(false));
    let change = changed.clone();
    let owner = std::thread::spawn(move || {
        while let Ok(mut call) = requests.recv_blocking() {
            call.projects[0].state = "open".into();
            call.projects[0].route_id = Some(
                if change.load(Ordering::SeqCst) {
                    "route-new"
                } else {
                    "route-a"
                }
                .into(),
            );
            call.projects[0].card_count = Some(0);
            call.projects[0].cards = Some(vec![]);
            let _ = call.reply.send(call.projects);
        }
    });
    let server = MobileServer::start_with_mobile_sources(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            snapshots: Some(snapshots.clone()),
            reviews: Some(reviews.clone()),
            ..Default::default()
        },
    )
    .expect("WSS服务");
    let action = json!({"type":"project_review","device_id":"phone-a","token":"a".repeat(64),"request":{"workspace_id":workspace,"route_id":"route-a"}});
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let mut result = support::exchange(&mut socket, &machine, "review", action.clone());
    let mut assembled = String::new();
    let mut transfer = None;
    // 8MiB payload cap and64KiB fragments bound this loop;5s covers the local capture plus delivery.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "分片传输必须有界结束");
        assert_eq!(
            result["result"]["status"], "review_fragment",
            "大结果必须使用成果分片"
        );
        assert!(result.to_string().len() < 500 * 1024, "单帧不得突破WSS预算");
        let part = &result["result"]["fragment"];
        assert_eq!(
            part["offset"].as_u64(),
            Some(assembled.len() as u64),
            "分片必须连续"
        );
        if let Some(id) = &transfer {
            assert_eq!(&part["transfer_id"], id, "不能混合不同快照");
        } else {
            transfer = Some(part["transfer_id"].clone());
        }
        assembled.push_str(part["text"].as_str().expect("分片正文"));
        if assembled.len() as u64 == part["total_bytes"].as_u64().expect("完整长度") {
            break;
        }
        result = serde_json::from_str(socket.read().expect("下片").to_text().expect("文本"))
            .expect("JSON");
    }
    let value: Value = serde_json::from_str(&assembled).expect("完整快照");
    assert_eq!(
        value["request"]["workspace_id"], workspace,
        "结果绑定获授权项目"
    );
    let bytes: Vec<u8> =
        serde_json::from_value(value["review"]["files"][0]["after"]["bytes"].clone())
            .expect("文件字节");
    assert_eq!(bytes, body.as_bytes(), "大正文不得截断或乱码");
    let picture = value["review"]["files"]
        .as_array()
        .expect("文件列表")
        .iter()
        .find(|file| file["path"] == "z-preview.png")
        .expect("图片必须进入相同的获授权传输");
    let pixels: Vec<u8> =
        serde_json::from_value(picture["after"]["bytes"].clone()).expect("图片字节");
    assert_eq!(pixels, image, "WSS分片重组必须保持PNG完整原始字节");
    assert_eq!(picture["after"]["kind"], "binary", "图片必须明确标记二进制");
    assert!(picture["before"].is_null(), "新增图片没有任务起点版本");
    assert!(picture["diff"].is_null(), "PNG不应被伪造为文本diff");
    assert!(
        value.get("receipt").is_none(),
        "只读预览不能伪造执行或验收回执"
    );
    let fetch = |action| {
        let mut socket = support::connect(server.addresses()[0], cert.clone());
        support::exchange(&mut socket, &machine, "denied", action)
    };
    let mut foreign = action.clone();
    foreign["request"]["workspace_id"] = json!("project-b");
    assert_eq!(fetch(foreign)["error"], "unauthorized", "越权项目必须拒绝");
    let mut injected = action.clone();
    injected["request"]["path"] = json!("/etc/passwd");
    assert!(
        fetch(injected).get("result").is_none(),
        "任意手机路径参数不得进入读取"
    );
    changed.store(true, Ordering::SeqCst);
    assert_eq!(
        fetch(action.clone())["error"],
        "route_expired",
        "旧窗口不能被新窗口冒充"
    );
    changed.store(false, Ordering::SeqCst);
    hold.store(true, Ordering::SeqCst);
    std::thread::scope(|scope| {
        let delayed = scope.spawn(|| fetch(action.clone()));
        captured
            .recv_timeout(Duration::from_secs(5))
            .expect("真实捕获必须先完成；5s覆盖Git与TLS");
        server.revoke("phone-a").expect("读取期间撤销");
        release.send(()).expect("释放迟到结果");
        let denied = delayed.join().expect("后置授权检查");
        assert_eq!(
            denied["error"], "unauthorized",
            "已读文件不能绕过读取期间的撤销"
        );
        assert!(
            denied.get("result").is_none(),
            "撤销后不得泄漏已捕获的文件字节"
        );
    });
    assert_eq!(
        fetch(action)["error"],
        "unauthorized",
        "撤销后不能获得已捕获内容"
    );
    snapshots.close();
    reviews.close();
    server.shutdown();
    reader.join().expect("捕获线程回收");
    owner.join().expect("前台fixture回收");
}
