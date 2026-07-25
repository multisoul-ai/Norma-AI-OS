use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

impl AgentCardAdapter for RecordingAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.commands
                .lock()
                .expect("delete target recording lock should not be poisoned")
                .push(command);
            Ok(AdapterResult::succeeded("卡片已关闭"))
        })
    }
}

fn tool_context(turn: &str, input: &str) -> ToolContext {
    ToolContext {
        project_id: "project-1".to_string(),
        turn_id: turn.to_string(),
        user_input: input.to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some(format!("{turn}:call-1")),
        cancellation: CancellationToken::new(),
    }
}

/// DEL-TARGET-1：用户说“关闭第二张卡片”时，只授权 1-based 位置 2。
///
/// 数据构造（含关键数值的推导过程）：
///   用户序数             = 第二张
///   1-based target       = 2
///   模型工具调用尝试      = 3 次（position=1 + node_id=2 + position=2）
///   非法目标             = 2（错误位置 + 把位置数字误当 node_id）
///   预期 Adapter 副作用数 = 3 - 2 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用原文“关闭第二张卡片”调用 `delete_canvas_card(position=1)`。
///   2. Runtime 对照原始用户位置，拒绝错误的第一张目标。
///   3. 使用新 turn 调用 `delete_canvas_card(node_id=2)`，验证序数不能授权同数字节点。
///   4. 再使用新 turn 调用 `delete_canvas_card(position=2)`。
///   5. Runtime 把唯一获授权的位置删除命令交给 Adapter。
///
/// 预期结果：
///   - 负断言：position=1 被拒绝，且不得产生删除副作用。
///   - 负断言：node_id=2 被拒绝，位置与稳定节点编号不能混用。
///   - 正断言：position=2 成功到达 Adapter。
///   - 正断言：Adapter 记录的是位置目标，不是模型猜测的 node_id。
#[tokio::test]
async fn delete_canvas_card_authorizes_only_the_spoken_position() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());

    let wrong_position = runtime
        .execute(
            "delete_canvas_card",
            json!({"position": 1}),
            &tool_context("turn-position-wrong", "关闭第二张卡片"),
        )
        .await;
    let guessed_node = runtime
        .execute(
            "delete_canvas_card",
            json!({"node_id": 2}),
            &tool_context("turn-position-as-node", "关闭第2张卡片"),
        )
        .await;
    let exact_position = runtime
        .execute(
            "delete_canvas_card",
            json!({"position": 2}),
            &tool_context("turn-position-exact", "关闭第二张卡片"),
        )
        .await
        .expect("用户明确说第二张时 position=2 应获得删除授权");
    let commands = adapter
        .commands
        .lock()
        .expect("delete target recording lock should not be poisoned");

    assert!(
        matches!(wrong_position, Err(AdapterError::Rejected(_))),
        "用户说第二张时 position=1 必须在 Adapter 前被拒绝；实际：{wrong_position:?}"
    );
    assert!(
        matches!(guessed_node, Err(AdapterError::Rejected(_))),
        "用户只说位置 2 时 node_id=2 必须在 Adapter 前被拒绝；实际：{guessed_node:?}"
    );
    assert_eq!(
        exact_position.status, "succeeded",
        "position=2 应返回 Adapter 的真实成功回执"
    );
    assert_eq!(
        commands.len(),
        1,
        "三次目标尝试中只能有用户明确授权的 position=2 到达 Adapter"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::DeleteCanvasCardAtPosition { position: 2 },
        "Adapter 必须收到 1-based position=2，不能收到错误 node_id 或 position=1"
    );
    assert!(
        !commands.contains(&AdapterCommand::DeleteCanvasCardAtPosition { position: 1 }),
        "错误的第一张卡片目标不得产生任何删除副作用"
    );
    assert!(
        !commands.contains(&AdapterCommand::DeleteCanvasCard { node_id: 2 }),
        "用户只说第二张时不得猜测并删除同数字的 node_id=2"
    );
}

/// DEL-TARGET-2：用户说“关闭chloe”时，只授权名称精确匹配的 Chloe AgentCard。
///
/// 数据构造（含关键数值的推导过程）：
///   用户目标                 = chloe
///   候选工具目标             = ChloeBackup + Chloe
///   工具调用尝试             = 2 次
///   预期 Adapter 副作用数     = 2 - 1 次名称前缀误匹配 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用原文“关闭chloe”调用 `delete_canvas_card(agent="ChloeBackup")`。
///   2. Runtime 拒绝未在原文中出现的较长名称，不能把前缀关系当成授权。
///   3. 使用新 turn 调用 `delete_canvas_card(agent="Chloe")`。
///   4. Runtime 忽略英文大小写并把精确名称目标交给 Adapter。
///
/// 预期结果：
///   - 负断言：ChloeBackup 被拒绝且不产生删除副作用。
///   - 正断言：Chloe 获得授权并到达 Adapter。
///   - 正断言：Adapter 收到名称目标，后续可解析对应活跃 AgentCard。
#[tokio::test]
async fn delete_canvas_card_authorizes_only_the_spoken_agent_name() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());

    let wrong_agent = runtime
        .execute(
            "delete_canvas_card",
            json!({"agent": "ChloeBackup"}),
            &tool_context("turn-agent-wrong", "关闭chloe"),
        )
        .await;
    let exact_agent = runtime
        .execute(
            "delete_canvas_card",
            json!({"agent": "Chloe"}),
            &tool_context("turn-agent-exact", "关闭chloe"),
        )
        .await
        .expect("用户明确说关闭 chloe 时同名 AgentCard 应获得删除授权");
    let commands = adapter
        .commands
        .lock()
        .expect("delete target recording lock should not be poisoned");

    assert!(
        matches!(wrong_agent, Err(AdapterError::Rejected(_))),
        "用户只说 chloe 时 ChloeBackup 必须在 Adapter 前被拒绝；实际：{wrong_agent:?}"
    );
    assert_eq!(
        exact_agent.status, "succeeded",
        "精确名称 Chloe 应返回 Adapter 的真实成功回执"
    );
    assert_eq!(
        commands.len(),
        1,
        "两次名称尝试中只能有用户明确授权的 Chloe 到达 Adapter"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::DeleteAgentCard {
            agent: "Chloe".to_string(),
        },
        "Adapter 必须收到 Chloe 名称目标，不能收到 ChloeBackup 或猜测的 node_id"
    );
    assert!(
        !commands.contains(&AdapterCommand::DeleteAgentCard {
            agent: "ChloeBackup".to_string(),
        }),
        "未被用户点名的 ChloeBackup 不得产生任何删除副作用"
    );
}

