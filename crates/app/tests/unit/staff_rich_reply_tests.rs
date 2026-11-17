use super::content_layout_tests::{done_reply, ConversationColumnHost};
use super::*;
use gpui::{px, size, AppContext, TestAppContext};

fn rich_markdown() -> String {
    let root = "/Users/example/Documents/norma-ai-os";
    format!(
        "## 原型文件\n\n| 文件 | 绝对路径 |\n| --- | --- |\n| iOS 工作区原型 | `{root}/docs/prototypes/norma-ios-workspace.html` |\n| 原型样式 | `{root}/docs/prototypes/norma-channel-prototype.css` |\n\n其他相关的：\n\n- 空间原型说明：`{root}/docs/product-specs/norma-spatial-prototype.md`\n- iOS 原型规格目录：`{root}/docs/product-specs/norma-ios-prototypes-2026-09-26`\n\n另外还有 HTML 版计划，比如 `{root}/docs/exec-plans/2026-10-24-brain-propose-next-step.html`。\n和你这个富文本测试最贴的是幕僚架构设计稿：`{root}/docs/design-docs/2026-11-12-norma-staff-architecture.html`\n"
    )
}

/// 场景：贴底时内容变高（表格换行、流式纯文字转 Markdown），视图必须补滚到底，不能停在中间。
///
/// 数据构造：
///   窗口 1000×700，首条回复为短文字（一屏内），第二条为含表格、列表和长路径的富文本回复
///   pinned = true（与选中任务时 scroll_to_bottom 同步进入跟随）
///
/// 执行过程：
///   1. 渲染一条短回复，滚动偏移为 0，near_bottom 为真（阈值 48 来自 staff_near_bottom）
///   2. 追加长回复。此时 max_offset 仍是上一帧的，near_bottom 仍为真
///   3. 只绘制一帧。贴底跟随时这一帧就必须滚到新高度，不能等下一帧
///
/// 预期结果：
///   - 贴底：max_offset + offset ≤ 48，最后一段不被输入框挡住
///   - 用户上翻（pinned = false）后不再被拉回底部
#[test]
fn staff_pinned_view_catches_up_when_content_grows() {
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "布局");
    task.messages.push(done_reply("短回复"));
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
            app.staff.pinned = true;
            app.staff.tasks.insert(task.id.clone(), task.clone());
        });
        ConversationColumnHost { app }
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let near_bottom = |host: &gpui::Entity<ConversationColumnHost>,
                       cx: &mut gpui::VisualTestContext| {
        host.update(cx, |host, cx| {
            host.app.update(cx, |app, _| app.staff_near_bottom())
        })
    };
    assert!(near_bottom(&host, cx), "短回复一屏内，天然贴底");

    host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| {
            let task = app.staff.tasks.get_mut("t").expect("task");
            task.messages.push(done_reply(&rich_markdown()));
            task.messages.push(done_reply(&rich_markdown()));
        })
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        near_bottom(&host, cx),
        "内容变高的这一帧就要补滚到底；只靠下一帧的话，没有人通知时最后几行会停在视口下面"
    );

    host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| {
            app.staff.pinned = false;
            app.staff.scroll.set_offset(gpui::point(px(0.), px(0.)));
        })
    });
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear());
    }
    assert!(!near_bottom(&host, cx), "用户上翻后不应再被拉回底部");
}

