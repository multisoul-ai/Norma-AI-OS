use super::{NormaPersona, PersonaTone, MAX_PERSONA_INSTRUCTION_CHARS};

/// 人设只定义表达方式，固定身份与工具边界继续存在。
///
/// 数据构造：默认暖心风格、专业风格、简洁风格各一份；补充说明为“优先中文”。
/// 执行过程：分别生成追加的提示词，并核对不同风格与边界文本。
/// 预期结果：选中的风格与补充说明存在；专业风格不含暖心的表达要求。
#[test]
fn persona_changes_style_without_replacing_identity_or_tool_rules() {
    let warm = NormaPersona::default().prompt();
    let professional = NormaPersona {
        tone: PersonaTone::Professional,
        instructions: "优先中文".into(),
    }
    .prompt();
    let concise = NormaPersona {
        tone: PersonaTone::Concise,
        ..Default::default()
    }
    .prompt();
    assert!(warm.contains("温暖自然"), "默认应保持 Norma 的温暖表达");
    assert!(
        professional.contains("专业清晰"),
        "专业模式应进入真实提示词"
    );
    assert!(
        professional.contains("优先中文"),
        "用户保存的说明应进入提示词"
    );
    assert!(
        professional.contains("固定身份与工具规则"),
        "人设不能覆盖工具边界"
    );
    assert!(
        !professional.contains("温暖自然"),
        "专业模式不能混入默认暖心风格"
    );
    assert!(concise.contains("简洁直接"), "简洁模式应使用独立表达要求");
}

/// 超长或控制字符说明不能进入设置和模型上下文。
///
/// 数据构造：中文说明上限=2000字符；允许2000个“好”，拒绝2001个；NUL为非法控制字符。
/// 执行过程：分别验证边界内、边界外、含NUL的人设。
/// 预期结果：2000中文字符可保存；超限和NUL均明确失败。
#[test]
fn persona_validation_counts_characters_and_rejects_invalid_input() {
    let allowed = NormaPersona {
        instructions: "好".repeat(MAX_PERSONA_INSTRUCTION_CHARS),
        ..Default::default()
    };
    let oversized = NormaPersona {
        instructions: "好".repeat(MAX_PERSONA_INSTRUCTION_CHARS + 1),
        ..Default::default()
    };
    let invalid = NormaPersona {
        instructions: "说明\0正文".into(),
        ..Default::default()
    };
    assert!(
        allowed.validate().is_ok(),
        "2000个中文字符应被允许，不能错误地按UTF-8字节计数"
    );
    assert!(
        oversized.validate().is_err(),
        "2001字符说明应被拒绝，避免无界上下文"
    );
    assert!(invalid.validate().is_err(), "NUL不得进入提示词或持久化设置");
}
