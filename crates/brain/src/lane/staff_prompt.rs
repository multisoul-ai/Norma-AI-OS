//! 给模型看的任务快照：只含对话文字和结构化状态，不含过程摘要（步骤、计划、耗时）。
//! 同一会话的后续轮次只发新增消息和状态变化；新会话发完整快照。
use norma::protocol::{StaffPart, Task, TaskMessage};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// 这个会话已经发给模型的消息和当时的结构化状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PromptCursor {
    sent: Vec<String>,
    state: String,
}

/// 新会话（`previous` 为空）发完整快照。否则只发还没发过的消息；状态有变化时附上新状态。
pub(super) fn incremental(task: &Task, previous: Option<&PromptCursor>) -> (String, PromptCursor) {
    let messages = projected(task);
    let state = state_json(task);
    let cursor = PromptCursor {
        sent: messages.clone(),
        state: state.clone(),
    };
    let Some(previous) = previous else {
        return (snapshot(task), cursor);
    };
    // 会话一直连续，两轮之间出现的幕僚回复就是它自己写的，不再回灌给它。
    let fresh: Vec<String> = unseen(&previous.sent, &messages)
        .into_iter()
        .filter(|item| !is_staff_reply(item))
        .collect();
    let mut body = format!("新增消息：{}", json!(fresh));
    if state != previous.state {
        body.push_str("\n状态变化：");
        body.push_str(&state);
    }
    (body, cursor)
}

fn projected(task: &Task) -> Vec<String> {
    task.messages
        .iter()
        .filter_map(|message| project(task, message).map(|value| value.to_string()))
        .collect()
}

fn is_staff_reply(item: &str) -> bool {
    serde_json::from_str::<Value>(item).is_ok_and(|value| value.get("staff").is_some())
}

/// 不含 `revision`：每次写入都会加 1，放进来会让每一轮都当作「状态有变化」而整份重发。
fn state_json(task: &Task) -> String {
    json!({
        "phase": task.phase,
        "proposals": task.proposals,
        "received_proposals": task.received_proposals,
        "workers": task.workers,
        "removed_workers": task.removed_workers,
        "questions": task.questions,
    })
    .to_string()
}

/// 按内容计数配对，不靠下标。删掉一条没发出去的消息后，旧消息不会重发，新消息也不会漏。
fn unseen(previous: &[String], current: &[String]) -> Vec<String> {
    let mut left: BTreeMap<&str, usize> = BTreeMap::new();
    for item in previous {
        *left.entry(item.as_str()).or_default() += 1;
    }
    let mut fresh = Vec::new();
    for item in current {
        let count = left.entry(item.as_str()).or_default();
        if *count > 0 {
            *count -= 1;
        } else {
            fresh.push(item.clone());
        }
    }
    fresh
}

pub(super) fn snapshot(task: &Task) -> String {
    let mut value = serde_json::to_value(task).expect("task JSON");
    value["messages"] = Value::Array(
        task.messages
            .iter()
            .filter_map(|message| project(task, message))
            .collect(),
    );
    value.to_string()
}

