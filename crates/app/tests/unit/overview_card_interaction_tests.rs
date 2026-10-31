use super::*;
use crate::workspace::desktop::overview_rows::{OverviewAction, OverviewRow, OverviewStatusTone};
use gpui::{AppContext, Entity, Modifiers, Render, TestAppContext};
use std::{cell::Cell, rc::Rc};

struct ApprovalHarness {
    app: Entity<MultisoulApp>,
    row: OverviewRow,
    outer_keys: Rc<Cell<usize>>,
}

impl Render for ApprovalHarness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let keys = self.outer_keys.clone();
        div()
            .on_key_down(move |event, _, _| {
                if event.keystroke.key == "enter" {
                    keys.set(keys.get() + 1);
                }
            })
            .child(self.app.update(cx, |app, cx| {
                app.render_approval_overview_row(&self.row, 0, false, app.desktop_theme(), cx)
            }))
    }
}

/// 场景：真实 GPUI 控件的审批按钮在收起态可点击，点击不改变展开状态。
/// 构造：隔离 App、可处理的审批行、但已不存在的请求；外层显示“允许/拒绝”。
/// 执行：直接点击允许，再点击标题展开，并检查相同两按钮仍存在。
/// 预期：允许动作得到失效回执而非展开/定位；标题才展开；按钮不重复也不消失。
#[test]
fn approval_buttons_stay_outside_the_disclosure() {
    let dir = tempfile::tempdir().unwrap();
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut source = None;
    let outer_keys = Rc::new(Cell::new(0));
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                storage::DatabasePaths::isolated_test(&dir.path().join("data")),
            )
        });
        source = Some(app.clone());
        let row = OverviewRow {
            primary: "允许运行归因测试？".into(),
            secondary: "Codex · Agent Runtime".into(),
            detail: None,
            task_line: None,
            project_tag: None,
            is_agent: false,
            action_label: "定位".into(),
            action: OverviewAction::LocateApproval {
                area_id: "a".into(),
                node_id: 7,
                run_id: "r".into(),
                turn_id: "t".into(),
                request_id: Some("p".into()),
            },
            status_label: None,
            lead_label: None,
            status_tone: OverviewStatusTone::Approval,
            approval_badge: None,
            progress: None,
            prelight: None,
            enabled: true,
            height: 48.,
        };
        let harness = cx.new(|cx| {
            cx.observe(&app, |_, _, cx| cx.notify()).detach();
            ApprovalHarness {
                app,
                row,
                outer_keys: outer_keys.clone(),
            }
        });
        ui::component::Root::new(harness, window, cx)
    });
    let source = source.unwrap();
    cx.update(|window, cx| window.draw(cx).clear());
    let allow = cx
        .debug_bounds("overview-allow-0")
        .expect("收起态必须显示允许按钮");
    assert!(
        cx.debug_bounds("overview-deny-0").is_some(),
        "收起态必须显示拒绝按钮"
    );
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_none()),
        "初始详情收起"
    );
    cx.update(|window, cx| {
        window.blur();
        window.focus_next();
        window.focus_next();
        window.draw(cx).clear();
        let stroke = gpui::Keystroke::parse("enter").unwrap();
        window.dispatch_event(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: stroke.clone(),
                is_held: false,
            }),
            cx,
        );
        window.draw(cx).clear();
        window.dispatch_event(
            gpui::PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke: stroke }),
            cx,
        );
        window.draw(cx).clear();
    });
    assert_eq!(outer_keys.get(), 0, "审批按钮 Enter 不得触发外层定位导航");
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_none()),
        "键盘审批不能展开正文"
    );
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.error.is_some()),
        "键盘应实际调用审批并获得过期回执"
    );
    cx.simulate_click(allow.center(), Modifiers::none());
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_none()),
        "审批不能触发展开"
    );
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.error.is_some()),
        "点击应实际进入审批入口并得到失效反馈"
    );
    let header = cx.debug_bounds("approval-toggle-0").unwrap();
    cx.simulate_click(header.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        source.read_with(cx, |app, _| app.desktop_workspace.attention.open.is_some()),
        "标题点击才展开"
    );
    assert!(
        cx.debug_bounds("overview-allow-0").is_some(),
        "展开后允许仍在外层"
    );
    assert!(
        cx.debug_bounds("overview-deny-0").is_some(),
        "展开后拒绝仍在外层"
    );
}
