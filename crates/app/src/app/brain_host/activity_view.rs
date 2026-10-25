//! 幕僚活动面板的状态：从库里读到的快照、筛选、展开、跳转与高亮，以及后台刷新。
//!
//! 面板的数据全部来自持久化的记录（写入延迟不到一秒），不再读各会话的内存缓冲。

use super::activity_feed::{
    build_feed, feed_areas, feed_triggers, find_turn, FeedFilter, FeedKind, FeedRow, TurnKey,
};
use super::activity_sink::{kind_from_key, unix_ms};
use crate::app::MultisoulApp;
use brain::acp::trace::TraceItem;
use gpui::{Context, ScrollHandle};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use storage::{BrainActivityStore, StorageResult};

/// 一页的轮次数；“显示更早”每次多读一页。
pub(crate) const PAGE_SIZE: usize = 100;
/// 跳转后高亮保持多久。
const FLASH: Duration = Duration::from_secs(1);
const REFRESH_OPEN: Duration = Duration::from_secs(1);
const REFRESH_CLOSED: Duration = Duration::from_secs(3);

/// （启动标识，时刻，Agent）：动作没有自增号可用，这三项足以区分。
pub(crate) type ActionKey = (i64, i64, String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RowKey {
    Turn(TurnKey),
    Action(ActionKey),
}

pub(crate) fn row_key(row: &FeedRow) -> RowKey {
    match row {
        FeedRow::Turn(turn) => RowKey::Turn(turn.key.clone()),
        FeedRow::Action(action) => RowKey::Action((
            action.row.launch_id,
            action.row.at_ms,
            action.row.agent.clone(),
        )),
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ActivitySnapshot {
    pub rows: Vec<FeedRow>,
    pub details: HashMap<TurnKey, Vec<TraceItem>>,
    pub unseen: usize,
    pub may_have_more: bool,
}

/// 读第 1 页到第 `pages` 页：轮次、预览、动作、展开轮次的正文与未读数。
pub(crate) fn read_snapshot(
    path: PathBuf,
    pages: usize,
    expanded: Vec<TurnKey>,
) -> StorageResult<ActivitySnapshot> {
    let store = BrainActivityStore::new(path);
    let limit = PAGE_SIZE * pages.max(1);
    let turns = store.recent_turns(limit, None)?;
    let may_have_more = turns.len() == limit;
    let mut actions = store.recent_actions(limit, None)?;
    if may_have_more {
        // 比最旧的轮次还早的动作没有上下文，留给下一页。
        let oldest = turns.last().map_or(0, |turn| turn.started_at_ms);
        actions.retain(|action| action.at_ms >= oldest);
    }
    let previews = store.turn_previews(&turns)?;
    let mut details = HashMap::new();
    for key in expanded {
        let items = store.turn_items(key.0, &key.1, &key.2, key.3)?;
        details.insert(
            key,
            items
                .into_iter()
                .map(|item| TraceItem::new(kind_from_key(&item.kind), item.title, item.text))
                .collect(),
        );
    }
    Ok(ActivitySnapshot {
        rows: build_feed(turns, previews, actions),
        details,
        unseen: store.unseen_actions()?,
        may_have_more,
    })
}

#[derive(Default)]
pub(crate) struct ActivityView {
    pub rows: Vec<FeedRow>,
    pub details: HashMap<TurnKey, Vec<TraceItem>>,
    pub unseen: usize,
    pub may_have_more: bool,
    pub filter: FeedFilter,
    pub expanded: HashSet<RowKey>,
    pages: usize,
    pub loading: bool,
    pub last_refresh: Option<Instant>,
    pub flash: Option<(RowKey, Instant)>,
    /// 下一次渲染要滚到的行。
    pub scroll_to: Option<RowKey>,
    pub scroll: ScrollHandle,
    /// 设置页的“清除”已点过一次，等待确认。
    pub confirm_clear: bool,
}

impl ActivityView {
    /// 返回这一次点击是否应当真正清除。
    pub(crate) fn request_clear(&mut self) -> bool {
        let confirmed = self.confirm_clear;
        self.confirm_clear = !confirmed;
        confirmed
    }

    pub(crate) fn pages(&self) -> usize {
        self.pages.max(1)
    }

    pub(crate) fn load_more(&mut self) {
        self.pages = self.pages() + 1;
        self.last_refresh = None;
    }

    pub(crate) fn apply(&mut self, snapshot: ActivitySnapshot) {
        let present: HashSet<RowKey> = snapshot.rows.iter().map(row_key).collect();
        self.expanded.retain(|key| present.contains(key));
        self.rows = snapshot.rows;
        self.details = snapshot.details;
        self.unseen = snapshot.unseen;
        self.may_have_more = snapshot.may_have_more;
    }

    pub(crate) fn set_kind(&mut self, kind: FeedKind) {
        self.filter.kind = kind;
    }

    pub(crate) fn cycle_area(&mut self) {
        self.filter.area = next_after(feed_areas(&self.rows), self.filter.area.clone());
    }

    pub(crate) fn cycle_trigger(&mut self) {
        self.filter.trigger = next_after(feed_triggers(&self.rows), self.filter.trigger);
    }

    pub(crate) fn is_expanded(&self, key: &RowKey) -> bool {
        self.expanded.contains(key)
    }

    /// 展开一行时顺带让下一次刷新立即去读它的正文。
    pub(crate) fn toggle(&mut self, key: RowKey) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
            self.last_refresh = None;
        }
    }

    pub(crate) fn expanded_turns(&self) -> Vec<TurnKey> {
        self.expanded
            .iter()
            .filter_map(|key| match key {
                RowKey::Turn(turn) => Some(turn.clone()),
                RowKey::Action(_) => None,
            })
            .collect()
    }

    /// 跳到某一轮：清筛选、展开、滚过去并短暂高亮。找不到时什么都不改。
    pub(crate) fn jump_to(&mut self, key: &TurnKey) -> bool {
        if find_turn(&self.rows, key).is_none() {
            return false;
        }
        self.filter = FeedFilter::default();
        let target = RowKey::Turn(key.clone());
        self.expanded.insert(target.clone());
        self.scroll_to = Some(target.clone());
        self.flash = Some((target, Instant::now()));
        self.last_refresh = None;
        true
    }

    pub(crate) fn flashing(&self, key: &RowKey, now: Instant) -> bool {
        self.flash
            .as_ref()
            .is_some_and(|(flash, at)| flash == key && now.duration_since(*at) < FLASH)
    }

    pub(crate) fn expire_flash(&mut self, now: Instant) -> bool {
        let expired = self
            .flash
            .as_ref()
            .is_some_and(|(_, at)| now.duration_since(*at) >= FLASH);
        if expired {
            self.flash = None;
        }
        expired
    }
}

/// 在“全部（空）→ 各项 → 全部”里取下一个。
fn next_after<T: PartialEq + Clone>(options: Vec<T>, current: Option<T>) -> Option<T> {
    match current {
        None => options.first().cloned(),
        Some(current) => options
            .iter()
            .position(|option| *option == current)
            .and_then(|at| options.get(at + 1).cloned()),
    }
}

impl MultisoulApp {
    pub(crate) fn activity(&self) -> &ActivityView {
        &self.brain.activity
    }

    pub(crate) fn activity_mut(&mut self) -> &mut ActivityView {
        &mut self.brain.activity
    }

    /// 抽屉入口上的数字：上次打开面板之后的新动作数。
    pub(crate) fn brain_unseen_actions(&self) -> usize {
        self.brain.activity.unseen
    }

    /// 在后台读一次库。面板打开时读整页并把动作标为已读，关着时只读未读数。
    /// 只有工作空间窗口读；每次最多一个在途请求，不到间隔不重复读。
    pub(crate) fn refresh_brain_activity(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.desktop_host.is_some() {
            return;
        }
        let open = self.brain.trace_open;
        let interval = if open { REFRESH_OPEN } else { REFRESH_CLOSED };
        let view = &mut self.brain.activity;
        let due = view.last_refresh.is_none_or(|at| at.elapsed() >= interval);
        if view.loading || !(force || due) {
            return;
        }
        view.loading = true;
        view.last_refresh = Some(Instant::now());
        let path = self.database_paths.app_db_path().to_path_buf();
        let (pages, expanded) = (view.pages(), view.expanded_turns());
        let work = cx.background_executor().spawn(async move {
            if open {
                let store = BrainActivityStore::new(path.clone());
                store.mark_actions_seen(unix_ms(SystemTime::now()))?;
                read_snapshot(path, pages, expanded).map(Some)
            } else {
                BrainActivityStore::new(path)
                    .unseen_actions()
                    .map(|unseen| {
                        Some(ActivitySnapshot {
                            unseen,
                            ..Default::default()
                        })
                    })
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let view = &mut app.brain.activity;
                view.loading = false;
                let mut changed = view.expire_flash(Instant::now());
                match result {
                    Ok(Some(snapshot)) if open => {
                        changed |= view.rows != snapshot.rows
                            || view.details != snapshot.details
                            || view.unseen != snapshot.unseen;
                        view.apply(snapshot);
                    }
                    Ok(Some(snapshot)) => {
                        changed |= view.unseen != snapshot.unseen;
                        view.unseen = snapshot.unseen;
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(%error, "读取幕僚活动失败"),
                }
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// 设置页的“清除”：第一次点击只进入确认态，第二次才清除。
    pub(crate) fn request_clear_brain_activity(&mut self, cx: &mut Context<Self>) {
        if self.brain.activity.request_clear() {
            self.clear_brain_activity(cx);
        }
        cx.notify();
    }

    /// 清除所有持久记录。后台执行，完成后刷新。
    pub(crate) fn clear_brain_activity(&mut self, cx: &mut Context<Self>) {
        let path = self.database_paths.app_db_path().to_path_buf();
        let work = cx
            .background_executor()
            .spawn(async move { BrainActivityStore::new(path).clear() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => {
                        app.brain.activity.apply(ActivitySnapshot::default());
                        app.brain.activity.last_refresh = None;
                    }
                    Err(error) => {
                        app.desktop_workspace.error =
                            Some(format!("清除幕僚活动失败：{error}").into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_activity_view_tests.rs"]
mod brain_activity_view_tests;
