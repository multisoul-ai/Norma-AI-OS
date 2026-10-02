use super::{InputHandler, PlatformWindow, UTF16Selection};
use crate::{
    AnyView, App, AppContext, Bounds, Context, FocusHandle, IntoElement, Pixels, Point, Render,
    StyleRefinement, TestAppContext, Window, canvas, div, point, prelude::*, px, scene, size,
};
use std::{cell::Cell, ops::Range, rc::Rc};

fn caret_bounds() -> Bounds<Pixels> {
    Bounds::new(point(px(40.), px(20.)), size(px(8.), px(18.)))
}

struct InputProbe;

impl InputHandler for InputProbe {
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut App,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 2..4,
            reversed: false,
        })
    }
    fn marked_text_range(&mut self, _: &mut Window, _: &mut App) -> Option<Range<usize>> {
        None
    }
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<String> {
        None
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        _: &str,
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        _: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut App) {}
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<Bounds<Pixels>> {
        Some(caret_bounds())
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<usize> {
        caret_bounds().contains(&point).then_some(7)
    }
}

struct InputView {
    focus: FocusHandle,
    paints: Rc<Cell<usize>>,
}

impl Render for InputView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let paints = self.paints.clone();
        canvas(
            |_, _, _| (),
            move |_, (), window, cx| {
                paints.set(paints.get() + 1);
                window.handle_input(&focus, InputProbe, cx);
            },
        )
        .size_full()
    }
}

struct InputScene {
    child: crate::Entity<InputView>,
    offset: Point<Pixels>,
    scale: f32,
    in_scene: bool,
    clip_width: Option<Pixels>,
}

impl Render for InputScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let child =
            AnyView::from(self.child.clone()).cached(StyleRefinement::default().size_full());
        div()
            .size_full()
            .when_some(self.clip_width, |this, width| this.w(width))
            .child(if self.in_scene {
                scene("input-scene", size(px(200.), px(100.)), child)
                    .scale(self.scale)
                    .offset(self.offset)
                    .into_any_element()
            } else {
                child.into_any_element()
            })
    }
}

