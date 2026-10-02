//! Retained native-view placements share the paint scene's current transform and clip.
use crate::{App, Bounds, ElementId, Pixels, Size, Window, scene_transform::SceneTransform};
use std::rc::Rc;

/// A native view's presentation in window logical pixels, independent of its content layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePresentation {
    /// Unclipped transformed frame.
    pub bounds: Bounds<Pixels>,
    /// Frame intersected with all scene and local content clips.
    pub visible_bounds: Bounds<Pixels>,
    /// Original content size; camera motion must not resize the native document.
    pub logical_size: Size<Pixels>,
    /// Uniform presentation scale, excluding the window backing scale.
    pub scale: f32,
    /// Combined element and scene opacity.
    pub opacity: f32,
}

#[derive(Clone)]
pub(crate) struct NativeSceneView {
    pub id: ElementId,
    bounds: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    opacity: f32,
    transform: Option<SceneTransform>,
    callback: Rc<dyn Fn(Option<ScenePresentation>, &mut Window, &mut App)>,
}

impl NativeSceneView {
    fn presentation(&self) -> ScenePresentation {
        let (bounds, clip, scale, opacity) = match &self.transform {
            Some(transform) => {
                let transform = transform.resolve();
                (
                    transform.bounds(self.bounds),
                    transform.bounds(self.clip).intersect(&transform.clip),
                    transform.scale,
                    self.opacity * transform.opacity,
                )
            }
            None => (self.bounds, self.clip, 1., self.opacity),
        };
        ScenePresentation {
            bounds,
            visible_bounds: bounds.intersect(&clip),
            logical_size: self.bounds.size,
            scale,
            opacity,
        }
    }
}

impl Window {
    /// Register a native-view placement during paint. The callback runs after the entire
    /// frame is built, including cached replays. IDs must be unique across this window.
    /// Missing IDs receive `None` once on the next frame so native children cannot linger.
    /// Callbacks may update the native view, but must not invalidate or draw GPUI content.
    pub fn on_scene_presentation(
        &mut self,
        id: impl Into<ElementId>,
        bounds: Bounds<Pixels>,
        callback: impl Fn(Option<ScenePresentation>, &mut Window, &mut App) + 'static,
    ) {
        self.next_frame.native_views.push(NativeSceneView {
            id: id.into(),
            bounds,
            clip: self.content_mask().bounds,
            opacity: self.element_opacity,
            transform: self.scene_transform.clone(),
            callback: Rc::new(callback),
        });
    }

    pub(crate) fn present_native_scene_views(&mut self, cx: &mut App) {
        let current = self.next_frame.native_views.clone();
        let ids = current
            .iter()
            .map(|view| &view.id)
            .collect::<std::collections::HashSet<_>>();
        let removed = self
            .rendered_frame
            .native_views
            .iter()
            .filter(|view| !ids.contains(&view.id))
            .cloned()
            .collect::<Vec<_>>();
        for view in removed {
            (view.callback)(None, self, cx);
        }
        for view in current {
            (view.callback)(Some(view.presentation()), self, cx);
        }
    }
}
