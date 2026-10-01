use super::slowest_hosts;
use serde_json::json;

/// R-1: 子资源耗时必须按 host 取最大值并降序，只保留最慢的 N 个。
///
/// 数据构造（含关键数值的推导过程）：
///   cdn.slow.com 有两个资源：1200ms 与 300ms → 该 host 取最大值 1200ms；
///   api.mid.com 一个资源 800ms；fast.com 一个资源 20ms；
///   另有一行缺少 ms 字段（脏数据），必须被忽略；
///   limit = 2，因此期望只保留前两名。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐行读取 host 与 ms，缺字段的行跳过
///   2. 同一 host 取最大耗时（1200 而不是 1500 的累加）
///   3. 按耗时降序排序并截取前 2 个
///
/// 预期结果：
///   - 正断言：结果精确等于 "cdn.slow.com=1200ms, api.mid.com=800ms"
///   - 负断言：不含 fast.com（被 limit 截掉），也不含脏数据行的 host
#[test]
fn test_slowest_hosts_takes_max_per_host_and_ranks() {
    let report = json!({"rows": [
        {"host": "cdn.slow.com", "ms": 1200.0},
        {"host": "cdn.slow.com", "ms": 300.0},
        {"host": "api.mid.com", "ms": 800.0},
        {"host": "fast.com", "ms": 20.0},
        {"host": "broken.com"},
    ]});

    let summary = slowest_hosts(&report, 2);

    assert_eq!(
        summary, "cdn.slow.com=1200ms, api.mid.com=800ms",
        "必须按 host 取最大耗时并降序，且只保留 limit 个"
    );
    assert!(!summary.contains("fast.com"), "超过 limit 的 host 不得出现");
    assert!(
        !summary.contains("broken.com"),
        "缺少 ms 的脏数据行必须被忽略"
    );
}

/// R-2: 没有任何子资源时返回空串，不 panic。
///
/// 数据构造：rows 缺失（例如脚本在 about:blank 上执行）。
/// 执行过程：直接汇总一个没有 rows 字段的对象。
/// 预期结果：
///   - 正断言：返回空字符串
///   - 负断言：不得 panic
#[test]
fn test_slowest_hosts_handles_missing_rows() {
    assert_eq!(slowest_hosts(&json!({}), 5), "", "缺少 rows 必须返回空串");
}
