use super::*;

#[derive(Clone, Debug)]
pub struct ContextBudget {
    pub max_bytes: usize,
    /// Output, tool schemas/results and provider-specific envelope reserved by the adapter.
    pub reserved_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct ContextQuery {
    pub text: String,
    pub references: Vec<ContextRef>,
    pub topic: Option<ContextRef>,
    pub refers_to_report: bool,
    pub memory_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct AssemblyRequest {
    pub consumer: ContextConsumer,
    pub turn_id: String,
    pub snapshot_id: String,
    pub scope: ContextScope,
    pub system_rules: String,
    pub environment: String,
    pub snapshots: Vec<ContextSnapshot>,
    pub candidates: Vec<ContextMaterial>,
    pub history: Vec<ContextHistoryTurn>,
    pub input: ContextInput,
    pub query: ContextQuery,
    pub budget: ContextBudget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextAssembly {
    pub bundle: ContextBundle,
    pub manifest: ContextManifest,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContextAssemblyError {
    #[error("required context needs {required_bytes} bytes, available {available_bytes}")]
    BudgetExceeded {
        required_bytes: usize,
        available_bytes: usize,
    },
    #[error("invalid context: {0}")]
    Invalid(&'static str),
    #[error("context requires clarification")]
    Ambiguous { candidates: Vec<ContextRef> },
    #[error("referenced context unavailable")]
    Unavailable { references: Vec<ContextRef> },
}

pub fn assemble_context(
    request: &AssemblyRequest,
) -> Result<ContextAssembly, ContextAssemblyError> {
    assemble_measured(request, bytes)
}

/// Adapters budget the representation they actually send, excluding separately billed input.
pub(super) fn assemble_measured(
    request: &AssemblyRequest,
    bytes: impl Fn(&ContextBundle) -> Result<usize, ContextAssemblyError>,
) -> Result<ContextAssembly, ContextAssemblyError> {
    super::history::validate(request)?;
    if !request.scope.is_valid() {
        return Err(ContextAssemblyError::Invalid("scope"));
    }
    let available = request
        .budget
        .max_bytes
        .saturating_sub(request.budget.reserved_bytes);
    let mut bundle = ContextBundle {
        consumer: request.consumer,
        turn_id: request.turn_id.clone(),
        snapshot_id: request.snapshot_id.clone(),
        scope: request.scope.clone(),
        system_rules: request.system_rules.clone(),
        environment: request.environment.clone(),
        working_state: super::snapshot::state_views(request)?,
        memories: vec![],
        brain: vec![],
        history: request
            .history
            .iter()
            .filter(|turn| turn.required)
            .cloned()
            .collect(),
        input: request.input.clone(),
    };
    let required = bytes(&bundle)?;
    if required > available {
        return Err(ContextAssemblyError::BudgetExceeded {
            required_bytes: required,
            available_bytes: available,
        });
    }
    let mut manifest = ContextManifest {
        snapshot_id: request.snapshot_id.clone(),
        consumer: request.consumer,
        bundle_bytes: required,
        reserved_bytes: request.budget.reserved_bytes,
        entries: vec![],
        dropped_history: vec![],
    };
    for chosen in super::selection::select(request)? {
        let material = chosen.material;
        list(&mut bundle, material.kind).push(material.clone());
        let size = bytes(&bundle)?;
        if size > available && chosen.required {
            return Err(ContextAssemblyError::BudgetExceeded {
                required_bytes: size,
                available_bytes: available,
            });
        }
        let selected = size <= available;
        if !selected {
            list(&mut bundle, material.kind).pop();
        }
        manifest.entries.push(ContextSelection {
            reference: material.reference.clone(),
            selected,
            reason: if selected { chosen.reason } else { "budget" }.into(),
            bytes: material.text.len(),
        });
    }
    for material in &request.candidates {
        if manifest
            .entries
            .iter()
            .any(|item| item.reference == material.reference)
        {
            continue;
        }
        let reason = if !material.reference.scope.readable_from(&request.scope) {
            "scope_mismatch"
        } else if material.kind == ContextMaterialKind::Memory && !request.query.memory_enabled {
            "recall_disabled"
        } else {
            "unrelated"
        };
        manifest.entries.push(ContextSelection {
            reference: material.reference.clone(),
            selected: false,
            reason: reason.into(),
            bytes: material.text.len(),
        });
    }
    let mut history_full = false;
    for turn in request.history.iter().rev().filter(|turn| !turn.required) {
        if history_full {
            manifest.dropped_history.push(turn.turn_id.clone());
            continue;
        }
        bundle.history.insert(0, turn.clone());
        if bytes(&bundle)? > available {
            bundle.history.remove(0);
            manifest.dropped_history.push(turn.turn_id.clone());
            history_full = true;
        }
    }
    manifest.bundle_bytes = bytes(&bundle)?;
    Ok(ContextAssembly { bundle, manifest })
}

fn list(bundle: &mut ContextBundle, kind: ContextMaterialKind) -> &mut Vec<ContextMaterial> {
    match kind {
        ContextMaterialKind::Memory => &mut bundle.memories,
        ContextMaterialKind::Brain => &mut bundle.brain,
    }
}

fn bytes(bundle: &ContextBundle) -> Result<usize, ContextAssemblyError> {
    serde_json::to_vec(bundle)
        .map(|value| value.len())
        .map_err(|_| ContextAssemblyError::Invalid("serialization"))
}
