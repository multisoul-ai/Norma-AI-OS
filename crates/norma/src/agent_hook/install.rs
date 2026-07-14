use super::cursor::{cursor_hooks_installed_at, install_cursor_hooks, uninstall_cursor_hooks};
use super::opencode::{install_opencode_plugin, uninstall_opencode_plugin};
use super::{
    build_hook_command, hook_events_for_source, hook_source_arg, AgentHookError, HookInstallPaths,
    HookSource, HookTarget, AGENT_HOOK_BINARY_NAME, OPENCODE_PLUGIN_MARKER,
};
use super::{default_install_paths, resolve_agent_hook_command_path};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub fn install_agent_hooks(target: HookTarget) -> Result<PathBuf, AgentHookError> {
    if target == HookTarget::OpenCode {
        let paths = default_install_paths();
        install_hook_config(target, Path::new(AGENT_HOOK_BINARY_NAME), &paths)?;
        return Ok(paths.opencode_plugin);
    }
    let command_path = resolve_agent_hook_command_path().ok_or_else(|| {
        AgentHookError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{AGENT_HOOK_BINARY_NAME} binary not found next to Norma OS or on PATH"),
        ))
    })?;
    install_hook_config(target, &command_path, &default_install_paths())?;
    Ok(command_path)
}

pub fn uninstall_agent_hooks(target: HookTarget) -> Result<(), AgentHookError> {
    uninstall_hook_config(target, &default_install_paths())
}

pub fn agent_hooks_installed(target: HookTarget, paths: &HookInstallPaths) -> bool {
    match target {
        HookTarget::Claude => agent_hooks_installed_for_source(HookSource::Claude, paths),
        HookTarget::Codex => agent_hooks_installed_for_source(HookSource::Codex, paths),
        HookTarget::Cursor => agent_hooks_installed_for_source(HookSource::Cursor, paths),
        HookTarget::OpenCode => agent_hooks_installed_for_source(HookSource::OpenCode, paths),
        HookTarget::All => {
            agent_hooks_installed_for_source(HookSource::Claude, paths)
                && agent_hooks_installed_for_source(HookSource::Codex, paths)
                && agent_hooks_installed_for_source(HookSource::Cursor, paths)
                && agent_hooks_installed_for_source(HookSource::OpenCode, paths)
        }
    }
}

pub fn agent_hooks_installed_for_source(source: HookSource, paths: &HookInstallPaths) -> bool {
    agent_hooks_installed_at(config_path_for_source(source, paths), source)
}

pub fn agent_hooks_installed_at(path: &Path, source: HookSource) -> bool {
    if source == HookSource::OpenCode {
        return fs::read_to_string(path)
            .is_ok_and(|source| source.contains(OPENCODE_PLUGIN_MARKER));
    }
    if source == HookSource::Cursor {
        return cursor_hooks_installed_at(path);
    }
    let Ok(root) = read_json_or_empty(path) else {
        return false;
    };
    hook_events_for_source(source)
        .iter()
        .any(|event_name| event_has_multisoul_hook(&root, event_name, source))
}

pub fn uninstall_hook_config(
    target: HookTarget,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    if matches!(target, HookTarget::Claude | HookTarget::All) {
        uninstall_for_source(paths.claude_settings.as_path(), HookSource::Claude)?;
    }
    if matches!(target, HookTarget::Codex | HookTarget::All) {
        uninstall_for_source(paths.codex_hooks.as_path(), HookSource::Codex)?;
    }
    if matches!(target, HookTarget::Cursor | HookTarget::All) {
        uninstall_cursor_hooks(paths.cursor_hooks.as_path())?;
    }
    if matches!(target, HookTarget::OpenCode | HookTarget::All) {
        uninstall_opencode_plugin(paths.opencode_plugin.as_path())?;
    }
    Ok(())
}

pub fn install_hook_config(
    target: HookTarget,
    command_path: &Path,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    if matches!(target, HookTarget::Claude | HookTarget::All) {
        install_for_source(
            paths.claude_settings.as_path(),
            command_path,
            HookSource::Claude,
        )?;
    }
    if matches!(target, HookTarget::Codex | HookTarget::All) {
        install_for_source(paths.codex_hooks.as_path(), command_path, HookSource::Codex)?;
    }
    if matches!(target, HookTarget::Cursor | HookTarget::All) {
        install_cursor_hooks(paths.cursor_hooks.as_path(), command_path)?;
    }
    if matches!(target, HookTarget::OpenCode | HookTarget::All) {
        install_opencode_plugin(paths.opencode_plugin.as_path())?;
    }
    Ok(())
}

