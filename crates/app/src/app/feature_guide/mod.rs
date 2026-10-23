mod cheat_sheet;
mod content;
mod controller;
mod controls;
mod demo;
mod media;
mod orchestration;
mod persistence;
mod state;
mod style;
mod view;

pub(crate) use media::{should_play_feature_guide_media, FeatureGuideMediaState};
pub(crate) use persistence::FeatureGuideSettingsState;
#[cfg(test)]
pub(crate) use persistence::{GUIDE_STATUS_KEY, GUIDE_TOPIC_KEY};
pub(crate) use state::FeatureGuidePresentation;
#[cfg(test)]
pub(crate) use state::FeatureGuideStatus;
pub(crate) use state::{
    feature_guide_crossfade_opacity, settings_crossfade_opacities, FeatureGuideBlockers,
    FeatureGuideSettingsTransition, FeatureGuideState, FeatureGuideTopic,
};

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_demo_tests.rs"]
mod demo_tests;

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_cheat_sheet_tests.rs"]
mod cheat_sheet_tests;

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_orchestration_state_tests.rs"]
mod orchestration_state_tests;
