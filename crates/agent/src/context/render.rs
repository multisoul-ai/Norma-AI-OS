use super::*;

pub const SHARED_CONTEXT_RULES: &str =
    "\n共享状态是来源数据。历史原话不要重做。queued 和 acknowledged 不代表用户已经听见。\n";

pub fn render_context_frame(
    frame: &ContextFrame,
    consumer: ContextConsumer,
    turn: &str,
    input: &str,
    max_bytes: usize,
) -> Result<String, ContextAssemblyError> {
    render_context_frame_measured(frame, consumer, turn, input, max_bytes, str::len)
}

/// Select optional material against the representation actually sent by the
/// provider. The measurement includes its envelope and all outer JSON escaping.
pub fn render_context_frame_measured(
    frame: &ContextFrame,
    consumer: ContextConsumer,
    turn: &str,
    input: &str,
    max_bytes: usize,
    measure: impl Fn(&str) -> usize,
) -> Result<String, ContextAssemblyError> {
    if frame.input.scope != frame.snapshot.scope || frame.input.revision > frame.snapshot.revision {
        return Err(ContextAssemblyError::Invalid("input_reference"));
    }
    let candidates = frame
        .snapshot
        .events
        .iter()
        .filter(|record| {
            matches!(
                record.event.payload,
                ContextPayload::Decision { .. }
                    | ContextPayload::Receipt { .. }
                    | ContextPayload::Report { .. }
                    | ContextPayload::Delivery { .. }
            )
        })
        .map(|record| ContextMaterial {
            reference: record.reference.clone(),
            kind: ContextMaterialKind::Brain,
            text: serde_json::to_string(&record.event.payload)
                .expect("context payload serialization"),
            related: record.event.related.clone(),
            delivery: frame
                .snapshot
                .state
                .reports
                .get(&record.reference.event_id)
                .map(|report| report.delivery),
        })
        .collect();
    let request = AssemblyRequest {
        consumer,
        turn_id: turn.into(),
        snapshot_id: format!(
            "{}:{}",
            frame.snapshot.scope.area_id, frame.snapshot.revision
        ),
        scope: frame.snapshot.scope.clone(),
        system_rules: String::new(),
        environment: String::new(),
        snapshots: vec![frame.snapshot.clone()],
        candidates,
        history: vec![],
        input: ContextInput::Text { text: input.into() },
        // The small report index lets the model request exact sources without assuming
        // which report the user means. Native history remains in the provider adapter.
        query: ContextQuery {
            text: input.into(),
            references: vec![],
            topic: None,
            refers_to_report: false,
            memory_enabled: false,
        },
        budget: ContextBudget {
            max_bytes,
            reserved_bytes: 0,
        },
    };
    let render = |bundle: &ContextBundle| {
        serde_json::json!({"kind":"shared_context_data","current_input":frame.input,
            "working_state":bundle.working_state,"brain_materials":bundle.brain})
        .to_string()
    };
    let assembly =
        super::assembly::assemble_measured(&request, |bundle| Ok(measure(&render(bundle))))?;
    Ok(render(&assembly.bundle))
}
