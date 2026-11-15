//! 给模型看的任务快照：只含对话文字和结构化状态，不含过程摘要（步骤、计划、耗时）。
use norma::protocol::{StaffPart, Task, TaskMessage};
use serde_json::{json, Value};

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