/// DEL-TARGET-3：Agent 名称后的自然标点不得阻断明确关闭意图。
///
/// 数据构造（含关键数值的推导过程）：
///   用户目标               = Chloe
///   名称后卡片限定语       = “的 Agent 卡片”
///   限定语后标点           = 1 个中文逗号
///   礼貌后缀               = “谢谢”
///   预期 Adapter 副作用数   = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入“请关闭 Chloe 的 Agent 卡片，谢谢”并调用 `delete_canvas_card(agent="Chloe")`。
///   2. Runtime 在精确名称后识别卡片限定语与自然语言标点边界。
///   3. Runtime 把关闭命令交给 Adapter。
///
/// 预期结果：
///   - 正断言：带标点的明确关闭请求成功到达 Adapter。
///   - 正断言：Adapter 只记录 Chloe 这个名称目标。
///   - 负断言：不得因为逗号后的礼貌文本把请求误判为无授权。
#[tokio::test]
async fn delete_canvas_card_accepts_punctuation_after_exact_agent_name() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());

    let result = runtime
        .execute(
            "delete_canvas_card",
            json!({"agent": "Chloe"}),
            &tool_context("turn-agent-punctuation", "请关闭 Chloe 的 Agent 卡片，谢谢"),
        )
        .await
        .expect("精确 Agent 名称后的自然标点不应阻断明确关闭授权");
    let commands = adapter
        .commands
        .lock()
        .expect("delete target recording lock should not be poisoned");

    assert_eq!(
        result.status, "succeeded",
        "带标点的 Chloe 关闭请求应返回 Adapter 的真实成功回执"
    );
    assert_eq!(
        commands.len(),
        1,
        "明确关闭 Chloe 的一次请求必须且只能产生一次 Adapter 副作用"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::DeleteAgentCard {
            agent: "Chloe".to_string(),
        },
        "标点边界后的礼貌文本不得改变实际关闭目标"
    );
    assert!(
        !commands.contains(&AdapterCommand::DeleteAgentCard {
            agent: "Chloe，谢谢".to_string(),
        }),
        "逗号后的礼貌文本不得被吸收到 Agent 名称中"
    );
}

/// DEL-TARGET-4：删除工具参数必须且只能包含一个目标维度。
///
/// 数据构造（含关键数值的推导过程）：
///   可选目标维度           = 3（node_id、position、agent）
///   非法调用               = 2 次（0 个目标 + 2 个目标）
///   合法调用               = 0 次
///   预期 Adapter 副作用数   = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用不带目标的 `delete_canvas_card`。
///   2. 调用同时携带 `node_id=7` 和 `position=2` 的工具请求。
///   3. 检查参数解析错误与 Adapter 记录。
///
/// 预期结果：
///   - 负断言：缺少目标必须作为无效参数被拒绝。
///   - 负断言：同时提供两个目标必须作为无效参数被拒绝。
///   - 正断言：两次非法调用均在 Adapter 前终止，副作用数保持为零。
#[tokio::test]
async fn delete_canvas_card_requires_exactly_one_target_dimension() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());

    let missing = runtime
        .execute(
            "delete_canvas_card",
            json!({}),
            &tool_context("turn-target-missing", "关闭第二张卡片"),
        )
        .await;
    let conflicting = runtime
        .execute(
            "delete_canvas_card",
            json!({"node_id": 7, "position": 2}),
            &tool_context("turn-target-conflicting", "关闭第二张卡片"),
        )
        .await;
    let commands = adapter
        .commands
        .lock()
        .expect("delete target recording lock should not be poisoned");

    assert!(
        matches!(missing, Err(AdapterError::InvalidArguments(_))),
        "缺少 node_id、position、agent 时必须报告无效参数；实际：{missing:?}"
    );
    assert!(
        matches!(conflicting, Err(AdapterError::InvalidArguments(_))),
        "同时提供 node_id 与 position 时必须报告无效参数；实际：{conflicting:?}"
    );
    assert_eq!(
        commands.len(),
        0,
        "缺少目标或目标冲突的工具调用都不得到达 Adapter"
    );
}
