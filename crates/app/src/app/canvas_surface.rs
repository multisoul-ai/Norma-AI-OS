use gpui::{div, prelude::*, Context, Entity, Subscription, WeakEntity, Window};

use super::MultisoulApp;

/// 缓存高成本工作区绘制，使 Voice 等兄弟叠层刷新时可以复用上一帧 Canvas。
pub(crate) struct CanvasSurfaceView {
    app: WeakEntity<MultisoulApp>,
    _app_observer: Subscription,
}

impl CanvasSurfaceView {
    pub(crate) fn new(
        app: WeakEntity<MultisoulApp>,
        app_entity: Entity<MultisoulApp>,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_observer = cx.observe(&app_entity, |_surface, _app, cx| {
            // 业务状态通过 MultisoulApp::notify 失效 Canvas；Voice 子 Entity 的局部
            // notify 只把祖先标脏，不会触发这个 observer，因此可以继续复用缓存。
            cx.notify();
        });
        Self {
            app,
            _app_observer: app_observer,
        }
    }
}

impl gpui::Render for CanvasSurfaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().size_full().into_any_element();
        };
        app.update(cx, |app, app_cx| {
            app.render_canvas_surface(window, app_cx).into_any_element()
        })
    }
}