fn project(task: &Task, message: &TaskMessage) -> Option<Value> {
    Some(match message {
        TaskMessage::User { text } => json!({"user": text}),
        // 本轮正在写的回复就是模型自己，不放进它的上下文。
        TaskMessage::Staff(reply) if reply.is_running() => return None,
        TaskMessage::Staff(reply) => {
            let cards: Vec<Value> = reply
                .parts
                .iter()
                .filter_map(|part| match part {
                    StaffPart::Proposal(id) => Some(json!({"proposal": id})),
                    StaffPart::Question(id) => Some(json!({"question": id})),
                    _ => None,
                })
                .collect();
            json!({"staff": reply.text(), "cards": cards})
        }
        // 回答正文展开成“问题 + 答案”，模型看到的上下文和用户回答时一致。
        TaskMessage::Answered { question_id } => {
            let question = task.question(question_id)?;
            json!({"answer": question.answer_text(&question.answers), "question": question_id})
        }
        other => serde_json::to_value(other).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use norma::protocol::{ReplyTrigger, StaffProposal, StaffReply, TaskMessage};

    fn task() -> Task {
        let mut task = Task::new("t1", "w", "p", "发布");
        task.user_message("USER-ONE");
        task
    }

    /// 场景：临时故障把一条还在写的空回复删掉，随后用户又说了一句。
    /// 数据构造：第一轮快照里有 USER-ONE，以及一条不进快照的运行中回复。删掉它后再加 USER-TWO。
    /// 执行过程：先发完整快照，再按上次游标做增量。
    /// 预期结果：增量里有 USER-TWO，没有 USER-ONE。
    #[test]
    fn discard_does_not_repeat_or_drop_messages() {
        let mut task = task();
        task.messages
            .push(TaskMessage::Staff(StaffReply::new(1, ReplyTrigger::User)));
        let (full, cursor) = incremental(&task, None);
        assert!(
            full.contains("USER-ONE"),
            "新会话的完整快照包含已有用户消息"
        );
        assert!(!full.contains("USER-TWO"), "完整快照里还没有后一句");
        task.messages.pop();
        task.user_message("USER-TWO");
        let (delta, _) = incremental(&task, Some(&cursor));
        assert!(delta.contains("USER-TWO"), "删掉空回复后，新消息仍要出现");
        assert!(!delta.contains("USER-ONE"), "已经发过的消息不能再出现一次");
    }

    /// 场景：对话没变，只有提议从未采用变成已采用。
    /// 数据构造：一条用户消息和一条未采用提议。记下游标后把 approved 改成 true。
    /// 执行过程：用上次游标做增量。
    /// 预期结果：增量写明状态变化且包含该提议；用户原话不重复。
    #[test]
    fn proposal_status_change_is_included() {
        let mut task = task();
        task.proposals.push(StaffProposal {
            id: 1,
            agent: "Grace".into(),
            title: "PROP-ONE".into(),
            prompt: "ship".into(),
            why: "now".into(),
            model: None,
            approved: false,
            ignored: false,
            dispatch_id: None,
            round: 1,
            auto: false,
            downgrade: None,
            superseded: false,
            target_worker: None,
        });
        let (_, cursor) = incremental(&task, None);
        task.proposals[0].approved = true;
        let (delta, _) = incremental(&task, Some(&cursor));
        assert!(delta.contains("状态变化"), "提议状态变了就要写出来");
        assert!(delta.contains("PROP-ONE"), "变化里要能看到是哪条提议");
        assert!(!delta.contains("USER-ONE"), "没有新消息时不重发旧对话");
    }

    /// 场景：会话刚建好，或者上一轮会话已经丢掉。
    /// 数据构造：任务里已有用户消息和提议。
    /// 执行过程：不带游标做增量。
    /// 预期结果：正文就是完整快照，消息和提议都在。
    #[test]
    fn new_session_sends_the_full_snapshot() {
        let mut task = task();
        task.proposals.push(StaffProposal {
            id: 1,
            agent: "Grace".into(),
            title: "PROP-ONE".into(),
            prompt: "ship".into(),
            why: "now".into(),
            model: None,
            approved: false,
            ignored: false,
            dispatch_id: None,
            round: 1,
            auto: false,
            downgrade: None,
            superseded: false,
            target_worker: None,
        });
        let (full, _) = incremental(&task, None);
        assert_eq!(full, snapshot(&task), "新会话发的就是完整快照");
        assert!(full.contains("USER-ONE"), "完整快照包含对话");
        assert!(full.contains("PROP-ONE"), "完整快照包含提议");
    }

    /// 场景：会话连续，上一轮幕僚的回复不能在下一轮的「新增消息」里回灌；写入次数增加也不算状态变化。
    /// 数据构造：用户说 USER-ONE，记下游标。随后幕僚写完一条回复 OWN-REPLY，用户又说 USER-TWO，
    ///   同时任务的 revision 加了 5（只是写入，没有任何结构化状态变化）。
    /// 执行过程：用上次游标做增量。
    /// 预期结果：
    ///   - 新增消息里有 USER-TWO。
    ///   - 新增消息里没有 OWN-REPLY（模型自己说过的话）。
    ///   - 没有「状态变化」段（revision 不参与比较）。
    #[test]
    fn own_reply_and_revision_do_not_trigger_resend() {
        let mut task = task();
        let (_, cursor) = incremental(&task, None);
        let mut reply = StaffReply::new(1, ReplyTrigger::User);
        reply.push_text("OWN-REPLY");
        reply.status = norma::protocol::ReplyStatus::Done;
        task.messages.push(TaskMessage::Staff(reply));
        task.user_message("USER-TWO");
        task.revision += 5;
        let (delta, _) = incremental(&task, Some(&cursor));
        assert!(delta.contains("USER-TWO"), "用户的新消息要发给模型");
        assert!(!delta.contains("OWN-REPLY"), "模型自己上一轮的回复不能回灌");
        assert!(!delta.contains("状态变化"), "revision 变化不能触发状态重发");
    }
}
