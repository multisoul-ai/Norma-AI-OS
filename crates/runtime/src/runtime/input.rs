use super::*;

impl Runtime {
    pub fn send_node_input(&mut self, node_id: NodeId, input: NodeInput) -> Result<(), NodeError> {
        self.track_native_agent(node_id);
        let resume = if let Some(run) = self.mobile_runs.get_mut(&node_id) {
            if run.message.is_some() {
                return self.fail_node_operation(
                    Some(node_id),
                    "input",
                    NodeError::Pty(terminal::PtyError::Io(std::io::Error::other(
                        "手机消息正在提交，请稍候输入",
                    ))),
                );
            }
            // Track attempted input too: a partial PTY write must not be treated as an empty draft.
            run.native_approvals.invalidate();
            run.native_input.observe(&input)
        } else {
            false
        };
        let cancelled = matches!(&input, NodeInput::Key(key) if key.key == terminal::Key::Escape);
        self.track_native_resume_input(node_id, resume, cancelled);
        let Some(node) = self.live.get_mut(&node_id) else {
            return self.fail_node_operation(Some(node_id), "input", NodeError::MissingNode);
        };
        let result = node.input(input);
        self.finish_node_operation(Some(node_id), "input", result)
    }
}
