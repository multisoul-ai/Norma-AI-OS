use super::*;
use gpui::{
    point, size, App, Bounds, Element, ElementId, FocusHandle, GlobalElementId, Hitbox,
    InspectorElementId, LayoutId, Modifiers, MouseButton, Pixels, Render, Style, TestAppContext,
};
use std::{cell::RefCell, rc::Rc};

// Observe the style GPUI actually paints, after resolving pointer and focus state.
struct ObservedTarget {
    target: Stateful<Div>,
    style: Rc<RefCell<Style>>,
}

impl Element for ObservedTarget {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Element::id(&self.target)
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.target.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.target.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.target
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        *self.style.borrow_mut() =
            self.target
                .interactivity()
                .compute_style(id, hitbox.as_ref(), window, cx);
        self.target
            .paint(id, inspector, bounds, layout, hitbox, window, cx);
    }
}

impl IntoElement for ObservedTarget {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct TargetHost {
    theme: ui::Theme,
    selected: bool,
    focus: FocusHandle,
    style: Rc<RefCell<Style>>,
}

impl Render for TargetHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .w(px(400.0))
            .h(px(225.0))
            .border_1()
            .child(ObservedTarget {
                target: project_area_open_target(
                    0,
                    canvas::Vec2::new(400.0, 225.0),
                    14.0,
                    self.selected,
                    self.theme,
                )
                .track_focus(&self.focus)
                .debug_selector(|| "project-open-target".to_owned()),
                style: self.style.clone(),
            })
    }
}

/// 卡片内部始终透明，悬停、按下、键盘焦点和选中只改变完整圆角边框。
/// 数据构造：六个主题各自绘制400×225卡片；点击层覆盖整张卡片；鼠标从外部移入中央。
/// 执行过程：绘制静止态→悬停→按下→松开并移出→选中→键盘聚焦，记录实际绘制样式与边界。
/// 预期结果：全部状态无背景填充和阴影；悬停1px、按下3px、选中/焦点2px；
///   点击层恰好400×225且状态变化不改变几何，选中移出后仍保留边框。
#[test]
fn project_card_feedback_uses_only_full_card_borders_in_all_themes() {
    for theme_id in ui::ThemeId::ALL {
        let theme = ui::Theme::for_id(theme_id);
        let style = Rc::new(RefCell::new(Style::default()));
        let mut test = TestAppContext::single();
        let (host, cx) = test.add_window_view(|_, cx| TargetHost {
            theme,
            selected: false,
            focus: cx.focus_handle(),
            style: style.clone(),
        });
        cx.simulate_resize(size(px(800.0), px(600.0)));
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_mouse_move(point(px(600.0), px(450.0)), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear());
        let bounds = cx
            .debug_bounds("project-open-target")
            .expect("应绘制完整点击层");
        let check = |state: &str, width: f32| {
            let painted = style.borrow();
            assert!(
                painted.background.is_none(),
                "{theme_id:?} {state} 不得绘制内部填充"
            );
            assert!(
                painted.box_shadow.is_empty(),
                "{theme_id:?} {state} 应只通过边框反馈"
            );
            assert_eq!(
                painted.border_widths.top,
                px(width).into(),
                "{theme_id:?} {state} 边框宽度应为{width}px"
            );
            if state != "静止" {
                assert_eq!(
                    painted.border_color,
                    Some(rgba(theme.focused_stroke).into()),
                    "{theme_id:?} {state} 应使用清晰的主题边框色"
                );
            } else {
                assert_eq!(
                    painted.border_color,
                    Some(rgba(0x00000000).into()),
                    "静止点击层不得覆盖卡片原边框"
                );
            }
        };
        check("静止", 1.0);
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear());
        check("悬停", 1.0);
        cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear());
        check("按下", 3.0);
        cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(point(px(600.0), px(450.0)), None, Modifiers::none());
        host.update_in(cx, |host, window, cx| {
            window.blur();
            host.selected = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        check("选中移出", 2.0);
        host.update_in(cx, |host, window, _| host.focus.focus(window));
        cx.update(|window, cx| window.draw(cx).clear());
        check("键盘焦点", 2.0);
        host.update(cx, |host, cx| {
            host.selected = false;
            cx.notify();
        });
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear());
        check("未选中但聚焦且悬停", 2.0);
        assert_eq!(
            bounds.size,
            size(px(400.0), px(225.0)),
            "点击层必须覆盖完整卡片而非内部画布"
        );
        assert_eq!(
            cx.debug_bounds("project-open-target"),
            Some(bounds),
            "所有视觉状态必须保持卡片位置与尺寸"
        );
    }
}
