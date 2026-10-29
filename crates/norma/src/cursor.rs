//! Bounded reads of Cursor's local Agent transcript. The path is supplied by an authenticated
//! local Hook and must resolve to this workspace and session under Cursor's projects directory.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::Path,
};

pub mod host;
pub mod questions;
pub mod screen;
pub mod store;

const MAX_LINE: usize = 8 * 1024 * 1024;
const MAX_SCAN: usize = 2 * MAX_LINE;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptCursor {
    device: u64,
    inode: u64,
    offset: u64,
    observed_len: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TranscriptMessage {
    pub id: String,
    pub role: String,
    pub content: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TranscriptPage {
    pub messages: Vec<TranscriptMessage>,
    pub cursor: TranscriptCursor,
    pub has_more: bool,
    pub pending_tail: bool,
}

/// Blocking file IO: callers must run this away from the GPUI foreground executor.
/// An incomplete final record is retried from the same byte boundary on the next read.
pub fn read_transcript(
    path: &Path,
    projects_root: &Path,
    session_id: &str,
    workspace: &Path,
    cursor: Option<TranscriptCursor>,
    limit: usize,
) -> Result<TranscriptPage, String> {
    if !(1..=100).contains(&limit) {
        return Err("invalid transcript page limit".into());
    }
    let mut file = open_transcript(path, projects_root, session_id, workspace)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("transcript is not a regular file".into());
    }
    let length = metadata.len();
    let mut offset = 0;
    if let Some(cursor) = cursor {
        if cursor.device != metadata.dev()
            || cursor.inode != metadata.ino()
            || length < cursor.observed_len
            || cursor.offset > length
        {
            return Err("transcript changed; restart from a fresh page".into());
        }
        offset = cursor.offset;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))
                .map_err(|error| error.to_string())?;
            let mut byte = [0];
            file.read_exact(&mut byte)
                .map_err(|error| error.to_string())?;
            if byte[0] != b'\n' {
                return Err("transcript cursor is not a line boundary".into());
            }
        }
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file.take(length - offset));
    let mut messages = Vec::new();
    let mut scanned = 0;
    let mut pending_tail = false;
    while messages.len() < limit && scanned < MAX_SCAN {
        let record_offset = offset;
        let mut line = Vec::new();
        (&mut reader)
            .take((MAX_LINE + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?;
        if line.is_empty() {
            break;
        }
        if line.len() > MAX_LINE {
            return Err("transcript record exceeds 8 MiB; no content was truncated".into());
        }
        if line.last() != Some(&b'\n') {
            pending_tail = true;
            break;
        }
        if scanned + line.len() > MAX_SCAN {
            break;
        }
        let event: Value = serde_json::from_slice(&line)
            .map_err(|_| format!("invalid transcript JSON at byte {record_offset}"))?;
        if let Some(message) = parse_message(&event, record_offset)? {
            messages.push(message);
        }
        offset += line.len() as u64;
        scanned += line.len();
    }
    Ok(TranscriptPage {
        messages,
        cursor: TranscriptCursor {
            device: metadata.dev(),
            inode: metadata.ino(),
            offset,
            observed_len: length,
        },
        has_more: offset < length && !pending_tail,
        pending_tail,
    })
}

fn open_transcript(
    path: &Path,
    projects_root: &Path,
    session_id: &str,
    workspace: &Path,
) -> Result<File, String> {
    if session_id.is_empty()
        || session_id.len() > 128
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("invalid transcript session".into());
    }
    let workspace = workspace
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let project_name = workspace
        .strip_prefix("/")
        .map_err(|_| "workspace is not absolute")?
        .to_string_lossy()
        .replace('/', "-");
    let expected = projects_root
        .join(project_name)
        .join("agent-transcripts")
        .join(session_id)
        .join(format!("{session_id}.jsonl"))
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let actual = path.canonicalize().map_err(|error| error.to_string())?;
    if actual != expected {
        return Err("transcript is outside the bound Cursor project or session".into());
    }
    File::open(actual).map_err(|error| error.to_string())
}

fn parse_message(event: &Value, offset: u64) -> Result<Option<TranscriptMessage>, String> {
    let Some(role @ ("user" | "assistant")) = event["role"].as_str() else {
        return Ok(None);
    };
    let message = event
        .get("message")
        .and_then(Value::as_object)
        .ok_or("Cursor transcript message missing body")?;
    if message
        .get("role")
        .and_then(Value::as_str)
        .is_some_and(|message_role| message_role != role)
    {
        return Err("Cursor transcript role mismatch".into());
    }
    let content = message
        .get("content")
        .filter(|content| content.is_string() || content.is_array())
        .ok_or("Cursor transcript message missing content")?;
    Ok(Some(TranscriptMessage {
        id: format!("cursor:{offset}"),
        role: role.into(),
        content: content.clone(),
    }))
}
