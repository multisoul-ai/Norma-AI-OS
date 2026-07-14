use std::collections::HashSet;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentName {
    display: String,
    match_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinAgentName {
    name: &'static str,
    speech_alias: &'static str,
}

impl BuiltinAgentName {
    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn speech_alias(self) -> &'static str {
        self.speech_alias
    }
}

const BUILTIN_AGENT_NAMES: [BuiltinAgentName; 30] = [
    builtin("Alan", "艾伦"),
    builtin("Bella", "贝拉"),
    builtin("Bruno", "布鲁诺"),
    builtin("Chloe", "克洛伊"),
    builtin("Daisy", "黛西"),
    builtin("Ethan", "伊森"),
    builtin("Felix", "菲利克斯"),
    builtin("Grace", "格蕾丝"),
    builtin("Henry", "亨利"),
    builtin("Iris", "艾瑞斯"),
    builtin("Jasper", "贾斯珀"),
    builtin("Kevin", "凯文"),
    builtin("Leo", "利奥"),
    builtin("Mason", "梅森"),
    builtin("Nora", "诺拉"),
    builtin("Oscar", "奥斯卡"),
    builtin("Piper", "派珀"),
    builtin("Quinn", "奎因"),
    builtin("Ruby", "鲁比"),
    builtin("Simon", "西蒙"),
    builtin("Sophie", "苏菲"),
    builtin("Tessa", "泰莎"),
    builtin("Tommy", "汤米"),
    builtin("Uma", "乌玛"),
    builtin("Victor", "维克多"),
    builtin("Wendy", "温迪"),
    builtin("Xena", "西娜"),
    builtin("Yara", "雅拉"),
    builtin("Zane", "赞恩"),
    builtin("Rowan", "罗文"),
];

const fn builtin(name: &'static str, speech_alias: &'static str) -> BuiltinAgentName {
    BuiltinAgentName { name, speech_alias }
}

pub fn builtin_agent_names() -> &'static [BuiltinAgentName] {
    &BUILTIN_AGENT_NAMES
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentNameError {
    #[error("Agent 名称必须是 2–16 个英文字母，或 2–8 个汉字")]
    UnsupportedFormat,
}

#[derive(Debug, Clone, Default)]
pub struct AgentNameRegistry {
    occupied_keys: HashSet<String>,
}

impl AgentNameRegistry {
    pub fn from_names<I, S>(names: I) -> Result<Self, AgentNameError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut registry = Self::default();
        for name in names {
            let parsed = AgentName::parse(name.as_ref())?;
            registry.reserve(&parsed);
        }
        Ok(registry)
    }

    pub fn next_available_builtin(&self) -> Option<BuiltinAgentName> {
        BUILTIN_AGENT_NAMES
            .iter()
            .copied()
            .find(|candidate| !self.builtin_is_occupied(*candidate))
    }

    fn reserve(&mut self, name: &AgentName) {
        self.occupied_keys.insert(name.match_key.clone());
        if let Some(builtin) = builtin_for_canonical_key(name.match_key()) {
            self.occupied_keys
                .insert(builtin.speech_alias().to_string());
        }
    }

    fn builtin_is_occupied(&self, candidate: BuiltinAgentName) -> bool {
        self.occupied_keys
            .contains(&candidate.name().to_ascii_lowercase())
            || self.occupied_keys.contains(candidate.speech_alias())
    }
}

impl AgentName {
    pub fn parse(input: &str) -> Result<Self, AgentNameError> {
        let character_count = input.chars().count();
        let is_english = (2..=16).contains(&character_count)
            && input
                .chars()
                .all(|character| character.is_ascii_alphabetic());
        let is_chinese =
            (2..=8).contains(&character_count) && input.chars().all(is_cjk_unified_ideograph);

        if !is_english && !is_chinese {
            return Err(AgentNameError::UnsupportedFormat);
        }

        Ok(Self {
            display: input.to_string(),
            match_key: if is_english {
                input.to_ascii_lowercase()
            } else {
                input.to_string()
            },
        })
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn match_key(&self) -> &str {
        &self.match_key
    }
}

fn is_cjk_unified_ideograph(character: char) -> bool {
    matches!(character, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}')
}

fn builtin_for_canonical_key(match_key: &str) -> Option<BuiltinAgentName> {
    BUILTIN_AGENT_NAMES
        .iter()
        .copied()
        .find(|builtin| builtin.name().eq_ignore_ascii_case(match_key))
}
