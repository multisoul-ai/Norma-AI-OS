use super::Changes;
use std::{
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

/// 变化通知同时唤醒所有订阅者；先于等待的变化不丢失，空闲等待不忙轮询。
///
/// 数据构造：独立Changes、两个订阅者；2s心跳预算，50ms空闲等待；500ms为线程调度的宽松上限。
/// 执行过程：两个线程记录同一版本并准备等待 → 改版本并广播 → 验证两者返回 → 使用旧版本再等 → 当前版本空闲等超时。
/// 预期结果：两个订阅均观察新版本，不等2s心跳；旧版本立即返回；无变化时不提前返回，不虚构新版本。
#[test]
fn changes_wake_all_watchers_without_losing_early_notifications() {
    let changes = Arc::new(Changes::new());
    let revision = changes.revision();
    let barrier = Arc::new(Barrier::new(3)); //两个订阅线程+主线程=3。
    let watch = || {
        let changes = changes.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            crate::app::shared_tokio_runtime()
                .expect("共享运行时")
                .block_on(changes.wait(revision, Duration::from_secs(2)));
            changes.revision()
        })
    };
    let first = watch();
    let second = watch();
    barrier.wait();
    let started = Instant::now();
    changes.changed();
    assert_eq!(
        first.join().expect("第一订阅"),
        revision + 1,
        "第一订阅必须观察变化"
    );
    assert_eq!(
        second.join().expect("第二订阅"),
        revision + 1,
        "广播不能只唤醒一个手机"
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "变化后不能仍等待2s心跳"
    );
    let started = Instant::now();
    crate::app::shared_tokio_runtime()
        .expect("共享运行时")
        .block_on(changes.wait(revision, Duration::from_secs(2)));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "发生在等待前的通知不能丢失"
    );
    let current = changes.revision();
    let idle = Instant::now();
    crate::app::shared_tokio_runtime()
        .expect("共享运行时")
        .block_on(changes.wait(current, Duration::from_millis(50)));
    assert!(
        idle.elapsed() >= Duration::from_millis(50),
        "无变化必须等到超时，不能忙循环提前返回"
    );
    assert_eq!(changes.revision(), current, "等待超时不能伪造变化");
    assert_ne!(current, revision, "旧版本不能被当成当前状态");
}
