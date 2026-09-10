use app::{MobileAccess, MobileServer};
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess, MobilePushStore};

#[path = "support/mobile.rs"]
mod support;

/// WSS通知订阅只影响已认证手机，不能扩大项目权限或在撤销后重新启用。
///
/// 数据构造：隔离Mac与只读手机phone-a；64位凭据与APNs token不同；订阅版本0→1→2→3。
/// 执行过程：未授权拒绝 → 读取关闭态 → 注册sandbox → 旧版本拒绝 → 关闭 → 重新注册 → Mac撤销。
/// 预期结果：只读可以收提醒但仍保持read；关闭后旧启用请求不能复活；撤销后数据库token已清除。
#[test]
fn authenticated_phone_can_manage_only_its_current_push_subscription() {
    let dir = tempfile::tempdir().expect("隔离WSS数据库");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Push")
        .expect("项目");
    let bearer = "a".repeat(64);
    let grant = store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &bearer,
            &[MobileProjectAccess {
                workspace_id: project.workspace_id,
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("隔离身份");
    let certificate = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
    )
    .expect("真实WSS服务");
    let fetch = |machine: &str, action| {
        let mut socket = support::connect(server.addresses()[0], certificate.clone());
        support::exchange(&mut socket, machine, "push-request", action)
    };
    let get = || json!({"type":"push_subscription","device_id":"phone-a","token":bearer});
    let registration = json!({"token":"b".repeat(64),"environment":"sandbox"});
    let set = |revision: u64, registration: serde_json::Value| json!({"type":"set_push_subscription","device_id":"phone-a","token":bearer,"expected_revision":revision,"registration":registration});
    assert_eq!(
        fetch("other-mac", get())["error"],
        "unauthorized",
        "另一电脑身份不能读取订阅"
    );
    let mut wrong = set(0, registration.clone());
    wrong["token"] = json!("c".repeat(64));
    assert_eq!(
        fetch(&machine, wrong)["error"],
        "unauthorized",
        "知道APNs token不等于具有配对权限"
    );
    assert_eq!(
        fetch(&machine, get())["result"]["subscription"]["revision"],
        0,
        "未授权请求不能产生注册记录"
    );
    let first = fetch(&machine, set(0, registration.clone()));
    assert_eq!(
        first["result"]["subscription"]["machine_id"], machine,
        "订阅状态必须绑定实际接受更新的电脑"
    );
    assert_eq!(
        first["result"]["subscription"]["device_id"], "phone-a",
        "订阅状态必须绑定本次认证的手机"
    );
    assert_eq!(
        first["result"]["status"], "push_subscription",
        "手机应取得真实订阅回执"
    );
    assert_eq!(
        first["result"]["subscription"]["revision"], 1,
        "首次订阅推进版本"
    );
    assert_eq!(
        first["result"]["subscription"]["registration"], registration,
        "已认证手机只能回读自己的token和环境"
    );
    let mut missing = set(1, serde_json::Value::Null);
    missing
        .as_object_mut()
        .expect("测试请求对象")
        .remove("registration");
    assert_eq!(
        fetch(&machine, missing)["error"],
        "invalid_request",
        "省略registration不等于用户明确关闭提醒"
    );
    assert_eq!(
        fetch(&machine, set(0, serde_json::Value::Null))["error"],
        "push_subscription_changed",
        "旧关闭请求不能覆盖新的订阅"
    );
    let off = fetch(&machine, set(1, serde_json::Value::Null));
    assert_eq!(
        off["result"]["subscription"]["revision"], 2,
        "关闭提醒也推进版本"
    );
    assert!(
        off["result"]["subscription"]["registration"].is_null(),
        "关闭回执不得保留投递token"
    );
    assert_eq!(
        fetch(&machine, set(1, registration.clone()))["error"],
        "push_subscription_changed",
        "关闭后旧启用请求不得复活"
    );
    let invalid = json!({"token":"not-hex","environment":"sandbox"});
    assert_eq!(
        fetch(&machine, set(2, invalid))["error"],
        "invalid_push_registration",
        "非法token须在存储前拒绝"
    );
    assert!(
        fetch(&machine, get())["result"]["subscription"]["registration"].is_null(),
        "非法输入不能重新开启提醒"
    );
    assert_eq!(
        fetch(&machine, set(2, registration))["result"]["subscription"]["revision"],
        3,
        "允许用户显式重新开启"
    );
    let auth = fetch(
        &machine,
        json!({"type":"authenticate","device_id":"phone-a","token":bearer}),
    );
    assert_eq!(
        auth["result"]["grant"]["projects"][0]["access"], "read",
        "注册通知不得扩大项目控制权限"
    );
    server.revoke("phone-a").expect("电脑撤销");
    assert_eq!(
        fetch(&machine, get())["error"],
        "unauthorized",
        "已撤销手机不能读取旧订阅"
    );
    assert_eq!(
        fetch(&machine, set(3, serde_json::Value::Null))["error"],
        "unauthorized",
        "已撤销手机不能修改订阅"
    );
    assert!(
        MobilePushStore::new(store.path().to_path_buf())
            .get(&grant)
            .expect("旧授权查询")
            .is_none(),
        "设备授权与投递目标必须一起失效"
    );
}
