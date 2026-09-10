//! Observe the native dialog without blocking a Hook or replacing the original CLI.
use super::*;
use norma::{
    agent_hook::AgentHookEvent,
    mobile::{ApprovalSnapshot, ApprovalTarget},
};
use serde_json::Value;

struct Tool {
    id: String,
    name: String,
    input: Value,
}

pub(super) struct Permission {
    pub(super) id: String,
    pub(super) tool_id: String,
    pub(super) tool: String,
    pub(super) input: Value,
    tool_input: Value,
    pub(super) active: Arc<AtomicBool>,
    pub(super) executed: Arc<AtomicBool>,
    pub(super) claimed: bool,
    pub(super) settled: bool,
}

impl Drop for Permission {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

#[derive(Default)]
pub(crate) struct NativeApprovals {
    tools: Vec<Tool>,
    pub(super) permission: Option<Permission>,
    ambiguous: bool,
}

impl NativeApprovals {
    pub(crate) fn invalidate(&mut self) {
        self.permission.take();
    }

    pub(super) fn observe(&mut self, event: &AgentHookEvent) {
        if !matches!(
            event.profile,
            norma::protocol::AgentProfile::Claude | norma::protocol::AgentProfile::Codex
        ) {
            return;
        }
        match event.provider_event.as_str() {
            "UserPromptSubmit" | "SessionEnd" => *self = Self::default(),
            "PreToolUse" => {
                let Some(id) = event.raw_json["tool_use_id"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                else {
                    self.ambiguous = true;
                    self.invalidate();
                    return;
                };
                // 32 * 64KiB = at most 2MiB retained input; larger events remain desktop-only.
                if self.tools.len() >= 32
                    || self.tools.iter().any(|tool| tool.id == id)
                    || serde_json::to_vec(&event.raw_json["tool_input"])
                        .map_or(true, |bytes| bytes.len() > 64 * 1024)
                {
                    self.ambiguous = true;
                    self.invalidate();
                    return;
                }
                self.tools.push(Tool {
                    id: id.into(),
                    name: event.tool_name.clone().unwrap_or_default(),
                    input: event.raw_json["tool_input"].clone(),
                });
                // Concurrent tools cannot be matched to a terminal dialog by arrival order.
                if self.tools.len() != 1 {
                    self.invalidate();
                }
            }
            "PermissionRequest" => {
                if self.permission.is_some() {
                    self.ambiguous = true;
                }
                self.invalidate();
                if self.ambiguous || self.tools.len() != 1 {
                    return;
                }
                let tool = &self.tools[0];
                if !(tool.name == "Bash"
                    || (event.profile == norma::protocol::AgentProfile::Claude
                        && tool.name == "Write"))
                    || event.tool_name.as_deref() != Some(&tool.name)
                    || !permission_input_matches(event, &tool.input)
                    || event.raw_json.get("agent_id").is_some()
                {
                    return;
                }
                let Ok(id) = fresh_control_id() else {
                    return;
                };
                self.permission = Some(Permission {
                    id: format!("native-{id}"),
                    tool_id: tool.id.clone(),
                    tool: tool.name.clone(),
                    input: event.raw_json["tool_input"].clone(),
                    tool_input: tool.input.clone(),
                    active: Arc::new(AtomicBool::new(true)),
                    executed: Arc::new(AtomicBool::new(false)),
                    claimed: false,
                    settled: false,
                });
            }
            "PostToolUse" | "PostToolUseFailure" => {
                let id = event.raw_json["tool_use_id"].as_str();
                if let Some(p) = self.permission.as_ref().filter(|p| {
                    p.claimed
                        && Some(p.tool_id.as_str()) == id
                        && event.tool_name.as_deref() == Some(&p.tool)
                        && event.raw_json["tool_input"] == p.tool_input
                        && (event.provider_event == "PostToolUse"
                            || event.raw_json["is_interrupt"] == false)
                }) {
                    // These native events occur after tool execution, including nonzero exit.
                    // Permission rejection/interrupt is not evidence that Allow was consumed.
                    p.executed.store(true, Ordering::Release);
                }
                self.tools.retain(|tool| Some(tool.id.as_str()) != id);
                if self
                    .permission
                    .as_ref()
                    .is_some_and(|p| Some(p.tool_id.as_str()) == id)
                {
                    self.settle();
                }
            }
            "Stop" | "StopFailure" => {
                self.tools.clear();
                self.settle();
            }
            _ => {}
        }
    }

    fn settle(&mut self) {
        if let Some(p) = &mut self.permission {
            if p.claimed {
                p.settled = true;
            } else {
                self.invalidate();
            }
        }
    }
}

impl Runtime {
    pub(super) fn native_approval_snapshot(
        &self,
        card: &norma::mobile::CardSnapshot,
    ) -> Option<ApprovalSnapshot> {
        let run = self.mobile_runs.get(&card.node_id)?;
        let p = run.native_approvals.permission.as_ref()?;
        if p.claimed || p.settled || !p.active.load(Ordering::Acquire) {
            return None;
        }
        let target = ApprovalTarget {
            node_id: card.node_id,
            profile: card.profile.clone(),
            run_id: card.run_id.clone()?,
            session_id: card.session_id.clone()?,
            turn_id: card.turn_id.clone()?,
            request_id: p.id.clone(),
        };
        if !target.valid() {
            return None;
        }
        let actionable = run.message.is_none()
            && run.interrupt.is_none()
            && !run.control.is_cancelled()
            && self
                .native_dialog_keys(card.node_id, &p.tool, &p.input)
                .is_some();
        if !actionable {
            // Use the existing native_terminal notice until the original menu is verified.
            // A Bash payload with zero choices is not a valid mobile approval snapshot.
            return None;
        }
        Some(ApprovalSnapshot {
            target,
            card_title: card.title.clone(),
            tool: p.tool.clone(),
            details: p.input.clone(),
            decisions: vec![ApprovalDecision::Allow, ApprovalDecision::Deny],
        })
    }

