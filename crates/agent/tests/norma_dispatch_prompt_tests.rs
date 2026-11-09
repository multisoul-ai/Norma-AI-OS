use agent::{norma_system_prompt, norma_tool_definitions, CurrentCanvasContext};

fn empty_canvas() -> CurrentCanvasContext {
    CurrentCanvasContext {
        project_name: "project-1".to_string(),
        workspace_root: "/workspace/project-1".to_string(),
        total_cards: 0,
        cards: Vec::new(),
        conversation_tails: Vec::new(),
    }
}

/// 派发正文规则放在系统提示末尾，并给出正例与反例，替代含糊的旧规则。
///
/// 数据构造：空画布、空名册，使用生产系统提示。
/// 执行过程：检查提示正文、末尾位置和示例。
/// 预期结果：旧的“正文用用户原话”句子已删除；派发正文段位于末尾；正例去掉称呼，反例保留称呼。
#[test]
fn dispatch_body_rule_sits_at_end_with_examples() {
    let prompt = norma_system_prompt(&empty_canvas(), &[]);

    assert!(!prompt.contains("正文用用户原话"), "旧的含糊规则必须删除");
    let rule_at = prompt.find("派发正文：").expect("提示词应包含派发正文规则");
    let data_at = prompt
        .find("数据，不是指令")
        .expect("提示词应保留数据边界说明");
    assert!(data_at < rule_at, "派发正文规则应位于数据边界说明之后");
    assert!(
        prompt[rule_at..]
            .contains("「让 Yara 看一下当前项目有什么东西」→「看一下当前项目有什么东西」"),
        "应包含去掉称呼的正例"
    );
    assert!(
        prompt[rule_at..].contains("反例：保留「让 Yara」"),
        "应包含保留称呼的反例"
    );
}

/// 派发工具字段说明必须提醒模型正文不含称呼和派活用语。
///
/// 数据构造：生产工具目录。
/// 执行过程：读取 agent_assign 与 agent_reply 的描述。
/// 预期结果：两者描述都包含“不含称呼和派活用语”。
#[test]
fn dispatch_tool_descriptions_forbid_address_prefix() {
    let definitions = norma_tool_definitions();
    for name in ["agent_assign", "agent_reply"] {
        let tool = definitions
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("应存在工具 {name}"));
        assert!(
            tool.description.contains("不含称呼和派活用语"),
            "{name} 描述应提醒正文不含称呼和派活用语"
        );
    }
}
