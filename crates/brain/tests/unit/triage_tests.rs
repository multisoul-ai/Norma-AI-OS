use super::*;
use crate::event::Outcome;

fn finished(outcome: Outcome, excerpt: &str, from_result: bool) -> BrainEvent {
    BrainEvent::AgentFinished {
        agent: "Alan".into(),
        outcome,
        excerpt: excerpt.into(),
        from_result,
    }
}

fn is_speak(event: &BrainEvent) -> bool {
    matches!(triage(event), Verdict::Speak(_))
}

/// 失败一定出声，不管摘要写了什么。
#[test]
fn failures_always_speak() {
    assert!(is_speak(&finished(Outcome::Failed, "", false)));
    assert!(is_speak(&finished(Outcome::Failed, "一切顺利", true)));
}

/// 普通成功、被中断都静默。
///
/// 数据构造：成功且摘要平平；被用户中断。
/// 预期结果：两者都是 Quiet。
#[test]
fn plain_success_and_interruptions_stay_quiet() {
    assert!(!is_speak(&finished(
        Outcome::Succeeded,
        "登录页做完了，12 个测试通过",
        true
    )));
    assert!(!is_speak(&finished(Outcome::Interrupted, "", false)));
}

/// 成功的结果里出现“需要你”“无法”“error”等词时出声（不区分大小写）。
///
/// 数据构造：几条带关键词的真实结果。
/// 预期结果：全部出声，理由是 Keyword。
#[test]
fn attention_words_in_a_real_result_speak() {
    for text in [
        "做完了，但需要你确认接口命名",
        "请确认是否继续",
        "无法连接数据库，已跳过",
        "tests passed but ERROR logged",
        "Blocked by missing credentials",
        "结果和预期不一致，不确定是否正确",
    ] {
        assert_eq!(
            triage(&finished(Outcome::Succeeded, text, true)),
            Verdict::Speak(SpeakReason::Keyword),
            "{text}"
        );
    }
}

/// 摘要不是 Agent 的结果、只是用户自己发的话时，里面的词不算。
///
/// 数据构造：摘要是用户的指令“修复报错并确认”，from_result=false。
/// 预期结果：Quiet，不被用户自己的词触发。
#[test]
fn words_from_the_users_own_prompt_do_not_count() {
    assert!(!is_speak(&finished(
        Outcome::Succeeded,
        "请修复这个报错，需要你确认后再改",
        false
    )));
}
