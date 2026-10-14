mod support;
use norma::connector::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use support::Credentials;
struct Fixed;
impl Clock for Fixed {
    fn now(&self) -> i64 {
        1_000_000
    }
}
struct Source(Arc<Mutex<Snapshot>>);
impl DataSource for Source {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        Ok(self.0.lock().expect("快照").clone())
    }
}
fn fixture() -> (ConnectorTools, Authenticated, Arc<Mutex<Snapshot>>) {
    let auth =
        Arc::new(Auth::load(Arc::new(Credentials::default()), Arc::new(Fixed)).expect("鉴权"));
    let token = auth.generate().expect("凭据");
    let identity = auth.authenticate(Some(&token)).expect("鉴权");
    let signer = CardSigner::new(&auth.signer_key().expect("密钥"));
    let cards =
        [("a", "canvas-a", "Alan"), ("b", "canvas-b", "ALAN")].map(|(area, canvas, name)| {
            let identity = CardIdentity {
                workspace_id: "w".into(),
                area_id: area.into(),
                canvas_id: canvas.into(),
                node_id: 1,
                definition_id: 1,
                created_at: 10,
            };
            Card {
                card_id: signer.sign(&identity),
                identity,
                card_revision: "r".into(),
                role_name: name.into(),
                role_name_unique: true,
                project_id: "p".into(),
                profile: "Codex".into(),
                state: "busy".into(),
                current_task_id: Some("fake".into()),
                loaded: false,
            }
        });
    let task = |id: &str, state: &str, time: i64| Task {
        task_id: id.into(),
        card_id: cards[0].card_id.clone(),
        role_name: "Alan".into(),
        workspace_id: "w".into(),
        state: state.into(),
        tracking: "tracked".into(),
        summary: "摘".repeat(501),
        created_at: time,
        updated_at: time,
        source: "other".into(),
        operation_id: None,
        progress: None,
        pending_approval: None,
        result_summary: None,
        error: None,
    };
    let tasks = vec![
        task("old-done", "done", 900_000),
        task("old-running", "running", 900_000),
        task("recent-done", "done", 999_999),
        task("recent-running", "running", 999_999),
    ];
    let snapshot = Arc::new(Mutex::new(Snapshot {
        workspaces: vec![Workspace {
            workspace_id: "w".into(),
            name: "空间".into(),
            active: true,
            projects: vec![
                Project {
                    project_id: "p".into(),
                    area_id: "a".into(),
                    name: "同目录".into(),
                },
                Project {
                    project_id: "p".into(),
                    area_id: "b".into(),
                    name: "同目录".into(),
                },
            ],
            cards: cards.into(),
        }],
        tasks,
    }));
    (
        ConnectorTools::new(auth, signer, Arc::new(Source(snapshot.clone()))),
        identity,
        snapshot,
    )
}
fn call(tools: &ConnectorTools, identity: &Authenticated, name: &str, args: Value) -> Value {
    tools
        .call(identity, name, args)
        .structured_content
        .expect("结构化响应")
}
/// 场景：两个画布共享项目、node_id=1、重复名称；读取目录后检查状态、身份、名称唯一性。
/// 预期：全部unloaded、身份不同、名称不唯一；原始快照不改变。
#[test]
fn directory_has_unique_canvas_identity_and_never_loads() {
    let (tools, identity, snapshot) = fixture();
    let result = call(&tools, &identity, "directory", json!({}));
    let cards = result["workspaces"][0]["cards"].as_array().expect("卡片");
    assert_eq!(cards.len(), 2, "保留所有卡片");
    assert_ne!(
        cards[0]["card_id"], cards[1]["card_id"],
        "同node_id跨画布必须唯一"
    );
    for card in cards {
        assert_eq!(
            card["role_name_unique"], false,
            "工作空间内名称大小写不敏感查重"
        );
        assert_eq!(card["state"], "unloaded", "未加载不能推断busy");
        assert_eq!(card["loaded"], false, "读取不加载区域");
        assert_eq!(
            card["current_task_id"],
            Value::Null,
            "不将过时任务视为正在运行"
        );
    }
    assert!(
        !snapshot.lock().expect("快照").workspaces[0].cards[0].loaded,
        "数据源无加载副作用"
    );
    let missing = call(
        &tools,
        &identity,
        "directory",
        json!({"workspace_id":"missing"}),
    );
    assert_eq!(
        missing["error"]["code"], "workspace_not_found",
        "未知空间明确错误"
    );
    let unknown = tools.call(&identity, "directory", json!({"force_untracked":true}));
    assert_eq!(unknown.is_error, Some(true), "未知字段必须拒绝");
}
/// 场景：一个24h前终态、一个旧running、两个新任务；第一页后修改任务集合与状态。
/// 预期：默认只排除旧终态，游标仍返回初始快照的下一页；错误游标和limit返回规范错误。
#[test]
fn default_filter_and_cursor_survive_concurrent_changes() {
    let (tools, identity, snapshot) = fixture();
    let page = call(&tools, &identity, "list_tasks", json!({"limit":2}));
    let rows = page["tasks"].as_array().expect("任务");
    assert_eq!(rows[0]["task_id"], "old-running", "所有未终态保留");
    assert_eq!(rows[1]["task_id"], "recent-done", "24h内终态保留");
    assert_eq!(
        rows[0]["summary"].as_str().expect("摘要").chars().count(),
        500,
        "摘要最多500字符"
    );
    let cursor = page["next_cursor"].as_str().expect("下一页");
    snapshot.lock().expect("快照").tasks.clear();
    let next = call(
        &tools,
        &identity,
        "list_tasks",
        json!({"limit":2,"cursor":cursor}),
    );
    assert_eq!(
        next["tasks"][0]["task_id"], "recent-running",
        "游标快照稳定"
    );
    assert_eq!(next["next_cursor"], Value::Null, "最后页无游标");
    let replay = call(
        &tools,
        &identity,
        "list_tasks",
        json!({"limit":2,"cursor":cursor}),
    );
    assert_eq!(next, replay, "重复游标相同结果");
    for args in [
        json!({"cursor":"bad"}),
        json!({"limit":51}),
        json!({"states":["bogus"]}),
        json!({"cursor":cursor,"limit":1}),
    ] {
        let result = call(&tools, &identity, "list_tasks", args);
        assert_eq!(
            result["error"]["code"], "invalid_argument",
            "游标和参数错误使用规范错误码"
        );
    }
}
/// 场景：无可信计划，查询存在/缺失任务并用含未知参数的输入和超过60次的读取。
/// 预期：进度不编造、缺失任务明确错误、超额返回isError及retry_after_s。
#[test]
fn progress_validation_and_structured_rate_error() {
    let (tools, identity, _) = fixture();
    let value = call(
        &tools,
        &identity,
        "get_task_progress",
        json!({"task_id":"old-running"}),
    );
    assert_eq!(value["progress"], Value::Null, "无分母不可编造进度");
    assert_eq!(value["pending_approval"], Value::Null, "无审批不编造");
    assert_eq!(
        call(
            &tools,
            &identity,
            "get_task_progress",
            json!({"task_id":"missing"})
        )["error"]["code"],
        "task_not_found",
        "任务不存在"
    );
    for input in [
        json!({}),
        json!({"task_id":"x","operation_id":"y"}),
        json!({"task_id":"x","unknown":true}),
    ] {
        assert_eq!(
            call(&tools, &identity, "get_task_progress", input)["error"]["code"],
            "invalid_argument",
            "输入需严格校验"
        );
    }
    for _ in 5..60 {
        let _ = tools.call(&identity, "directory", json!({}));
    }
    let limited = tools.call(&identity, "directory", json!({}));
    assert_eq!(limited.is_error, Some(true), "限流为工具错误");
    let error = limited.structured_content.expect("错误结构");
    assert_eq!(error["error"]["code"], "rate_limited", "规范错误码");
    assert_eq!(error["error"]["retry_after_s"], 60, "窗口剩余60秒");
}
/// 场景：身份编码包含canvas，篡改HMAC后校验；改变canvas但其他字段相同。
/// 预期：合法身份往返、篡改拒绝，跨canvas不同。
#[test]
fn card_id_roundtrip_and_tamper_rejection() {
    let signer = CardSigner::new(&[42; 32]);
    let mut identity = CardIdentity {
        workspace_id: "w".into(),
        area_id: "a".into(),
        canvas_id: "one".into(),
        node_id: 1,
        definition_id: 2,
        created_at: 3,
    };
    let first = signer.sign(&identity);
    assert_eq!(signer.verify(&first), Ok(identity.clone()), "合法身份往返");
    identity.canvas_id = "two".into();
    assert_ne!(first, signer.sign(&identity), "canvas标识包含在认证身份中");
    let forged = format!("{}00", &first[..first.len() - 2]);
    assert!(signer.verify(&forged).is_err(), "篡改HMAC拒绝");
}
