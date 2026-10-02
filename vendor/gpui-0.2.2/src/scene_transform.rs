//! Presentation coordinates for fixed-layout subtrees. No rotation or general scene graph.
use crate::{Bounds, ContentMask, Pixels, Point, Window};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug)]
pub(crate) struct SceneTransform(Rc<RefCell<TransformState>>);

#[derive(Debug)]
struct TransformState {
    parent: Option<SceneTransform>,
    scale: f32,
    opacity: f32,
    offset: Point<Pixels>,
    clip: Bounds<Pixels>,
}

#[derive(Clone, Copy)]
pub(crate) struct ResolvedTransform {
    pub scale: f32,
    pub opacity: f32,
    pub offset: Point<Pixels>,
    pub clip: Bounds<Pixels>,
}

impl SceneTransform {
    pub(crate) fn new() -> Self {
        Self(Rc::new(RefCell::new(TransformState {
            parent: None,
            scale: 1.,
            opacity: 1.,
            offset: Point::default(),
            clip: Bounds::default(),
        })))
    }

    pub(crate) fn update(
        &self,
        parent: Option<Self>,
        scale: f32,
        offset: Point<Pixels>,
        clip: Bounds<Pixels>,
    ) {
        *self.0.borrow_mut() = TransformState {
            parent,
            scale,
            opacity: 1.,
            offset,
            clip,
        };
    }

    pub(crate) fn set_opacity(&self, opacity: f32) {
        self.0.borrow_mut().opacity = opacity;
    }

    pub(crate) fn resolve(&self) -> ResolvedTransform {
        let state = self.0.borrow();
        if let Some(parent) = &state.parent {
            let parent = parent.resolve();
            ResolvedTransform {
                scale: parent.scale * state.scale,
                opacity: parent.opacity * state.opacity,
                offset: parent.point(state.offset),
                clip: parent.bounds(state.clip).intersect(&parent.clip),
            }
        } else {
            ResolvedTransform {
                scale: state.scale,
                opacity: state.opacity,
                offset: state.offset,
                clip: state.clip,
            }
        }
    }
}

impl ResolvedTransform {
    pub(crate) fn point(self, point: Point<Pixels>) -> Point<Pixels> {
        point * self.scale + self.offset
    }

    pub(crate) fn inverse(self, point: Point<Pixels>) -> Point<Pixels> {
        (point - self.offset) / self.scale
    }

    pub(crate) fn bounds(self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            self.point(bounds.origin),
            bounds.size.map(|v| v * self.scale),
        )
    }
}

impl Window {
    pub(crate) fn with_saved_scene<R>(
        &mut self,
        scene: Option<&(SceneTransform, Bounds<Pixels>)>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        match scene {
            Some((transform, bounds)) => self.with_scene_transform(transform.clone(), *bounds, f),
            None => f(self),
        }
    }

    pub(crate) fn with_scene_transform<R>(
        &mut self,
        transform: SceneTransform,
        local_bounds: Bounds<Pixels>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.scene_transform.replace(transform.clone());
        let previous_paint =
            self.next_frame
                .scene
                .transform
                .replace(super::scene_transform_paint::ScenePlacement {
                    transform,
                    scale_factor: self.scale_factor(),
                });
        // The outer clip is retained by the transform. Keep local masks stable so camera
        // motion does not invalidate AnyView's fixed-layout cache.
        self.content_mask_stack.push(ContentMask {
            bounds: local_bounds,
        });
        let result = f(self);
        self.content_mask_stack.pop();
        self.next_frame.scene.transform = previous_paint;
        self.scene_transform = previous;
        result
    }
}

#[cfg(test)]
#[path = "../tests/unit/scene_transform.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/scene_native.rs"]
mod native_tests;
