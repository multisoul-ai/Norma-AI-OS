use card_task::{compile_card_task_steps, CardTaskStep};
use norma::protocol::{CardCommand, Placement};
use voice::{CompiledVoicePlan, ExecutableCommand, RiskLevel};

/// M-1: compiled voice command 到 card task step 的转换应保留命令种类和 previous-step 引用。
///
/// 数据构造（含关键数值的推导过程）：
///   commands.len = 3
///   step0 = Card(Create Terminal)
///   step1 = SendTextToPrevious(step=0, submit=false)
///   step2 = FocusPrevious(step=0)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个含 3 条 executable commands 的 compiled plan
///   2. 调用 compile_card_task_steps(plan)
///   3. 检查每一条 step 是否按原语义映射
///
/// 预期结果：
///   - 正断言：step0 转成 CardTaskStep::Command(Create Terminal)
///   - 正断言：step1 转成 SendTextToPrevious，保留 previous_step/text/submit
///   - 正断言：step2 转成 FocusPrevious，保留 previous_step
#[test]
fn test_compile_card_task_steps_maps_all_supported_variants() {
    let compiled = CompiledVoicePlan {
        commands: vec![
            ExecutableCommand::Card(CardCommand::Create {
                kind: norma::protocol::CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            }),
            ExecutableCommand::SendTextToPrevious {
                previous_step: 0,
                text: "hello".to_string(),
                submit: false,
            },
            ExecutableCommand::FocusPrevious { previous_step: 0 },
        ],
        requires_confirmation: false,
        risk: RiskLevel::Low,
        reason: Some("map executable commands".to_string()),
    };

    let steps = compile_card_task_steps(&compiled);

    assert_eq!(
        steps[0],
        CardTaskStep::Command(CardCommand::Create {
            kind: norma::protocol::CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        }),
        "step0 应保持为创建 Terminal 的 CardCommand"
    );
    assert_eq!(
        steps[1],
        CardTaskStep::SendTextToPrevious {
            previous_step: 0,
            text: "hello".to_string(),
            submit: false,
        },
        "step1 应保持 previous_step=0、text=hello、submit=false"
    );
    assert_eq!(
        steps[2],
        CardTaskStep::FocusPrevious { previous_step: 0 },
        "step2 应保持为 FocusPrevious(previous_step=0)"
    );
}
