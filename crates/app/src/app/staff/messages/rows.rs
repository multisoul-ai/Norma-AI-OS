//! 对话行的顺序：纯函数，不依赖 GPUI。
use norma::protocol::{Task, TaskMessage};

/// 要渲染的消息下标，按显示顺序。
/// - 进行中的幕僚回复永远在最底部；它运行期间到达的 Worker 事件显示在它上方。
/// - `Answered` 不单独成行：答案已经在问题卡上原地显示。
pub(in crate::app::staff) fn rows(task: &Task) -> Vec<usize> {
    let running = task
        .messages
        .iter()
        .rposition(|message| matches!(message, TaskMessage::Staff(reply) if reply.is_running()));
    let mut rows: Vec<usize> = task
        .messages
        .iter()
        .enumerate()
        .filter(|(index, message)| {
            Some(*index) != running && !matches!(message, TaskMessage::Answered { .. })
        })
        .map(|(index, _)| index)
        .collect();
    rows.extend(running);
    rows
}

#[cfg(test)]
#[path = "../../../../tests/unit/staff_rows_tests.rs"]
mod tests;
