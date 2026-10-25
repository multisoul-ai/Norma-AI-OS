use super::*;
use crate::canvas::master_layout::{close_node_and_update_layout_for_area, MasterLayoutState};
use storage::{LayoutMode, LayoutStateSnapshot};

/// 场景：后台旧项目尚未首次展开时，CLI 新增或删除卡片可能已经完成重排。
/// 数据构造：两张 62px 终端和已经缺失的旧 Master，状态带待修复快照。
/// 执行过程：分别通过生产 CLI 新增布局桥接、删除布局入口重排。
/// 预期结果：终端不再是细条，持久化为正常平铺；旧待修复 Master 不能覆盖新布局。
#[test]
fn desktop_master_background_reflow_finishes_pending_repair() {
    for remove in [false, true] {
        let mut runtime = Runtime::new();
        let strip = CanvasRect::new(969.0, 8.0, 476.0, 62.0);
        let first = runtime
            .document
            .add_node(CanvasNodeKind::Term(canvas::TermSpec), strip);
        let second = runtime
            .document
            .add_node(CanvasNodeKind::Term(canvas::TermSpec), strip);
        let mut state = MasterLayoutState::default();
        assert!(
            !state.restore_from_snapshot(
                LayoutStateSnapshot {
                    mode: LayoutMode::MasterDetail,
                    master_id: Some(999),
                    original_bounds: vec![],
                },
                &runtime
            ),
            "缺失的 Master 不能活跃"
        );
        state.bind_desktop(&runtime);
        let mut motion = MotionStore::new(true);
        let area = CanvasRect::new(0.0, 0.0, 1454.0, 925.0);
        let strategy = CanvasLayoutStrategy {
            chrome_in_parent: true,
            ..Default::default()
        };
        if remove {
            assert!(
                close_node_and_update_layout_for_area(
                    &mut runtime,
                    &mut motion,
                    &mut state,
                    second,
                    area,
                    1.0,
                    strategy
                ),
                "删除真实卡片应完成"
            );
        } else {
            apply_layout_for_new_node_or_master(
                &mut state,
                &mut runtime,
                &mut motion,
                second,
                area,
                1.0,
                strategy,
            );
        }
        assert!(
            runtime
                .document
                .node(first)
                .expect("剩余终端")
                .bounds
                .size
                .y
                > 200.0,
            "实际重排应修复细条"
        );
        assert_eq!(
            state.to_snapshot().mode,
            LayoutMode::Tiled,
            "新布局不能写回旧 Master"
        );
        assert!(!state.take_repair_pending(), "完成布局后不再残留待修复状态");
    }
}
