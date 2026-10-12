use std::time::Duration;

use crate::app::wallpaper_resolvers::WallpaperKind;
use crate::motion::{scalar::AnimatedScalar, util::ease_out_cubic};

use super::state::{selection_kind, WallpaperSelection, WallpaperSettingsState};

#[derive(Clone, Debug)]
pub(super) struct WallpaperTransition {
    layers: Vec<WallpaperTransitionLayer>,
    progress: AnimatedScalar,
}

#[derive(Clone, Debug)]
struct WallpaperTransitionLayer {
    selection: WallpaperSelection,
    opacity: f32,
}

impl WallpaperSettingsState {
    pub(crate) fn set_effective_selection(
        &mut self,
        selection: WallpaperSelection,
        reduce_motion: bool,
    ) {
        if self.selection == selection {
            return;
        }
        let previous = std::mem::replace(&mut self.selection, selection);
        if reduce_motion
            || selection_kind(&previous) == WallpaperKind::Video
            || selection_kind(&self.selection) == WallpaperKind::Video
        {
            // shortcut: video surfaces have a separate lifecycle; fade them when that lifecycle can retain two surfaces.
            self.transition = None;
            return;
        }
        self.start_transition(previous);
    }

    pub(crate) fn image_resource_ready(&mut self, rel: &str, reduce_motion: bool) {
        if !matches!(&self.selection, WallpaperSelection::Builtin { rel: selected } if *selected == rel)
            || selection_kind(&self.selection) == WallpaperKind::Video
        {
            return;
        }
        if reduce_motion {
            self.transition = None;
            return;
        }
        self.start_transition(WallpaperSelection::default());
    }

    fn start_transition(&mut self, previous: WallpaperSelection) {
        let mut transition = self.transition.take().unwrap_or(WallpaperTransition {
            layers: Vec::new(),
            progress: AnimatedScalar::on(),
        });
        let visible = 1.0 - ease_out_cubic(transition.progress.value);
        for layer in &mut transition.layers {
            layer.opacity *= visible;
        }
        transition.layers.retain(|layer| layer.opacity > 0.01);
        transition.layers.insert(
            0,
            WallpaperTransitionLayer {
                selection: previous,
                opacity: 1.0,
            },
        );
        transition.progress.value = 0.0;
        transition.progress.target = 1.0;
        self.transition = Some(transition);
    }

    #[cfg(test)]
    pub(crate) fn transition_from(&self) -> Option<&WallpaperSelection> {
        self.transition
            .as_ref()
            .and_then(|transition| transition.layers.first())
            .map(|layer| &layer.selection)
    }

    #[cfg(test)]
    pub(crate) fn transition_from_opacity(&self) -> f32 {
        self.transition_layers()
            .next()
            .map_or(0.0, |(_, opacity)| opacity)
    }

    pub(crate) fn transition_layers(&self) -> impl Iterator<Item = (&WallpaperSelection, f32)> {
        self.transition.iter().flat_map(|transition| {
            let visible = 1.0 - ease_out_cubic(transition.progress.value);
            transition
                .layers
                .iter()
                .map(move |layer| (&layer.selection, layer.opacity * visible))
        })
    }

    pub(crate) fn transition_active(&self) -> bool {
        self.transition.is_some()
    }

    pub(crate) fn tick_transition(&mut self, delta: Duration) -> bool {
        let Some(transition) = self.transition.as_mut() else {
            return false;
        };
        transition.progress.tick(delta, Duration::from_millis(220));
        if transition.progress.value >= 1.0 {
            self.transition = None;
        }
        true
    }
}
