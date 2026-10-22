use super::*;
use crate::{bounds, point, size};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    bounds(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(w), ScaledPixels(h)),
    )
}

fn quad_at(bounds: Bounds<ScaledPixels>) -> Quad {
    Quad {
        bounds,
        content_mask: ContentMask { bounds },
        ..Default::default()
    }
}

fn glass_at(bounds: Bounds<ScaledPixels>) -> LiquidGlass {
    LiquidGlass {
        order: 0,
        pad: 0,
        bounds,
        content_mask: ContentMask { bounds },
        corner_radii: Corners::default(),
        tint: crate::hsla(0.0, 0.0, 0.0, 0.3),
        bezel: 16.0,
        refraction: 12.0,
        blur: 3.0,
        saturation: 1.5,
        specular: 0.5,
        dispersion: 0.3,
        opacity: 1.0,
        max_luminance: 0.2,
    }
}

fn batch_kinds(scene: &Scene) -> Vec<&'static str> {
    scene
        .batches()
        .map(|batch| match batch {
            PrimitiveBatch::Quads(_) => "quads",
            PrimitiveBatch::LiquidGlasses(_) => "glass",
            _ => "other",
        })
        .collect()
}

/// Scenario: a dock paints glass over the canvas, then its icon on top.
///
/// Data: a full-window canvas quad, a 200×50 glass slab inside it, then an icon
/// quad inside the slab — painted in that order, as the Command Dock does.
/// Steps: finish the scene and list its batches.
/// Expectation: the canvas is drawn first (the glass refracts it), the glass
/// second, and the icon last (it sits on the glass, never under it).
#[test]
fn liquid_glass_batches_between_backdrop_and_content() {
    let mut scene = Scene::default();
    scene.insert_primitive(quad_at(rect(0.0, 0.0, 800.0, 600.0)));
    scene.insert_primitive(glass_at(rect(300.0, 520.0, 200.0, 50.0)));
    scene.insert_primitive(quad_at(rect(310.0, 530.0, 30.0, 30.0)));
    scene.finish();

    assert_eq!(
        batch_kinds(&scene),
        vec!["quads", "glass", "quads"],
        "the glass must split the backdrop from the content painted on it"
    );
    assert_eq!(scene.liquid_glasses.len(), 1, "exactly one glass slab is recorded");
}

/// Scenario: the glass slab is fully clipped away (for example scrolled out).
///
/// Data: a glass whose content mask does not intersect its bounds.
/// Steps: insert it and finish the scene.
/// Expectation: nothing is recorded, so the renderer never pays for a backdrop copy.
#[test]
fn clipped_liquid_glass_is_dropped() {
    let mut scene = Scene::default();
    let mut glass = glass_at(rect(0.0, 0.0, 100.0, 40.0));
    glass.content_mask = ContentMask {
        bounds: rect(500.0, 500.0, 10.0, 10.0),
    };
    scene.insert_primitive(glass);
    scene.finish();

    assert!(
        scene.liquid_glasses.is_empty(),
        "a glass outside its clip must not produce a batch"
    );
    assert_eq!(batch_kinds(&scene), Vec::<&str>::new(), "no batches at all");
}
