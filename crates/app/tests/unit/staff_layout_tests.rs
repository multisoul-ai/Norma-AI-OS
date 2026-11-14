use super::*;
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：Markdown行内文字先按零宽测量，随后flex查询自然宽度。
/// 数据构造：混排正文“产物：STAFF-RESULT.md”；自然宽度与0px测量交替。
/// 执行过程：通过StyledText真实Element布局依次测量自然、零宽、自然尺寸。
/// 预期结果：窄列高度增加，恢复自然测量后尺寸等于初始值，不复用换行缓存。
#[test]
fn staff_markdown_text_recovers_intrinsic_size() {
    use gpui::{AvailableSpace, Element, StyledText};
    let mut test = TestAppContext::single();
    let cx = test.add_empty_window();
    cx.draw(
        gpui::point(px(0.), px(0.)),
        size(px(600.), px(400.)),
        |window, cx| {
            let mut text = StyledText::new("产物：STAFF-RESULT.md");
            let mut measure = |width| {
                let (id, _) = text.request_layout(None, None, window, cx);
                window.compute_layout(id, size(width, AvailableSpace::MaxContent), cx);
                window.layout_bounds(id).size
            };
            let natural = measure(AvailableSpace::MaxContent);
            let narrow = measure(AvailableSpace::Definite(px(0.)));
            assert!(
                narrow.height > natural.height,
                "零宽应实际触发换行：{natural:?} → {narrow:?}"
            );
            let restored = measure(AvailableSpace::MaxContent);
            assert_eq!(restored, natural, "自然宽度必须重新测量，不能沿用窄列缓存");
            assert_ne!(restored, narrow, "不能保持零宽换行尺寸");
            gpui::div()
        },
    );
}

/// 场景：幕僚模式拖宽右栏后仍保留任务栏与可用对话区，不修改画布的宽度偏好。
///
/// 数据构造：1280px 窗口，用户右栏宽960=1280−旧画布最小320；任务栏280px。
/// 执行过程：打开幕僚并读取生产布局 → 切回画布再读取 → 收起右栏。
/// 预期结果：幕僚中央至少320px，即总内容≥280+320+36=636（外边距24+栏间距12）；画布仍用960px；收起不占空间。
#[test]
fn staff_drawer_keeps_conversation_visible_with_wide_saved_width() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    cx.simulate_resize(size(px(1280.), px(800.)));
    source.update_in(cx, |app, window, _| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.staff.enabled = true;
        app.desktop_workspace.drawers.right_open = true;
        app.desktop_workspace.drawer_widths.right = Some(960.);
        let staff = app.desktop_assistant_layout(window);
        assert!(
            staff.canvas_width >= 636.,
            "扣除280px任务栏和36px留白后必须保留至少320px对话，实际总宽{}",
            staff.canvas_width
        );
        assert!(
            staff.right_width < 960.,
            "幕僚不能照搬会挤没对话的画布右栏宽度"
        );
        assert_eq!(
            app.desktop_workspace.drawer_widths.right,
            Some(960.),
            "适配布局不能覆盖用户原宽度"
        );
        app.staff.enabled = false;
        assert_eq!(
            app.desktop_assistant_layout(window).right_width,
            960.,
            "切回画布恢复用户宽度"
        );
        app.staff.enabled = true;
        app.desktop_workspace.drawers.right_open = false;
        assert_eq!(
            app.desktop_assistant_layout(window).right_space,
            0.,
            "收起右栏不保留空白"
        );
    });
}

/// 场景：原型窄窗折叠辅助栏，使任务对话始终可见。
/// 数据构造：760px窗口原三栏会仅剩760−280−185=295px；600px时应隐藏任务列表。
/// 执行过程：保持右抽屉打开与记忆宽960，依次缩窗760、600，读取生产布局。
/// 预期结果：760px隐藏右栏占位且对话≥320px；600px左栏宽0，整窗用于任务对话。
#[test]
fn staff_narrow_window_prioritizes_conversation() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    cx.simulate_resize(size(px(760.), px(800.)));
    source.update_in(cx, |app, window, _| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.staff.enabled = true;
        app.desktop_workspace.drawers.right_open = true;
        app.desktop_workspace.drawer_widths.right = Some(960.);
        let layout = app.desktop_assistant_layout(window);
        assert_eq!(layout.right_space, 0., "窄窗应按原型隐藏右抽屉占位");
        assert!(
            layout.canvas_width - sidebar_width(window) >= 320.,
            "窄窗不能挤没对话"
        );
        assert!(
            app.desktop_workspace.drawers.right_open,
            "响应式隐藏不能改用户展开偏好"
        );
    });
    cx.simulate_resize(size(px(600.), px(800.)));
    source.update_in(cx, |app, window, _| {
        assert_eq!(sidebar_width(window), 0., "小于760px应隐藏任务列表");
        assert_eq!(
            app.desktop_assistant_layout(window).canvas_width,
            600.,
            "600px时整窗给任务对话"
        );
    });
}

