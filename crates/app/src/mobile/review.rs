use serde::{Deserialize, Serialize};
use std::time::Instant;
use workspace::review::ProjectReview;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectReviewRequest {
    pub workspace_id: String,
    pub route_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_key: Option<storage::MobileTurnKey>,
}
impl ProjectReviewRequest {
    pub fn valid(&self) -> bool {
        super::command::valid_id(&self.workspace_id)
            && super::command::valid_id(&self.route_id)
            && self
                .delivery_key
                .as_ref()
                .is_none_or(|key| key.validate().is_ok())
    }
}

pub struct LiveReviewRequest {
    pub request: ProjectReviewRequest,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Result<ProjectReview, &'static str>>,
}
