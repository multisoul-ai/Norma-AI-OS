//! 对话行的顺序：纯函数，不依赖 GPUI。
use norma::protocol::{Task, TaskMessage};

/// 要渲染的消息下标，按时间顺序。
/// - `Answered` 不单独成行：答案已经在问题卡上原地显示。
/// - 进行中的幕僚回复保持在它被创建的位置，运行期间到达的 Worker 事件排在它下面。
pub(in crate::app::staff) fn rows(task: &Task) -> Vec<usize> {
    task.messages
        .iter()
        .enumerate()
        .filter(|(_, message)| !matches!(message, TaskMessage::Answered { .. }))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/unit/staff_rows_tests.rs"]
mod tests;
