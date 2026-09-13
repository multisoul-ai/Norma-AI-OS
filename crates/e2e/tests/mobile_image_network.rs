use app::{MobileAccess, MobileServer};
use serde_json::{json, Value};
use storage::{AppStore, MobilePermission, MobileProjectAccess};
use tungstenite::Message;

#[path = "support/mobile.rs"]
mod support;

const IMAGE: &[u8] = include_bytes!("fixtures/mobile-image.png");
const ENCODED: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

/// 图片上传使用正式TLS与项目授权，完整校验后才产生可供原Agent读取的文件。
/// 数据构造：1×1 PNG共68字节；只读phone-a、control revision=2；分片与原有256KiB帧限制兼容。
/// 执行过程：只读上传拒绝→升权→超限/错位/坏摘要拒绝→半上传取消→上传校验→读取原图→跨项目拒绝→撤销后拒绝。
/// 预期结果：精确图片字节和文件路径存在；未授权/越界无文件，原始图片可通过同一项目读取。
#[test]
fn image_upload_requires_control_and_preserves_exact_bytes() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "Images")
        .expect("隔离项目")
        .workspace_id;
    let grant = |access| {
        vec![MobileProjectAccess {
            workspace_id: workspace.clone(),
            access,
        }]
    };
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "phone",
            &"a".repeat(64),
            &grant(MobilePermission::Read),
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
    )
    .expect("正式服务");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let image_id = "431ced6916a2a21a156e38701afe55bbd7f88969fbbfc56d7fe099d47f265460";
    let upload = |revision, size| json!({"type":"upload_image","device_id":"phone-a","token":"a".repeat(64),"workspace_id":workspace,"grant_revision":revision,"image_id":image_id,"byte_count":size});
    let denied = support::exchange(&mut socket, &machine, "read-only", upload(1, IMAGE.len()));
    assert_eq!(denied["error"], "unauthorized", "只读不能写入附件");
    server
        .replace_permissions("phone-a", 1, &grant(MobilePermission::Control))
        .expect("明确升权");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    // 2MiB+1：比单图上限多一个字节，不能先接收再限制。
    assert_eq!(
        support::exchange(
            &mut socket,
            &machine,
            "oversize",
            upload(2, 2 * 1024 * 1024 + 1)
        )["error"],
        "invalid_image",
        "超限应在读取图片前拒绝"
    );
    let image_folder = directory.path().join("app.mobile-images");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(
            &mut socket,
            &machine,
            "wrong-offset",
            upload(2, IMAGE.len())
        )["result"]["status"],
        "image_upload",
        "先建立合法上传"
    );
    socket
        .send(Message::text(
            json!({"offset":1,"data":ENCODED}).to_string(),
        ))
        .expect("错位分片");
    let invalid: Value =
        serde_json::from_str(socket.read().expect("拒绝错位").to_text().expect("JSON"))
            .expect("回执");
    assert_eq!(
        invalid["error"], "invalid_image",
        "首片offset只能为0，不能跳过字节"
    );
    assert!(!image_folder.exists(), "错位上传不能留下文件");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    support::exchange(&mut socket, &machine, "wrong-hash", upload(2, IMAGE.len()));
    let wrong = ENCODED.replacen('i', "j", 1);
    socket
        .send(Message::text(json!({"offset":0,"data":wrong}).to_string()))
        .expect("错误摘要");
    let invalid: Value = serde_json::from_str(
        socket
            .read()
            .expect("拒绝损坏内容")
            .to_text()
            .expect("JSON"),
    )
    .expect("回执");
    assert_eq!(invalid["error"], "invalid_image", "内容摘要不符不能保存");
    assert!(!image_folder.exists(), "损坏上传不能留下文件");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    support::exchange(&mut socket, &machine, "partial", upload(2, IMAGE.len()));
    // 34=68/2，只传一半后关闭；完整图片才允许原子保存。
    socket
        .send(Message::text(
            json!({"offset":0,"data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAA=="})
                .to_string(),
        ))
        .expect("半图");
    let partial: Value =
        serde_json::from_str(socket.read().expect("分片确认").to_text().expect("JSON"))
            .expect("回执");
    assert_eq!(partial["result"]["image_offset"], 34, "只能确认收到的半图");
    socket.close(None).expect("主动取消");
    assert!(!image_folder.exists(), "取消上传不能留下半文件");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(&mut socket, &machine, "upload", upload(2, IMAGE.len()))["result"]
            ["status"],
        "image_upload",
        "授权后才接收分片"
    );
    socket
        .send(Message::text(
            json!({"offset":0,"data":ENCODED}).to_string(),
        ))
        .expect("原图片分片");
    let reply: Value =
        serde_json::from_str(socket.read().expect("上传回执").to_text().expect("JSON"))
            .expect("回执");
    assert_eq!(reply["result"]["status"], "image_saved", "校验完成才可使用");
    let path = reply["result"]["image"]["path"]
        .as_str()
        .expect("电脑本地路径");
    assert_eq!(
        std::fs::read(path).expect("文件已写入"),
        IMAGE,
        "Agent读取的必须是同一张图"
    );
    assert!(
        std::path::Path::new(path).starts_with(directory.path()),
        "不能写入隔离目录之外"
    );
    let read = |scope: &str| json!({"type":"chat_image","device_id":"phone-a","token":"a".repeat(64),"workspace_id":scope,"image_id":image_id});
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(&mut socket, &machine, "read-image", read(&workspace))["result"]["image"]
            ["data"],
        ENCODED,
        "历史缩略图应来自完整原图"
    );
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    assert_eq!(
        support::exchange(&mut socket, &machine, "wrong-scope", read("other-project"))["error"],
        "unauthorized",
        "知道摘要也不能跨项目读图"
    );
    server.revoke("phone-a").expect("撤销配对");
    let mut socket = support::connect(server.addresses()[0], cert);
    assert_eq!(
        support::exchange(&mut socket, &machine, "revoked", read(&workspace))["error"],
        "unauthorized",
        "撤销后不能继续读取图片"
    );
    server.shutdown();
}
