use super::*;
use gpui::{div, prelude::*, px, relative, size, AppContext, TestAppContext};
use norma::protocol::TaskMessage;

struct ContentHost {
    app: Entity<MultisoulApp>,
    task: Task,
    text: String,
}

fn portrait_path(root: &std::path::Path) -> String {
    let directory = root.join(".mobile-images");
    std::fs::create_dir_all(&directory).expect("image directory");
    let path = directory.join(format!("{}.png", "a".repeat(64)));
    image::RgbaImage::from_pixel(120, 240, image::Rgba([240, 120, 40, 255]))
        .save(&path)
        .expect("portrait fixture");
    path.to_string_lossy().into_owned()
}

impl gpui::Render for ContentHost {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (list, composer) = self.app.update(cx, |app, cx| {
            app.ensure_staff_inputs(window, cx);
            (
                app.staff_message_list(&self.task, window, cx),
                app.staff_composer(window, cx),
            )
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .absolute()
                    .left(px(20.))
                    .right(px(20.))
                    .flex()
                    .justify_center()
                    .opacity(0.)
                    .child(
                        div().w_full().max_w(px(760.)).child(
                            div().w(relative(0.78)).px(px(16.)).child(
                                div()
                                    .text_size(px(14.))
                                    .line_height(relative(1.7))
                                    .debug_selector(|| "user-text-reference".into())
                                    .child(self.text.clone()),
                            ),
                        ),
                    ),
            )
            .child(list)
            .child(composer)
    }
}

/// 场景：单图/四图草稿在窄窗换行，预览不能缩成薄片或覆盖文字与按钮。
/// 数据构造：120×240竖图，104×78预览，最多四张；320/520/900px宽、400px高窗口。
/// 执行过程：渲染真实消息列表及composer，逐个读取图片、输入框和按钮边界。
/// 预期结果：每张图保留78px高度、位于输入框内且在文字输入区上方，按钮仍完整。
#[test]
fn staff_draft_images_reserve_space_inside_composer() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let path = portrait_path(dir.path());
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        ContentHost {
            app,
            task: Task::new("t", "w", "p", "布局"),
            text: String::new(),
        }
    });
    for count in [1, 4] {
        host.update(cx, |host, cx| {
            host.app.update(cx, |app, _| {
                app.staff.images = vec![path.clone(); count];
            });
        });
        for width in [320., 520., 900., 320.] {
            cx.simulate_resize(size(px(width), px(400.)));
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear());
            let composer = cx.debug_bounds("staff-composer").expect("composer");
            let send = cx.debug_bounds("staff-send").expect("send");
            let attach = cx.debug_bounds("staff-attach-image").expect("attach");
            let content = cx
                .debug_bounds("staff-preview-content-0")
                .expect("image element");
            assert_eq!(
                content.size.height,
                px(78.),
                "图片元素须使用预览高度，不能按原图比例长到208px"
            );
            for index in 0..count {
                let image = cx
                    .debug_bounds(
                        [
                            "staff-image-0",
                            "staff-image-1",
                            "staff-image-2",
                            "staff-image-3",
                        ][index],
                    )
                    .expect("preview");
                assert!(image.size.height >= px(78.), "预览不能被压缩：{image:?}");
                assert!(
                    image.left() >= composer.left() && image.right() <= composer.right(),
                    "图片须在输入框横向范围内：{image:?} / {composer:?}"
                );
                assert!(
                    image.top() >= composer.top() && image.bottom() + px(34. + 8.) <= attach.top(),
                    "预览下方须留34px输入行及8px按钮间隙：{image:?} / {attach:?}"
                );
            }
            assert!(
                send.bottom() <= composer.bottom() && attach.bottom() <= composer.bottom(),
                "两个按钮须完整留在输入框内"
            );
            assert!(
                send.size.height >= px(34.) && attach.size.height >= px(32.),
                "按钮不能被压缩"
            );
        }
    }
}

