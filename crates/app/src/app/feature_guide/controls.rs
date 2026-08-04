use super::style::{with_alpha, FeatureGuidePalette};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, KeyDownEvent};
use std::rc::Rc;
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GuideButtonKind {
    Primary,
    OrchestrationSettings,
    Playback,
    Skip,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GuideButtonInteraction {
    pub(super) background: u32,
    pub(super) foreground: u32,
    pub(super) hover: u32,
    pub(super) active: u32,
    pub(super) tab_index: isize,
    pub(super) tab_stop: bool,
}

impl GuideButtonKind {
    fn selector(self) -> &'static str {
        match self {
            Self::Primary => "feature-guide-primary",
            Self::OrchestrationSettings => "feature-guide-orchestration-open-settings",
            Self::Playback => "feature-guide-playback",
            Self::Skip => "feature-guide-skip",
            Self::Close => "feature-guide-close",
        }
    }

    pub(super) fn interaction(
        self,
        theme: Theme,
        palette: FeatureGuidePalette,
    ) -> GuideButtonInteraction {
        let primary = matches!(self, Self::Primary | Self::OrchestrationSettings);
        GuideButtonInteraction {
            background: if primary {
                palette.primary_background
            } else {
                with_alpha(theme.text_primary, 0x0d)
            },
            foreground: if primary {
                palette.primary_foreground
            } else {
                palette.supporting_text
            },
            hover: if primary {
                palette.primary_hover
            } else {
                with_alpha(theme.text_primary, 0x18)
            },
            active: if primary {
                palette.primary_active
            } else {
                with_alpha(theme.text_primary, 0x24)
            },
            tab_index: match self {
                Self::Primary => 1,
                Self::OrchestrationSettings => 6,
                Self::Playback => 7,
                Self::Skip => 8,
                Self::Close => 9,
            },
            tab_stop: self != Self::Primary,
        }
    }
}

pub(super) fn is_unmodified_activation(event: &KeyDownEvent) -> bool {
    !event.keystroke.modifiers.modified()
        && matches!(event.keystroke.key.as_str(), "enter" | "space")
}

pub(super) fn guide_button(
    kind: GuideButtonKind,
    label: &'static str,
    theme: Theme,
    palette: FeatureGuidePalette,
    cx: &mut Context<MultisoulApp>,
    handler: impl Fn(&mut MultisoulApp, &mut Context<MultisoulApp>) + 'static,
) -> AnyElement {
    let selector = kind.selector();
    let interaction = kind.interaction(theme, palette);
    let handler = Rc::new(handler);
    let click_handler = handler.clone();
    let keyboard_handler = handler;
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(interaction.background).into())
        .foreground(rgba(interaction.foreground).into())
        .border(rgba(with_alpha(theme.accent_blue, 0x58)).into())
        .hover(rgba(interaction.hover).into())
        .active(rgba(interaction.active).into());
    Button::new(selector)
        .debug_selector(move || selector.to_string())
        .child(div().text_color(rgba(interaction.foreground)).child(label))
        .tab_index(interaction.tab_index)
        .tab_stop(interaction.tab_stop)
        .custom(variant)
        .h(px(36.0))
        .px(px(14.0))
        .rounded(px(10.0))
        .border_color(rgba(with_alpha(theme.node_stroke, 0x78)))
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            click_handler(app, cx);
        }))
        .on_key_down(cx.listener(move |app, event, _, cx| {
            if is_unmodified_activation(event) {
                cx.stop_propagation();
                keyboard_handler(app, cx);
            }
        }))
        .into_any_element()
}
