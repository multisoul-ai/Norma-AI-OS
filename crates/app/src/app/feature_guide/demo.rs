use super::style::{with_alpha, FeatureGuidePalette};
use super::FeatureGuideTopic;
use gpui::{div, img, prelude::*, px, rgba, AnyElement, ObjectFit, SharedString, Surface};
use std::path::PathBuf;
use ui::Theme;

pub(super) fn render_demo(
    topic: FeatureGuideTopic,
    poster_path: Option<PathBuf>,
    video_surface: Option<Surface>,
    compact: bool,
    reduce_motion: bool,
    playing: bool,
    theme: Theme,
    palette: FeatureGuidePalette,
) -> AnyElement {
    let has_poster = poster_path.is_some();
    let status = if reduce_motion {
        "Static Poster"
    } else if playing {
        "Playing"
    } else {
        "Paused"
    };
    let media = div()
        .id("feature-guide-demo-media")
        .debug_selector(|| "feature-guide-demo-media".to_string())
        .size_full()
        .relative()
        .overflow_hidden()
        .bg(rgba(0x07090cff))
        .when_some(poster_path, |media, path| {
            media.child(
                img(path)
                    .id(SharedString::from(format!(
                        "feature-guide-poster-{}",
                        topic.video_asset()
                    )))
                    .debug_selector(|| "feature-guide-demo-poster".to_string())
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(move || {
                        demo_placeholder("Demo poster unavailable", palette).into_any_element()
                    }),
            )
        })
        .when(!has_poster, |media| {
            media.child(demo_placeholder("Demo unavailable", palette))
        })
        .when_some(
            (!reduce_motion).then_some(video_surface).flatten(),
            |media, surface| {
                media.child(
                    surface
                        .absolute()
                        .top(px(0.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(0.0))
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
            },
        )
        .child(
            div()
                .absolute()
                .left(px(10.0))
                .top(px(10.0))
                .px(px(9.0))
                .py(px(5.0))
                .rounded(px(8.0))
                .bg(rgba(0x080b10c2))
                .text_size(px(10.0))
                .text_color(rgba(0xf4f7fbef))
                .child("REAL PRODUCT DEMO"),
        )
        .child(
            div()
                .absolute()
                .right(px(10.0))
                .bottom(px(10.0))
                .px(px(9.0))
                .py(px(5.0))
                .rounded(px(8.0))
                .bg(rgba(0x080b10c2))
                .text_size(px(10.0))
                .text_color(rgba(0xf4f7fbef))
                .child(status),
        );

    div()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .id("feature-guide-demo")
                .debug_selector(|| "feature-guide-demo".to_string())
                .w_full()
                .max_w(px(if compact { 520.0 } else { 568.0 }))
                .h(px(if compact { 190.0 } else { 320.0 }))
                .flex_none()
                .p(px(8.0))
                .rounded(px(18.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.accent_blue, 0x48)))
                .bg(rgba(with_alpha(theme.graphite, 0x18)))
                .child(media),
        )
        .into_any_element()
}

fn demo_placeholder(message: &'static str, palette: FeatureGuidePalette) -> gpui::Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .text_color(rgba(palette.metadata_text))
        .child(message)
}