/// 场景：用户长文字及四图消息发送后必须自然撑高气泡，而不是溢出消息行。
/// 数据构造：长中英文及无空格URL、纯文字/四图/纯图消息；320/520/900/1400px宽窗口。
/// 执行过程：渲染生产列表，以同宽原生文本的自然高度作独立基准，并核对所有图片边界。
/// 预期结果：文本完整，四图各160px不压缩，间隙8px、上下内边距共24px；消息互不覆盖。
#[test]
fn staff_user_bubbles_contain_wrapped_text_and_images() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let text = format!(
        "{}https://example.com/{}",
        "再开发一个坦克，完整保留用户输入 TANK。".repeat(6),
        "unbroken".repeat(20)
    );
    let images = vec![portrait_path(dir.path()); 4];
    let mut task = Task::new("t", "w", "p", "布局");
    task.messages.push(TaskMessage::User { text: text.clone() });
    task.messages.push(TaskMessage::User {
        text: norma::protocol::compose_image_message(&text, &images).expect("image message"),
    });
    task.messages.push(TaskMessage::User {
        text: norma::protocol::compose_image_message("", &images[..1]).expect("image-only message"),
    });
    task.messages.push(TaskMessage::User {
        text: "再开发一个坦克".into(),
    });
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        ContentHost { app, task, text }
    });
    for width in [320., 520., 900., 1400., 320.] {
        cx.simulate_resize(size(px(width), px(400.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let reference = cx
            .debug_bounds("user-text-reference")
            .expect("natural text");
        for index in 0..2 {
            let bubble = cx
                .debug_bounds(["staff-user-bubble-0", "staff-user-bubble-1"][index])
                .expect("bubble");
            let row = cx
                .debug_bounds(["staff-message-0", "staff-message-1"][index])
                .expect("message");
            let next = cx
                .debug_bounds(["staff-message-1", "staff-message-2"][index])
                .expect("next message");
            let image_height = if index == 1 { 4. * (160. + 8.) } else { 0. };
            let text_bounds = cx
                .debug_bounds(["staff-user-text-0", "staff-user-text-1"][index])
                .expect("user text");
            assert!(
                (text_bounds.size.width - reference.size.width).abs() < px(0.5),
                "正文与自然文字基准须同宽：{text_bounds:?} / {reference:?}"
            );
            assert!(
                bubble.size.height + px(0.5) >= reference.size.height + px(24. + image_height),
                "气泡应包住自然换行文字及图片：{bubble:?} / 正文{text_bounds:?} / 基准{reference:?}"
            );
            assert!(
                bubble.bottom() <= row.bottom() + px(0.5),
                "气泡不能溢出消息行：{bubble:?} / {row:?}"
            );
            assert!(
                next.top() >= bubble.bottom() + px(23.5),
                "下一消息须保留24px间距"
            );
            if index == 1 {
                let content = cx
                    .debug_bounds("staff-user-image-content-1-0")
                    .expect("sent image content");
                assert_eq!(
                    content.size.height,
                    px(160.),
                    "发送图片元素必须限制在160px显示区域"
                );
                for image_index in 0..4 {
                    let image = cx
                        .debug_bounds(
                            [
                                "staff-user-image-1-0",
                                "staff-user-image-1-1",
                                "staff-user-image-1-2",
                                "staff-user-image-1-3",
                            ][image_index],
                        )
                        .expect("sent image");
                    assert!(
                        image.size.height >= px(160.),
                        "已发送图片不能被压缩：{image:?}"
                    );
                    assert!(
                        image.left() >= bubble.left() + px(15.5)
                            && image.right() <= bubble.right() - px(15.5)
                            && image.bottom() <= bubble.bottom() - px(11.5),
                        "图片须在气泡内容区内：{image:?} / {bubble:?}"
                    );
                }
            }
        }
        let image_only = cx
            .debug_bounds("staff-user-bubble-2")
            .expect("image-only bubble");
        let image = cx
            .debug_bounds("staff-user-image-2-0")
            .expect("image-only content");
        assert!(
            image_only.size.height >= px(184.),
            "纯图片气泡须包含160px图片与24px上下内边距"
        );
        assert!(
            image.right() <= image_only.right() - px(15.5)
                && image.bottom() <= image_only.bottom() - px(11.5),
            "纯图片消息也须限制在气泡内容区"
        );
        let short = cx
            .debug_bounds("staff-user-bubble-3")
            .expect("short bubble");
        let row = cx.debug_bounds("staff-message-3").expect("short row");
        assert!(
            short.size.width < row.size.width * 0.78,
            "短消息保持自然宽度，不拉成满宽气泡"
        );
    }
}
