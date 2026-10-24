//! 汇报分级：哪些事件要立刻告诉用户，哪些先静默攒着。纯函数，没有 IO，也不问模型。
//!
//! 只做能确定的判断：失败、结果里出现需要用户的词。更细的语义（结果存疑、有风险）
//! 留给静默池里的大脑去扫，规则不假装看得懂。

use crate::event::{BrainEvent, Outcome};

/// 结果文字里出现这些词，就当作需要用户知道。全小写，匹配时统一转小写。
const ATTENTION_WORDS: &[&str] = &[
    "需要你",
    "请确认",
    "请你",
    "是否继续",
    "需要决定",
    "无法",
    "失败",
    "报错",
    "未通过",
    "不确定",
    "阻塞",
    "error",
    "failed",
    "panic",
    "blocked",
    "cannot",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeakReason {
    Failed,
    Keyword,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// 需要告诉用户：1 秒去抖后立刻交给大脑。
    Speak(SpeakReason),
    /// 先进静默池，攒满窗口再让大脑扫一遍。
    Quiet,
}

pub fn triage(event: &BrainEvent) -> Verdict {
    let BrainEvent::AgentFinished {
        outcome,
        excerpt,
        from_result,
        ..
    } = event
    else {
        return Verdict::Quiet;
    };
    match outcome {
        Outcome::Failed => Verdict::Speak(SpeakReason::Failed),
        Outcome::Interrupted => Verdict::Quiet,
        // 摘要若只是用户自己发的话，里面的词不是 Agent 的结果，不能算。
        Outcome::Succeeded if *from_result && mentions_attention(excerpt) => {
            Verdict::Speak(SpeakReason::Keyword)
        }
        Outcome::Succeeded => Verdict::Quiet,
    }
}

fn mentions_attention(text: &str) -> bool {
    let lower = text.to_lowercase();
    ATTENTION_WORDS.iter().any(|word| lower.contains(word))
}

#[cfg(test)]
#[path = "../tests/unit/triage_tests.rs"]
mod triage_tests;