/// 场景：已经上翻到离底部 30px（仍在 48px 贴底带内）且 pinned = false。
/// 下一块流式回复不能把视图拽回底部。
///
/// 数据构造：
///   窗口 1000×700，一条长回复，滚动范围 > 80px
///   偏移设为 -(max - 30)。30 ≤ 48，near_bottom 为真，pinned 为 false
///
/// 执行过程：
///   1. 绘制长回复，确认滚得动
///   2. 上移到离底部 30px，receive_staff_reply 再写一次，绘制一帧
///   3. pinned = true，再收一次，绘制一帧
///
/// 预期结果：
///   - 第 2 步仍离底部大约 30px，而不是 0（靠近底部不等于仍在跟随）
///   - 第 3 步贴底（跟随中的增量会补滚）
#[test]
fn staff_streaming_chunk_does_not_yank_after_small_scroll_up() {
    let mut test = TestAppContext::single();
    let (_dir, host, cx) = open_staff(&mut test, &rich_markdown(), false);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let max_h = host.update(cx, |host, cx| {
        host.app
            .update(cx, |app, _| app.staff.scroll.max_offset().height)
    });
    assert!(
        f32::from(max_h) > 80.0,
        "长回复滚动范围应大于 80px，实际 {max_h:?}"
    );

    host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| {
            app.staff.pinned = false;
            app.staff
                .scroll
                .set_offset(gpui::point(px(0.), -max_h + px(30.)));
            assert!(
                app.staff_near_bottom(),
                "离底部 30px 仍在 48px 带内，用来区分「靠近底部」和「仍在跟随」"
            );
        })
    });
    replace_reply(&host, cx, &rich_markdown());
    cx.update(|window, cx| window.draw(cx).clear());
    let gap = host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| {
            f32::from(app.staff.scroll.max_offset().height) + f32::from(app.staff.scroll.offset().y)
        })
    });
    assert!(
        (20.0..=48.0).contains(&gap),
        "退出跟随后应留在离底部约 30px，不能被拽回 0，实际 {gap}"
    );

    host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| app.staff.pinned = true)
    });
    replace_reply(&host, cx, &rich_markdown());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(is_near_bottom(&host, cx), "恢复跟随后再来一块，应补滚到底");
}

/// 场景：滚轮监听在 GPUI 改偏移之前。下翻是否恢复跟随，要看这一笔之后的位置。
///
/// 数据构造：
///   长回复绘制后 max_offset > 80px，先停在顶部（offset = 0，pinned = false）
///   贴底带是 48px
///
/// 执行过程：
///   1. 下翻整整一个 max_offset，落点是底部
///   2. 上翻 30px
///   3. 停在离底部 80px 处，下翻 20px（落点离底部 60px，仍在带外）
///   4. 再下翻 20px，落点离底部 40px，进入 48px 带
///
/// 预期结果：
///   - 第 1 步 pinned = true（用滑动前的 near_bottom 会漏掉这次）
///   - 第 2 步 pinned = false
///   - 第 3 步 pinned 仍为 false
///   - 第 4 步 pinned = true
#[test]
fn staff_wheel_pins_from_position_after_delta() {
    let mut test = TestAppContext::single();
    let (_dir, host, cx) = open_staff(&mut test, &rich_markdown(), true);
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| {
            let max_h = app.staff.scroll.max_offset().height;
            assert!(
                f32::from(max_h) > 80.0,
                "长回复滚动范围应大于 80px，实际 {max_h:?}"
            );
            app.staff.pinned = false;
            app.staff.scroll.set_offset(gpui::point(px(0.), px(0.)));
            app.staff_note_scroll_wheel(-max_h);
            assert!(app.staff.pinned, "一次下翻落到贴底带内应恢复跟随");
            app.staff_note_scroll_wheel(px(30.));
            assert!(!app.staff.pinned, "上翻应退出跟随");
            app.staff
                .scroll
                .set_offset(gpui::point(px(0.), -max_h + px(80.)));
            app.staff_note_scroll_wheel(px(-20.));
            assert!(!app.staff.pinned, "下翻 20px 后仍离底部 60px，不应恢复跟随");
            // 监听自己不改偏移；下一笔到来时 GPUI 已经加上了上一笔。
            app.staff
                .scroll
                .set_offset(gpui::point(px(0.), -max_h + px(60.)));
            app.staff_note_scroll_wheel(px(-20.));
            assert!(app.staff.pinned, "再下翻 20px 进入 48px 带，应恢复跟随");
        })
    });
}

/// 真机 `staff-long-reply` 的回复原文：两张 5 行表格、5 条列表、软换行段落和收尾。
fn e2e_long_reply() -> String {
    let root = "/Users/example/Documents/norma-ai-os";
    let rows: String = (1..=5)
        .map(|i| format!("| 原型 {i} | `{root}/docs/prototypes/norma-long-path-prototype-number-{i}-with-a-very-long-name.html` |\n"))
        .collect();
    let bullets: String = (1..=5)
        .map(|i| format!("- 条目 {i}：`{root}/docs/product-specs/norma-ios-prototypes-2026-09-{i:02}/overview-with-long-name.md`\n"))
        .collect();
    format!(
        "LONG-REPLY 验收\n\n## 原型文件\n\n| 文件 | 绝对路径 |\n| --- | --- |\n{rows}\n其他相关的：\n\n{bullets}\n## 实施计划\n\n| 文件 | 绝对路径 |\n| --- | --- |\n{rows}\n另外还有 HTML 版计划，比如 `{root}/docs/exec-plans/2026-10-24-brain-propose-next-step.html`。\n和你这个富文本测试最贴的是幕僚架构设计稿：`{root}/docs/design-docs/2026-11-12-norma-staff-architecture.html`\n原因是消息行被撑成了很高的虚高，滚动按虚高算，首条就被顶出首屏。\n还没验收，是因为他只用布局边界测试验证过，没有在真实 App 里看过。\nLONG-REPLY END"
    )
}

