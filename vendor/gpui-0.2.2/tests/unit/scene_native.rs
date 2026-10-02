use crate::{
    AnyView, AppContext, Bounds, Context, Entity, IntoElement, Render, ScenePresentation,
    StyleRefinement, TestAppContext, Window, canvas, div, point, prelude::*, px, scene, size,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct NativeProbe {
    calls: Rc<RefCell<Vec<Option<ScenePresentation>>>>,
    paints: Rc<Cell<usize>>,
}

impl Render for NativeProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let calls = self.calls.clone();
        let paints = self.paints.clone();
        canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                paints.set(paints.get() + 1);
                window.on_scene_presentation(
                    "native-probe",
                    Bounds::new(point(px(40.), px(20.)), size(px(80.), px(60.))),
                    move |placement, _, _| {
                        calls.borrow_mut().push(placement);
                    },
                );
            },
        )
        .size_full()
    }
}

struct NativeScene {
    child: Entity<NativeProbe>,
    shifted: bool,
    visible: bool,
}

impl Render for NativeScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().when(self.visible, |view| {
            view.child(
                scene(
                    "native-scene",
                    size(px(200.), px(100.)),
                    AnyView::from(self.child.clone())
                        .cached(StyleRefinement::default().size_full()),
                )
                .scale(if self.shifted { 1. } else { 0.5 })
                .opacity(if self.shifted { 0.25 } else { 1. })
                .offset(if self.shifted {
                    point(px(150.), px(50.))
                } else {
                    point(px(20.), px(10.))
                }),
            )
        })
    }
}

/// 原生视图与缓存画布同步变换，并在节点离开绘制树后隐藏一次。
///
/// 数据构造：窗口200×120，固定逻辑内容200×100；原生内容(40,20,80,60)。
///   初帧scale=1/2、offset=(20,10) → frame=(40,20,40,30)。
///   次帧scale=1、offset=(150,50) → frame=(190,70,80,60)，
///   窗口裁剪后visible=(190,70,10,50)，透明度1/4。
/// 执行过程：绘制缓存子视图 → 只移动父镜头 → 隐藏子视图 → 再绘制空帧。
/// 预期结果：坐标、裁剪与透明度及时更新；局部尺寸不变且不重画子树；
///   隐藏仅通知一次，不保留旧窗口位置或把离场节点当可见。
#[crate::test]
fn scene_native_tracks_cached_camera_and_removal(cx: &mut TestAppContext) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let paints = Rc::new(Cell::new(0));
    let child = cx.new(|_| NativeProbe {
        calls: calls.clone(),
        paints: paints.clone(),
    });
    let (root, window) = cx.add_window_view(|_, _| NativeScene {
        child,
        shifted: false,
        visible: true,
    });
    window.simulate_resize(size(px(200.), px(120.)));
    window.update(|window, cx| window.draw(cx).clear());
    let first = calls
        .borrow()
        .last()
        .copied()
        .flatten()
        .expect("首帧必须同步原生视图");
    assert_eq!(
        first.bounds,
        Bounds::new(point(px(40.), px(20.)), size(px(40.), px(30.))),
        "原生框必须包含镜头缩放和平移"
    );
    let painted = paints.get();
    root.update(window, |root, cx| {
        root.shifted = true;
        cx.notify();
    });
    window.update(|window, cx| window.draw(cx).clear());
    let moved = calls
        .borrow()
        .last()
        .copied()
        .flatten()
        .expect("缓存帧也必须同步原生视图");
    assert_eq!(
        moved.bounds,
        Bounds::new(point(px(190.), px(70.)), size(px(80.), px(60.))),
        "不能滞后一帧或保留旧坐标"
    );
    assert_eq!(
        moved.visible_bounds,
        Bounds::new(point(px(190.), px(70.)), size(px(10.), px(50.))),
        "原生视图不得覆盖场景外侧"
    );
    assert_eq!(moved.opacity, 0.25, "缓存原生内容必须同步当前透明度");
    assert_eq!(
        moved.logical_size,
        size(px(80.), px(60.)),
        "镜头不得改变网页逻辑布局"
    );
    assert_eq!(paints.get(), painted, "镜头变化不得使缓存子树重新paint");
    assert_ne!(moved.bounds, first.bounds, "不能继续使用上帧呈现位置");
    root.update(window, |root, cx| {
        root.visible = false;
        cx.notify();
    });
    window.update(|window, cx| window.draw(cx).clear());
    assert!(
        calls.borrow().last().unwrap().is_none(),
        "离场原生视图必须收到隐藏通知"
    );
    let after_hide = calls.borrow().len();
    window.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        calls.borrow().len(),
        after_hide,
        "移除后不得重复保留回调或反复隐藏"
    );
}
