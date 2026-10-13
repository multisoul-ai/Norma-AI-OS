use super::{ConnectorTools, ListTasks, Task};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(super) struct Page {
    query: String,
    tasks: Vec<Task>,
    expires: i64,
}
impl ConnectorTools {
    pub(super) fn page(&self, args: ListTasks) -> Result<Value, &'static str> {
        let query = hex::encode(Sha256::digest(serde_json::to_vec(&json!({"workspace_id":args.workspace_id,"card_id":args.card_id,"states":args.states,"since":args.since,"limit":args.limit})).map_err(|_|"internal")?));
        let now = self.auth.now();
        let (key, offset) = if let Some(cursor) = &args.cursor {
            if cursor.len() > 256 {
                return Err("invalid_argument");
            }
            let (key, offset) = cursor.split_once('.').ok_or("invalid_argument")?;
            let offset = offset.parse::<usize>().map_err(|_| "invalid_argument")?;
            (key.to_owned(), offset)
        } else {
            let mut snapshot = self.source.snapshot().map_err(|_| "internal")?;
            self.project(&mut snapshot);
            let mut tasks = snapshot.tasks;
            tasks.retain(|t| {
                args.workspace_id
                    .as_ref()
                    .is_none_or(|w| w == &t.workspace_id)
                    && args.card_id.as_ref().is_none_or(|c| c == &t.card_id)
                    && args.states.as_ref().is_none_or(|s| s.contains(&t.state))
                    && args.since.is_none_or(|s| t.updated_at >= s)
                    && (args.states.is_some()
                        || args.since.is_some()
                        || !t.terminal()
                        || t.updated_at >= now - 86400)
            });
            tasks.sort_by(|a, b| (a.created_at, &a.task_id).cmp(&(b.created_at, &b.task_id)));
            if tasks.len() <= args.limit {
                return Ok(json!({"tasks":tasks,"next_cursor":null}));
            }
            let mut random = [0; 32];
            SystemRandom::new()
                .fill(&mut random)
                .map_err(|_| "internal")?;
            let key = URL_SAFE_NO_PAD.encode(random);
            let mut pages = self.pages.lock().map_err(|_| "internal")?;
            pages.retain(|_, p| p.expires > now);
            if pages.len() >= 128 {
                return Err("internal");
            }
            pages.insert(
                key.clone(),
                Page {
                    query: query.clone(),
                    tasks,
                    expires: now + 300,
                },
            );
            (key, 0)
        };
        let pages = self.pages.lock().map_err(|_| "internal")?;
        let page = pages
            .get(&key)
            .filter(|p| p.expires > now && p.query == query)
            .ok_or("invalid_argument")?;
        if offset > page.tasks.len() || offset % args.limit != 0 {
            return Err("invalid_argument");
        }
        let end = offset.saturating_add(args.limit).min(page.tasks.len());
        let cursor = (end < page.tasks.len()).then(|| format!("{key}.{end}"));
        Ok(json!({"tasks":page.tasks[offset..end],"next_cursor":cursor}))
    }
}
