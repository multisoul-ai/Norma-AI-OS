use super::*;
use crate::cards::term::path_underline::{
    dashed_segments, underline_decision, LinkMark, PathMarks,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal::{PathTier, TerminalGrid};

fn request(raw: &str) -> PathRequest {
    let bases: Arc<[PathBuf]> = Arc::from(vec![PathBuf::from("/proj")]);
    PathRequest::new(bases, raw)
}

fn key(raw: &str) -> PathKey {
    request(raw).key
}

/// 缓存命中：后台确认存在的路径，之后再次出现时不得重复排队检查。
///
/// 数据构造（含关键数值的推导过程）：
///   base = "/proj"，raw = "src/a.rs"，键 = (/proj, src/a.rs)
///   t0 = 任意起点时刻；第二次出现发生在 t0 + 1s（远小于任何过期时间，且 Exists 不过期）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. note 一次 → wanted 含 1 项，状态仍为 Unknown（尚无结果）
///   2. take_batch → 取出 1 项并标记 Pending，wanted 清空
///   3. apply(Exists{outside_project:false}) → 状态 Exists，版本号 +1
///   4. t0+1s 再 note 同一键 → 不应再排队
///
/// 预期结果：
///   - 步骤 1 状态为 Unknown，步骤 2 状态为 Pending
///   - 步骤 3 状态为 Exists 且 version 从 0 变为 1
///   - 步骤 4 take_batch 为空（命中缓存，不重复检查）
#[test]
fn test_cache_hit_does_not_requeue_confirmed_path() {
    let t0 = Instant::now();
    let mut cache = TerminalPathCache::default();
    let req = request("src/a.rs");

    cache.note(req.clone(), t0);
    assert_eq!(
        cache.state(&req.key),
        PathState::Unknown,
        "尚未检查的路径应为 Unknown"
    );

    let batch = cache.take_batch(t0);
    assert_eq!(batch.len(), 1, "首次出现应排队一次检查");
    assert_eq!(
        cache.state(&req.key),
        PathState::Pending,
        "取出检查后应为 Pending"
    );
    assert_eq!(cache.version(), 0, "尚无结果，版本号不应变化");

    let changed = cache.apply(
        vec![(
            req.key.clone(),
            PathState::Exists {
                outside_project: false,
            },
        )],
        t0,
    );
    assert!(changed, "Unknown→Exists 是显示相关变化，应报告 changed");
    assert_eq!(
        cache.state(&req.key),
        PathState::Exists {
            outside_project: false
        },
        "确认存在后应记录 Exists"
    );
    assert_eq!(cache.version(), 1, "结果落地后版本号应 +1 以使渲染缓存失效");

    cache.note(req, t0 + Duration::from_secs(1));
    assert!(
        cache.take_batch(t0 + Duration::from_secs(1)).is_empty(),
        "已确认存在的路径再次出现时不得重复排队"
    );
}

/// Missing 过期：不存在的结果在 TTL 内不重查，TTL 到期后重新排队，且重查期间保持 Missing 以免闪烁。
///
/// 数据构造（含关键数值的推导过程）：
///   MISSING_TTL = 5s；结果写入时刻 t0
///   TTL 前一刻 = t0 + 5s - 1ms = t0 + 4.999s  → 未过期
///   TTL 边界   = t0 + 5s                       → 过期（>= 判定）
///
/// 执行过程：
///   1. note→take→apply(Missing, t0)
///   2. t0+4.999s note → 不排队
///   3. t0+5s note → 排队；此时 state 仍应是 Missing（stale-while-revalidate）
///   4. take_batch 取出后，重查结果为 Exists → 状态翻转
///
/// 预期结果：
///   - 步骤 2 take_batch 为空；步骤 3 take_batch 恰 1 项
///   - 步骤 3 排队期间 state 仍为 Missing，不是 Unknown（否则强候选会闪现下划线）
///   - 步骤 4 状态变为 Exists，changed=true
#[test]
fn test_missing_entry_expires_after_ttl_and_keeps_state_while_revalidating() {
    let t0 = Instant::now();
    let mut cache = TerminalPathCache::default();
    let req = request("src/gone.rs");
    cache.note(req.clone(), t0);
    cache.take_batch(t0);
    cache.apply(vec![(req.key.clone(), PathState::Missing)], t0);
    assert_eq!(
        cache.state(&req.key),
        PathState::Missing,
        "写入后应为 Missing"
    );

    let just_before = t0 + MISSING_TTL - Duration::from_millis(1);
    cache.note(req.clone(), just_before);
    assert!(
        cache.take_batch(just_before).is_empty(),
        "TTL 到期前不得重查 Missing"
    );

    let at_ttl = t0 + MISSING_TTL;
    cache.note(req.clone(), at_ttl);
    assert_eq!(
        cache.state(&req.key),
        PathState::Missing,
        "重查排队期间应保持 Missing，避免下划线闪烁"
    );
    let batch = cache.take_batch(at_ttl);
    assert_eq!(batch.len(), 1, "TTL 到期后应重新排队恰好一次");

    let changed = cache.apply(
        vec![(
            req.key.clone(),
            PathState::Exists {
                outside_project: false,
            },
        )],
        at_ttl,
    );
    assert!(changed, "Missing→Exists 应报告 changed");
    assert_eq!(
        cache.state(&req.key),
        PathState::Exists {
            outside_project: false
        },
        "文件出现后应翻转为 Exists"
    );
}

/// 单轮上限：一轮最多检查 64 个候选，余量留到下一轮，且已排队的不重复。
///
/// 数据构造（含关键数值的推导过程）：
///   候选 = 100 个互不相同的 raw（"f0.rs".."f99.rs"）
///   MAX_CANDIDATES_PER_ROUND = 64
///   第 1 轮 64，第 2 轮 100 - 64 = 36，第 3 轮 0
///   同一 raw 重复 note 一次，不应使总数变为 101
///
/// 执行过程：
///   1. 依次 note 100 个候选，并重复 note 其中一个
///   2. 连续 take_batch 三次
///
/// 预期结果：
///   - 三轮数量依次为 64、36、0
///   - 第 1 轮不含第 65 个及以后的候选（f64.rs 在第 2 轮才出现）
#[test]
fn test_take_batch_caps_each_round_at_64_candidates() {
    let t0 = Instant::now();
    let mut cache = TerminalPathCache::default();
    for index in 0..100 {
        cache.note(request(&format!("f{index}.rs")), t0);
    }
    cache.note(request("f0.rs"), t0);

    let first = cache.take_batch(t0);
    assert_eq!(
        first.len(),
        MAX_CANDIDATES_PER_ROUND,
        "第 1 轮应恰为上限 64"
    );
    assert_eq!(MAX_CANDIDATES_PER_ROUND, 64, "产品约定每轮上限为 64");
    assert!(
        first.iter().all(|req| req.key != key("f64.rs")),
        "第 65 个候选不得进入第 1 轮"
    );

    let second = cache.take_batch(t0);
    assert_eq!(second.len(), 36, "第 2 轮应取走剩余 100-64=36 个");
    assert!(
        second.iter().any(|req| req.key == key("f64.rs")),
        "f64.rs 应在第 2 轮被检查"
    );
    assert!(
        cache.take_batch(t0).is_empty(),
        "第 3 轮应为空，重复 note 不产生第 101 项"
    );
}

/// 缓存超限清理：只丢不可见的 Missing 结果，已确认存在的下划线不得闪烁，版本号不得因此变化。
///
/// 数据构造（含关键数值的推导过程）：
///   MAX_ENTRIES = 4096；写入 1 个 Exists + 4096 个 Missing = 4097 > 4096，触发清理
///   清理后再 note 一个新候选 "fresh.rs"
///
/// 执行过程：
///   1. 用 apply 一次写入 4097 条结果（版本号因 Exists 的出现而变化，记为 v）
///   2. note("fresh.rs") 触发超限清理
///
/// 预期结果：
///   - 正断言：Exists 条目仍为 Exists（用户正看着的下划线保留）。
///   - 负断言：任一 Missing 条目被清空回 Unknown（它们本就不画下划线，丢弃无可见影响）。
///   - 版本号仍为 v：没有可见变化，不应使所有终端布局重建。
#[test]
fn test_purge_keeps_confirmed_paths_and_does_not_bump_version() {
    let t0 = Instant::now();
    let mut cache = TerminalPathCache::default();
    let mut results = vec![(
        key("keep.rs"),
        PathState::Exists {
            outside_project: false,
        },
    )];
    results.extend((0..MAX_ENTRIES).map(|i| (key(&format!("gone{i}.rs")), PathState::Missing)));
    cache.apply(results, t0);
    let version = cache.version();

    cache.note(request("fresh.rs"), t0);

    assert_eq!(
        cache.state(&key("keep.rs")),
        PathState::Exists {
            outside_project: false
        },
        "已确认存在的路径不能在清理中丢失，否则下划线会闪烁"
    );
    assert_eq!(
        cache.state(&key("gone0.rs")),
        PathState::Unknown,
        "Missing 结果应在清理中被丢弃"
    );
    assert_eq!(cache.version(), version, "清理不可见结果不应改变展示版本");
}

/// 下划线决策：强候选默认画（直到确认不存在），待验证候选只在确认存在后才画，工作区外为虚线。
///
/// 数据构造：
///   tier ∈ {Strong, NeedsVerify}，state ∈ {Unknown, Pending, Missing, Exists 内/外}
///
/// 预期结果：
///   - Strong：Unknown/Pending → Solid；Missing → None；Exists 内 → Solid；Exists 外 → Dashed
///   - NeedsVerify：Unknown/Pending/Missing → None；Exists 内 → Solid；Exists 外 → Dashed
#[test]
fn test_underline_decision_matrix_for_strong_and_needs_verify() {
    let inside = PathState::Exists {
        outside_project: false,
    };
    let outside = PathState::Exists {
        outside_project: true,
    };
    for state in [PathState::Unknown, PathState::Pending] {
        assert_eq!(
            underline_decision(PathTier::Strong, state),
            Some(LinkMark::Solid),
            "强候选在验证完成前应先画实线: {state:?}"
        );
        assert_eq!(
            underline_decision(PathTier::NeedsVerify, state),
            None,
            "待验证候选未确认前不得画下划线: {state:?}"
        );
    }
    assert_eq!(
        underline_decision(PathTier::Strong, PathState::Missing),
        None,
        "强候选确认不存在后应撤掉下划线"
    );
    assert_eq!(
        underline_decision(PathTier::NeedsVerify, PathState::Missing),
        None,
        "待验证候选不存在时不得画"
    );
    assert_eq!(
        underline_decision(PathTier::Strong, inside),
        Some(LinkMark::Solid),
        "项目内为实线"
    );
    assert_eq!(
        underline_decision(PathTier::NeedsVerify, inside),
        Some(LinkMark::Solid),
        "确认存在的待验证候选画实线"
    );
    assert_eq!(
        underline_decision(PathTier::Strong, outside),
        Some(LinkMark::Dashed),
        "项目外为虚线"
    );
    assert_eq!(
        underline_decision(PathTier::NeedsVerify, outside),
        Some(LinkMark::Dashed),
        "项目外的待验证候选确认后也是虚线"
    );
}

/// 行内标记：列区间与缓存状态一致，NeedsVerify 未确认不画、确认后才画，区间右开。
///
/// 数据构造（含关键数值的推导过程）：
///   行文本 = "edit src/a.rs:3 and main.rs ok"
///   e0 d1 i2 t3 空4；"src/a.rs:3" 起点 5、长 10 → 区间 5..15（Strong）
///   空15；"and" 16..19；空19；"main.rs" 起点 20、长 7 → 区间 20..27（NeedsVerify）
///   "ok" 无扩展名，不应出现
///
/// 执行过程：
///   1. 状态全为 Unknown 时建标记 → 仅 src/a.rs:3 被标记
///   2. main.rs 确认存在（项目内）→ 20..27 被标记为实线
///   3. src/a.rs:3 确认在项目外 → 5..15 为虚线，且 dashed_cells 给出 (row0,5,15)
///   4. src/a.rs:3 确认不存在 → 5..15 不再标记
///
/// 预期结果：
///   - 步骤 1：列 5、14 为 Solid；列 20 为 None（未确认）
///   - 步骤 2：列 20、26 为 Solid；列 27（右开边界）与列 19（空格）为 None
///   - 步骤 3：列 5 为 Dashed；dashed_cells == [(0,5,15)]
///   - 步骤 4：列 5 为 None
#[test]
fn test_path_marks_follow_cache_state_and_exclude_unconfirmed_needs_verify() {
    let mut grid = TerminalGrid::new(80, 4);
    grid.advance(b"edit src/a.rs:3 and main.rs ok");
    let snapshot = grid.snapshot();

    let unknown = PathMarks::build(&snapshot, |_| PathState::Unknown);
    assert_eq!(
        unknown.mark_at(0, 5),
        Some(LinkMark::Solid),
        "强候选首列应画"
    );
    assert_eq!(
        unknown.mark_at(0, 14),
        Some(LinkMark::Solid),
        "强候选末列应画"
    );
    assert_eq!(unknown.mark_at(0, 20), None, "裸文件名未确认前不得画");

    let confirmed = PathMarks::build(&snapshot, |path_ref| {
        if path_ref.raw == "main.rs" {
            PathState::Exists {
                outside_project: false,
            }
        } else {
            PathState::Unknown
        }
    });
    assert_eq!(
        confirmed.mark_at(0, 20),
        Some(LinkMark::Solid),
        "确认后首列应画"
    );
    assert_eq!(
        confirmed.mark_at(0, 26),
        Some(LinkMark::Solid),
        "确认后末列应画"
    );
    assert_eq!(confirmed.mark_at(0, 27), None, "区间右开，列 27 不应画");
    assert_eq!(confirmed.mark_at(0, 19), None, "区间前的空格不应画");

    let outside = PathMarks::build(&snapshot, |path_ref| {
        if path_ref.raw == "src/a.rs:3" {
            PathState::Exists {
                outside_project: true,
            }
        } else {
            PathState::Unknown
        }
    });
    assert_eq!(
        outside.mark_at(0, 5),
        Some(LinkMark::Dashed),
        "项目外路径应为虚线"
    );
    assert_eq!(
        outside.dashed_cells(),
        vec![(0, 5, 15)],
        "虚线区间应是行 0 的 5..15"
    );

    let missing = PathMarks::build(&snapshot, |_| PathState::Missing);
    assert_eq!(missing.mark_at(0, 5), None, "确认不存在的强候选不应画");
}

/// 虚线分段：按 dash/gap 周期铺满宽度，末段截断到宽度内。
///
/// 数据构造（含关键数值的推导过程）：
///   width = 30，dash = 4，gap = 3，周期 = 7
///   起点 = 0, 7, 14, 21, 28（35 ≥ 30 停止）
///   长度 = 4, 4, 4, 4, min(4, 30-28=2)=2
///   宽度 0 或 dash ≤ 0 → 空
///
/// 预期结果：
///   - 恰 5 段，起点与长度如上；最后一段终点 28+2=30 不越界
///   - 退化输入返回空，避免死循环
#[test]
fn test_dashed_segments_tile_width_and_clip_last_dash() {
    let segments = dashed_segments(30.0, 4.0, 3.0);
    assert_eq!(
        segments,
        vec![
            (0.0, 4.0),
            (7.0, 4.0),
            (14.0, 4.0),
            (21.0, 4.0),
            (28.0, 2.0)
        ],
        "应按 7px 周期铺 5 段，末段截断为 2px"
    );
    let end = segments.last().map(|(start, len)| start + len);
    assert_eq!(end, Some(30.0), "末段终点必须正好贴到宽度，不越界");
    assert!(
        dashed_segments(0.0, 4.0, 3.0).is_empty(),
        "宽度为 0 不应有分段"
    );
    assert!(
        dashed_segments(30.0, 0.0, 3.0).is_empty(),
        "dash 为 0 必须返回空而非死循环"
    );
}

/// 后台检查：存在的文件/目录为 Exists（区分项目内/外），不存在的为 Missing。
///
/// 数据构造（含关键数值的推导过程）：
///   临时目录 root 内建 src/in.rs（项目内文件）与 root 之外的另一临时目录 outer/out.md
///   bases = [root]；
///   - raw "src/in.rs:7" → 解析后在 root 内 → Exists{outside_project:false}
///   - raw 绝对路径 outer/out.md → 在 root 外 → Exists{outside_project:true}
///   - raw "src/none.rs" → 不存在 → Missing
///   - raw "src" → 是项目内目录（resolve 现接受目录）→ Exists{outside_project:false}
///
/// 预期结果：四项状态与上述一一对应，且结果键与请求键一致。
#[test]
fn test_check_batch_classifies_inside_outside_missing_and_directories() {
    let root = tempfile::tempdir().expect("创建项目临时目录");
    let outer = tempfile::tempdir().expect("创建项目外临时目录");
    std::fs::create_dir_all(root.path().join("src")).expect("建 src");
    std::fs::write(root.path().join("src/in.rs"), "fn main() {}").expect("写 in.rs");
    let outside_file = outer.path().join("out.md");
    std::fs::write(&outside_file, "# out").expect("写 out.md");

    let bases: Arc<[PathBuf]> = Arc::from(vec![root.path().to_path_buf()]);
    let outside_raw = outside_file.to_string_lossy().into_owned();
    let batch = vec![
        PathRequest::new(Arc::clone(&bases), "src/in.rs:7"),
        PathRequest::new(Arc::clone(&bases), outside_raw.clone()),
        PathRequest::new(Arc::clone(&bases), "src/none.rs"),
        PathRequest::new(Arc::clone(&bases), "src"),
    ];

    let results = check_batch(batch, None);
    let state_of = |raw: &str| {
        results
            .iter()
            .find(|(key, _)| key.raw == raw)
            .map(|(_, state)| *state)
    };
    assert_eq!(
        state_of("src/in.rs:7"),
        Some(PathState::Exists {
            outside_project: false
        }),
        "项目内文件应是 Exists 且不在项目外"
    );
    assert_eq!(
        state_of(&outside_raw),
        Some(PathState::Exists {
            outside_project: true
        }),
        "项目外绝对路径应是 Exists 且标记为项目外"
    );
    assert_eq!(
        state_of("src/none.rs"),
        Some(PathState::Missing),
        "不存在应为 Missing"
    );
    assert_eq!(
        state_of("src"),
        Some(PathState::Exists {
            outside_project: false
        }),
        "项目内目录可在文件树中定位，应为 Exists 且不在项目外"
    );
}
