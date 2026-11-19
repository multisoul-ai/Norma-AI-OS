use super::{StaffPool, Submit, CLAUDE_SESSION_LIMIT};
use crate::event::Trigger;
use crate::lane::worker::Job;
use crate::staff::gate::{StaffTokens, Tokens};
use norma::protocol::BrainScope;
use std::sync::{atomic::AtomicBool, Arc};

fn scope(route: &str) -> BrainScope {
    BrainScope {
        workspace_id: "ws".into(),
        route_id: route.into(),
    }
}

fn job(id: &str) -> Job {
    job_in("route", id)
}

fn job_in(route: &str, id: &str) -> Job {
    Job {
        task_id: Some(id.to_owned()),
        cancelled: Arc::new(AtomicBool::new(false)),
        scope: scope(route),
        text: String::new(),
        approval_id: None,
        trigger: Trigger::unknown(),
    }
}

fn runs(submit: Submit) -> bool {
    matches!(submit, Submit::Run { evicted: None, .. })
}

/// 场景：4 个会话都在忙时第 5 个排队；最早空闲的那个被驱逐后第 5 个开始，旧 token 失效。
/// 数据构造：上限 4。a/b/c/d 先占满并各持一把 token。a 先空闲，e 再来。
/// 执行过程：提交 4 个，标记 a 空闲，再提交 e；然后看 token 角色。
/// 预期结果：e 直接顶掉 a 并开始；a 的 token 角色为空，对应 staff.invalid_source；b 的 token 仍有效。
#[test]
fn fifth_task_evicts_the_oldest_idle_session() {
    let staff = StaffTokens::default();
    let tokens = Tokens::with_staff("main-token", staff.clone());
    let mut pool = StaffPool::new(CLAUDE_SESSION_LIMIT, Some(staff.clone()));
    let mut held = Vec::new();
    for id in ["a", "b", "c", "d"] {
        let token = staff.mint(id);
        assert!(runs(pool.submit(job(id))), "{id} 应该直接占一个会话");
        pool.note(id, &token);
        held.push(token);
    }
    pool.on_idle("a");
    match pool.submit(job("e")) {
        Submit::Run {
            evicted: Some(evicted),
            ..
        } => assert_eq!(
            evicted,
            (scope("route"), "a".to_owned()),
            "第 5 个应该驱逐最早空闲的 a"
        ),
        Submit::Run { evicted: None, .. } | Submit::Wait => {
            panic!("第 5 个应该顶掉空闲会话并开始");
        }
    }
    assert_eq!(tokens.role(&held[0]), None, "被驱逐的 a 不再有角色");
    assert_eq!(
        tokens
            .role(&held[0])
            .map(|_| "ok")
            .unwrap_or("staff.invalid_source"),
        "staff.invalid_source",
        "宿主把失效 token 报成 staff.invalid_source"
    );
    assert!(tokens.role(&held[1]).is_some(), "还在忙的 b 不能被一起撤销");
}

/// 场景：4 个都在忙时第 5 个不能抢占，要等其中一个变空闲。
/// 数据构造：a/b/c/d 占满且没有人调用 on_idle。e 再提交。
/// 执行过程：提交 e，再让 a 空闲。
/// 预期结果：e 先得到 Wait；a 空闲后 on_idle 交出 e。
#[test]
fn fifth_task_waits_until_a_session_is_idle() {
    let mut pool = StaffPool::new(CLAUDE_SESSION_LIMIT, None);
    for id in ["a", "b", "c", "d"] {
        assert!(runs(pool.submit(job(id))), "{id} 占位");
    }
    assert!(
        matches!(pool.submit(job("e")), Submit::Wait),
        "4 个都在忙时第 5 个必须排队"
    );
    let (evicted, next) = pool.on_idle("a").expect("a 空闲后应该让出位置");
    assert_eq!(evicted, (scope("route"), "a".to_owned()), "空出来的是 a");
    assert_eq!(next.task_id.as_deref(), Some("e"), "排队的 e 接着开始");
}