/// 场景：编辑提议时保留尚未发送的用户草稿。
/// 数据构造：两行用户草稿 + 一条Worker提议；0条已发送消息。
/// 执行过程：创建生产输入框，写入草稿，调用提议按钮实际共用操作。
/// 预期结果：原两行仍完整存在，提议追加到末尾；不能仅剩提议，不能创建任务或发送消息。
#[test]
fn staff_proposal_edit_preserves_unsent_draft() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update_in(cx, |app, window, cx| {
        app.ensure_staff_inputs(window, cx);
        let input = app.staff.input.clone().expect("composer initialized");
        input.update(cx, |input, cx| {
            input.set_value("用户第一行\n用户第二行", window, cx)
        });
        app.edit_staff_proposal("Worker提议", window, cx);
        let value = input.read(cx).value().to_string();
        assert_eq!(
            value, "用户第一行\n用户第二行\n\nWorker提议",
            "编辑提议应保留用户两行草稿并追加提议"
        );
        assert_ne!(value, "Worker提议", "不得覆盖用户未发送内容");
        assert!(app.staff.tasks.is_empty(), "编辑只填草稿，不创建任务");
        assert!(app.staff.error.is_none(), "编辑草稿不应触发发送失败");
    });
}

pub(super) struct MessageListHost {
    pub(super) app: gpui::Entity<MultisoulApp>,
    pub(super) task: Task,
}

struct StaffTopBarHost {
    app: gpui::Entity<MultisoulApp>,
}

impl gpui::Render for StaffTopBarHost {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        gpui::div()
            .size_full()
            .child(self.app.update(cx, |app, cx| {
                app.render_desktop_workspace_top_bar(window, cx)
            }))
    }
}

/// 场景：宽窗缩至600px后，隐藏的任务侧栏必须有可点击的入口。
/// 数据构造：幕僚模式、右栏保持打开偏好，1280×800缩至600×800。
/// 执行过程：重绘生产顶栏，读取任务列表、全屏和首页按钮的实际边界。
/// 预期结果：任务列表入口宽高至少32px，位于窗口内且不与全屏按钮重叠。
#[test]
fn staff_compact_top_bar_keeps_task_list_button_visible() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        app.update(cx, |app, _| {
            app.screen = crate::app::AppScreen::DesktopWorkspace;
            app.staff.enabled = true;
            app.desktop_workspace.drawers.right_open = true;
        });
        StaffTopBarHost { app }
    });
    cx.simulate_resize(size(px(1280.), px(800.)));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("staff-show-tasks").is_none(),
        "宽窗已有任务侧栏，无需重复入口"
    );
    cx.simulate_resize(size(px(600.), px(800.)));
    cx.update(|window, cx| window.draw(cx).clear());
    let tasks = cx
        .debug_bounds("staff-show-tasks")
        .expect("窄窗必须显示任务列表入口");
    let fullscreen = cx.debug_bounds("staff-fullscreen").expect("全屏入口");
    assert!(
        tasks.size.width >= px(32.) && tasks.size.height >= px(32.),
        "入口必须可见可点：{tasks:?}"
    );
    assert!(
        tasks.left() >= px(0.) && tasks.right() <= px(600.),
        "入口不能移出窗口：{tasks:?}"
    );
    assert!(
        tasks.right() <= fullscreen.left(),
        "任务入口不得与全屏重叠：{tasks:?} / {fullscreen:?}"
    );
}

