use agent::jev_choice::{JevChoiceRequest, JevError};
use serde_json::json;
use std::collections::BTreeMap;

/// 场景：外部 Choice 输出的选项、分布或模型版本不符合请求契约。
/// 构造：两条完整方案（动作与目标不拆开）以及可拒绝的选项。
/// 执行：通过公共请求/解码接口验证官方 JSON 响应。
/// 预期：有效答案保留真实版本；遗漏概率、未知选项和 latest 别名均被拒绝。
#[test]
fn choice_contract_rejects_incomplete_or_unbound_answers() {
    let request = JevChoiceRequest::new(
        "Henry 检查 Bruno 的方案".into(),
        BTreeMap::from([
            ("PLAN_0".into(), "Henry 执行检查，Bruno 仅被提及".into()),
            ("PLAN_1".into(), "Bruno 执行检查".into()),
            ("CLARIFY".into(), "目标或动作不明确，只澄清".into()),
        ]),
    )
    .expect("人工合成选择请求应有效");
    let valid = json!({"model":"jev-1.13.0","answers":{"plan":{"type":"choice","choice":"PLAN_0","confidence":0.8,"probabilities":{"PLAN_0":0.9,"PLAN_1":0.05,"CLARIFY":0.05}}}});
    let answer = request
        .decode(valid.clone())
        .expect("完整的绑定分布应可观测");
    assert_eq!(answer.model, "jev-1.13.0", "须记录实际版本而非请求别名");
    assert_eq!(answer.choice, "PLAN_0", "完整方案 ID 不能拆成独立动作/目标");
    for changed in [
        {
            let mut v = valid.clone();
            v["answers"]["plan"]["probabilities"]
                .as_object_mut()
                .expect("对象")
                .remove("CLARIFY");
            v
        },
        {
            let mut v = valid.clone();
            v["answers"]["plan"]["choice"] = json!("OTHER");
            v
        },
        {
            let mut v = valid.clone();
            v["model"] = json!("jev-latest");
            v
        },
        {
            let mut v = valid.clone();
            v["answers"]["plan"]["probabilities"]["PLAN_0"] = json!(0.1);
            v
        },
    ] {
        assert_eq!(
            request.decode(changed),
            Err(JevError::InvalidResponse),
            "无效结果不得进入候选选择"
        );
    }
}
