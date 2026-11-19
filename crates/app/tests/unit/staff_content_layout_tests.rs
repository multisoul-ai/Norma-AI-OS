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

pub(super) struct ConversationColumnHost {
    pub(super) app: Entity<MultisoulApp>,
}

impl gpui::Render for ConversationColumnHost {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let conversation = self.app.update(cx, |app, cx| {
            app.render_staff_conversation("infcode", window, cx)
        });
        div()
            .size_full()
            .flex()
            .p(px(12.))
            .gap(px(12.))
            .child(div().flex_none().w(px(280.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(conversation),
            )
    }
}

pub(super) fn done_reply(text: &str) -> TaskMessage {
    let mut reply = norma::protocol::StaffReply::new(0, norma::protocol::ReplyTrigger::User);
    reply.push_text(text);
    reply.finish(norma::protocol::ReplyStatus::Done, 1_000);
    TaskMessage::Staff(reply)
}

fn long_markdown() -> String {
    let item =
        "两个 demo 共用这一套语义，只是传输不同。ACP 给编辑器，HTTP SDK 给自己写代码来调的人。";
    format!(
        "待你确认的三点：\n\n- {item}\n- {item}\n- {item}\n\n记忆、多 Agent、人机确认这期先不做。你要是觉得哪条不对，直接说，我们改完再排两个 demo。\n\n```\nRuntimeBackend\n一期：对接层自己把 Runtime 拉起来\n二期：换成进程内的 Agent_core\n```\n"
    )
}

/// 场景：首条带 URL 的用户消息完整出现在首屏，且不把后面的回复顶出一大段空白；
/// 回复正文不压到下一条署名；贴底后最后一段停在输入框上方。
///
/// 数据构造：
///   窗口 1600×900，左侧占位 280，对话列宽于阅读宽 760，消息列因此是 760
///   用户消息 89 字且含 https，走 Markdown 气泡，上限为列宽的 78%（≈593），
///   14×1.7 行高下至少两行（正文 ≥ 47.6），再加上下内边距 24，气泡约 72
///   回复 A、B 同文：三段列表 + 一段正文 + 代码块，完成后走 Markdown
///
/// 执行过程：
///   1. 渲染生产对话列（标题、滚动消息、composer），滚动偏移为 0
///   2. 读取标题、视口、气泡、消息行和两条回复的边界
///   3. scroll_to_bottom 后重绘，读取视口、最后一段正文和输入框
///
/// 预期结果：
///   - 消息行高等于气泡高（不能是按最小内容宽度折出的上千像素，也不能矮于气泡）
///   - 气泡右缘贴齐消息列，宽度等于列宽的 78%，不是整列也不是超出列宽
///   - 下一条顶 = 气泡底 + 列表间距 24，中间没有空白洞
///   - 首条气泡完整落在视口内；视口夹在标题和输入框之间
///   - 回复正文落在本条消息内，且底边低于下一条顶至少 20
///   - 贴底后最后一段正文和消息底都不越过视口底，视口底不超过输入框顶
#[test]
fn staff_conversation_keeps_first_message_and_replies_uncovered() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let user = "我们来聊聊 InfCode SDK 的设计，之前的 https://tokfinity.feishu.cn/docx/Iabc 需要把会话边界收成能往下写的一版，调用方不拉进程。".to_owned();
    let reply_a = long_markdown();
    let reply_b = long_markdown();
    let mut task = Task::new("t", "w", "p", "布局");
    task.messages.push(TaskMessage::User { text: user });
    task.messages.push(done_reply(&reply_a));
    task.messages.push(done_reply(&reply_b));
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        app.update(cx, |app, _| {
            app.staff.loaded = true;
            app.staff.selected = Some(task.id.clone());
            app.staff.tasks.insert(task.id.clone(), task.clone());
        });
        ConversationColumnHost { app }
    });
    cx.simulate_resize(size(px(1600.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());

    let header = cx
        .debug_bounds("staff-conversation-header")
        .expect("header");
    let viewport = cx.debug_bounds("staff-messages").expect("message viewport");
    let composer = cx.debug_bounds("staff-composer").expect("composer");
    let bubble = cx
        .debug_bounds("staff-user-bubble-0")
        .expect("first user bubble");
    let first = cx.debug_bounds("staff-message-0").expect("first message");
    let first_body = cx
        .debug_bounds("staff-message-body-0")
        .expect("first message body");
    let user_row = cx.debug_bounds("staff-user-row-0").expect("user row");
    let second = cx.debug_bounds("staff-message-1").expect("first reply");
    let third = cx.debug_bounds("staff-message-2").expect("second reply");
    let reply_text = cx
        .debug_bounds("staff-reply-text-1-0")
        .expect("first reply text");
    let user_text = cx.debug_bounds("staff-user-text-0").expect("user text");
    let actions = cx
        .debug_bounds("staff-reply-actions-1")
        .expect("reply actions");

    assert!(
        (first.size.height - bubble.size.height).abs() < px(0.5)
            && (first_body.size.height - bubble.size.height).abs() < px(0.5)
            && (user_row.size.height - bubble.size.height).abs() < px(0.5),
        "消息行必须刚好包住气泡，不能按最小内容高度撑开，也不能矮于气泡：行 {first:?} / 体 {first_body:?} / 用户行 {user_row:?} / 气泡 {bubble:?}"
    );
    assert!(
        (bubble.right() - first.right()).abs() < px(0.5),
        "用户气泡应靠右对齐到消息列：气泡 {bubble:?} / 列 {first:?}"
    );
    assert!(
        (bubble.size.width - first.size.width * 0.78).abs() < px(1.),
        "长消息气泡宽度应为列宽的 78%，不能拉满整列也不能超出列宽：气泡 {bubble:?} / 列 {first:?}"
    );
    assert!(
        user_text.left() + px(0.5) >= bubble.left()
            && user_text.right() <= bubble.right() + px(0.5)
            && user_text.top() + px(0.5) >= bubble.top()
            && user_text.bottom() <= bubble.bottom() + px(0.5),
        "用户正文必须落在气泡内：正文 {user_text:?} / 气泡 {bubble:?}"
    );
    assert!(
        user_text.size.height >= px(47.),
        "89 字在 78% 列宽内至少折成两行，不能挤成一行：{user_text:?}"
    );
    assert!(
        (second.top() - bubble.bottom() - px(24.)).abs() < px(0.5),
        "下一条应紧跟气泡，间距就是列表的 24px，中间不能有空白：气泡 {bubble:?} / 下一条 {second:?}"
    );
    assert!(
        reply_text.top() >= second.top() + px(36.)
            && reply_text.bottom() <= actions.top() + px(0.5)
            && actions.bottom() <= second.bottom() + px(0.5),
        "回复正文应在署名之下、操作行之上，不能压住头像：正文 {reply_text:?} / 操作 {actions:?} / 消息 {second:?}"
    );
    assert!(
        reply_text.size.height >= px(80.),
        "列表加代码块的回复不能塌成一行：{reply_text:?}"
    );
    assert!(
        third.top() + px(0.5) >= reply_text.bottom() + px(20.),
        "回复正文与下一条署名至少留出间距：正文 {reply_text:?} / 下一条 {third:?}"
    );
    assert!(
        viewport.top() + px(0.5) >= header.bottom(),
        "消息视口不能伸进标题栏：视口 {viewport:?} / 标题 {header:?}"
    );
    assert!(
        viewport.bottom() <= composer.top() + px(0.5),
        "消息视口不能伸进输入框：视口 {viewport:?} / 输入框 {composer:?}"
    );
    assert!(
        bubble.top() + px(0.5) >= viewport.top() && bubble.bottom() <= viewport.bottom() + px(0.5),
        "首条用户消息应完整出现在首屏视口内：气泡 {bubble:?} / 视口 {viewport:?}"
    );
    assert!(
        second.bottom() <= third.top() + px(0.5),
        "两条回复重叠：{second:?} / {third:?}"
    );

    host.update(cx, |host, cx| {
        host.app
            .update(cx, |app, _| app.staff.scroll.scroll_to_bottom());
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx.debug_bounds("staff-messages").expect("viewport");
    let composer = cx.debug_bounds("staff-composer").expect("composer");
    let third = cx.debug_bounds("staff-message-2").expect("last reply");
    let last_text = cx
        .debug_bounds("staff-reply-text-2-0")
        .expect("last reply text");
    assert!(
        viewport.bottom() <= composer.top() + px(0.5),
        "贴底后视口仍不能压住输入框：视口 {viewport:?} / 输入框 {composer:?}"
    );
    assert!(
        third.bottom() <= viewport.bottom() + px(0.5)
            && last_text.bottom() <= viewport.bottom() + px(0.5)
            && last_text.top() >= viewport.top(),
        "贴底后最后一段应完整落在视口内，不能被输入框挡住：正文 {last_text:?} / 消息 {third:?} / 视口 {viewport:?}"
    );
}
