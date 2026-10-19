//! 终端文件路径下划线的纯决策：由 [`PathState`] 与候选等级决定实线、虚线或不画。

use crate::canvas::terminal_path_cache::PathState;
use terminal::{GridSnapshot, PathRef, PathTier};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LinkMark {
    Solid,
    Dashed,
}

/// 强候选在确认不存在前都画；待验证候选只在确认存在后才画；项目外文件用虚线。
pub(crate) fn underline_decision(tier: PathTier, state: PathState) -> Option<LinkMark> {
    match (tier, state) {
        (_, PathState::Exists { outside_project }) => Some(if outside_project {
            LinkMark::Dashed
        } else {
            LinkMark::Solid
        }),
        (PathTier::Strong, PathState::Unknown | PathState::Pending) => Some(LinkMark::Solid),
        (PathTier::Strong, PathState::Missing) => None,
        (PathTier::NeedsVerify, _) => None,
    }
}

/// 一帧可见行里需要画下划线的列区间（右开）。
#[derive(Clone, Debug, Default)]
pub(crate) struct PathMarks {
    rows: Vec<Vec<(usize, usize, LinkMark)>>,
}

impl PathMarks {
    pub(crate) fn build(
        snapshot: &GridSnapshot,
        mut state_of: impl FnMut(&PathRef) -> PathState,
    ) -> Self {
        let mut rows = Vec::with_capacity(snapshot.rows.len());
        for row in 0..snapshot.rows.len() {
            let marks = snapshot
                .path_ref_spans_for_row(row)
                .into_iter()
                .filter_map(|span| {
                    let state = state_of(&span.path_ref);
                    underline_decision(span.path_ref.tier, state)
                        .map(|mark| (span.start, span.end, mark))
                })
                .collect();
            rows.push(marks);
        }
        Self { rows }
    }

    pub(crate) fn mark_at(&self, row: usize, col: usize) -> Option<LinkMark> {
        self.rows
            .get(row)?
            .iter()
            .find(|(start, end, _)| col >= *start && col < *end)
            .map(|(_, _, mark)| *mark)
    }

    pub(crate) fn dashed_cells(&self) -> Vec<(usize, usize, usize)> {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(row, marks)| {
                marks
                    .iter()
                    .filter(|(_, _, mark)| *mark == LinkMark::Dashed)
                    .map(move |(start, end, _)| (row, *start, *end))
            })
            .collect()
    }
}

/// 把宽度按 `dash + gap` 周期切成虚线段，末段截断到宽度内。
pub(crate) fn dashed_segments(width: f32, dash: f32, gap: f32) -> Vec<(f32, f32)> {
    if width <= 0.0 || dash <= 0.0 {
        return Vec::new();
    }
    let period = dash + gap.max(0.0);
    let mut segments = Vec::new();
    let mut start = 0.0;
    while start < width {
        segments.push((start, dash.min(width - start)));
        start += period;
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 退化周期：gap 为负时按 0 处理，仍按 dash 步进，不会死循环。
    ///
    /// 数据构造：width = 10，dash = 5，gap = -3 → period = 5 + 0 = 5
    ///
    /// 预期结果：起点 0、5 两段，各长 5。
    #[test]
    fn test_dashed_segments_negative_gap_does_not_loop_forever() {
        assert_eq!(
            dashed_segments(10.0, 5.0, -3.0),
            vec![(0.0, 5.0), (5.0, 5.0)],
            "负 gap 应按 0 处理并按 dash 步进"
        );
    }
}
