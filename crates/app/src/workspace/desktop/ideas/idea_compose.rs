//! 想法输入区：多行输入（回车添加、⇧回车换行）、粘贴图片、待添加图片的缩略图。
//! 图片文件与写库都在后台执行器上完成；前台只改状态并重绘。

use crate::app::clipboard_image::{clipboard_image_extension, read_clipboard_image_first};
use crate::app::MultisoulApp;
use gpui::{div, img, prelude::*, px, rgba, Context, Div, ElementId, Window};
use storage::IdeasStore;
use ui::component::input::{InputEvent, InputState};
use workspace::ideas::{IdeaError, MAX_IDEA_IMAGES};

impl MultisoulApp {
    /// 想法清单顶部的输入框：回车或点“添加”记一条手动想法，⇧回车换行。
    /// 创建需要窗口，所以在渲染前惰性建立。
    pub(in crate::workspace::desktop) fn ensure_idea_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.ideas_input.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 5)
                .placeholder("写下一个想法，回车添加，⇧回车换行；可粘贴图片和链接")
        });
        // 多行输入框会先插入换行再发出 PressEnter：添加时正文会被去掉首尾空白，换行不会残留。
        let subscription =
            cx.subscribe_in(&input, window, |app, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    app.add_idea_from_input(window, cx);
                }
            });
        self.desktop_workspace.ideas_input = Some(input);
        self.desktop_ideas_input_subscription = Some(subscription);
    }

    /// ⇧回车：在光标处插入换行（普通回车保留给“添加”）。
    pub(in crate::workspace::desktop) fn insert_idea_newline(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.desktop_workspace.ideas_input.clone() {
            input.update(cx, |state, cx| state.insert("\n", window, cx));
        }
    }

    /// 把输入框里的文字和已粘贴的图片记成一条手动想法：写库在后台，完成后刷新清单；超长或为空给出提示。
    pub(in crate::workspace::desktop) fn add_idea_from_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(input), Some(workspace_id)) = (
            self.desktop_workspace.ideas_input.clone(),
            self.desktop_workspace.active_id.clone(),
        ) else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let images = self.desktop_workspace.ideas_aux.pending_images.clone();
        let body = match workspace::ideas::normalize_body_with_images(&text, images.len()) {
            Ok(body) => body,
            Err(IdeaError::Empty) => {
                // 回车已经插进了换行：纯空白的输入清回空，避免残留空行。
                input.update(cx, |state, cx| state.set_value("", window, cx));
                return;
            }
            Err(_) => {
                self.desktop_workspace.ideas_notice = Some("想法太长了，最多 500 个字".to_owned());
                cx.notify();
                return;
            }
        };
        input.update(cx, |state, cx| state.set_value("", window, cx));
        self.desktop_workspace.ideas_aux.pending_images.clear();
        self.desktop_workspace.ideas_notice = None;
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move {
                    store.capture_with_images(
                        &workspace_id,
                        &body,
                        storage::IdeaSource::Manual,
                        &images,
                    )
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if let Err(error) = saved {
                    app.desktop_workspace.ideas_notice = Some(format!("没能记下：{error}"));
                }
                app.refresh_ideas(cx);
            });
        })
        .detach();
    }

    /// 剪贴板里只有图片（没有文字）时，把图片存下来作为待添加附件，返回是否已接管这次粘贴。
    pub(in crate::workspace::desktop) fn paste_idea_image(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let item = cx.read_from_clipboard();
        if item
            .as_ref()
            .and_then(|item| item.text())
            .is_some_and(|text| !text.trim().is_empty())
        {
            return false;
        }
        let Some(image) = read_clipboard_image_first(item.as_ref()) else {
            return false;
        };
        let aux = &self.desktop_workspace.ideas_aux;
        if aux.pending_images.len() >= MAX_IDEA_IMAGES {
            self.desktop_workspace.ideas_notice =
                Some(format!("一条想法最多附 {MAX_IDEA_IMAGES} 张图片"));
            cx.notify();
            return true;
        }
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        let extension = clipboard_image_extension(image.format());
        let bytes = image.bytes().to_vec();
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { store.save_image(&bytes, extension) })
                .await;
            let _ = this.update(cx, |app, cx| {
                match saved {
                    Ok(name) => app.desktop_workspace.ideas_aux.pending_images.push(name),
                    Err(error) => {
                        app.desktop_workspace.ideas_notice = Some(format!("没能保存图片：{error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        true
    }

    pub(in crate::workspace::desktop) fn remove_pending_idea_image(
        &mut self,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace
            .ideas_aux
            .pending_images
            .retain(|pending| pending != name);
        // 还没随想法保存的图片，移除即删文件。
        IdeasStore::new(self.database_paths.app_db_path().to_owned()).discard_image(name);
        cx.notify();
    }

    /// 输入框上方的待添加图片缩略图，每张右上角有移除按钮；没有图片时不占位。
    pub(in crate::workspace::desktop) fn render_pending_idea_images(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let pending = &self.desktop_workspace.ideas_aux.pending_images;
        if pending.is_empty() {
            return None;
        }
        let store = IdeasStore::new(self.database_paths.app_db_path().to_owned());
        Some(
            div()
                .mx(px(8.0))
                .mt(px(4.0))
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .children(pending.iter().filter_map(|name| {
                    let path = store.image_path(name).ok()?;
                    let remove = name.clone();
                    Some(
                        div()
                            .id(ElementId::Name(format!("idea-pending-image-{name}").into()))
                            .relative()
                            .w(px(48.0))
                            .h(px(48.0))
                            .rounded(px(7.0))
                            .overflow_hidden()
                            .border_1()
                            .border_color(rgba(0x83c4ff40))
                            .child(
                                img(path)
                                    .w(px(48.0))
                                    .h(px(48.0))
                                    .object_fit(gpui::ObjectFit::Cover),
                            )
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("idea-pending-remove-{name}").into(),
                                    ))
                                    .absolute()
                                    .top(px(2.0))
                                    .right(px(2.0))
                                    .w(px(16.0))
                                    .h(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(rgba(0x000000a0))
                                    .text_size(px(10.0))
                                    .text_color(rgba(0xffffffff))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgba(0x000000d8)))
                                    .active(|style| style.bg(rgba(0x000000ff)))
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        cx.stop_propagation();
                                        app.remove_pending_idea_image(&remove, cx);
                                    }))
                                    .child("×"),
                            ),
                    )
                })),
        )
    }
}
