//! Liquid Glass material for the floating Command Dock and its suggestion drawer.
//!
//! 参考 apple-design §12：Dock 是浮在画布上的功能层，玻璃折射其下内容而不遮挡它；
//! 光从左上方掠过边缘形成高光，取代原先的实色底与描边。背景过亮时玻璃自适应压暗，
//! 保证白色图标与文字的对比度。开启“降低透明度”时退回不透明的 Dock 底色。
//! 玻璃只在需要的帧绘制一次背景拷贝，不加动效；抽屉展开时与 Dock 合成同一块玻璃。

use gpui::{canvas, fill, point, prelude::*, px, Bounds, Corners, LiquidGlassStyle, Pixels};
use ui::Theme;

/// Linear-luminance ceiling for the dock glass. White over this ceiling is 5.25:1
/// ((1.0 + 0.05) / (0.15 + 0.05)), so dock text stays legible on a white wallpaper.
pub(crate) const DOCK_GLASS_MAX_LUMINANCE: f32 = 0.15;

/// Optical parameters of the dock glass for `theme`.
///
/// The tint borrows the theme's dock hue at a light strength; the dock stays a dark
/// material in every theme, matching its white icons.
pub(crate) fn dock_glass_style(theme: Theme) -> LiquidGlassStyle {
    let dock = gpui::rgba(theme.dock);
    LiquidGlassStyle {
        tint: gpui::Rgba { a: 0.30, ..dock }.into(),
        bezel: px(18.0),
        refraction: px(13.0),
        blur: px(5.0),
        saturation: 1.6,
        specular: 0.5,
        dispersion: 0.4,
        max_luminance: DOCK_GLASS_MAX_LUMINANCE,
    }
}

/// An absolutely positioned layer that paints the glass behind its parent's content.
///
/// It must be the parent's first child so the glass sits under the parent's controls.
/// `extend_top` grows the slab above the parent, letting the suggestion drawer and the
/// dock read as one piece of glass.
pub(crate) fn liquid_glass_layer(
    theme: Theme,
    corner_radii: Corners<Pixels>,
    extend_top: f32,
    reduce_transparency: bool,
) -> impl IntoElement {
    let style = dock_glass_style(theme);
    let solid = theme.dock;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let slab = Bounds::new(
                point(bounds.origin.x, bounds.origin.y - px(extend_top)),
                gpui::size(bounds.size.width, bounds.size.height + px(extend_top)),
            );
            if reduce_transparency {
                window.paint_quad(fill(slab, gpui::rgba(solid)).corner_radii(corner_radii));
            } else {
                window.paint_liquid_glass(slab, corner_radii, style);
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// macOS “Reduce transparency”, overridable with `MULTISOUL_REDUCE_TRANSPARENCY` for tests.
pub(crate) fn prefers_reduced_transparency() -> bool {
    match std::env::var("MULTISOUL_REDUCE_TRANSPARENCY") {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => system_reduce_transparency(),
    }
}

fn system_reduce_transparency() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        unsafe {
            let Some(class) = AnyClass::get(c"NSWorkspace") else {
                return false;
            };
            let workspace: *mut AnyObject = msg_send![class, sharedWorkspace];
            if workspace.is_null() {
                return false;
            }
            let reduced: bool = msg_send![workspace, accessibilityDisplayShouldReduceTransparency];
            reduced
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/dock_liquid_glass_tests.rs"]
mod tests;
