mod content;
mod controller;
mod demo;
mod media;
mod persistence;
mod state;
mod style;
mod view;

pub(crate) use media::{should_play_feature_guide_media, FeatureGuideMediaState};
pub(crate) use persistence::FeatureGuideSettingsState;
#[cfg(test)]
pub(crate) use persistence::{GUIDE_STATUS_KEY, GUIDE_TOPIC_KEY};
#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) use state::FeatureGuidePresentation;
#[cfg(test)]
pub(crate) use state::FeatureGuideStatus;
pub(crate) use state::{FeatureGuideBlockers, FeatureGuideState, FeatureGuideTopic};

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_demo_tests.rs"]
mod demo_tests;