/// 场景：单条短英文用户消息在宽窗口中应横向排版，不能被百分比最大宽度压成逐字竖排。
/// 数据构造：1280×800窗口、仅含12字符STREAM-ALPHA的一条消息。
/// 执行过程：渲染生产消息列表并读取真实气泡bounds。
/// 预期结果：一行文字加上下各12px内边距应低于60px，宽度大于高度且保持右对齐。
#[test]
fn staff_short_user_message_keeps_natural_width() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "STREAM-ALPHA");
    task.messages.push(norma::protocol::TaskMessage::User {
        text: "STREAM-ALPHA".into(),
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
        app.update(cx, |app, _| {
            app.staff.loaded = true;
            app.staff.selected = Some(task.id.clone());
            app.staff.tasks.insert(task.id.clone(), task);
        });
        ConversationHost { app }
    });
    cx.simulate_resize(size(px(1280.), px(800.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let bubble = cx
        .debug_bounds("staff-user-bubble-0")
        .expect("actual user bubble");
    let outer = cx.debug_bounds("staff-message-0").expect("message bounds");
    assert!(
        bubble.size.height < px(60.),
        "短消息应为单行，不应竖排：{bubble:?}"
    );
    assert!(
        bubble.size.width > bubble.size.height,
        "短气泡宽度必须容纳单行：{bubble:?}"
    );
    assert!(
        (outer.right() - bubble.right()).abs() < px(1.),
        "用户气泡仍右对齐"
    );
}

struct ConversationHost {
    app: gpui::Entity<MultisoulApp>,
}

impl gpui::Render for ConversationHost {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{div, prelude::*};
        let conversation = self.app.update(cx, |app, cx| {
            app.render_staff_conversation("Alpha", window, cx)
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

impl gpui::Render for MessageListHost {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{div, prelude::*};
        let task = self.task.clone();
        let list = self
            .app
            .update(cx, |app, cx| app.staff_message_list(&task, window, cx));
        div().size_full().flex().flex_col().child(list)
    }
}

/// 场景：消息总高度超过可视区时，相邻消息不能互相压缩、覆盖（用户气泡压住提议卡）。
/// 数据构造：400px 高窗口；9 条消息，含长用户文本、长提议卡与 Worker 结果卡。
/// 执行过程：用生产的消息列表渲染一帧，读取每条消息外层与内容的 bounds。
/// 预期结果：每条外层高度不小于其内容高度，且下一条顶部不高于上一条底部。
#[test]
fn staff_messages_do_not_overlap_when_content_overflows() {
    use norma::protocol::{StaffProposal, TaskMessage, WorkerOutcome};
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "坦克大战");
    let long = "很长的内容，需要换行。".repeat(40);
    task.proposals.push(StaffProposal {
        id: 1,
        agent: "Simon".into(),
        title: "做可玩的坦克大战".into(),
        prompt: long.clone(),
        why: "空闲".into(),
        model: None,
        approved: false,
        ignored: false,
        dispatch_id: None,
    });
    for index in 0..3 {
        task.messages.push(TaskMessage::User { text: long.clone() });
        task.messages.push(TaskMessage::Proposal { proposal_id: 1 });
        task.messages.push(TaskMessage::WorkerFinished {
            node_id: index,
            dispatch_id: format!("d{index}"),
            outcome: WorkerOutcome::Succeeded,
            text: long.clone(),
        });
    }
    let count = task.messages.len();
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|_, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        MessageListHost { app, task }
    });
    cx.simulate_resize(size(px(900.), px(400.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let outer_selectors = [
        "staff-message-0",
        "staff-message-1",
        "staff-message-2",
        "staff-message-3",
        "staff-message-4",
        "staff-message-5",
        "staff-message-6",
        "staff-message-7",
        "staff-message-8",
    ];
    let body_selectors = [
        "staff-message-body-0",
        "staff-message-body-1",
        "staff-message-body-2",
        "staff-message-body-3",
        "staff-message-body-4",
        "staff-message-body-5",
        "staff-message-body-6",
        "staff-message-body-7",
        "staff-message-body-8",
    ];
    for (outer, bubble) in [
        ("staff-message-0", "staff-user-bubble-0"),
        ("staff-message-3", "staff-user-bubble-3"),
        ("staff-message-6", "staff-user-bubble-6"),
    ] {
        let outer = cx.debug_bounds(outer).expect("user message bounds");
        let bubble = cx.debug_bounds(bubble).expect("actual user bubble bounds");
        assert!(
            bubble.bottom() <= outer.bottom() + px(0.5),
            "气泡真实底部不能溢出消息容器：{bubble:?} / {outer:?}"
        );
    }
    let mut previous_bottom = None;
    for index in 0..count {
        let outer = cx
            .debug_bounds(outer_selectors[index])
            .unwrap_or_else(|| panic!("message {index} outer bounds"));
        let body = cx
            .debug_bounds(body_selectors[index])
            .unwrap_or_else(|| panic!("message {index} body bounds"));
        assert!(
            outer.size.height + px(0.5) >= body.size.height,
            "message {index} 被压缩：外层 {:?} < 内容 {:?}",
            outer.size.height,
            body.size.height
        );
        if let Some(bottom) = previous_bottom {
            assert!(
                outer.origin.y + px(0.5) >= bottom,
                "message {index} 与上一条重叠"
            );
        }
        previous_bottom = Some(outer.bottom());
    }
}
