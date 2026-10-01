use crate::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, Size, Style, Window, scene_transform::SceneTransform,
};

/// Present a fixed-layout child with translation and positive uniform scale.
/// Fills its parent and clips to that viewport; child layout uses `logical_size`.
/// Use an outer div to size or decorate the viewport.
pub fn scene(
    id: impl Into<ElementId>,
    logical_size: Size<Pixels>,
    child: impl IntoElement,
) -> SceneElement {
    assert!(
        f32::from(logical_size.width).is_finite()
            && f32::from(logical_size.height).is_finite()
            && logical_size.width >= crate::px(0.)
            && logical_size.height >= crate::px(0.),
        "scene logical size must be finite and non-negative"
    );
    SceneElement {
        id: id.into(),
        logical_size,
        child: child.into_any_element(),
        scale: 1.,
        opacity: 1.,
        offset: Point::default(),
        transform: None,
    }
}

/// A finite presentation transform; cached child views keep their local coordinates.
pub struct SceneElement {
    id: ElementId,
    logical_size: Size<Pixels>,
    child: AnyElement,
    scale: f32,
    opacity: f32,
    offset: Point<Pixels>,
    transform: Option<SceneTransform>,
}

impl SceneElement {
    /// Uniform presentation scale. Zero, negative and non-finite scales are invalid.
    pub fn scale(mut self, scale: f32) -> Self {
        assert!(
            scale.is_finite() && scale > 0.,
            "scene scale must be finite and positive"
        );
        self.scale = scale;
        self
    }

    /// Presentation opacity, applied when retained paint commands are replayed.
    pub fn opacity(mut self, opacity: f32) -> Self {
        assert!(opacity.is_finite() && (0.0..=1.0).contains(&opacity), "scene opacity must be between zero and one");
        self.opacity = opacity;
        self
    }

    /// Presentation translation, relative to this element's origin.
    pub fn offset(mut self, offset: Point<Pixels>) -> Self {
        assert!(
            f32::from(offset.x).is_finite() && f32::from(offset.y).is_finite(),
            "scene offset must be finite"
        );
        self.offset = offset;
        self
    }
}

impl IntoElement for SceneElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for SceneElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: crate::size(crate::relative(1.).into(), crate::relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_state::<SceneTransform, _>(id.unwrap(), |previous, window| {
            let transform = previous.unwrap_or_else(SceneTransform::new);
            transform.update(
                window.scene_transform.clone(),
                self.scale,
                bounds.origin + self.offset,
                bounds.intersect(&window.content_mask().bounds),
            );
            transform.set_opacity(self.opacity);
            window.with_scene_transform(
                transform.clone(),
                Bounds::new(Point::default(), self.logical_size),
                |window| {
                    self.child
                        .layout_as_root(self.logical_size.into(), window, cx);
                    self.child.prepaint_at(Point::default(), window, cx);
                },
            );
            self.transform = Some(transform.clone());
            ((), transform)
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_scene_transform(
            self.transform.clone().unwrap(),
            Bounds::new(Point::default(), self.logical_size),
            |window| self.child.paint(window, cx),
        );
    }
}
