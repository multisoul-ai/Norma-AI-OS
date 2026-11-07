use brain::{goal::GoalStore, persistence::Writer};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Barrier;

/// 全局幕僚的模型日志和App审批审计并发写入时，每条记录必须仍是完整JSON行。
/// 数据：两个GoalStore线程与一个App Writer线程，各500条1KiB正文，共3×500=1500条。
/// 执行：Barrier同时释放三个生产者，等待线程及写入屏障完成，再逐行解析并核对唯一身份。
/// 预期：无交错正文/空行/丢行；1500个(worker,sequence)身份均唯一且正文完整。
#[test]
fn global_decision_log_serializes_model_and_app_audit_appends() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let start = Barrier::new(3);
    let body = "x".repeat(1024);
    std::thread::scope(|scope| {
        for worker in 0..3 {
            let start = &start;
            let body = &body;
            scope.spawn(move || {
                let store = GoalStore::open(root.to_path_buf());
                let writer = Writer::default();
                start.wait();
                for sequence in 0..500 {
                    let line = json!({"workspace_id":format!("workspace-{worker}"),"worker":worker,"sequence":sequence,"body":body}).to_string();
                    if worker == 2 {
                        writer.append(root.join("decisions.jsonl"), line);
                    } else {
                        store.append_decision(&line);
                    }
                }
                writer.flush().recv().unwrap().unwrap();
            });
        }
    });
    let log = std::fs::read_to_string(root.join("decisions.jsonl")).unwrap();
    let records: Vec<Value> = log
        .lines()
        .map(|line| {
            serde_json::from_str(line).expect("每条日志必须是一个完整JSON，不能正文或换行交错")
        })
        .collect();
    assert_eq!(records.len(), 1500, "3个生产者各500条记录全部保留");
    let mut ids = BTreeSet::new();
    for record in records {
        assert_eq!(record["body"], body, "1KiB正文完整");
        ids.insert((
            record["worker"].as_u64().unwrap(),
            record["sequence"].as_u64().unwrap(),
        ));
    }
    assert_eq!(ids.len(), 1500, "不能重复或丢失任一生产者记录");
}
