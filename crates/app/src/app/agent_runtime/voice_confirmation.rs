use agent::AdapterCommand;

#[derive(Default)]
pub(super) struct PendingVoiceConfirmation {
    turn: u64,
    actions: Vec<AdapterCommand>,
}

impl PendingVoiceConfirmation {
    pub(super) fn authorize(
        &mut self,
        command: &AdapterCommand,
        transcript: &str,
        turn_id: &str,
    ) -> bool {
        let turn = turn_id.parse::<u64>().unwrap_or(0);
        if agent::realtime_command_authorized(command, transcript) {
            self.actions.clear();
            self.turn = turn;
            return true;
        }
        let confirmation = transcript
            .trim()
            .trim_end_matches(['。', '！', '!', '.', '，', ',']);
        if matches!(confirmation, "可以" | "继续") {
            let allowed = self.turn.checked_add(1) == Some(turn)
                && self.actions.len() == 1
                && self.actions[0] == *command;
            self.actions.clear();
            return allowed;
        }
        if turn == 0 || transcript.trim().is_empty() {
            return false;
        }
        if self.turn != turn {
            self.turn = turn;
            self.actions.clear();
        }
        if !self.actions.contains(command) {
            self.actions.push(command.clone());
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VC-1：只有紧邻上一轮的唯一待确认动作可由“可以”授权。
    ///
    /// 数据构造（含关键数值的推导过程）：回合 1 的讨论产生一个 Terminal 候选；
    /// 回合 2 = 1 + 1，属于紧邻确认；回合 4 与回合 2 相差 2，已过期。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 讨论式提问提出 Terminal → 暂存且不执行。
    ///   2. 下一轮“可以”且工具目标相同 → 授权一次。
    ///   3. 重复确认或跨轮确认 → 拒绝；同轮两个候选后确认也拒绝。
    ///
    /// 预期结果：只有唯一、紧邻、同目标的确认生效；重复或含糊确认不执行。
    #[test]
    fn unique_adjacent_confirmation_only() {
        let mut gate = PendingVoiceConfirmation::default();
        let terminal = AdapterCommand::CreateTerminal;
        let browser = AdapterCommand::CreateBrowser { url: None };
        assert!(
            !gate.authorize(&terminal, "可以创建终端吗？", "1"),
            "讨论问题不能直接执行"
        );
        assert!(
            gate.authorize(&terminal, "可以", "2"),
            "下一轮唯一目标应可确认"
        );
        assert!(
            !gate.authorize(&terminal, "可以", "3"),
            "一次确认不能重复授权"
        );
        assert!(
            !gate.authorize(&terminal, "可以创建终端吗？", "4"),
            "新讨论仍不能直接执行"
        );
        assert!(
            !gate.authorize(&browser, "可以打开浏览器吗？", "4"),
            "同轮第二个候选也不能直接执行"
        );
        assert!(
            !gate.authorize(&terminal, "可以", "5"),
            "两个候选后的确认目标不唯一"
        );
        assert!(
            !gate.authorize(&terminal, "可以创建终端吗？", "6"),
            "讨论应重新建立待确认候选"
        );
        assert!(
            !gate.authorize(&terminal, "可以", "8"),
            "隔了一个回合的旧候选必须失效"
        );
    }
}
