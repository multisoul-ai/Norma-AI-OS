//! 大脑自己派给 Agent 的任务。这些不是用户的意图，不能当成目标记下来。

use std::collections::VecDeque;

const MAX_REMEMBERED: usize = 10;

#[derive(Debug, Default)]
pub(crate) struct OwnPrompts {
    texts: VecDeque<String>,
}

impl OwnPrompts {
    pub(crate) fn remember(&mut self, task: &str) {
        self.texts.push_back(task.trim().to_string());
        while self.texts.len() > MAX_REMEMBERED {
            self.texts.pop_front();
        }
    }

    pub(crate) fn contains(&self, text: &str) -> bool {
        let text = text.trim();
        self.texts.iter().any(|own| own == text)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_own_prompts_tests.rs"]
mod brain_own_prompts_tests;
