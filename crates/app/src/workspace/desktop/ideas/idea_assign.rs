//! 想法派发快捷键（`⌃1…9`、`⌃0`、`⌃⇧2…9`）：先由纯函数决定“派哪条、派给谁”，再由宿主执行。
//! 目标范围与拖拽一致：当前展开项目里的智能体卡片，序号按文档顺序（与 reflow 平铺的阅读顺序一致）。

use super::{IdeaTargetOption, IdeaUiTarget};
use crate::app::MultisoulApp;
use crate::shortcuts::{idea_assign::new_agent_profile, ShortcutCommandId};
use gpui::{Context, Window};
use norma::protocol::AgentProfile;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_idea_assign_tests.rs"]
mod tests;

/// `ShortcutCommandId::AssignIdeaToNewAgent` 里代表类型选择器的槽位（`⌃⇧0`）。
const PICKER_SLOT: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AssignRequest {
    /// 第 N 张已有卡片（从 1 开始）。
    Card(u8),
    /// 新建智能体；槽位对应类型，0 或未绑定槽位用默认类型。
    NewAgent(u8),
    /// 打开类型选择器（`⌃⇧0`）。
    Picker,
    /// 选择器里选定的类型（含没有数字键的类型）。
    Profile(AgentProfile),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AssignPlan {
    Dispatch {
        idea_id: String,
        target: IdeaUiTarget,
    },
    /// 校验通过，打开类型选择器。
    OpenPicker,
    /// 无法执行：把原因告诉用户，不静默忽略。
    Notice(String),
}

/// 光标落在待办行就派该行，否则（无光标、光标在已指派行）派待办队首。
pub(super) fn plan_assignment(
    request: AssignRequest,
    todo_ids: &[String],
    cursor: Option<usize>,
    options: &[IdeaTargetOption],
    expanded_area: Option<&str>,
    default_profile: AgentProfile,
) -> AssignPlan {
    let Some(idea_id) = cursor
        .and_then(|index| todo_ids.get(index))
        .or_else(|| todo_ids.first())
    else {
        return AssignPlan::Notice("没有待办想法可派发".to_owned());
    };
    let Some(area_id) = expanded_area else {
        return AssignPlan::Notice("先展开一个项目，再派发想法".to_owned());
    };
    let target = match request {
        AssignRequest::Picker => return AssignPlan::OpenPicker,
        AssignRequest::Profile(profile) => IdeaUiTarget::New {
            area_id: area_id.to_owned(),
            profile,
        },
        AssignRequest::Card(number) => {
            let card = options
                .iter()
                .filter(|option| option.area_id == area_id)
                .nth(usize::from(number).saturating_sub(1))
                .filter(|_| number >= 1);
            match card {
                Some(option) => IdeaUiTarget::Existing {
                    area_id: option.area_id.clone(),
                    node_id: option.node_id,
                },
                None => {
                    return AssignPlan::Notice(format!("当前项目没有第 {number} 张智能体卡片"));
                }
            }
        }
        AssignRequest::NewAgent(slot) => IdeaUiTarget::New {
            area_id: area_id.to_owned(),
            profile: new_agent_profile(slot).unwrap_or(default_profile),
        },
    };
    AssignPlan::Dispatch {
        idea_id: idea_id.clone(),
        target,
    }
}

pub(super) fn request_for(command: ShortcutCommandId) -> Option<AssignRequest> {
    match command {
        ShortcutCommandId::AssignIdeaToCard(number) => Some(AssignRequest::Card(number)),
        ShortcutCommandId::AssignIdeaToNewAgent(slot) if slot == PICKER_SLOT => {
            Some(AssignRequest::Picker)
        }
        ShortcutCommandId::AssignIdeaToNewAgent(slot) => Some(AssignRequest::NewAgent(slot)),
        _ => None,
    }
}

impl MultisoulApp {
    /// 快捷键入口。项目画布是嵌入实例，想法清单属于桌面外壳，落在项目上时交给宿主。
    pub(crate) fn assign_idea_by_shortcut(
        &mut self,
        command: ShortcutCommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |app, cx| {
                app.assign_idea_by_shortcut(command, window, cx)
            });
            return;
        }
        let Some(request) = request_for(command) else {
            return;
        };
        self.run_assign_request(request, window, cx);
    }

    /// 派发想法的统一入口：快捷键与类型选择器都走这里。
    pub(super) fn run_assign_request(
        &mut self,
        request: AssignRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, crate::app::AppScreen::DesktopWorkspace) {
            return;
        }
        let screen = self.ideas_screen(cx);
        let todo_ids: Vec<String> = screen.todo.iter().map(|row| row.id.clone()).collect();
        let options = self.idea_target_options(cx);
        let plan = plan_assignment(
            request,
            &todo_ids,
            self.desktop_workspace.ideas_cursor,
            &options,
            self.desktop_workspace.expanded_area_id.as_deref(),
            self.desktop_workspace.ideas_aux.default_profile,
        );
        match plan {
            AssignPlan::Dispatch { idea_id, target } => {
                self.dispatch_idea(idea_id, target, None, window, cx);
            }
            AssignPlan::OpenPicker => self.open_idea_picker(window, cx),
            AssignPlan::Notice(text) => {
                // 清单可能没显示：展开想法栏，让提示可见（不抢焦点，终端里的输入不被打断）。
                self.desktop_workspace.ideas_notice = Some(text);
                self.reveal_ideas_group(cx);
                cx.notify();
            }
        }
    }
}
