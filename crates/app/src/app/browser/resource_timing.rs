//! 页面加载完成后读取 WebKit Resource Timing，按 host 定位最慢的子资源。

use canvas::NodeId;

pub(crate) const RESOURCE_TIMING_SCRIPT: &str = r#"
(() => {
    const rows = performance.getEntriesByType("resource").map((entry) => ({
        host: new URL(entry.name).host,
        ms: entry.duration,
    }));
    const nav = performance.getEntriesByType("navigation")[0];
    return JSON.stringify({
        dom_ms: nav ? nav.domContentLoadedEventEnd : null,
        load_ms: nav ? nav.loadEventEnd : null,
        rows,
    });
})()
"#;

pub(crate) fn log_resource_timing(node_id: NodeId, json: &str) {
    let Ok(text) = serde_json::from_str::<String>(json) else {
        return;
    };
    let Ok(report) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    tracing::info!(
        node_id,
        dom_ms = report["dom_ms"].as_f64(),
        load_ms = report["load_ms"].as_f64(),
        resources = report["rows"].as_array().map(Vec::len),
        slowest_hosts = %slowest_hosts(&report, 5),
        "browser_resource_timing"
    );
}

/// 按 host 汇总子资源最长耗时，返回最慢的 `limit` 个，格式 `host=耗时ms`。
pub(crate) fn slowest_hosts(report: &serde_json::Value, limit: usize) -> String {
    let mut slowest = std::collections::HashMap::<String, f64>::new();
    for row in report["rows"].as_array().into_iter().flatten() {
        let (Some(host), Some(ms)) = (row["host"].as_str(), row["ms"].as_f64()) else {
            continue;
        };
        let entry = slowest.entry(host.to_string()).or_insert(0.0);
        *entry = entry.max(ms);
    }
    let mut ranked = slowest.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(host, ms)| format!("{host}={ms:.0}ms"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "../../../tests/unit/browser_resource_timing_tests.rs"]
mod tests;
