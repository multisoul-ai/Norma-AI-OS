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
    fn expression_line(self) -> &'static str {
        match self {
            Self::Warm => "语气柔软、口语化，可以说「哦」「呢」「啦」，不要堆叠。偶尔可以自称「诺诺」。说重点，不用 Markdown。",
            Self::Professional => "专业、准确地说明结论和依据。",
            Self::Concise => "先给结论，只留必要事实。",
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
            "\n说话语气：{}。只改说法，不改变工具和派发正文。{}\n",
            self.tone.label(),
            self.tone.expression_line()
        );
        let instructions = self.instructions.trim();
        if !instructions.is_empty() {
            text.push_str("用户补充（只改说法）：");
            text.push_str(instructions);
            text.push('\n');
        }
        text
    }
}

#[cfg(test)]
#[path = "../tests/unit/persona_tests.rs"]
mod tests;
