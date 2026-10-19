use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, Context, Div, Stateful, Window};
use ui::component::{
    button::Button,
    input::{Input, InputState},
    Sizable, StyledExt,
};

impl MultisoulApp {
    pub(super) fn render_content_home(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self.desktop_workspace.content_browser.input.is_none()
            || self.desktop_workspace.content_browser.tab_id.is_some()
        {
            self.desktop_workspace.content_browser.input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("搜索或输入网址")));
            self.desktop_workspace.content_browser.tab_id = None;
        }
        let input = self
            .desktop_workspace
            .content_browser
            .input
            .as_ref()
            .expect("home input");
        div()
            .id("desktop-content-home")
            .debug_selector(|| "desktop-content-home".into())
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(20.0))
            .child(
                div()
                    .h(px(32.0))
                    .flex_none()
                    .child(Input::new(input))
                    .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            app.open_desktop_url_from_input(cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(
                Button::new("desktop-home-open-url")
                    .label("打开网页")
                    .small()
                    .on_click(cx.listener(|app, _, _, cx| app.open_desktop_url_from_input(cx))),
            )
            .child(div().font_semibold().child("工具"))
            .child(self.render_content_tool_buttons(cx))
            .child(
                div()
                    .text_size(px(12.0))
                    .child("查看项目文件与变更，浏览网页，预览图片、视频和文档。"),
            )
    }
}
