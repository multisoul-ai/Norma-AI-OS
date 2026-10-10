use super::display::excerpt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CoverageFacet {
    Directory,
    Tasks,
    Activity,
    Outcomes,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Complete,
    Partial,
    Missing,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressCoverageFact {
    pub workspace_id: String,
    pub area_id: String,
    pub facet: CoverageFacet,
    pub state: CoverageState,
    pub observed_at_unix_ms: Option<u64>,
    pub reason: Option<String>,
}
pub(super) fn scoped_coverage(
    workspace: &str,
    expected: &BTreeSet<String>,
    known_areas: &BTreeSet<String>,
    facts: Vec<ProgressCoverageFact>,
) -> Vec<ProgressCoverageFact> {
    let mut ranges: BTreeMap<(String, CoverageFacet), ProgressCoverageFact> = BTreeMap::new();
    for fact in facts
        .into_iter()
        .filter(|f| f.workspace_id == workspace && expected.contains(&f.area_id))
    {
        let key = (fact.area_id.clone(), fact.facet);
        if ranges.get(&key).is_none_or(|old| {
            (fact.observed_at_unix_ms, fact.state) >= (old.observed_at_unix_ms, old.state)
        }) {
            ranges.insert(key, fact);
        }
    }
    for area in expected {
        for facet in [
            CoverageFacet::Directory,
            CoverageFacet::Tasks,
            CoverageFacet::Activity,
            CoverageFacet::Outcomes,
        ] {
            let fact =
                ranges
                    .entry((area.clone(), facet))
                    .or_insert_with(|| ProgressCoverageFact {
                        workspace_id: workspace.into(),
                        area_id: area.clone(),
                        facet,
                        state: CoverageState::Missing,
                        observed_at_unix_ms: None,
                        reason: Some("未取得此范围的只读事实".into()),
                    });
            if facet == CoverageFacet::Directory && !known_areas.contains(area) {
                fact.state = CoverageState::Missing;
                fact.reason = Some("区域目录元数据缺失，项目归属待核对".into());
            }
        }
    }
    ranges
        .into_values()
        .map(|mut f| {
            f.reason = f.reason.map(|s| excerpt(&s, 240));
            f
        })
        .collect()
}