    pub(super) fn native_dialog_keys(
        &self,
        node: NodeId,
        tool: &str,
        input: &Value,
    ) -> Option<(terminal::Key, terminal::Key)> {
        let crate::NodeView::Term {
            snapshot, status, ..
        } = self.node_view(node)?;
        if status != TerminalStatus::Running {
            return None;
        }
        let lines: Vec<String> = snapshot
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.c)
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .filter(|s| !s.is_empty())
            .collect();
        let CanvasNodeKind::Agent(spec) = &self.document.node(node)?.kind else {
            return None;
        };
        if spec.profile == AgentProfile::Claude && tool == "Write" {
            return write_dialog_keys(&lines, input, self.workspace_root()?)
                .map(|(allow, deny)| (terminal::Key::Char(allow), terminal::Key::Char(deny)));
        }
        if tool != "Bash" {
            return None;
        }
        let command = input["command"].as_str()?;
        let description = input["description"].as_str()?;
        match spec.profile {
            AgentProfile::Claude => dialog_keys(&lines, command, description)
                .map(|(allow, deny)| (terminal::Key::Char(allow), terminal::Key::Char(deny))),
            AgentProfile::Codex => codex_dialog_keys(&lines, command, description),
            _ => None,
        }
    }
}

fn permission_input_matches(event: &AgentHookEvent, original: &Value) -> bool {
    let requested = &event.raw_json["tool_input"];
    if event.profile == norma::protocol::AgentProfile::Claude {
        return original == requested;
    }
    // Codex 0.153.4 adds only description at PermissionRequest; unknown fields are not discarded.
    original
        .as_object()
        .is_some_and(|o| o.len() == 1 && o.get("command").is_some_and(Value::is_string))
        && requested.as_object().is_some_and(|o| {
            o.len() == 2
                && o.get("description").is_some_and(Value::is_string)
                && o.get("command") == original.get("command")
        })
}

// ponytail: observed Codex 0.153.4 local Bash menu only; unknown layout/tool remains desktop-only.
fn codex_dialog_keys(
    lines: &[String],
    command: &str,
    description: &str,
) -> Option<(terminal::Key, terminal::Key)> {
    if command.is_empty()
        || command.len() > 2048
        || command.chars().any(char::is_control)
        || description.is_empty()
        || description.len() > 2048
        || description.chars().any(char::is_control)
    {
        return None;
    }
    let header = lines
        .iter()
        .rposition(|s| s == "Would you like to run the following command?")?;
    if lines
        .iter()
        .filter(|s| s.as_str() == "Would you like to run the following command?")
        .count()
        != 1
    {
        return None;
    }
    let tail = &lines[header + 1..];
    let first = tail
        .iter()
        .position(|s| s.trim_start_matches('›').trim() == "1. Yes, proceed (y)")?;
    let body = tail[..first].join(" ");
    if body != format!("Environment: local Reason: {description} $ {command}") {
        return None;
    }
    let options: Vec<&str> = tail[first..]
        .iter()
        .map(|s| s.trim_start_matches('›').trim())
        .collect();
    let (footer, options) = options.split_last()?;
    if *footer != "Press enter to confirm or esc to cancel" {
        return None;
    }
    match options {
        [_, "2. No, and tell Codex what to do differently (esc)"] => {}
        [_, middle, rest @ ..]
            if (1..=4).contains(&rest.len())
                && middle
                    .starts_with("2. Yes, and don't ask again for commands that start with `")
                && rest.last() == Some(&"3. No, and tell Codex what to do differently (esc)")
                && rest[..rest.len() - 1].iter().all(|s| {
                    !s.split_once(". ")
                        .is_some_and(|(p, _)| p.chars().all(|c| c.is_ascii_digit()))
                }) => {}
        _ => return None,
    }
    Some((terminal::Key::Char('y'), terminal::Key::Escape))
}

// ponytail: Claude 2.1.202 native Bash dialog only. Unknown layouts/tools fail closed;
// replace this small contract with a Provider request-response API when native CLI exposes one.
fn dialog_keys(lines: &[String], command: &str, description: &str) -> Option<(char, char)> {
    if command.is_empty() || command.len() > 2048 || command.chars().any(char::is_control) {
        return None;
    }
    let header = lines.iter().rposition(|line| line == "Bash command")?;
    let prompt = lines
        .iter()
        .rposition(|line| line == "Do you want to proceed?")?;
    if header >= prompt
        || lines
            .iter()
            .filter(|line| line.as_str() == "Do you want to proceed?")
            .count()
            != 1
    {
        return None;
    }
    // Preserve shell token/quoted whitespace. Unknown wrapping fails closed instead of
    // erasing spaces and accepting a different command that happens to look similar.
    let body = lines[header + 1..prompt].join(" ");
    let expected = format!("{command} {description}");
    if body != expected
        && body.strip_suffix(" This command requires approval") != Some(expected.as_str())
    {
        return None;
    }
    let tail = &lines[prompt + 1..];
    claude_choices(tail, "Esc to cancel · Tab to amend · ctrl+e to explain")
}

fn claude_choices(tail: &[String], expected_footer: &str) -> Option<(char, char)> {
    let footer = tail.iter().position(|line| line == expected_footer)?;
    // Three numbered choices plus up to three wrapped lines for the middle choice.
    if !(2..=6).contains(&footer) || footer + 1 != tail.len() {
        return None;
    }
    let options: Vec<&str> = tail[..footer]
        .iter()
        .map(|s| s.trim_start_matches('❯').trim())
        .collect();
    if options[0] != "1. Yes" {
        return None;
    }
    match options.as_slice() {
        [_, "2. No"] => Some(('1', '2')),
        [_, middle, rest @ ..]
            if middle.starts_with("2. Yes, ")
                && rest.last() == Some(&"3. No")
                && rest[..rest.len() - 1].iter().all(|line| {
                    !line
                        .split_once(". ")
                        .is_some_and(|(prefix, _)| prefix.chars().all(|c| c.is_ascii_digit()))
                }) =>
        {
            Some(('1', '3'))
        }
        _ => None,
    }
}

// ponytail: accept only the complete native Create file preview; clipped or wrapped diffs stay on desktop.
pub(super) fn write_dialog_keys(
    lines: &[String],
    input: &Value,
    cwd: &Path,
) -> Option<(char, char)> {
    let fields = input.as_object()?;
    if fields.len() != 2 {
        return None;
    }
    let path = input["file_path"].as_str()?;
    let content = input["content"].as_str()?;
    if !Path::new(path).is_absolute()
        || path.chars().any(char::is_control)
        || content.chars().any(|c| c.is_control() && c != '\n')
    {
        return None;
    }
    let header = lines.iter().rposition(|s| s == "Create file")?;
    if lines.iter().filter(|s| s.as_str() == "Create file").count() != 1 {
        return None;
    }
    let displayed = Path::new(lines.get(header + 1)?);
    let joined = if displayed.is_absolute() {
        displayed.to_path_buf()
    } else {
        cwd.join(displayed)
    };
    let mut resolved = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => {
                if !resolved.pop() {
                    return None;
                }
            }
            std::path::Component::CurDir => {}
            part => resolved.push(part.as_os_str()),
        }
    }
    if resolved != Path::new(path) {
        return None;
    }
    let separator = |line: &str| !line.is_empty() && line.chars().all(|c| c == '╌');
    if !separator(lines.get(header + 2)?) {
        return None;
    }
    let expected: Vec<_> = content
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{} {}", i + 1, line).trim_end().to_owned())
        .collect();
    let start = header + 3;
    if lines.get(start..start + expected.len())? != expected {
        return None;
    }
    let end = start + expected.len();
    if !separator(lines.get(end)?) {
        return None;
    }
    let name = Path::new(path).file_name()?.to_str()?;
    if lines.get(end + 1)? != &format!("Do you want to create {name}?") {
        return None;
    }
    claude_choices(lines.get(end + 2..)?, "Esc to cancel · Tab to amend")
}
