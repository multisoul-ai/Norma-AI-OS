use super::*;
use std::path::PathBuf;

/// CPU-B-1：未显式设置 scenario 时不得改变正常业务启动。
///
/// 数据构造（含关键数值的推导过程）：
///   scenario = None（0 个 benchmark 环境值）
///   workspace = Some("/tmp/norma-cpu")
///   expected config = None（workspace 单独存在不能启用 benchmark）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将空 scenario 与一个 workspace 路径交给纯解析 seam。
///   2. 读取解析结果。
///
/// 预期结果：
///   - 正断言：结果为 None，正常启动路径不创建任何测试卡片。
///   - 负断言：不得仅因 workspace 值存在就启用 benchmark。
#[test]
fn test_cpu_benchmark_is_opt_in_and_does_not_change_normal_startup() {
    let config = CpuBenchmarkConfig::from_values(None, Some("/tmp/norma-cpu"));

    assert_eq!(
        config,
        Ok(None),
        "scenario 未设置时必须保持普通业务启动，不能创建 benchmark 卡片"
    );
    assert_ne!(
        config,
        Ok(Some(CpuBenchmarkConfig {
            scenario: CpuBenchmarkScenario::IdleSevenCards,
            workspace: PathBuf::from("/tmp/norma-cpu"),
        })),
        "workspace 单独存在不得意外启用 CPU benchmark"
    );
}

/// CPU-B-2：四个固定场景必须映射为精确的 7 卡片与活动源数量。
///
/// 数据构造（含关键数值的推导过程）：
///   idle/background = idle terminals 7 + active terminals 0 = total 7
///   one-active      = idle terminals 6 + active terminals 1 = total 7
///   two-active      = idle terminals 5 + active terminals 2 = total 7
///   workspace = "/tmp/norma-cpu"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐个解析四个 canonical scenario。
///   2. 从配置读取 idle/active/total 数量。
///   3. 逐项比较固定值。
///
/// 预期结果：
///   - 正断言：每个场景 total 都为 7。
///   - 正断言：活动源分别为 0/0/1/2。
///   - 负断言：two-active 不得退化为一个活动源。
#[test]
fn test_cpu_benchmark_scenarios_define_seven_cards_and_exact_activity() {
    let workspace = Some("/tmp/norma-cpu");
    let idle = CpuBenchmarkConfig::from_values(Some("idle-seven-cards"), workspace)
        .expect("idle scenario 应成功解析")
        .expect("idle scenario 应启用 benchmark");
    let background = CpuBenchmarkConfig::from_values(Some("background-seven-cards"), workspace)
        .expect("background scenario 应成功解析")
        .expect("background scenario 应启用 benchmark");
    let one_active = CpuBenchmarkConfig::from_values(Some("one-active-terminal"), workspace)
        .expect("one-active scenario 应成功解析")
        .expect("one-active scenario 应启用 benchmark");
    let two_active = CpuBenchmarkConfig::from_values(Some("two-active-terminals"), workspace)
        .expect("two-active scenario 应成功解析")
        .expect("two-active scenario 应启用 benchmark");

    assert_eq!(
        idle.scenario.card_counts(),
        (7, 0),
        "idle 场景应创建 7 个 idle Terminal 和 0 个 active Terminal"
    );
    assert_eq!(
        background.scenario.card_counts(),
        (7, 0),
        "background 场景应先构造与 idle 完全相同的 7 张卡片"
    );
    assert_eq!(
        one_active.scenario.card_counts(),
        (6, 1),
        "one-active 场景应满足 6 + 1 = 7 张卡片"
    );
    assert_eq!(
        two_active.scenario.card_counts(),
        (5, 2),
        "two-active 场景应满足 5 + 2 = 7 张卡片"
    );
    assert_ne!(
        two_active.scenario.card_counts(),
        (6, 1),
        "two-active 场景不得退化为单活动源，否则压力级别不一致"
    );
}

/// CPU-B-3：未知场景或缺失 workspace 必须显式失败。
///
/// 数据构造（含关键数值的推导过程）：
///   invalid scenario = "lighter-scene"（不在 4 个 canonical 值中）
///   missing workspace = None
///   invalid cases = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用未知场景和有效路径解析。
///   2. 用有效场景和缺失路径解析。
///   3. 检查两条错误分别包含 scenario/workspace 语义。
///
/// 预期结果：
///   - 正断言：两种无效输入都返回 Err。
///   - 负断言：不得静默回退到 idle 场景制造虚假性能结果。
#[test]
fn test_cpu_benchmark_rejects_unknown_scenario_and_missing_workspace() {
    let unknown = CpuBenchmarkConfig::from_values(Some("lighter-scene"), Some("/tmp/norma-cpu"));
    let missing_workspace = CpuBenchmarkConfig::from_values(Some("idle-seven-cards"), None);

    assert!(
        unknown
            .as_ref()
            .is_err_and(|error| error.contains("scenario")),
        "未知 benchmark 场景必须返回包含 scenario 的明确错误"
    );
    assert!(
        missing_workspace
            .as_ref()
            .is_err_and(|error| error.contains("workspace")),
        "启用 benchmark 但缺失 workspace 时必须明确失败"
    );
    assert_ne!(
        unknown,
        Ok(Some(CpuBenchmarkConfig {
            scenario: CpuBenchmarkScenario::IdleSevenCards,
            workspace: PathBuf::from("/tmp/norma-cpu"),
        })),
        "未知场景不得静默回退为更轻的 idle 场景"
    );
}
