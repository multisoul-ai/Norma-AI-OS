use serde_json::json;
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand};

fn command(device: &str, operation: &str) -> NewMobileCommand {
    NewMobileCommand {
        device_id: device.into(),
        operation_id: operation.into(),
        workspace_id: "project-a".into(),
        route_id: "window-a".into(),
        owner_id: "boot-a".into(),
        request_hash: "a".repeat(64),
    }
}

/// 同一手机命令并发提交只能领取一次执行权，换参数冲突；掉线或重开不能重放副作用。
///
/// 数据构造：两个线程同时提交 phone-a/op-a，hash=64 个 a 表示同一规范化请求；结果为 node_id=7。
/// 执行过程：并发 reserve → 重开 store 查询 → 完成回执 → 重复完成 → 换参数 / 另一手机 / 另一项目查询。
/// 预期结果：仅一次 New；持久化结果可回查；不同参数不能认领；不同手机 / 项目不能读取。
#[test]
fn mobile_command_is_reserved_once_and_receipt_is_immutable() {
    let directory = tempfile::tempdir().expect("临时数据库");
    let store = MobileCommandStore::new(directory.path().join("app.sqlite"));
    let request = command("phone-a", "op-a");
    let first = store.clone();
    let a = request.clone();
    let second = store.clone();
    let b = request.clone();
    let left = std::thread::spawn(move || first.reserve(&a).expect("首线程 reserve"));
    let right = std::thread::spawn(move || second.reserve(&b).expect("次线程 reserve"));
    let outcomes = [left.join().expect("线程一"), right.join().expect("线程二")];
    assert_eq!(
        outcomes
            .iter()
            .filter(|x| matches!(x, MobileCommandReservation::New))
            .count(),
        1,
        "必须只有一次执行权"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|x| matches!(x, MobileCommandReservation::Existing(_)))
            .count(),
        1,
        "重试只能收到已有回执"
    );
    let reopened = MobileCommandStore::new(directory.path().join("app.sqlite"));
    let unknown = reopened
        .get("phone-a", "op-a", "project-a", "boot-b")
        .expect("重启查询")
        .expect("保留记录");
    assert_eq!(
        unknown.state, "outcome_unknown",
        "不同启动实例不得把未完成请求当可重放队列"
    );
    reopened
        .finish(&request, Ok(json!({"node_id":7})))
        .expect("保存完成结果");
    reopened
        .finish(&request, Ok(json!({"node_id":7})))
        .expect("重复同结果幂等");
    assert!(
        reopened.finish(&request, Ok(json!({"node_id":8}))).is_err(),
        "终态不能被另一个节点结果覆盖"
    );
    let saved = reopened
        .get("phone-a", "op-a", "project-a", "boot-b")
        .expect("回查")
        .expect("回执");
    assert_eq!(saved.state, "succeeded", "完成后重开可取得真实终态");
    assert_eq!(
        saved.result,
        Some(json!({"node_id":7})),
        "必须保留原来的具体卡片身份"
    );
    assert!(
        reopened
            .get("phone-b", "op-a", "project-a", "boot-b")
            .expect("另一手机")
            .is_none(),
        "手机 B 不得读取 A 回执"
    );
    assert!(
        reopened
            .get("phone-a", "op-a", "project-b", "boot-b")
            .expect("另一项目")
            .is_none(),
        "项目 B 不得读取 A 回执"
    );
    let mut changed = request.clone();
    changed.request_hash = "b".repeat(64);
    assert!(
        matches!(
            reopened.reserve(&changed).expect("参数冲突检查"),
            MobileCommandReservation::Conflict
        ),
        "同 ID 改参数不能再次创建"
    );
    let other = MobileCommandStore::new(directory.path().join("other.sqlite"));
    assert!(
        other
            .get("phone-a", "op-a", "project-a", "boot-b")
            .expect("另一安装")
            .is_none(),
        "另一安装不得共享命令账本"
    );
}

/// 写入失败、无效身份和尚未登记的操作都不能伪造成功或破坏已有拒绝记录。
///
/// 数据构造：op-denied 被拒绝；op-missing 未登记；hash 长度 63 比 SHA256 十六进制 64 少 1。
/// 执行过程：预留 → 拒绝 → 重开回查 → 错 hash → 无预留完成 → 不可写路径预留。
/// 预期结果：拒绝无 node 结果；无效 / 缺失 / 不可写请求报错，不能出现成功回执。
#[test]
fn mobile_command_failures_preserve_known_outcome() {
    let directory = tempfile::tempdir().expect("临时数据库");
    let store = MobileCommandStore::new(directory.path().join("app.sqlite"));
    let request = command("phone-a", "op-denied");
    store.reserve(&request).expect("预留");
    store
        .finish(&request, Err("route_expired".into()))
        .expect("拒绝落盘");
    let receipt = store
        .get("phone-a", "op-denied", "project-a", "boot-b")
        .expect("回查")
        .expect("拒绝回执");
    assert_eq!(
        receipt.state, "rejected",
        "没有执行的旧窗口请求必须明确拒绝"
    );
    assert_eq!(
        receipt.error.as_deref(),
        Some("route_expired"),
        "保留稳定错误码"
    );
    assert!(receipt.result.is_none(), "拒绝不得伪造节点结果");
    let mut invalid = command("phone-a", "op-invalid");
    invalid.request_hash = "a".repeat(63);
    assert!(store.reserve(&invalid).is_err(), "不完整指纹不能进入账本");
    assert!(
        store
            .finish(&command("phone-a", "op-missing"), Ok(json!({"node_id":1})))
            .is_err(),
        "未预留不得声明完成"
    );
    std::fs::write(directory.path().join("blocked"), b"file").expect("阻塞路径");
    let blocked = MobileCommandStore::new(directory.path().join("blocked/app.sqlite"));
    assert!(
        blocked.reserve(&request).is_err(),
        "无法写账本不得获得执行权"
    );
}
