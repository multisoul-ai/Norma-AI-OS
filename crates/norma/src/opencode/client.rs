//! Bounded background HTTP to one owned loopback instance, directory and native session.
use super::questions::OpenCodeQuestion;
use crate::protocol::QuestionAnswer;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{path::Path, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct OpenCodeQuestionClient {
    endpoint: Url,
    directory: String,
    session: String,
    authorization: String,
    connection: String,
    agent: ureq::Agent,
}

#[derive(Clone, Debug)]
pub struct OpenCodeQuestionLease {
    connection: String,
    question: OpenCodeQuestion,
}
impl OpenCodeQuestionLease {
    pub fn question(&self) -> &OpenCodeQuestion {
        &self.question
    }
    pub fn connection_id(&self) -> &str {
        &self.connection
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyError {
    WrongConnection,
    InvalidAnswer,
    Unavailable,
    NotPending,
    Changed,
    Rejected,
    Unknown,
}

pub(super) struct PreparedReply {
    connection: String,
    request_id: String,
    body: Value,
}

impl OpenCodeQuestionClient {
    /// Parameters are captured from the owned launch, never accepted from a question/proposal.
    pub fn new(
        endpoint: &str,
        directory: &str,
        session: &str,
        user: &str,
        password: &str,
    ) -> Result<Self, &'static str> {
        let endpoint = Url::parse(endpoint).map_err(|_| "opencode_invalid_endpoint")?;
        let loopback = match endpoint.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if !loopback
            || endpoint.scheme() != "http"
            || endpoint.port().is_none()
            || endpoint.path() != "/"
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || !Path::new(directory).is_absolute()
            || directory.len() > 32768
            || session.is_empty()
            || session.len() > 4096
            || user.is_empty()
            || user.contains(':')
            || password.is_empty()
            || user.len() + password.len() > 8192
        {
            return Err("opencode_invalid_instance_binding");
        }
        let connection = format!(
            "opencode-native:{}",
            crate::agent_hook::fresh_control_id().map_err(|_| "opencode_identity_unavailable")?
        );
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(3)))
            .build()
            .new_agent();
        Ok(Self {
            endpoint,
            directory: directory.into(),
            session: session.into(),
            authorization: format!("Basic {}", STANDARD.encode(format!("{user}:{password}"))),
            connection,
            agent,
        })
    }

    /// Background startup only. The returned identity comes from this authenticated instance.
    /// Unknown versions keep the original TUI available, but cannot enable answer delivery.
    pub fn prepare_native_session(
        endpoint: &str,
        directory: &str,
        resume: Option<&str>,
        password: &str,
    ) -> Result<(Self, bool), &'static str> {
        let mut client = Self::new(endpoint, directory, "native-startup", "opencode", password)?;
        let health = client.get(&["global", "health"])?;
        if health["healthy"] != true {
            return Err("opencode_not_ready");
        }
        let supported = health["version"] == "1.19.3";
        let session = if let Some(id) = resume {
            let session = client.get(&["session", id])?;
            if session["id"] != id {
                return Err("opencode_resume_identity_mismatch");
            }
            session
        } else {
            let mut response = client
                .agent
                .post(client.url(&["session"]).as_str())
                .header("Authorization", &client.authorization)
                .send_json(serde_json::json!({}))
                .map_err(|_| "opencode_session_create_unknown")?;
            if response.status().as_u16() != 200 {
                return Err("opencode_session_create_failed");
            }
            response
                .body_mut()
                .with_config()
                .limit(65536)
                .read_json::<Value>()
                .map_err(|_| "opencode_invalid_session")?
        };
        let id = session["id"]
            .as_str()
            .filter(|id| id.starts_with("ses_") && id.len() <= 4096)
            .ok_or("opencode_invalid_session")?;
        if session["directory"] != directory {
            return Err("opencode_session_directory_mismatch");
        }
        client.session = id.into();
        Ok((client, supported))
    }

    pub fn instance_ready(endpoint: &str, directory: &str, password: &str) -> bool {
        Self::new(endpoint, directory, "native-startup", "opencode", password)
            .ok()
            .and_then(|client| client.get(&["global", "health"]).ok())
            .is_some_and(|health| health["healthy"] == true)
    }

    pub(super) fn follow_session(&mut self, session: &str) -> Result<(), &'static str> {
        self.session = session.into();
        self.connection = format!(
            "opencode-native:{}",
            crate::agent_hook::fresh_control_id().map_err(|_| "opencode_identity_unavailable")?
        );
        Ok(())
    }

    pub fn connection_id(&self) -> &str {
        &self.connection
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }

    /// Also used on every reconnect. Events alone cannot reconstruct the server's pending set.
    pub fn pending(&self) -> Result<Vec<OpenCodeQuestionLease>, &'static str> {
        let value = self.get(&["question"])?;
        let pending = value
            .as_array()
            .filter(|values| values.len() <= 1024)
            .ok_or("opencode_invalid_pending")?;
        let mut seen = std::collections::HashSet::new();
        pending
            .iter()
            .filter(|raw| raw["sessionID"] == self.session)
            .map(|raw| {
                let question = OpenCodeQuestion::from_request(raw)?;
                if !seen.insert(question.request_id().to_owned()) {
                    return Err("opencode_duplicate_request");
                }
                Ok(OpenCodeQuestionLease {
                    connection: self.connection.clone(),
                    question,
                })
            })
            .collect()
    }

    /// A 200/true acknowledges this POST only. The host must still correlate tool consumption.
    /// No network/HTTP error retries a POST; the calling host owns the once-only delivery lease.
    pub fn reply(
        &self,
        lease: &OpenCodeQuestionLease,
        answers: &[QuestionAnswer],
    ) -> Result<(), ReplyError> {
        self.send_prepared(self.prepare_reply(lease, answers)?)
    }

    pub(super) fn prepare_reply(
        &self,
        lease: &OpenCodeQuestionLease,
        answers: &[QuestionAnswer],
    ) -> Result<PreparedReply, ReplyError> {
        if lease.connection != self.connection || lease.question.session_id() != self.session {
            return Err(ReplyError::WrongConnection);
        }
        let body = lease
            .question
            .response(answers)
            .map_err(|_| ReplyError::InvalidAnswer)?;
        let current = self.pending().map_err(|_| ReplyError::Unavailable)?;
        let current = current
            .iter()
            .find(|current| current.question.request_id() == lease.question.request_id())
            .ok_or(ReplyError::NotPending)?;
        if current.question != lease.question {
            return Err(ReplyError::Changed);
        }
        Ok(PreparedReply {
            connection: self.connection.clone(),
            request_id: lease.question.request_id().into(),
            body,
        })
    }

    pub(super) fn send_prepared(&self, prepared: PreparedReply) -> Result<(), ReplyError> {
        if prepared.connection != self.connection {
            return Err(ReplyError::WrongConnection);
        }
        let endpoint = self.url(&["question", &prepared.request_id, "reply"]);
        let mut response = self
            .agent
            .post(endpoint.as_str())
            .header("Authorization", &self.authorization)
            .send_json(prepared.body)
            .map_err(|_| ReplyError::Unknown)?;
        match response.status().as_u16() {
            404 | 409 => Err(ReplyError::NotPending),
            400 | 401 | 403 | 422 => Err(ReplyError::Rejected),
            200 => {
                let body: Value = response
                    .body_mut()
                    .with_config()
                    .limit(4096)
                    .read_json()
                    .map_err(|_| ReplyError::Unknown)?;
                if body == Value::Bool(true) {
                    Ok(())
                } else {
                    Err(ReplyError::Unknown)
                }
            }
            _ => Err(ReplyError::Unknown),
        }
    }

    /// Point-read only the original message; no full session scans on the foreground path.
    pub(super) fn tool_part(
        &self,
        lease: &OpenCodeQuestionLease,
    ) -> Result<Option<Value>, &'static str> {
        if lease.connection != self.connection || lease.question.session_id() != self.session {
            return Err("opencode_wrong_connection");
        }
        let message = self.get(&[
            "session",
            &self.session,
            "message",
            lease.question.message_id(),
        ])?;
        let parts = message["parts"]
            .as_array()
            .filter(|parts| parts.len() <= 1024)
            .ok_or("opencode_invalid_message")?;
        Ok(parts
            .iter()
            .find(|part| {
                part["type"] == "tool"
                    && part["callID"] == lease.question.call_id()
                    && part["sessionID"] == self.session
                    && part["messageID"] == lease.question.message_id()
            })
            .cloned())
    }

    fn get(&self, path: &[&str]) -> Result<Value, &'static str> {
        let mut response = self
            .agent
            .get(self.url(path).as_str())
            .header("Authorization", &self.authorization)
            .call()
            .map_err(|_| "opencode_transport_unavailable")?;
        if response.status().as_u16() != 200 {
            return Err("opencode_http_unavailable");
        }
        response
            .body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_json()
            .map_err(|_| "opencode_invalid_response")
    }
    fn url(&self, path: &[&str]) -> Url {
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .expect("validated HTTP base")
            .clear()
            .extend(path);
        url.query_pairs_mut()
            .append_pair("directory", &self.directory);
        url
    }
}
