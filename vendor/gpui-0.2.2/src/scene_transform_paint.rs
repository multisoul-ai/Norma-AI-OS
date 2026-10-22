//! Re-present retained paint commands without rerunning content layout or reading pixels back.
use crate::{
    Bounds, ContentMask, Point, ScaledPixels, scene::Primitive, scene_transform::SceneTransform,
};

#[derive(Clone)]
pub(crate) struct ScenePlacement {
    pub transform: SceneTransform,
    pub scale_factor: f32,
}

impl ScenePlacement {
    pub(crate) fn bounds(&self, bounds: Bounds<ScaledPixels>) -> Bounds<ScaledPixels> {
        let transform = self.transform.resolve();
        Bounds::new(
            bounds.origin * transform.scale + transform.offset.scale(self.scale_factor),
            bounds.size.map(|v| v * transform.scale),
        )
    }

    pub(crate) fn primitive(&self, primitive: &mut Primitive) {
        let transform = self.transform.resolve();
        let scale = transform.scale;
        let opacity = transform.opacity;
        let offset = transform.offset.scale(self.scale_factor);
        let point = |point: Point<ScaledPixels>| point * scale + offset;
        let bounds = |bounds: Bounds<ScaledPixels>| {
            Bounds::new(point(bounds.origin), bounds.size.map(|v| v * scale))
        };
        let mask = |mask: &ContentMask<ScaledPixels>| ContentMask {
            bounds: bounds(mask.bounds).intersect(&transform.clip.scale(self.scale_factor)),
        };
        match primitive {
            Primitive::Quad(quad) => {
                quad.background = quad.background.opacity(opacity);
                quad.border_color = quad.border_color.opacity(opacity);
                quad.bounds = bounds(quad.bounds);
                quad.content_mask = mask(&quad.content_mask);
                quad.corner_radii = quad.corner_radii.map(|v| *v * scale);
                quad.border_widths = quad.border_widths.map(|v| *v * scale);
            }
            Primitive::Shadow(shadow) => {
                shadow.color = shadow.color.opacity(opacity);
                shadow.bounds = bounds(shadow.bounds);
                shadow.content_mask = mask(&shadow.content_mask);
                shadow.corner_radii = shadow.corner_radii.map(|v| *v * scale);
                shadow.blur_radius *= scale;
            }
            Primitive::Underline(underline) => {
                underline.color = underline.color.opacity(opacity);
                underline.bounds = bounds(underline.bounds);
                underline.content_mask = mask(&underline.content_mask);
                underline.thickness *= scale;
            }
            Primitive::MonochromeSprite(sprite) => {
                sprite.color = sprite.color.opacity(opacity);
                sprite.bounds = bounds(sprite.bounds);
                sprite.content_mask = mask(&sprite.content_mask);
                // New bounds already contain A. Conjugate the glyph/SVG matrix T as
                // A*T*A^-1, preserving rotation and translation without scaling twice.
                let matrix = &mut sprite.transformation;
                let [x, y] = [offset.x.0, offset.y.0];
                matrix.translation = [
                    scale * matrix.translation[0] + x
                        - matrix.rotation_scale[0][0] * x
                        - matrix.rotation_scale[0][1] * y,
                    scale * matrix.translation[1] + y
                        - matrix.rotation_scale[1][0] * x
                        - matrix.rotation_scale[1][1] * y,
                ];
            }
            Primitive::PolychromeSprite(sprite) => {
                sprite.opacity *= opacity;
                sprite.bounds = bounds(sprite.bounds);
                sprite.content_mask = mask(&sprite.content_mask);
                sprite.corner_radii = sprite.corner_radii.map(|v| *v * scale);
            }
            Primitive::Path(path) => {
                path.color = path.color.opacity(opacity);
                path.bounds = bounds(path.bounds);
                path.content_mask = mask(&path.content_mask);
                for vertex in &mut path.vertices {
                    vertex.xy_position = point(vertex.xy_position);
                    vertex.content_mask = mask(&vertex.content_mask);
                }
            }
            Primitive::Surface(surface) => {
                surface.opacity *= opacity;
                surface.bounds = bounds(surface.bounds);
                surface.content_mask = mask(&surface.content_mask);
            }
        }
    }
}
