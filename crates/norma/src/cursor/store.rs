//! Read only the bound Cursor store's current root. All IO belongs on a background worker.
use super::questions::CursorQuestion;
use md5::Md5;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

const MAX_META: usize = 128 * 1024;
const MAX_BLOB: usize = 2 * 1024 * 1024;
const MAX_READ: usize = 8 * 1024 * 1024;
const MAX_MESSAGES: usize = 64;

#[derive(Clone, Debug)]
pub struct CursorQuestionStore {
    path: PathBuf,
    session: String,
}

#[derive(Debug)]
pub struct CursorStoreSnapshot {
    pub root_id: String,
    pub pending: Vec<CursorQuestion>,
    pub results: Vec<Value>,
}

impl CursorQuestionStore {
    pub fn for_session(
        config_root: &Path,
        workspace: &str,
        session: &str,
    ) -> Result<Self, &'static str> {
        if session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("cursor_store_invalid_session");
        }
        // Cursor hashes path.resolve(cwd), which deliberately retains symlink aliases.
        let path = Path::new(workspace);
        if !path.is_absolute() || workspace.len() > 16384 || workspace.contains('\0') {
            return Err("cursor_store_invalid_workspace");
        }
        let mut resolved = PathBuf::from("/");
        for component in path.components() {
            match component {
                Component::Normal(name) => resolved.push(name),
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::RootDir | Component::CurDir => {}
                Component::Prefix(_) => return Err("cursor_store_invalid_workspace"),
            }
        }
        let hash = format!("{:x}", Md5::digest(resolved.as_os_str().as_encoded_bytes()));
        Ok(Self {
            path: config_root
                .join("chats")
                .join(hash)
                .join(session)
                .join("store.db"),
            session: session.into(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<CursorStoreSnapshot, &'static str> {
        let mut connection =
            Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|_| "cursor_store_unavailable")?;
        connection
            .busy_timeout(std::time::Duration::from_millis(50))
            .map_err(|_| "cursor_store_unavailable")?;
        let transaction = connection
            .transaction()
            .map_err(|_| "cursor_store_unavailable")?;
        let metadata: String = transaction
            .query_row(
                "SELECT value FROM meta WHERE key='0' AND length(value)<=?1",
                [MAX_META],
                |row| row.get(0),
            )
            .map_err(|_| "cursor_store_invalid_metadata")?;
        let metadata: Value = serde_json::from_slice(
            &hex::decode(metadata).map_err(|_| "cursor_store_invalid_metadata")?,
        )
        .map_err(|_| "cursor_store_invalid_metadata")?;
        if metadata["agentId"] != self.session {
            return Err("cursor_store_session_mismatch");
        }
        let root_id = metadata["latestRootBlobId"]
            .as_str()
            .ok_or("cursor_store_missing_root")?;
        let mut budget = MAX_READ;
        let root = read_blob(&transaction, root_id, &mut budget)?;
        let (pending, references) = root_fields(&root)?;
        let mut snapshot = CursorStoreSnapshot {
            root_id: root_id.into(),
            pending: Vec::new(),
            results: Vec::new(),
        };
        if let Some(bytes) = pending {
            let message: Value =
                serde_json::from_slice(bytes).map_err(|_| "cursor_store_invalid_message")?;
            if message["role"] != "assistant" {
                return Err("cursor_store_invalid_pending");
            }
            if let Some(content) = message["content"].as_array() {
                for tool in content {
                    if tool["type"] == "tool-call" && tool["toolName"] == "AskQuestion" {
                        let question = CursorQuestion::from_tool(&self.session, tool)?;
                        if snapshot
                            .pending
                            .iter()
                            .any(|old| old.call_id() == question.call_id())
                        {
                            return Err("cursor_store_duplicate_call");
                        }
                        snapshot.pending.push(question);
                    }
                }
            }
        }
        // Only message references belonging to this root can prove consumption. Unlinked blobs
        // (including tool results from another branch) are never searched.
        for reference in references.iter().rev().take(MAX_MESSAGES).rev() {
            let bytes = read_blob(&transaction, &hex::encode(reference), &mut budget)?;
            let message: Value =
                serde_json::from_slice(&bytes).map_err(|_| "cursor_store_invalid_message")?;
            if message["role"] == "tool" {
                if let Some(content) = message["content"].as_array() {
                    snapshot.results.extend(
                        content
                            .iter()
                            .filter(|tool| {
                                tool["type"] == "tool-result" && tool["toolName"] == "AskQuestion"
                            })
                            .cloned(),
                    );
                }
            }
        }
        Ok(snapshot)
    }
}

fn read_blob(
    connection: &Connection,
    id: &str,
    budget: &mut usize,
) -> Result<Vec<u8>, &'static str> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("cursor_store_invalid_blob_id");
    }
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT data FROM blobs WHERE id=?1 AND length(data)<=?2",
            rusqlite::params![id, MAX_BLOB.min(*budget)],
            |row| row.get(0),
        )
        .map_err(|_| "cursor_store_missing_or_large_blob")?;
    *budget -= bytes.len();
    if format!("{:x}", Sha256::digest(&bytes)) != id {
        return Err("cursor_store_blob_mismatch");
    }
    Ok(bytes)
}

type RootFields<'a> = (Option<&'a [u8]>, Vec<&'a [u8]>);
fn root_fields(mut bytes: &[u8]) -> Result<RootFields<'_>, &'static str> {
    let mut pending = None;
    let mut references = Vec::new();
    while !bytes.is_empty() {
        let tag = varint(&mut bytes)?;
        let number = tag >> 3;
        if number == 0 || number > 536_870_911 {
            return Err("cursor_store_invalid_root");
        }
        let value = match tag & 7 {
            0 => {
                varint(&mut bytes)?;
                None
            }
            1 => {
                take(&mut bytes, 8)?;
                None
            }
            2 => {
                let length = usize::try_from(varint(&mut bytes)?)
                    .map_err(|_| "cursor_store_invalid_root")?;
                Some(take(&mut bytes, length)?)
            }
            5 => {
                take(&mut bytes, 4)?;
                None
            }
            _ => return Err("cursor_store_invalid_root"),
        };
        match number {
            1 => {
                let value = value
                    .filter(|v| v.len() == 32)
                    .ok_or("cursor_store_invalid_reference")?;
                references.push(value);
            }
            4 => {
                let value = value.ok_or("cursor_store_invalid_pending")?;
                if pending.replace(value).is_some() {
                    return Err("cursor_store_duplicate_pending");
                }
            }
            _ => {}
        }
    }
    Ok((pending, references))
}

fn take<'a>(bytes: &mut &'a [u8], size: usize) -> Result<&'a [u8], &'static str> {
    if size > bytes.len() {
        return Err("cursor_store_incomplete_root");
    }
    let (value, tail) = bytes.split_at(size);
    *bytes = tail;
    Ok(value)
}

fn varint(bytes: &mut &[u8]) -> Result<u64, &'static str> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let byte = take(bytes, 1)?[0];
        if shift == 63 && byte > 1 {
            return Err("cursor_store_invalid_varint");
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err("cursor_store_invalid_varint")
}