/// 场景：真机看图里，长富文本回复贴底后视口停在表格中间，最后一行不在画面里，也没有「回到最新」。
///
/// 数据构造：
///   窗口 1280×800 与 1000×700，一条回复含两张 5 行表格、5 条列表和 4 行收尾（与 e2e fixture 同文）
///   pinned = true，滚动偏移 0
///
/// 执行过程：
///   1. 绘制一帧，贴底跟随在本帧把偏移设到 −max_offset
///   2. 读取视口、回复正文、操作行的边界以及 max_offset / offset
///
/// 预期结果：
///   - near_bottom 为真（max_offset + offset ≤ 48）
///   - 回复正文底边不低于视口底边 −16 内边距之外，即不越过视口底
///   - 操作行在视口内，也就是最后一块内容完整可见
#[test]
fn staff_e2e_long_reply_ends_inside_viewport() {
    for (width, height) in [(1280., 800.), (1000., 700.)] {
        let mut test = TestAppContext::single();
        let (_dir, host, cx) = open_staff(&mut test, &e2e_long_reply(), true);
        cx.simulate_resize(size(px(width), px(height)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let (max_h, offset) = host.update(cx, |host, cx| {
            host.app.update(cx, |app, _| {
                (
                    f32::from(app.staff.scroll.max_offset().height),
                    f32::from(app.staff.scroll.offset().y),
                )
            })
        });
        let viewport = cx.debug_bounds("staff-messages").expect("viewport");
        let text = cx.debug_bounds("staff-reply-text-0-0").expect("reply text");
        let actions = cx
            .debug_bounds("staff-reply-actions-0")
            .expect("reply actions");
        assert!(
            is_near_bottom(&host, cx),
            "{width}x{height}：贴底后 max_offset {max_h} + offset {offset} 应 ≤ 48"
        );
        assert!(
            text.bottom() <= viewport.bottom() + px(0.5),
            "{width}x{height}：回复正文底边越过视口底：正文 {text:?} / 视口 {viewport:?} / max {max_h} / offset {offset}"
        );
        assert!(
            actions.bottom() <= viewport.bottom() + px(0.5),
            "{width}x{height}：操作行不在视口内：操作 {actions:?} / 视口 {viewport:?}"
        );
    }
}

fn is_near_bottom(
    host: &gpui::Entity<ConversationColumnHost>,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    host.update(cx, |host, cx| {
        host.app.update(cx, |app, _| app.staff_near_bottom())
    })
}

fn replace_reply(
    host: &gpui::Entity<ConversationColumnHost>,
    cx: &mut gpui::VisualTestContext,
    text: &str,
) {
    host.update(cx, |host, cx| {
        host.app.update(cx, |app, cx| {
            let norma::protocol::TaskMessage::Staff(reply) = done_reply(text) else {
                panic!("done_reply must be a staff message");
            };
            app.receive_staff_reply("t", 0, reply, cx);
        })
    });
}

fn open_staff<'a>(
    test: &'a mut TestAppContext,
    text: &str,
    pinned: bool,
) -> (
    tempfile::TempDir,
    gpui::Entity<ConversationColumnHost>,
    &'a mut gpui::VisualTestContext,
) {
    // 返回的 TempDir 要由调用方留到测试结束；删掉的话 sqlite 文件会在绘制前消失。
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut task = Task::new("t", "w", "p", "布局");
    task.messages.push(done_reply(text));
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
            app.staff.pinned = pinned;
            app.staff.tasks.insert(task.id.clone(), task.clone());
        });
        ConversationColumnHost { app }
    });
    (dir, host, cx)
}
