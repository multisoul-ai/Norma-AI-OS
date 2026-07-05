use norma::protocol::{AgentProfile, CardCommand, CardKind, Placement};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MIN_EXECUTE_CONFIDENCE: f32 = 0.65;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoicePlanningRequest {
    pub transcript: String,
    pub cards: Vec<CardRef>,
    pub focused_node_id: Option<u64>,
    pub selected_node_ids: Vec<u64>,
    pub max_steps: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CardRef {
    pub node_id: u64,
    pub title: String,
    pub alias: Option<String>,
    pub kind: VoiceCardKind,
    pub selected: bool,
    pub focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VoiceCardKind {
    Terminal,
    Agent { profile: AgentProfile },
    Preview,
    Browser,
    Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoicePlan {
    pub commands: Vec<PlannedCommand>,
    pub confidence: f32,
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PlannedCommand {
    Create {
        kind: PlannedCardKind,
    },
    Focus {
        target: PlannedTarget,
    },
    SendText {
        target: PlannedTarget,
        text: String,
        #[serde(default)]
        submit: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PlannedCardKind {
    Terminal,
    Agent { profile: AgentProfile },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PlannedTarget {
    NodeId { node_id: u64 },
    Alias { alias: String },
    PreviousStep { step: usize },
    Focused,
    Selected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoicePlanDecision {
    Execute(CompiledVoicePlan),
    TranscriptOnly { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledVoicePlan {
    pub commands: Vec<ExecutableCommand>,
    pub requires_confirmation: bool,
    pub risk: RiskLevel,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutableCommand {
    Card(CardCommand),
    SendTextToPrevious {
        previous_step: usize,
        text: String,
        submit: bool,
    },
    FocusPrevious {
        previous_step: usize,
    },
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum VoicePlanError {
    #[error("planner returned no commands")]
    EmptyPlan,
    #[error("planner returned {actual} commands, max allowed is {max}")]
    TooManySteps { actual: usize, max: usize },
    #[error("planner confidence {confidence} is below execute threshold {threshold}")]
    LowConfidence {
        confidence: String,
        threshold: String,
    },
    #[error("target alias `{alias}` matched no card")]
    AliasNotFound { alias: String },
    #[error("target alias `{alias}` matched multiple cards")]
    AliasAmbiguous { alias: String },
    #[error("selected target requires exactly one selected card, found {count}")]
    SelectedTargetAmbiguous { count: usize },
    #[error("focused target requires a focused card")]
    FocusedTargetMissing,
    #[error("previous step {step} cannot be referenced here")]
    InvalidPreviousStep { step: usize },
    #[error("card {node_id} was not found in planner context")]
    MissingCard { node_id: u64 },
    #[error("delete or unsupported command is not allowed in voice v1")]
    UnsupportedCommand,
}

pub fn compile_voice_plan(
    request: &VoicePlanningRequest,
    plan: VoicePlan,
) -> Result<VoicePlanDecision, VoicePlanError> {
    if plan.commands.is_empty() {
        return Err(VoicePlanError::EmptyPlan);
    }
    if plan.commands.len() > request.max_steps {
        return Err(VoicePlanError::TooManySteps {
            actual: plan.commands.len(),
            max: request.max_steps,
        });
    }
    if plan.confidence < MIN_EXECUTE_CONFIDENCE {
        return Ok(VoicePlanDecision::TranscriptOnly {
            reason: format!(
                "planner confidence {:.2} below {:.2}",
                plan.confidence, MIN_EXECUTE_CONFIDENCE
            ),
        });
    }

    let cards_by_id: HashMap<u64, &CardRef> = request
        .cards
        .iter()
        .map(|card| (card.node_id, card))
        .collect();
    let mut commands = Vec::with_capacity(plan.commands.len());
    let mut created_kinds: HashMap<usize, PlannedCardKind> = HashMap::new();
    let mut requires_confirmation = plan.requires_confirmation || plan.risk == RiskLevel::High;

    for (step_index, command) in plan.commands.iter().enumerate() {
        match command {
            PlannedCommand::Create { kind } => {
                created_kinds.insert(step_index, kind.clone());
                commands.push(ExecutableCommand::Card(CardCommand::Create {
                    kind: card_kind(kind),
                    placement: Placement::VisibleArea,
                }));
            }
            PlannedCommand::Focus { target } => {
                match resolve_target(request, &cards_by_id, target, step_index, &created_kinds)? {
                    ResolvedTarget::Existing(card) => {
                        commands.push(ExecutableCommand::Card(CardCommand::Focus {
                            node_id: card.node_id,
                        }));
                    }
                    ResolvedTarget::Previous(step) => {
                        commands.push(ExecutableCommand::FocusPrevious {
                            previous_step: step,
                        });
                    }
                }
            }
            PlannedCommand::SendText {
                target,
                text,
                submit,
            } => match resolve_target(request, &cards_by_id, target, step_index, &created_kinds)? {
                ResolvedTarget::Existing(card) => {
                    if *submit && matches!(card.kind, VoiceCardKind::Terminal) {
                        requires_confirmation = true;
                    }
                    commands.push(ExecutableCommand::Card(CardCommand::SendText {
                        node_id: card.node_id,
                        text: text.clone(),
                        submit: *submit,
                    }));
                }
                ResolvedTarget::Previous(step) => {
                    if *submit
                        && matches!(created_kinds.get(&step), Some(PlannedCardKind::Terminal))
                    {
                        requires_confirmation = true;
                    }
                    commands.push(ExecutableCommand::SendTextToPrevious {
                        previous_step: step,
                        text: text.clone(),
                        submit: *submit,
                    });
                }
            },
        }
    }

    Ok(VoicePlanDecision::Execute(CompiledVoicePlan {
        commands,
        requires_confirmation,
        risk: plan.risk,
        reason: plan.reason,
    }))
}

enum ResolvedTarget<'a> {
    Existing(&'a CardRef),
    Previous(usize),
}

fn resolve_target<'a>(
    request: &'a VoicePlanningRequest,
    cards_by_id: &HashMap<u64, &'a CardRef>,
    target: &PlannedTarget,
    current_step: usize,
    created_kinds: &HashMap<usize, PlannedCardKind>,
) -> Result<ResolvedTarget<'a>, VoicePlanError> {
    match target {
        PlannedTarget::NodeId { node_id } => cards_by_id
            .get(node_id)
            .copied()
            .map(ResolvedTarget::Existing)
            .ok_or(VoicePlanError::MissingCard { node_id: *node_id }),
        PlannedTarget::Alias { alias } => resolve_alias(request, alias),
        PlannedTarget::Focused => {
            let node_id = request
                .focused_node_id
                .ok_or(VoicePlanError::FocusedTargetMissing)?;
            cards_by_id
                .get(&node_id)
                .copied()
                .map(ResolvedTarget::Existing)
                .ok_or(VoicePlanError::MissingCard { node_id })
        }
        PlannedTarget::Selected => {
            if request.selected_node_ids.len() != 1 {
                return Err(VoicePlanError::SelectedTargetAmbiguous {
                    count: request.selected_node_ids.len(),
                });
            }
            let node_id = request.selected_node_ids[0];
            cards_by_id
                .get(&node_id)
                .copied()
                .map(ResolvedTarget::Existing)
                .ok_or(VoicePlanError::MissingCard { node_id })
        }
        PlannedTarget::PreviousStep { step } => {
            if *step >= current_step || !created_kinds.contains_key(step) {
                return Err(VoicePlanError::InvalidPreviousStep { step: *step });
            }
            Ok(ResolvedTarget::Previous(*step))
        }
    }
}

fn resolve_alias<'a>(
    request: &'a VoicePlanningRequest,
    alias: &str,
) -> Result<ResolvedTarget<'a>, VoicePlanError> {
    let normalized = normalize_name(alias);
    let matches: Vec<&CardRef> = request
        .cards
        .iter()
        .filter(|card| {
            normalize_name(&card.title) == normalized
                || card
                    .alias
                    .as_deref()
                    .map(normalize_name)
                    .is_some_and(|value| value == normalized)
        })
        .collect();

    match matches.as_slice() {
        [card] => Ok(ResolvedTarget::Existing(card)),
        [] => Err(VoicePlanError::AliasNotFound {
            alias: alias.to_string(),
        }),
        _ => Err(VoicePlanError::AliasAmbiguous {
            alias: alias.to_string(),
        }),
    }
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn card_kind(kind: &PlannedCardKind) -> CardKind {
    match kind {
        PlannedCardKind::Terminal => CardKind::Terminal { shell: None },
        PlannedCardKind::Agent { profile } => CardKind::Agent { profile: *profile },
    }
}

#[cfg(test)]
#[path = "../tests/unit/plan_tests.rs"]
mod tests;
