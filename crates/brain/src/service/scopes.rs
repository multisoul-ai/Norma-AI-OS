//! Scope cancellation is synchronous; dispatcher cleanup may follow asynchronously.
use norma::protocol::BrainScope;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type TurnKey = (BrainScope, String);
type LiveTurns = Mutex<HashMap<TurnKey, (u64, Arc<AtomicBool>)>>;

#[derive(Clone)]
pub(crate) struct Scopes {
    stop: Arc<AtomicBool>,
    tokens: Arc<Mutex<HashMap<BrainScope, Arc<AtomicBool>>>>,
    turns: Arc<LiveTurns>,
}

pub(crate) struct StaffTurn<'a> {
    scopes: &'a Scopes,
    key: TurnKey,
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl Drop for StaffTurn<'_> {
    fn drop(&mut self) {
        self.scopes
            .turns
            .lock()
            .expect("staff turn lock")
            .remove(&self.key);
    }
}

impl Scopes {
    pub(super) fn new(stop: Arc<AtomicBool>) -> Self {
        Self {
            stop,
            tokens: Arc::default(),
            turns: Arc::default(),
        }
    }

    pub(super) fn token(&self, scope: &BrainScope) -> Arc<AtomicBool> {
        self.tokens
            .lock()
            .expect("scope lock")
            .entry(scope.clone())
            .or_insert_with(|| Arc::new(AtomicBool::new(self.stop.load(Ordering::Relaxed))))
            .clone()
    }

    /// The route token remains valid for queued jobs. Only the active staff turn gets its own flag.
    pub(crate) fn staff_turn(
        &self,
        scope: &BrainScope,
        task_id: &str,
        round: u64,
        route_cancelled: &AtomicBool,
    ) -> StaffTurn<'_> {
        let key = (scope.clone(), task_id.to_owned());
        let mut turns = self.turns.lock().expect("staff turn lock");
        let cancelled = Arc::new(AtomicBool::new(route_cancelled.load(Ordering::Relaxed)));
        turns.insert(key.clone(), (round, cancelled.clone()));
        StaffTurn {
            scopes: self,
            key,
            cancelled,
        }
    }

    pub(super) fn stop_staff_turn(&self, scope: &BrainScope, task_id: &str, round: u64) -> bool {
        self.turns
            .lock()
            .expect("staff turn lock")
            .get(&(scope.clone(), task_id.to_owned()))
            .is_some_and(|(active_round, cancelled)| {
                *active_round == round && !cancelled.swap(true, Ordering::Relaxed)
            })
    }

    pub(super) fn staff_turn_active(&self, scope: &BrainScope, task_id: &str) -> bool {
        !self.stop.load(Ordering::Relaxed)
            && self
                .turns
                .lock()
                .expect("staff turn lock")
                .get(&(scope.clone(), task_id.to_owned()))
                .is_some_and(|(_, cancelled)| !cancelled.load(Ordering::Relaxed))
    }

    pub(super) fn release(&self, scope: &BrainScope) {
        // Keep a tombstone: a delayed producer cannot revive a released route.
        // Reopening a project receives a fresh route ID from the App endpoint.
        self.token(scope).store(true, Ordering::Relaxed);
        for ((owner, _), (_, cancelled)) in self.turns.lock().expect("staff turn lock").iter() {
            if owner == scope {
                cancelled.store(true, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn stop_all(&self) {
        self.stop.store(true, Ordering::Relaxed);
        for token in self.tokens.lock().expect("scope lock").values() {
            token.store(true, Ordering::Relaxed);
        }
        for (_, cancelled) in self.turns.lock().expect("staff turn lock").values() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 场景：连续轮次的权限判断不能等界面收到快照。
    /// 数据构造：同任务有两轮，并准备另一个路由。
    /// 执行过程：启动第一轮，取消并释放，再启动第二轮。
    /// 预期结果：仅同一路由的当前活跃轮有权限，取消或释放立即失效。
    #[test]
    fn active_turn_is_available_before_ui_update_and_revoked_on_stop() {
        let scopes = Scopes::new(Arc::new(AtomicBool::default()));
        let scope = BrainScope {
            workspace_id: "w".into(),
            route_id: "route".into(),
        };
        let other = BrainScope {
            route_id: "other".into(),
            ..scope.clone()
        };
        let cancelled = AtomicBool::default();
        assert!(
            !scopes.staff_turn_active(&scope, "a"),
            "a bound but idle task cannot act"
        );
        let first = scopes.staff_turn(&scope, "a", 1, &cancelled);
        assert!(
            scopes.staff_turn_active(&scope, "a"),
            "the running lane is authoritative before its first UI update"
        );
        assert!(
            !scopes.staff_turn_active(&other, "a"),
            "other routes cannot use the task turn"
        );
        assert!(
            scopes.stop_staff_turn(&scope, "a", 1),
            "the first round was active"
        );
        assert!(
            !scopes.staff_turn_active(&scope, "a"),
            "cancelled turns cannot authorize calls"
        );
        drop(first);
        let second = scopes.staff_turn(&scope, "a", 2, &cancelled);
        assert!(
            scopes.staff_turn_active(&scope, "a"),
            "a later turn can act as soon as it starts"
        );
        drop(second);
        assert!(
            !scopes.staff_turn_active(&scope, "a"),
            "a finished turn cannot act"
        );
    }
}