fn install_for_source(
    path: &Path,
    command_path: &Path,
    source: HookSource,
) -> Result<(), AgentHookError> {
    let mut root = read_json_or_empty(path)?;
    let command = build_hook_command(command_path, source);
    for event_name in hook_events_for_source(source) {
        add_command_hook(&mut root, event_name, &command);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn uninstall_for_source(path: &Path, source: HookSource) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_or_empty(path)?;
    let mut changed = false;
    for event_name in hook_events_for_source(source) {
        if remove_multisoul_hooks_for_event(&mut root, event_name, source) {
            changed = true;
        }
    }
    if changed {
        cleanup_empty_hooks(&mut root);
        fs::write(path, serde_json::to_string_pretty(&root)?)?;
    }
    Ok(())
}

fn config_path_for_source(source: HookSource, paths: &HookInstallPaths) -> &Path {
    match source {
        HookSource::Claude => paths.claude_settings.as_path(),
        HookSource::Codex => paths.codex_hooks.as_path(),
        HookSource::Cursor => paths.cursor_hooks.as_path(),
        HookSource::OpenCode => paths.opencode_plugin.as_path(),
    }
}

fn event_has_multisoul_hook(root: &Value, event_name: &str, source: HookSource) -> bool {
    root.get("hooks")
        .and_then(|hooks| hooks.get(event_name))
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups
                .iter()
                .any(|group| group_has_multisoul_hook(group, source))
        })
}

fn group_has_multisoul_hook(group: &Value, source: HookSource) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|handlers| {
            handlers.iter().any(|handler| {
                handler
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| is_multisoul_hook_command(command, source))
            })
        })
}

pub(super) fn is_multisoul_hook_command(command: &str, source: HookSource) -> bool {
    command.contains(AGENT_HOOK_BINARY_NAME)
        && command.contains(&format!("forward --source {}", hook_source_arg(source)))
}

fn remove_multisoul_hooks_for_event(
    root: &mut Value,
    event_name: &str,
    source: HookSource,
) -> bool {
    ensure_object(root);
    let hooks = match root.get_mut("hooks") {
        Some(value) => value,
        None => return false,
    };
    ensure_object(hooks);
    let event_hooks = match hooks.get_mut(event_name) {
        Some(value) => value,
        None => return false,
    };
    if !event_hooks.is_array() {
        return false;
    }
    let groups = event_hooks.as_array_mut().expect("array ensured");
    let mut changed = false;
    for group in groups.iter_mut() {
        if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            let before = handlers.len();
            handlers.retain(|handler| {
                !handler
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| is_multisoul_hook_command(command, source))
            });
            if handlers.len() != before {
                changed = true;
            }
        }
    }
    let before_groups = groups.len();
    groups.retain(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|handlers| !handlers.is_empty())
    });
    changed || groups.len() != before_groups
}

fn cleanup_empty_hooks(root: &mut Value) {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    hooks.retain(|_, event_hooks| {
        event_hooks
            .as_array()
            .is_some_and(|groups| !groups.is_empty())
    });
    if hooks.is_empty() {
        if let Some(obj) = root.as_object_mut() {
            obj.remove("hooks");
        }
    }
}

fn add_command_hook(root: &mut Value, event_name: &str, command: &str) {
    ensure_object(root);
    let hooks = root
        .as_object_mut()
        .expect("object ensured")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    ensure_object(hooks);
    let event_hooks = hooks
        .as_object_mut()
        .expect("object ensured")
        .entry(event_name)
        .or_insert_with(|| json!([]));
    if command_exists(event_hooks, command) {
        return;
    }
    ensure_array(event_hooks);
    event_hooks
        .as_array_mut()
        .expect("array ensured")
        .push(json!({
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": 5
            }]
        }));
}

fn command_exists(event_hooks: &Value, command: &str) -> bool {
    event_hooks.as_array().is_some_and(|groups| {
        groups.iter().any(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|handlers| {
                    handlers.iter().any(|handler| {
                        handler.get("command").and_then(Value::as_str) == Some(command)
                    })
                })
        })
    })
}

pub(super) fn read_json_or_empty(path: &Path) -> Result<Value, AgentHookError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(super) fn ensure_object(value: &mut Value) {
    if !value.is_object() {
        *value = json!({});
    }
}

pub(super) fn ensure_array(value: &mut Value) {
    if !value.is_array() {
        *value = json!([]);
    }
}

pub(super) fn opencode_plugin_source() -> &'static str {
    r#"// multisoul-opencode-plugin-marker
// Auto-generated by Norma OS. Forwards OpenCode Agent lifecycle events to the local Norma app.

function stringValue(value) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function toolName(value) {
  const raw = stringValue(value) || "tool";
  return raw.charAt(0).toUpperCase() + raw.slice(1);
}

function textFromParts(parts) {
  if (!Array.isArray(parts)) return undefined;
  for (const part of parts) {
    const text = stringValue(part?.text) || stringValue(part?.content);
    if (text) return text;
  }
  return undefined;
}