/// 场景：同一个任务在排队时又来一条，只占一个名额；任务完成后它还在排队的活被丢掉。
/// 数据构造：上限 4，a/b/c/d 占满。e 排两条，然后 e 被归档。
/// 执行过程：
///   1. 提交 e 两次，都得到 Wait。
///   2. a 空闲，on_idle 交出第一条 e，占用 1 个名额。
///   3. 交出后 e 的第二条仍在排队；此时 e 已经占着名额，再让 b 空闲不应给 e 开第二个名额。
///   4. close("e") 之后，e 排队的第二条应该被丢掉，不会再被启动。
///
/// 预期结果：
///   - 4 个名额里 e 只占一个；close(e) 后没有任何排队任务被交出。
#[test]
fn duplicate_waiting_jobs_share_one_slot_and_close_drops_them() {
    let mut pool = StaffPool::new(CLAUDE_SESSION_LIMIT, None);
    for id in ["a", "b", "c", "d"] {
        assert!(runs(pool.submit(job(id))), "{id} 占位");
    }
    assert!(
        matches!(pool.submit(job("e")), Submit::Wait),
        "e 第一条排队"
    );
    assert!(
        matches!(pool.submit(job("e")), Submit::Wait),
        "e 第二条也排队"
    );
    let (_, first) = pool.on_idle("a").expect("a 让出位置给 e");
    assert_eq!(first.task_id.as_deref(), Some("e"), "第一条 e 开始");
    let again = pool.on_idle("b").expect("b 空闲时 e 的第二条被交出");
    assert_eq!(
        again.1.task_id.as_deref(),
        Some("e"),
        "第二条 e 沿用同一个名额"
    );
    assert!(
        pool.slots_for_test("e") == 1,
        "e 的两条排队任务只能占一个名额，实际 {}",
        pool.slots_for_test("e")
    );
    assert!(pool.close("e").is_none(), "e 归档后没有排队任务被交出");
}

/// 场景：任务归档时，它还没开始的排队任务不能再被启动。
/// 数据构造：a/b/c/d 占满且都在忙，e 排队一条，f 排队一条。
/// 执行过程：close("e")（e 没有会话，没有释放名额，所以不能顺带放行别的排队任务，但 e 排队的活要被清掉），再让 a 空闲。
/// 预期结果：close 返回 None 且仍是 4 个名额；a 空闲后交出的是 f，不是已归档的 e。
#[test]
fn close_removes_that_tasks_waiting_job() {
    let mut pool = StaffPool::new(CLAUDE_SESSION_LIMIT, None);
    for id in ["a", "b", "c", "d"] {
        assert!(runs(pool.submit(job(id))), "{id} 占位");
    }
    assert!(matches!(pool.submit(job("e")), Submit::Wait), "e 排队");
    assert!(matches!(pool.submit(job("f")), Submit::Wait), "f 排队");
    assert!(
        pool.close("e").is_none(),
        "e 没有会话可关，也没有人接它的位置"
    );
    let (_, next) = pool.on_idle("a").expect("a 空闲后应该交出一个排队任务");
    assert_eq!(
        next.task_id.as_deref(),
        Some("f"),
        "交出的是 f，已归档的 e 不能再启动"
    );
}

/// 场景：路由被释放后，它占的名额归还，排队的其他路由任务得以开始。
/// 数据构造：r1 路由的 a/b/c/d 占满；r2 路由的 e 排队；r1 路由还有一条排队的 g。
/// 执行过程：release_scope(r1)。
/// 预期结果：r1 的 g 被丢掉；交出的只有 r2 的 e；r1 的 4 个会话都不再占名额。
#[test]
fn release_scope_frees_slots_for_other_routes() {
    let mut pool = StaffPool::new(CLAUDE_SESSION_LIMIT, None);
    for id in ["a", "b", "c", "d"] {
        assert!(runs(pool.submit(job_in("r1", id))), "{id} 占位");
    }
    assert!(
        matches!(pool.submit(job_in("r1", "g")), Submit::Wait),
        "g 排队"
    );
    assert!(
        matches!(pool.submit(job_in("r2", "e")), Submit::Wait),
        "e 排队"
    );
    let started = pool.release_scope(&scope("r1"));
    assert_eq!(
        started.len(),
        1,
        "只有 r2 的 e 应该开始，实际 {}",
        started.len()
    );
    assert_eq!(started[0].task_id.as_deref(), Some("e"), "开始的是 e");
    assert!(!pool.holds(&scope("r1"), "a"), "r1 的 a 不再占名额");
    assert!(pool.holds(&scope("r2"), "e"), "e 已占一个名额");
}

/// 场景：不同路由下同名任务互不影响；驱逐返回的是被驱逐会话自己的路由。
/// 数据构造：上限 1。r1 的 t 占位并空闲；r2 的 t 来了。
/// 执行过程：提交 r2 的 t。
/// 预期结果：被驱逐的是 (r1, t)，r2 的 t 占位；r1 的 t 不再占名额。
#[test]
fn eviction_reports_the_evicted_sessions_own_scope() {
    let mut pool = StaffPool::new(1, None);
    assert!(runs(pool.submit(job_in("r1", "t"))), "r1 的 t 占位");
    pool.on_idle("t");
    match pool.submit(job_in("r2", "t")) {
        Submit::Run {
            evicted: Some(evicted),
            ..
        } => {
            assert_eq!(evicted, (scope("r1"), "t".to_owned()), "驱逐的是 r1 的会话");
        }
        _ => panic!("上限 1 时空闲的 r1 会话应该被驱逐"),
    }
    assert!(!pool.holds(&scope("r1"), "t"), "r1 的 t 不再占名额");
    assert!(pool.holds(&scope("r2"), "t"), "r2 的 t 占位");
}
