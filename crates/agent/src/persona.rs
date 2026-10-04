//! User-selected communication style, independent of memory access permissions.
use serde::{Deserialize, Serialize};

pub const MAX_PERSONA_INSTRUCTION_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaTone {
    #[default]
    Warm,
    Professional,
    Concise,
}

impl PersonaTone {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warm => "温暖自然",
            Self::Professional => "专业清晰",
            Self::Concise => "简洁直接",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Warm => "自然亲切，像熟悉的伙伴",
            Self::Professional => "结构清晰，准确解释与建议",
            Self::Concise => "结论优先，尽量简短",
        }
    }
    pub fn example(self) -> &'static str {
        match self {
            Self::Warm => "好呀，我们先把最重要的一步做好，再一起往前走。",
            Self::Professional => "建议先完成核心流程，再验证结果，最后补齐细节。",
            Self::Concise => "先完成核心流程，再验证。",
        }
    }
    fn expression_rules(self) -> &'static str {
        match self {
            Self::Warm => "- 语气柔软、自然、口语化；可以自然使用“哦”“呢”“啦”等柔软尾音，不要机械堆叠。\n- 偶尔可以自称“诺诺”，不要每句重复，也不要刻意卖萌。\n- 说重点，不使用 Markdown、标题、列表或代码块；不要写书面长句。\n",
            Self::Professional => "- 专业、准确地说明结论与依据；根据内容需要使用段落、列表或代码，不刻意使用柔软尾音或昵称。\n",
            Self::Concise => "- 先给结论，保留必要事实，减少铺垫和重复；根据内容需要使用列表或代码。\n",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NormaPersona {
    pub tone: PersonaTone,
    pub instructions: String,
}

impl NormaPersona {
    pub fn validate(&self) -> Result<(), String> {
        if self.instructions.chars().count() > MAX_PERSONA_INSTRUCTION_CHARS {
            return Err(format!(
                "补充说明最多 {MAX_PERSONA_INSTRUCTION_CHARS} 个字符"
            ));
        }
        if self
            .instructions
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
        {
            return Err("补充说明含有不支持的控制字符".into());
        }
        Ok(())
    }
    pub fn prompt(&self) -> String {
        let mut text = format!(
            "\n用户在 Norma 设置中选择的交流风格：{}。{}。这只调整默认的对话表达，保留固定身份与工具规则。以下表达偏好可由用户的交流补充说明调整。\n",
            self.tone.label(), self.tone.description()
        );
        text.push_str(self.tone.expression_rules());
        let instructions = self.instructions.trim();
        if !instructions.is_empty() {
            text.push_str("用户的交流补充说明（仅适用于表达方式，不授予额外工具权限）：\n");
            text.push_str(instructions);
            text.push('\n');
        }
        text
    }
}

#[cfg(test)]
#[path = "../tests/unit/persona_tests.rs"]
mod tests;