function textFromOutput(output) {
  return (
    textFromParts(output?.parts) ||
    stringValue(output?.message?.text) ||
    stringValue(output?.message?.content) ||
    stringValue(output?.message)
  );
}

async function postHook(providerEvent, sessionID, extra = {}) {
  const nodeID = Number(process.env.MULTISOUL_NODE_ID);
  const endpoint = process.env.MULTISOUL_HOOK_ENDPOINT;
  const token = process.env.MULTISOUL_HOOK_TOKEN;
  if (!Number.isFinite(nodeID) || !endpoint || !token) return;

  const rawJson = {
    hook_event_name: providerEvent,
    session_id: sessionID,
    ...extra.raw_json,
  };
  if (extra.tool_name) rawJson.tool_name = extra.tool_name;
  if (extra.message) rawJson.message = extra.message;
  if (extra.prompt) rawJson.prompt = extra.prompt;

  const body = {
    node_id: nodeID,
    profile: "OpenCode",
    provider_event: providerEvent,
    session_id: stringValue(sessionID),
    turn_id: stringValue(extra.turn_id),
    tool_name: stringValue(extra.tool_name),
    message: stringValue(extra.message),
    raw_json: rawJson,
  };

  try {
    await fetch(endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Authorization": `Bearer ${token}`,
      },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(300),
    });
  } catch {
    // Fail open: OpenCode must never block on Norma OS being unavailable.
  }
}

export default async () => {
  const msgRoles = new Map();
  const sentPrompts = new Set();

  async function sendPrompt(sessionID, prompt, key) {
    if (!prompt) return;
    const dedupeKey = key || `${sessionID}:${prompt}`;
    if (sentPrompts.has(dedupeKey)) return;
    sentPrompts.add(dedupeKey);
    if (sentPrompts.size > 200) sentPrompts.delete(sentPrompts.keys().next().value);
    await postHook("UserPromptSubmit", sessionID, {
      prompt,
      raw_json: { prompt },
    });
  }

  function rememberMessage(info) {
    if (!info?.id || !info?.sessionID) return;
    msgRoles.set(info.id, { role: info.role, sessionID: info.sessionID });
    if (msgRoles.size > 200) msgRoles.delete(msgRoles.keys().next().value);
  }

  return {
    "chat.message": async (input, output) => {
      await sendPrompt(input.sessionID, textFromOutput(output), input.messageID);
    },

    "tool.execute.before": async (input, output) => {
      const name = toolName(input.tool);
      await postHook("PreToolUse", input.sessionID, {
        tool_name: name,
        raw_json: { tool_name: name, tool_input: output?.args },
      });
    },

    "tool.execute.after": async (input) => {
      const name = toolName(input.tool);
      await postHook("PostToolUse", input.sessionID, {
        tool_name: name,
        raw_json: { tool_name: name },
      });
    },

    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties || {};

      if (type === "session.created" && properties.info?.id) {
        await postHook("SessionStart", properties.info.id, {
          raw_json: { cwd: properties.info.directory },
        });
        return;
      }

      if (type === "session.deleted" && properties.info?.id) {
        await postHook("SessionEnd", properties.info.id);
        return;
      }

      if (type === "session.updated" && properties.info?.id && properties.info?.time?.archived) {
        await postHook("SessionEnd", properties.info.id);
        return;
      }

      if (type === "session.status" && properties.sessionID && properties.status?.type === "idle") {
        await postHook("Stop", properties.sessionID);
        return;
      }

      if (type === "session.error" && properties.sessionID) {
        await postHook("StopFailure", properties.sessionID, {
          message: stringValue(properties.error?.message) || "OpenCode session error",
        });
        return;
      }

      if (type === "message.updated") {
        rememberMessage(properties.info);
        return;
      }

      if (type === "message.part.updated" && properties.part?.messageID) {
        const meta = msgRoles.get(properties.part.messageID);
        if (!meta) return;
        if (meta.role === "user" && properties.part.type === "text") {
          await sendPrompt(meta.sessionID, stringValue(properties.part.text), properties.part.messageID);
        }
        return;
      }

      if (type === "message.part.updated" && properties.part?.type === "tool" && properties.part?.sessionID) {
        const status = properties.part.state?.status;
        const name = toolName(properties.part.tool);
        if (status === "running" || status === "pending") {
          await postHook("PreToolUse", properties.part.sessionID, {
            tool_name: name,
            raw_json: { tool_name: name, tool_input: properties.part.state?.input },
          });
        } else if (status === "completed") {
          await postHook("PostToolUse", properties.part.sessionID, {
            tool_name: name,
          });
        } else if (status === "error") {
          await postHook("PostToolUseFailure", properties.part.sessionID, {
            tool_name: name,
            message: "OpenCode tool failed",
          });
        }
      }
    },
  };
};
"#
}
