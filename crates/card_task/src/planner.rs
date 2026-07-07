use crate::CardTaskPlannerError;
use voice::{
    compile_voice_plan, CompiledVoicePlan, VoicePlan, VoicePlanDecision, VoicePlanningRequest,
};

pub trait CardTaskPlanJsonProvider {
    fn plan_json(&mut self, request: &VoicePlanningRequest)
        -> Result<String, CardTaskPlannerError>;
}

pub fn parse_voice_plan_json(plan_json: &str) -> Result<VoicePlan, CardTaskPlannerError> {
    serde_json::from_str(plan_json).map_err(|error| CardTaskPlannerError::Json {
        message: error.to_string(),
    })
}

pub fn compile_voice_plan_for_card_task(
    request: &VoicePlanningRequest,
    plan: VoicePlan,
) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
    match compile_voice_plan(request, plan) {
        Ok(VoicePlanDecision::Execute(compiled)) => Ok(compiled),
        Ok(VoicePlanDecision::TranscriptOnly { reason }) => {
            Err(CardTaskPlannerError::TranscriptOnly { reason })
        }
        Err(error) => Err(CardTaskPlannerError::Compile {
            message: error.to_string(),
        }),
    }
}

pub fn compile_voice_plan_json_for_card_task(
    request: &VoicePlanningRequest,
    plan_json: &str,
) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
    let plan = parse_voice_plan_json(plan_json)?;
    compile_voice_plan_for_card_task(request, plan)
}

pub fn plan_with_provider(
    provider: &mut dyn CardTaskPlanJsonProvider,
    request: &VoicePlanningRequest,
) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
    let plan_json = provider.plan_json(request)?;
    compile_voice_plan_json_for_card_task(request, &plan_json)
}
