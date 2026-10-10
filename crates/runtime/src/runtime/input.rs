use super::*;

impl Runtime {
    pub fn agent_task_input_held(&self, node_id: NodeId) -> bool {
        self.workspace_card_input_held(node_id)
            || self.mobile_terminal_is_controlled(node_id)
            || self
                .mobile_runs
                .get(&node_id)
                .is_some_and(|run| run.native_input.has_controlled_draft())
    }

    /// Automatic task paste must not overwrite a human draft after control returns or expires.
    pub fn send_agent_task_text(&mut self, node_id: NodeId, text: &str) -> Result<(), NodeError> {
        if self.workspace_card_input_held(node_id)
            || self
                .mobile_runs
                .get(&node_id)
                .is_some_and(|run| run.native_input.has_controlled_draft())
        {
            return self.fail_node_operation(
                Some(node_id),
                "input",
                NodeError::Pty(terminal::PtyError::Io(std::io::Error::other(
                    "终端中有未提交的文字，请先处理原输入",
                ))),
            );
        }
        self.send_node_input(node_id, NodeInput::Paste(text.into()))
    }

    pub fn send_node_input(&mut self, node_id: NodeId, input: NodeInput) -> Result<(), NodeError> {
        if self.workspace_inputs.blocks_human(node_id) {
            return self.fail_node_operation(
                Some(node_id),
                "input",
                NodeError::Pty(terminal::PtyError::Io(std::io::Error::other(
                    "工作空间任务正在提交，请稍候输入",
                ))),
            );
        }
        self.track_native_agent(node_id);
        if self.mobile_terminal_is_controlled(node_id) {
            return self.fail_node_operation(
                Some(node_id),
                "input",
                NodeError::Pty(terminal::PtyError::Io(std::io::Error::other(
                    "终端输入已由 iPhone 接管，请先交还控制权",
                ))),
            );
        }
        let result = self.write_node_input(node_id, input.clone());
        self.workspace_inputs
            .observe_user_input(node_id, &input, result.is_ok());
        result
    }

    // Only the ordinary input guard and the validated exclusive-owner path may call this seam.
    pub(super) fn write_node_input(
        &mut self,
        node_id: NodeId,
        input: NodeInput,
    ) -> Result<(), NodeError> {
        let menu_navigation = matches!(&input, NodeInput::Key(key) if !key.ctrl && !key.alt && !key.shift
            && matches!(key.key, terminal::Key::Up | terminal::Key::Down | terminal::Key::Enter | terminal::Key::Escape))
            && self.native_menu_navigation(node_id);
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
            run.native_input.observe(&input, menu_navigation)
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