/// 平台 IME 在缓存场景中沿同一镜头投影候选框，并对文字命中执行逆变换。
///
/// 数据构造：固定局部视区200×100容纳候选框(40,20,8,18)，初始镜头scale=1/2、offset=(100,60)，
///   平台框应为(120,70,4,9)；移动后scale=1、offset=(300,100)，框为(340,120,8,18)。
///   平台点(122,72)逆变换为(44,24)，位于候选框内，对应探针字符7。
/// 执行过程：通过Window::handle_input注册 → 调用平台几何API → 移动父镜头并重放缓存 → 再次查询。
/// 预期结果：初始和移动后的候选框均为屏幕坐标，不能返回局部框或旧镜头位置；
///   合法点命中字符7，旧位置不再命中；父镜头移动不重新paint子内容。
///   最后将视区右界缩至342；x=341仍可命中，x=344虽在局部字形内但已被裁剪，不能命中。
#[test]
fn scene_ime_tracks_cached_presentation_and_inverse_hit_testing() {
    let mut cx = TestAppContext::single();
    let paints = Rc::new(Cell::new(0));
    let child = cx.new(|cx| InputView {
        focus: cx.focus_handle(),
        paints: paints.clone(),
    });
    let focus = child.read_with(&cx, |view, _| view.focus.clone());
    let window = cx.add_window(|_, _| InputScene {
        child,
        offset: point(px(100.), px(60.)),
        scale: 0.5,
        in_scene: true,
        clip_width: None,
    });
    cx.update_window(*window, |_, window, cx| {
        window.focus(&focus);
        window.draw(cx).clear();
    })
    .unwrap();
    let mut platform = cx.test_window(*window);
    let mut input = platform
        .take_input_handler()
        .expect("绘制后必须注册平台输入处理器");
    let expected = Bounds::new(point(px(120.), px(70.)), size(px(4.), px(9.)));
    assert_eq!(
        input.bounds_for_range(2..4),
        Some(expected),
        "IME候选框必须投影至平台坐标"
    );
    assert_ne!(
        input.bounds_for_range(2..4),
        Some(caret_bounds()),
        "不能向平台返回局部坐标"
    );
    assert_eq!(
        input.character_index_for_point(point(px(122.), px(72.))),
        Some(7),
        "平台文字命中必须逆变换"
    );
    cx.update_window(*window, |_, window, cx| {
        assert_eq!(
            input.selected_bounds(window, cx),
            Some(expected),
            "系统选区菜单应使用相同的投影边界"
        );
    })
    .unwrap();
    let paints_before = paints.get();
    platform.set_input_handler(input);
    window
        .update(&mut cx, |view, _, cx| {
            view.offset = point(px(300.), px(100.));
            view.scale = 1.;
            cx.notify();
        })
        .unwrap();
    cx.update_window(*window, |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut input = platform
        .take_input_handler()
        .expect("缓存重放后输入处理器必须保留");
    assert_eq!(
        input.bounds_for_range(2..4),
        Some(Bounds::new(
            point(px(340.), px(120.)),
            size(px(8.), px(18.))
        )),
        "缓存输入应跟随当前镜头"
    );
    assert_ne!(
        input.bounds_for_range(2..4),
        Some(expected),
        "候选框不能停在上一帧"
    );
    assert_eq!(
        input.character_index_for_point(point(px(344.), px(124.))),
        Some(7),
        "移动后的文字位置应命中"
    );
    assert_eq!(
        input.character_index_for_point(point(px(122.), px(72.))),
        None,
        "旧屏幕位置不能继续命中文字"
    );
    assert_eq!(
        paints.get(),
        paints_before,
        "纯镜头移动不得重绘缓存子内容来修复IME"
    );
    platform.set_input_handler(input);
    window
        .update(&mut cx, |view, _, cx| {
            // Glyph spans x=340..348; a viewport right edge of 342 leaves only 2px visible.
            view.clip_width = Some(px(342.));
            cx.notify();
        })
        .unwrap();
    cx.update_window(*window, |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut input = platform
        .take_input_handler()
        .expect("裁剪后可见文字仍需输入处理器");
    assert_eq!(
        input.character_index_for_point(point(px(341.), px(124.))),
        Some(7),
        "裁剪内的可见文字仍应命中"
    );
    assert_eq!(
        input.character_index_for_point(point(px(344.), px(124.))),
        None,
        "裁剪之外的不可见文字不得命中"
    );
}

/// 普通未进入scene的输入框保留原有平台坐标，不被桌面场景补丁重复平移。
///
/// 数据构造：候选框(40,20,8,18)，点(44,24)在框内，点(122,72)在框外。
/// 执行过程：在无scene的普通视图中注册同一输入探针，再调用平台几何API。
/// 预期结果：候选框原样返回；原坐标命中字符7，带外部偏移的点不命中。
#[test]
fn plain_ime_keeps_existing_window_coordinates() {
    let mut cx = TestAppContext::single();
    let child = cx.new(|cx| InputView {
        focus: cx.focus_handle(),
        paints: Rc::new(Cell::new(0)),
    });
    let focus = child.read_with(&cx, |view, _| view.focus.clone());
    let window = cx.add_window(|_, _| InputScene {
        child,
        offset: Point::default(),
        scale: 1.,
        in_scene: false,
        clip_width: None,
    });
    cx.update_window(*window, |_, window, cx| {
        window.focus(&focus);
        window.draw(cx).clear();
    })
    .unwrap();
    let mut input = cx
        .test_window(*window)
        .take_input_handler()
        .expect("普通视图必须注册输入处理器");
    assert_eq!(
        input.bounds_for_range(2..4),
        Some(caret_bounds()),
        "普通视图必须保留原坐标"
    );
    assert_eq!(
        input.character_index_for_point(point(px(44.), px(24.))),
        Some(7),
        "原始局部点应正常命中"
    );
    assert_eq!(
        input.character_index_for_point(point(px(122.), px(72.))),
        None,
        "普通视图不能凭空添加场景偏移"
    );
}
