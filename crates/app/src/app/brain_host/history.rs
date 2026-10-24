//! `norma brain history`：读某个 Agent 卡片最近的完整对话。
//!
//! 前台只做“准备”：校验令牌、定位卡片、拿到读取句柄；真正读文件或请求 Codex 在后台执行。
//! 只支持带手机控制的 Claude 与 Codex 卡片，其他卡片如实返回不可用。

use super::ops::reject;
use crate::app::MultisoulApp;
use brain::history::{render, HistoryItem, Options};
use gpui::{Context, Task};
use norma::mobile::{ConversationItem, ConversationPage};
use norma::protocol::CardCommandError;
use std::time::Duration;

const DEFAULT_LAST: u32 = 6;
const MAX_LAST: u32 = 20;
/// Claude 会话文件按 100 条一页向后读；页数封顶，避免超长会话拖慢读取。
const MAX_PAGES: usize = 30;
const CODEX_WAIT: Duration = Duration::from_secs(20);

/// 把手机端的会话条目转成大脑读的条目。
pub(super) fn to_history_items(items: &[ConversationItem]) -> Vec<HistoryItem> {
    items
        .iter()
        .map(|item| HistoryItem {
            role: item.role.clone(),
            text: item.text.clone(),
            details: item.details.clone(),
        })
        .collect()
}

/// 一页读到的条目，以及下一页的游标；没有下一页时 `next` 为 `None`。
pub(super) struct Page<C> {
    pub items: Vec<ConversationItem>,
    pub next: Option<C>,
}

/// 从头向后一页页读，直到没有下一页或页数封顶；读失败立刻返回原因。
pub(super) fn collect_pages<C>(
    mut fetch: impl FnMut(Option<C>) -> Result<Page<C>, String>,
) -> Result<Vec<ConversationItem>, String> {
    let mut items = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let page = fetch(cursor.take())?;
        items.extend(page.items);
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(items)
}

/// 把读取失败的原因翻成大脑和你都读得懂的话；认不出的原样带上。
pub(super) fn explain(error: &str) -> String {
    if error.contains("transcript path") {
        return "Claude 还没有给出会话文件（刚启动，或还没有产生对话）".into();
    }
    if error.contains("closed") {
        return "这张卡片的会话已经结束".into();
    }
    format!("读取对话失败：{error}")
}

pub(super) fn options(last: Option<u32>, grep: Option<String>) -> Options {
    Options {
        last: last.unwrap_or(DEFAULT_LAST).clamp(1, MAX_LAST) as usize,
        grep: grep.filter(|word| !word.trim().is_empty()),
        ..Options::default()
    }
}

impl MultisoulApp {
    pub(crate) fn prepare_brain_history(
        &mut self,
        token: &str,
        agent: &str,
        last: Option<u32>,
        grep: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<Task<Result<String, String>>, CardCommandError> {
        if !self.brain.token_matches(token) {
            return Err(CardCommandError::Unsupported {
                message: "大脑令牌无效".into(),
            });
        }
        let node_id = self
            .agent_roster_for_provider()
            .iter()
            .find(|entry| entry.name == agent)
            .and_then(|entry| entry.active_node_id)
            .ok_or_else(|| reject("no_card：这个 Agent 没有打开的卡片，没有对话可读"))?;
        let profile = self
            .runtime
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == node_id)
            .map(|card| card.profile)
            .ok_or_else(|| {
                reject("unsupported：只支持带手机控制的 Claude 和 Codex 卡片，这张卡片读不了")
            })?;
        let options = options(last, grep);
        let name = agent.to_string();
        match profile.as_str() {
            "Claude" => {
                let run_id = self
                    .runtime
                    .mobile_agent_run_id(node_id)
                    .map(str::to_string)
                    .ok_or_else(|| reject("unsupported：这张 Claude 卡片没有可读的会话"))?;
                let read = self
                    .runtime
                    .mobile_claude_history(node_id, &run_id)
                    .map_err(|_| reject("unavailable：Claude 会话暂时读不了"))?;
                Ok(cx.background_executor().spawn(async move {
                    let items = collect_pages(|cursor| {
                        let raw = read.read(cursor, 100).map_err(|e| explain(&e))?;
                        let next = (raw.has_more && !raw.pending_tail).then(|| raw.cursor.clone());
                        let page = ConversationPage::from_claude(raw).map_err(|e| explain(&e))?;
                        Ok(Page {
                            items: page.items,
                            next,
                        })
                    })?;
                    Ok(render(&name, &to_history_items(&items), &options))
                }))
            }
            "Codex" => {
                let session = self
                    .runtime
                    .agent_session_id(node_id)
                    .ok_or_else(|| reject("unsupported：这张 Codex 卡片还没有会话"))?;
                let receiver = self
                    .runtime
                    .mobile_codex_history(node_id, &session, None)
                    .map_err(|_| reject("unsupported：只支持带手机控制的 Codex 卡片"))?;
                Ok(cx.background_executor().spawn(async move {
                    let raw = receiver
                        .recv_timeout(CODEX_WAIT)
                        .map_err(|_| "Codex 没有在时限内返回对话".to_string())?
                        .map_err(|e| explain(&e))?;
                    let page = ConversationPage::from_codex(raw).map_err(|e| explain(&e))?;
                    Ok(render(&name, &to_history_items(&page.items), &options))
                }))
            }
            other => Err(reject(&format!(
                "unsupported：{other} 卡片暂不支持读取对话，只支持带手机控制的 Claude 和 Codex"
            ))),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_history_tests.rs"]
mod brain_history_tests;
