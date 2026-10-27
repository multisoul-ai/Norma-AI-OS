use super::{assembly::AssemblyRequest, *};

pub(super) struct Selected<'a> {
    pub material: &'a ContextMaterial,
    pub required: bool,
    pub reason: &'static str,
}

pub(super) fn select(request: &AssemblyRequest) -> Result<Vec<Selected<'_>>, ContextAssemblyError> {
    let mut readable: Vec<&ContextMaterial> = Vec::new();
    for material in &request.candidates {
        if !material.reference.scope.readable_from(&request.scope)
            || (material.kind == ContextMaterialKind::Memory && !request.query.memory_enabled)
        {
            continue;
        }
        if let Some(previous) = readable
            .iter()
            .find(|item| item.reference == material.reference)
        {
            if **previous != *material {
                return Err(ContextAssemblyError::Invalid("conflicting_source"));
            }
        } else {
            readable.push(material);
        }
    }
    let mut references = request.query.references.clone();
    if let Some(topic) = &request.query.topic {
        references.push(topic.clone());
    }
    if request.query.refers_to_report && references.is_empty() {
        let spoken: Vec<_> = readable
            .iter()
            .filter(|item| {
                matches!(
                    item.delivery,
                    Some(
                        ReportDelivery::Playing
                            | ReportDelivery::Completed
                            | ReportDelivery::Interrupted
                    )
                )
            })
            .map(|item| item.reference.clone())
            .collect();
        match spoken.len() {
            0 => return Err(ContextAssemblyError::Unavailable { references: vec![] }),
            1 => references.extend(spoken),
            _ => return Err(ContextAssemblyError::Ambiguous { candidates: spoken }),
        }
    }
    let mut selected = Vec::new();
    while let Some(reference) = references.pop() {
        if selected
            .iter()
            .any(|item: &Selected<'_>| item.material.reference == reference)
        {
            continue;
        }
        let material = readable
            .iter()
            .copied()
            .find(|item| item.reference == reference)
            .ok_or_else(|| ContextAssemblyError::Unavailable {
                references: vec![reference.clone()],
            })?;
        references.extend(material.related.clone());
        selected.push(Selected {
            material,
            required: true,
            reason: "referenced",
        });
    }
    let query = request.query.text.trim().to_lowercase();
    for material in readable {
        if selected
            .iter()
            .any(|item| item.material.reference == material.reference)
        {
            continue;
        }
        if !query.is_empty() && material.text.to_lowercase().contains(&query) {
            selected.push(Selected {
                material,
                required: false,
                reason: "relevant",
            });
        }
    }
    // Stable ordering independent of retrieval arrival order.
    selected.sort_by(|a, b| {
        b.required.cmp(&a.required).then_with(|| {
            a.material
                .reference
                .event_id
                .cmp(&b.material.reference.event_id)
        })
    });
    Ok(selected)
}
