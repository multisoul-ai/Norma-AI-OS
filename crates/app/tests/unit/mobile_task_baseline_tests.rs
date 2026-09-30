use super::confirmed_turn;
use serde_json::json;

/// 任务起点只绑定本次已确认的新回合，不能认领旧回合或另一卡片/会话。
///
/// 数据构造：Codex节点7/run-a/session-a，旧turn-a，新turn-b；冷启动快照不含任何运行身份。
/// 执行过程：验证正常续发 → 原回合重现 → 改节点/运行/会话 → 缺少turn → 冷启动确认。
/// 预期结果：仅正常续发及冷启动可绑定；旧turn与错身份返回错误，不能生成可保存的起点key。
#[test]
fn confirmed_baseline_requires_a_new_turn_in_the_expected_session() {
    let expected: norma::mobile::CardSnapshot = serde_json::from_value(json!({
        "node_id":7,"profile":"Codex","title":"Card","mobile_control":true,"state":"done",
        "run_id":"run-a","session_id":"session-a","turn_id":"turn-a"
    }))
    .expect("准确旧快照");
    let receipt = json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-b"});
    let valid = confirmed_turn(&receipt, &expected).expect("新回合");
    assert_eq!(valid.turn_id, "turn-b", "必须绑定实际新turn");
    let mut wrong = receipt.clone();
    wrong["turn_id"] = json!("turn-a");
    assert!(
        confirmed_turn(&wrong, &expected).is_err(),
        "旧回合不能被新发送的起点冒领"
    );
    wrong = receipt.clone();
    wrong["node_id"] = json!(8);
    assert!(
        confirmed_turn(&wrong, &expected).is_err(),
        "节点8不属于节点7发送"
    );
    wrong = receipt.clone();
    wrong["run_id"] = json!("run-b");
    assert!(
        confirmed_turn(&wrong, &expected).is_err(),
        "重开的运行不能冒充原运行"
    );
    wrong = receipt.clone();
    wrong["session_id"] = json!("session-b");
    assert!(
        confirmed_turn(&wrong, &expected).is_err(),
        "另一会话不能绑定原请求"
    );
    wrong = receipt.clone();
    wrong["turn_id"] = serde_json::Value::Null;
    assert!(
        confirmed_turn(&wrong, &expected).is_err(),
        "缺少真实turn不能推断成功"
    );
    let mut cold = expected.clone();
    cold.run_id = None;
    cold.session_id = None;
    cold.turn_id = None;
    let started = confirmed_turn(&receipt, &cold).expect("首条实际确认");
    assert_eq!(
        started.session_id, "session-a",
        "空卡以真实Provider确认建立新session"
    );
    assert!(started.turn_id != "turn-a", "不能从空卡推断任何旧回合");
}
