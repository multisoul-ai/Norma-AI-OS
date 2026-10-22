use super::support::next_default_agent_name;
use super::AgentDefinitionsState;
use canvas::AgentProfile;

impl AgentDefinitionsState {
    /// 抽屉只在有可编辑对象时展示：已选中的 Agent，或正在新建的草稿。
    pub(crate) fn is_drawer_open(&self) -> bool {
        self.drawer_open && (self.creating_new || self.draft.id.is_some())
    }

    /// 关闭抽屉。新建草稿直接取消；已有 Agent 有未保存修改时拒绝关闭，避免静默丢稿。
    pub(crate) fn close_drawer(&mut self) -> bool {
        if self.creating_new {
            if self.new_draft_modified() {
                return self.ensure_clean_for_navigation();
            }
            return self.cancel_card_edit();
        }
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        self.cancel_edit();
        self.drawer_open = false;
        true
    }

    /// 新建草稿相对初始默认值是否被用户改动过（默认名称本身不算改动）。
    fn new_draft_modified(&self) -> bool {
        let draft = self.draft_with_pending_edit();
        draft.name != next_default_agent_name(&self.definitions)
            || !draft.workflow_description.is_empty()
            || draft.runtime_profile != AgentProfile::Codex
            || !draft.enabled
    }

    /// 卡片上的开关直接持久化启用状态；有未保存草稿时先要求保存或放弃。
    pub(crate) fn toggle_enabled_from_card(&mut self, id: i64) -> bool {
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        self.toggle_enabled(id)
    }
}
