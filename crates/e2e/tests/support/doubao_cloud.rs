use serde_json::{json, Value};
use std::path::Path;
use storage::{MobileTaskBaselineStore, MobileTaskDeliveryStore, TaskDeliveryContent};

pub fn verify(app_root: &Path, directory: &Path, project: &Path, workspace: &str) {
    let evidence: Value = serde_json::from_slice(
        &std::fs::read(directory.join("native-verified.json")).expect("cloud native evidence"),
    )
    .expect("JSON");
    if std::env::var("NORMA_DOUBAO_REAL_CLI").as_deref() == Ok("1") {
        assert_eq!(evidence["realCLI"], true, "真实CLI模式不能退回确定性替身");
        assert!(
            !project
                .parent()
                .expect("fixture root")
                .join("release-first")
                .exists(),
            "真实模型应自然完成，不能依赖替身完成门"
        );
    }
    assert_eq!(
        std::fs::read_to_string(project.join("result.txt")).expect("actual artifact"),
        "云端验证完成\n",
        "真实云端派工必须产生约定文件"
    );
    assert_eq!(
        evidence["turns"].as_array().map(Vec::len),
        Some(1),
        "云端不能重复派工"
    );
    let db = app_root.join("data/app.sqlite");
    let store = MobileTaskDeliveryStore::new(db.clone());
    let list = store
        .list(workspace, None, None)
        .expect("actual deliveries");
    assert_eq!(list.len(), 1, "正式成果库必须只有原任务成果");
    let delivery = store
        .get(workspace, &list[0].outcome.key)
        .expect("frozen result")
        .expect("exists");
    assert_eq!(
        delivery.outcome.key.turn_id, evidence["turns"][0]["turn"],
        "成果必须来自真实Hook回合"
    );
    if evidence["realCLI"] == true {
        assert_eq!(
            delivery.outcome.key.session_id, evidence["turns"][0]["session"],
            "成果必须绑定真实Codex会话，不能只沿用fixture回合字段"
        );
        let root = project.parent().expect("fixture root");
        let hooks: Vec<Value> = std::fs::read_to_string(root.join("hooks.jsonl"))
            .expect("actual CLI Hook records")
            .lines()
            .map(|line| serde_json::from_str(line).expect("Hook JSON"))
            .collect();
        assert_eq!(
            hooks
                .iter()
                .filter(|e| e["hook_event_name"] == "UserPromptSubmit")
                .count(),
            1,
            "真实CLI只应收到一次派工，不能补发任务制造结果"
        );
        assert!(
            hooks.iter().any(|e| e["hook_event_name"] == "Stop"
                && e["turn_id"] == delivery.outcome.key.turn_id
                && e["session_id"] == delivery.outcome.key.session_id),
            "完成必须来自原会话的真实Stop事件"
        );
    }
    let TaskDeliveryContent::Available { review } = &delivery.content else {
        panic!("云端任务必须有可验收差异");
    };
    assert_eq!(review.files.len(), 1, "只能创建约定的result.txt");
    assert_eq!(review.files[0].path, "result.txt", "文件路径不能被模型改派");
    assert!(review.files[0].before.is_none(), "派工前文件必须不存在");
    assert_eq!(
        review.files[0].after.as_ref().expect("after bytes").bytes,
        "云端验证完成\n".as_bytes(),
        "冻结的是实际CLI产物"
    );
    let baseline = MobileTaskBaselineStore::new(db)
        .get(workspace, &delivery.outcome.key)
        .expect("source")
        .expect("baseline");
    assert!(
        evidence["receipts"]
            .as_array()
            .expect("actual tool receipts")
            .iter()
            .any(|r| r["status"] == "accepted" && r["operation_id"] == baseline.operation_id),
        "原云端工具回执必须绑定该成果"
    );
    assert!(list[0].acceptance.is_none(), "语音已播不能自动验收");
    assert!(
        !directory.join("phone-auth.json").exists(),
        "精确原生测试凭据必须回收"
    );
    std::fs::write(
        app_root.join("doubao-verified.json"),
        serde_json::to_vec_pretty(&json!({
            "cloud":evidence,"delivery":delivery,"baseline":baseline
        }))
        .expect("evidence JSON"),
    )
    .expect("save cloud evidence");
}
