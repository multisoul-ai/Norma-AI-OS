mod content;
mod controller;
mod demo;
mod persistence;
mod state;
mod style;
mod view;
mod voice_action;

pub(crate) use persistence::FeatureGuideSettingsState;
#[cfg(test)]
pub(crate) use persistence::{GUIDE_STATUS_KEY, GUIDE_TOPIC_KEY};
#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) use state::FeatureGuidePresentation;
#[cfg(test)]
pub(crate) use state::FeatureGuideStatus;
pub(crate) use state::{FeatureGuideBlockers, FeatureGuideState, FeatureGuideTopic};
#[cfg(test)]
pub(crate) use voice_action::FeatureGuideVoiceAction;

#[cfg(test)]
#[path = "../../../tests/unit/feature_guide_state_tests.rs"]
mod tests;
