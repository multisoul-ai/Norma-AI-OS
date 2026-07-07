#[cfg(test)]
use card_task::{plan_with_provider, CardTaskPlanJsonProvider};
use card_task::{CardTaskPlanner, CardTaskPlannerError, CardTaskRequest};
use std::collections::HashMap;
use voice::{CompiledVoicePlan, VoicePlanningRequest};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CardTaskRequestKey {
    intent: String,
    tool_call_id: Option<String>,
}

impl From<&CardTaskRequest> for CardTaskRequestKey {
    fn from(value: &CardTaskRequest) -> Self {
        Self {
            intent: value.intent.clone(),
            tool_call_id: value.tool_call_id.clone(),
        }
    }
}

#[cfg(test)]
pub(super) struct StoredRequestPlanner {
    provider: Box<dyn CardTaskPlanJsonProvider>,
    requests: HashMap<CardTaskRequestKey, VoicePlanningRequest>,
}

#[cfg(test)]
impl StoredRequestPlanner {
    fn new(provider: Box<dyn CardTaskPlanJsonProvider>) -> Self {
        Self {
            provider,
            requests: HashMap::new(),
        }
    }

    fn insert(&mut self, request: &CardTaskRequest, planning_request: VoicePlanningRequest) {
        self.requests
            .insert(CardTaskRequestKey::from(request), planning_request);
    }

    #[cfg(test)]
    fn replace_provider(&mut self, provider: Box<dyn CardTaskPlanJsonProvider>) {
        self.provider = provider;
        self.requests.clear();
    }
}

#[cfg(test)]
impl CardTaskPlanner for StoredRequestPlanner {
    fn plan(
        &mut self,
        request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        let key = CardTaskRequestKey::from(request);
        let planning_request =
            self.requests
                .get(&key)
                .ok_or_else(|| CardTaskPlannerError::Provider {
                    message: format!("missing planning request snapshot for {}", request.intent),
                })?;
        match plan_with_provider(self.provider.as_mut(), planning_request) {
            Err(CardTaskPlannerError::Pending) => Err(CardTaskPlannerError::Pending),
            result => {
                self.requests.remove(&key);
                result
            }
        }
    }
}

#[derive(Clone, Debug)]
enum BackgroundPlanningResult {
    Pending,
    Ready(CompiledVoicePlan),
    Failed(CardTaskPlannerError),
}

#[derive(Default)]
pub(super) struct BackgroundRequestPlanner {
    results: HashMap<CardTaskRequestKey, BackgroundPlanningResult>,
}

impl BackgroundRequestPlanner {
    fn insert_pending(&mut self, request: &CardTaskRequest) {
        self.results.insert(
            CardTaskRequestKey::from(request),
            BackgroundPlanningResult::Pending,
        );
    }

    fn finish(
        &mut self,
        request: CardTaskRequest,
        result: Result<CompiledVoicePlan, CardTaskPlannerError>,
    ) {
        let result = match result {
            Ok(compiled) => BackgroundPlanningResult::Ready(compiled),
            Err(error) => BackgroundPlanningResult::Failed(error),
        };
        self.results
            .insert(CardTaskRequestKey::from(&request), result);
    }
}

impl CardTaskPlanner for BackgroundRequestPlanner {
    fn plan(
        &mut self,
        request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        let key = CardTaskRequestKey::from(request);
        let Some(result) = self.results.get(&key).cloned() else {
            return Err(CardTaskPlannerError::Provider {
                message: format!("missing background planning result for {}", request.intent),
            });
        };

        match result {
            BackgroundPlanningResult::Pending => Err(CardTaskPlannerError::Pending),
            BackgroundPlanningResult::Ready(compiled) => {
                self.results.remove(&key);
                Ok(compiled)
            }
            BackgroundPlanningResult::Failed(error) => {
                self.results.remove(&key);
                Err(error)
            }
        }
    }
}

pub(super) enum RealtimeCardTaskPlannerMode {
    Background(BackgroundRequestPlanner),
    #[cfg(test)]
    PreparedJson(StoredRequestPlanner),
    #[cfg(test)]
    Direct(Box<dyn CardTaskPlanner>),
}

impl RealtimeCardTaskPlannerMode {
    pub(super) fn insert_snapshot(
        &mut self,
        request: &CardTaskRequest,
        planning_request: VoicePlanningRequest,
    ) {
        #[cfg(not(test))]
        let _ = &planning_request;
        match self {
            Self::Background(planner) => planner.insert_pending(request),
            #[cfg(test)]
            Self::PreparedJson(planner) => planner.insert(request, planning_request),
            #[cfg(test)]
            Self::Direct(_) => {}
        }
    }

    pub(super) fn finish_background_result(
        &mut self,
        request: CardTaskRequest,
        result: Result<CompiledVoicePlan, CardTaskPlannerError>,
    ) {
        match self {
            Self::Background(planner) => planner.finish(request, result),
            #[cfg(test)]
            Self::PreparedJson(_) | Self::Direct(_) => {}
        }
    }

    #[cfg(test)]
    pub(super) fn set_compiled_planner_for_tests(&mut self, planner: Box<dyn CardTaskPlanner>) {
        *self = Self::Direct(planner);
    }

    #[cfg(test)]
    pub(super) fn set_json_planner_for_tests(
        &mut self,
        planner: Box<dyn CardTaskPlanJsonProvider>,
    ) {
        match self {
            Self::Background(_) => *self = Self::PreparedJson(StoredRequestPlanner::new(planner)),
            Self::PreparedJson(stored) => stored.replace_provider(planner),
            Self::Direct(_) => *self = Self::PreparedJson(StoredRequestPlanner::new(planner)),
        }
    }
}

impl Default for RealtimeCardTaskPlannerMode {
    fn default() -> Self {
        Self::Background(BackgroundRequestPlanner::default())
    }
}

impl CardTaskPlanner for RealtimeCardTaskPlannerMode {
    fn plan(
        &mut self,
        request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        match self {
            Self::Background(planner) => planner.plan(request),
            #[cfg(test)]
            Self::PreparedJson(planner) => planner.plan(request),
            #[cfg(test)]
            Self::Direct(planner) => planner.plan(request),
        }
    }
}
