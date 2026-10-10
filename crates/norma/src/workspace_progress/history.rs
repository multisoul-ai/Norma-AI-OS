use super::display::excerpt;
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedHistoricalEvidence {
    pub source: EvidenceSource,
    pub kind: EvidenceKind,
    /// Unknown original clocks stay None; the query time is not historical evidence.
    pub observed_at_unix_ms: Option<u64>,
    /// Monotonic only within the original source record, when available.
    pub source_sequence: Option<u64>,
    pub freshness: FactFreshness,
    pub quality: Vec<ProgressQuality>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedHistoricalFact {
    pub workspace_id: String,
    pub area_id: String,
    /// Original stable source record reference, never inferred from a role or task title.
    pub record_ref: String,
    pub name_at_observation: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub reason: String,
    pub evidence: UnresolvedHistoricalEvidence,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedHistoricalRecord {
    pub workspace_id: String,
    pub area_id: String,
    pub record_ref: String,
    pub name_at_observation: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub reason: String,
    pub evidence: UnresolvedHistoricalEvidence,
}

/// Filters before validating/deduplicating; no out-of-scope identity or error can leak.
pub(super) fn scoped_history(
    workspace_id: &str,
    areas: &BTreeMap<String, ProgressArea>,
    records: Vec<UnresolvedHistoricalFact>,
) -> Result<Vec<UnresolvedHistoricalRecord>, ProgressError> {
    let mut by_reference = BTreeMap::new();
    for mut record in records
        .into_iter()
        .filter(|r| r.workspace_id == workspace_id && areas.contains_key(&r.area_id))
    {
        if record.record_ref.is_empty() {
            return Err(ProgressError::InvalidFacts);
        }
        record
            .evidence
            .quality
            .push(ProgressQuality::UnresolvedLink);
        record.evidence.quality.sort();
        record.evidence.quality.dedup();
        let key = (
            record.workspace_id.clone(),
            record.area_id.clone(),
            record.evidence.source,
            record.record_ref.clone(),
        );
        let old: Option<&UnresolvedHistoricalFact> = by_reference.get(&key);
        if old.is_none_or(|old| revision_key(old) < revision_key(&record)) {
            by_reference.insert(key, record);
        }
    }
    Ok(by_reference
        .into_values()
        .map(|r| UnresolvedHistoricalRecord {
            workspace_id: r.workspace_id,
            area_id: r.area_id,
            record_ref: r.record_ref,
            name_at_observation: r.name_at_observation.map(|s| excerpt(&s, 80)),
            title: excerpt(&r.title, 120),
            summary: r.summary.map(|s| excerpt(&s, 600)),
            reason: excerpt(&r.reason, 240),
            evidence: r.evidence,
        })
        .collect())
}

fn revision_key(
    r: &UnresolvedHistoricalFact,
) -> (Option<u64>, Option<u64>, &UnresolvedHistoricalFact) {
    // Stable source sequence precedes observation time. A deterministic tie does not prove
    // progress, ownership, or a result; the selected record always remains unresolved.
    (
        r.evidence.source_sequence,
        r.evidence.observed_at_unix_ms,
        r,
    )
}
