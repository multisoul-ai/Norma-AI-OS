//! Bounded, lossless reads of the main Claude conversation. Paths come from an authenticated
//! local Hook, never from a remote client; every returned message is checked against its project.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const MAX_LINE: usize = 8 * 1024 * 1024;
// At most two maximum-sized JSONL records are scanned per page, including non-message events.
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
    pub parent_id: Option<String>,
    pub prompt_id: Option<String>,
    pub message_id: Option<String>,
    pub role: String,
    pub content: Value,
    pub timestamp: Option<String>,
    #[serde(default)]
    pub interrupted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TranscriptPage {
    pub messages: Vec<TranscriptMessage>,
    pub cursor: TranscriptCursor,
    pub has_more: bool,
    pub pending_tail: bool,
}

/// Blocking file IO: call on a background executor. The cursor is a position, not authorization.
/// Incomplete trailing JSON is retried at the same byte offset, never silently dropped.
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
    let (mut file, workspace) = open_transcript(path, projects_root, session_id, workspace)?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
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
                .map_err(|e| e.to_string())?;
            let mut byte = [0];
            file.read_exact(&mut byte).map_err(|e| e.to_string())?;
            if byte[0] != b'\n' {
                return Err("transcript cursor is not a line boundary".into());
            }
        }
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    // Freeze this page at the observed EOF; concurrent append is read by the next request.
    let mut reader = BufReader::new(file.take(length - offset));
    let mut messages = Vec::new();
    let mut scanned = 0;
    let mut pending_tail = false;
    while messages.len() < limit && scanned < MAX_SCAN {
        let mut line = Vec::new();
        (&mut reader)
            .take((MAX_LINE + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
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
            .map_err(|_| format!("invalid transcript JSON at byte {offset}"))?;
        if let Some(message) = parse_message(&event, session_id, &workspace)? {
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

/// Capture a complete EOF before sending a native interrupt. An unfinished existing record
/// is rejected, so its later completion cannot be mistaken for post-interrupt evidence.
/// Blocking file IO: call off the UI thread.
pub fn transcript_end(
    path: &Path,
    projects_root: &Path,
    session_id: &str,
    workspace: &Path,
) -> Result<TranscriptCursor, String> {
    let (mut file, _) = open_transcript(path, projects_root, session_id, workspace)?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("transcript is not a regular file".into());
    }
    if metadata.len() > 0 {
        file.seek(SeekFrom::Start(metadata.len() - 1))
            .map_err(|e| e.to_string())?;
        let mut byte = [0];
        file.read_exact(&mut byte).map_err(|e| e.to_string())?;
        if byte[0] != b'\n' {
            return Err(
                "transcript tail incomplete; retry interrupt after append completes".into(),
            );
        }
    }
    Ok(TranscriptCursor {
        device: metadata.dev(),
        inode: metadata.ino(),
        offset: metadata.len(),
        observed_len: metadata.len(),
    })
}

fn open_transcript(
    path: &Path,
    projects_root: &Path,
    session_id: &str,
    workspace: &Path,
) -> Result<(File, PathBuf), String> {
    if session_id.is_empty()
        || session_id.len() > 128
        || !session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid transcript session or page limit".into());
    }
    if path.file_name().and_then(|s| s.to_str()) != Some(&format!("{session_id}.jsonl")) {
        return Err("transcript filename does not match bound session".into());
    }
    let root = projects_root.canonicalize().map_err(|e| e.to_string())?;
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("transcript is outside Claude projects".into());
    }
    let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    Ok((file, workspace))
}

fn parse_message(
    event: &Value,
    session: &str,
    workspace: &Path,
) -> Result<Option<TranscriptMessage>, String> {
    let Some(role @ ("user" | "assistant")) = event["type"].as_str() else {
        return Ok(None);
    };
    if event["isSidechain"] == true {
        return Ok(None);
    }
    if event["sessionId"] != session {
        return Err("transcript message belongs to another session".into());
    }
    let cwd = event["cwd"]
        .as_str()
        .ok_or("transcript message missing cwd")?;
    if !Path::new(cwd)
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(workspace)
    {
        return Err("transcript message belongs to another project".into());
    }
    let message = &event["message"];
    if message["role"] != role || !(message["content"].is_string() || message["content"].is_array())
    {
        return Err("invalid transcript message content".into());
    }
    let id = event["uuid"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("transcript message missing identity")?;
    Ok(Some(TranscriptMessage {
        id: id.into(),
        parent_id: event["parentUuid"].as_str().map(str::to_owned),
        prompt_id: event["promptId"].as_str().map(str::to_owned),
        message_id: message["id"].as_str().map(str::to_owned),
        role: role.into(),
        content: message["content"].clone(),
        timestamp: event["timestamp"].as_str().map(str::to_owned),
        // Claude native 2.1.202 appends a synthetic user block. Typed/pasted user input
        // has origin/promptSource and must never be interpreted as a control result.
        interrupted: role == "user"
            && event["origin"].is_null()
            && event["promptSource"].is_null()
            && event["promptId"].as_str().is_some_and(|id| !id.is_empty())
            && message["content"].as_array().is_some_and(|blocks| {
                blocks.len() == 1
                    && blocks[0]["type"] == "text"
                    && matches!(
                        blocks[0]["text"].as_str(),
                        Some(
                            "[Request interrupted by user]"
                                | "[Request interrupted by user for tool use]"
                        )
                    )
            }),
    }))
}
