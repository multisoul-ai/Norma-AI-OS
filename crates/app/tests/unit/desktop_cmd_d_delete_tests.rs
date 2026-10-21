use canvas::{CanvasNodeKind, Vec2};
use gpui::TestAppContext;

#[path = "../support/desktop_embedded_project.rs"]
mod embedded_project;
use embedded_project::{open_wide_embedded_project, place_tiled_terminals};

/// 场景：嵌入项目里用户选中第 1 张卡片后按 Cmd+D 删除。
///
/// 数据构造：945px 宽画布上三张终端并排，单选第 1 张。
/// 执行过程：向项目发送 Cmd+D 的 KeyDown，而不是直接调用 `remove_node_and_reflow`。
/// 预期结果：第 1 张被删除，剩下两张补位到前两个网格格子，与直接重排的结果一致。
#[test]
fn desktop_cmd_d_delete_reflows_remaining_cards() {
    use gpui::{KeyDownEvent, Keystroke};

    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let mut test = TestAppContext::single();
    let (project, cx) = open_wide_embedded_project(&mut test, temp.path(), "Cmd D Delete");
    let (ids, second_before) = project.update_in(cx, |app, window, _| {
        let ids = place_tiled_terminals(app, window, 3);
        app.runtime.selection.select_only(ids[0]);
        let second = app
            .runtime
            .document
            .node(ids[1])
            .expect("第 2 张应存在")
            .bounds;
        (ids, second)
    });
    project.update_in(cx, |app, window, cx| {
        let event = KeyDownEvent {
            keystroke: Keystroke::parse("cmd-d").expect("应解析 Cmd+D"),
            is_held: false,
        };
        app.on_key_down(&event, window, cx);
    });
    project.read_with(cx, |app, _| {
        assert!(
            app.runtime.document.node(ids[0]).is_none(),
            "Cmd+D 应删除选中卡片"
        );
        let second_after = app
            .runtime
            .document
            .node(ids[1])
            .expect("第 2 张应留下")
            .bounds;
        assert_ne!(
            second_after, second_before,
            "Cmd+D 删除后剩余卡片必须补位，保存坐标不能停在旧格子"
        );
    });
}

/// 场景：嵌入项目里聚焦（而非仅选中）一张 Agent 卡片后按 Cmd+D。
///
/// 数据构造：945px 宽画布，三张 Agent 卡片并排；聚焦第 1 张。
/// 执行过程：发送 Cmd+D 的 KeyDown。
/// 预期结果：第 1 张被删除，剩余 Agent 卡片补位。
#[test]
fn desktop_cmd_d_on_focused_agent_card_reflows_remaining_cards() {
    use canvas::{AgentProfile, AgentSpec};
    use gpui::{KeyDownEvent, Keystroke};

    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let mut test = TestAppContext::single();
    let (project, cx) = open_wide_embedded_project(&mut test, temp.path(), "Cmd D Agent");
    let (ids, second_before) = project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        let mut ids = Vec::new();
        for _ in 0..3 {
            let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
            let id = app.runtime.document.add_node(
                CanvasNodeKind::Agent(AgentSpec {
                    profile: AgentProfile::Codex,
                    alias: None,
                    definition_id: None,
                    mobile_control: false,
                }),
                bounds,
            );
            app.apply_layout_for_new_node_for_area(id, viewport, zoom);
            ids.push(id);
        }
        app.runtime.selection.select_only(ids[0]);
        app.runtime.focus.focus_node(ids[0]);
        let second = app
            .runtime
            .document
            .node(ids[1])
            .expect("第 2 张应存在")
            .bounds;
        (ids, second)
    });
    project.update_in(cx, |app, window, cx| {
        let event = KeyDownEvent {
            keystroke: Keystroke::parse("cmd-d").expect("应解析 Cmd+D"),
            is_held: false,
        };
        app.on_key_down(&event, window, cx);
    });
    project.read_with(cx, |app, _| {
        assert!(
            app.runtime.document.node(ids[0]).is_none(),
            "Cmd+D 应删除聚焦卡片"
        );
        let second_after = app
            .runtime
            .document
            .node(ids[1])
            .expect("第 2 张应留下")
            .bounds;
        assert_ne!(second_after, second_before, "删除后剩余 Agent 卡片必须补位");
    });
}
