//! 桌面宿主窗口持有键盘焦点时，把语音快捷键转交给展开的项目会话。
use super::*;

impl MultisoulApp {
    /// 展开项目才是语音的承载者；宿主焦点（顶栏、空白处、Dock）不得吞掉快捷键。
    pub(super) fn forward_voice_key_down_to_expanded_project(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.expanded_desktop_project() else {
            return false;
        };
        project.update(cx, |project, cx| {
            project.handle_configured_voice_key_down(keystroke, window, cx)
        })
    }

    pub(super) fn forward_voice_key_up_to_expanded_project(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.expanded_desktop_project() else {
            return false;
        };
        project.update(cx, |project, cx| {
            project.handle_configured_voice_hold_release(keystroke, window, cx)
        })
    }

    pub(super) fn forward_voice_modifiers_to_expanded_project(
        &mut self,
        modifiers: &Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.expanded_desktop_project() {
            project.update(cx, |project, cx| {
                project.handle_configured_voice_modifier_release(modifiers, window, cx)
            });
        }
    }
}
