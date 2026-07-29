use super::MultisoulApp;
use canvas::NodeId;
use gpui::{Action, Context, Window};
use runtime::EgoTaskCommand;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EGO_RESULT_PREFIX: &str = "NORMA_EGO_RESULT=";
const EGO_HELPER_TIMEOUT: Duration = Duration::from_secs(10);
const EGO_HELPER_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Default)]
pub(crate) struct EgoController {
    creating: HashSet<NodeId>,
    #[cfg(feature = "e2e-test-support")]
    next_fake_task_space_result: Option<Result<EgoTaskSpaceResult, String>>,
}

impl EgoController {
    fn begin_create(&mut self, node_id: NodeId) -> bool {
        self.creating.insert(node_id)
    }

    fn finish_create(&mut self, node_id: NodeId) {
        self.creating.remove(&node_id);
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn set_next_fake_task_space_result(
        &mut self,
        task_space_id: String,
        task_space_name: String,
    ) {
        self.next_fake_task_space_result = Some(Ok(EgoTaskSpaceResult {
            task_space_id,
            task_space_name,
        }));
    }

    #[cfg(feature = "e2e-test-support")]
    fn take_next_fake_task_space_result(&mut self) -> Option<Result<EgoTaskSpaceResult, String>> {
        self.next_fake_task_space_result.take()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct EgoTaskSpaceResult {
    task_space_id: String,
    task_space_name: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct EgoTaskSpaceOwnershipResult {
    found: bool,
    ownership: Option<String>,
}

enum EgoTaskSpaceWorkResult {
    Created(EgoTaskSpaceResult),
    InitialCreateFailed(String),
    ReplacementCloseFailed(String),
    ReplacementCreateFailed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EgoCardCommandKind {
    Open,
    SetMaster,
    Takeover,
    ReturnToAgent,
}

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = multisoul, no_json)]
pub(crate) struct EgoCardCommand {
    pub(crate) node_id: NodeId,
    pub(crate) command: EgoCardCommandKind,
}

impl MultisoulApp {
    pub(crate) fn ensure_ego_task_space(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        let has_task_space = self.ego_task_space_id_for(node_id).is_some();
        if has_task_space || !self.ego.begin_create(node_id) {
            return;
        }

        let task_space_name = self
            .ego_task_space_name_for(node_id)
            .unwrap_or_else(|| format!("Norma · Ego · {node_id}"));
        self.start_ego_task_space_work(node_id, None, task_space_name, cx);
    }

    pub(crate) fn replace_ego_task_space(
        &mut self,
        node_id: NodeId,
        previous_task_space_id: Option<String>,
        task_space_name: String,
        cx: &mut Context<Self>,
    ) {
        if !self.ego.begin_create(node_id) {
            return;
        }
        self.start_ego_task_space_work(node_id, previous_task_space_id, task_space_name, cx);
    }

    pub(crate) fn ego_task_space_id_for(&self, node_id: NodeId) -> Option<String> {
        self.ego_spec_for(node_id)
            .and_then(|spec| spec.task_space_id.clone())
    }

    fn ego_task_space_name_for(&self, node_id: NodeId) -> Option<String> {
        self.ego_spec_for(node_id)
            .and_then(|spec| spec.task_space_name.clone())
    }

    fn ego_spec_for(&self, node_id: NodeId) -> Option<&canvas::EgoLiteSpec> {
        let node = self.runtime.document.node(node_id)?;
        let canvas::CanvasNodeKind::Browser(canvas::BrowserSpec {
            surface: canvas::BrowserSurfaceKind::EgoLite(spec),
            ..
        }) = &node.kind
        else {
            return None;
        };
        Some(spec)
    }

    fn start_ego_task_space_work(
        &mut self,
        node_id: NodeId,
        previous_task_space_id: Option<String>,
        task_space_name: String,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "e2e-test-support")]
        let fake_result = self.ego.take_next_fake_task_space_result();
        let work = cx.background_executor().spawn(async move {
            #[cfg(feature = "e2e-test-support")]
            if let Some(result) = fake_result {
                return match result {
                    Ok(task_space) => EgoTaskSpaceWorkResult::Created(task_space),
                    Err(message) => EgoTaskSpaceWorkResult::InitialCreateFailed(message),
                };
            }
            if let Some(previous_task_space_id) = previous_task_space_id {
                return match close_task_space(&previous_task_space_id) {
                    Ok(()) => match create_task_space(&task_space_name) {
                        Ok(task_space) => EgoTaskSpaceWorkResult::Created(task_space),
                        Err(message) => EgoTaskSpaceWorkResult::ReplacementCreateFailed(message),
                    },
                    Err(message) => EgoTaskSpaceWorkResult::ReplacementCloseFailed(message),
                };
            }
            match create_task_space(&task_space_name) {
                Ok(task_space) => EgoTaskSpaceWorkResult::Created(task_space),
                Err(message) => EgoTaskSpaceWorkResult::InitialCreateFailed(message),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.ego.finish_create(node_id);
                let update = match result {
                    EgoTaskSpaceWorkResult::Created(task_space) => {
                        app.runtime.apply_ego_task_command(
                            node_id,
                            EgoTaskCommand::TaskSpaceReady {
                                task_space_id: task_space.task_space_id,
                                task_space_name: task_space.task_space_name,
                            },
                        )
                    }
                    EgoTaskSpaceWorkResult::InitialCreateFailed(message) => app
                        .runtime
                        .apply_ego_task_command(node_id, EgoTaskCommand::Disconnect { message }),
                    EgoTaskSpaceWorkResult::ReplacementCloseFailed(message) => app
                        .runtime
                        .fail_ego_task_space_replacement(node_id, true, message),
                    EgoTaskSpaceWorkResult::ReplacementCreateFailed(message) => app
                        .runtime
                        .fail_ego_task_space_replacement(node_id, false, message),
                };
                if let Err(error) = update {
                    tracing::warn!(node_id, %error, "failed to apply Ego helper result");
                } else {
                    app.mark_canvas_dirty();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn open_ego_lite(&mut self) {
        std::thread::Builder::new()
            .name("open_ego_lite".to_string())
            .spawn(|| {
                if let Err(error) = Command::new("open").args(["-a", "ego lite"]).spawn() {
                    tracing::warn!(%error, "failed to open Ego Lite");
                }
            })
            .ok();
    }

    pub(crate) fn on_ego_card_command(
        &mut self,
        action: &EgoCardCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.command {
            EgoCardCommandKind::Open => self.open_ego_lite(),
            EgoCardCommandKind::SetMaster => {
                self.runtime.selection.select_only(action.node_id);
                self.on_toggle_master_layout(&super::ToggleMasterLayout, window, cx);
            }
            EgoCardCommandKind::Takeover => {
                if self.master_layout.master_id() != Some(action.node_id) {
                    self.workspace_state.error =
                        Some("请先将 Ego 卡片设为 Master，再接管浏览器。".to_string());
                    cx.notify();
                    return;
                }
                match self
                    .runtime
                    .apply_ego_task_command(action.node_id, EgoTaskCommand::UserTakeover)
                {
                    Ok(_) => {
                        self.open_ego_lite();
                        self.mark_canvas_dirty();
                    }
                    Err(error) => self.workspace_state.error = Some(error.to_string()),
                }
            }
            EgoCardCommandKind::ReturnToAgent => {
                let bound_agent = self
                    .runtime
                    .ego_task_state(action.node_id)
                    .and_then(|state| state.bound_agent_node_id);
                match self
                    .runtime
                    .apply_ego_task_command(action.node_id, EgoTaskCommand::UserReturn)
                {
                    Ok(_) => {
                        self.mark_canvas_dirty();
                        if let Some(agent_node_id) = bound_agent {
                            self.notify_ego_returned_agent(action.node_id, agent_node_id);
                        }
                    }
                    Err(error) => self.workspace_state.error = Some(error.to_string()),
                }
            }
        }
        cx.notify();
    }

    fn notify_ego_returned_agent(&mut self, ego_node_id: NodeId, agent_node_id: NodeId) {
        let Some((definition_id, name)) =
            self.runtime.document.node(agent_node_id).and_then(|node| {
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                let definition_id = spec.definition_id?;
                let name = self
                    .agent_definitions
                    .definitions()
                    .iter()
                    .find(|definition| definition.id == definition_id)?
                    .name
                    .clone();
                Some((definition_id, name))
            })
        else {
            tracing::warn!(
                ego_node_id,
                agent_node_id,
                "cannot notify missing bound Ego Agent"
            );
            return;
        };
        let task_space_id = self
            .runtime
            .document
            .node(ego_node_id)
            .and_then(|node| {
                let canvas::CanvasNodeKind::Browser(spec) = &node.kind else {
                    return None;
                };
                let canvas::BrowserSurfaceKind::EgoLite(ego) = &spec.surface else {
                    return None;
                };
                ego.task_space_id.clone()
            })
            .unwrap_or_else(|| "unknown".to_string());
        let session_id = self
            .runtime
            .agent_session_id(agent_node_id)
            .unwrap_or_default();
        let message = format!(
            "用户已交还 Ego Task Space {task_space_id}。请先显式调用 takeOverTaskSpace，成功后再运行 `norma ego running {ego_node_id}`。"
        );
        if let Err(error) = self.enqueue_or_send_session_reply(
            definition_id,
            name,
            agent_node_id,
            session_id,
            message,
        ) {
            tracing::warn!(ego_node_id, agent_node_id, %error, "failed to notify returned Ego Agent");
        }
    }
}

fn create_task_space(name: &str) -> Result<EgoTaskSpaceResult, String> {
    let helper = ego_browser_helper();
    let encoded_name =
        serde_json::to_string(name).map_err(|error| format!("无法编码 Ego 空间名称：{error}"))?;
    let script = format!(
        r#"const name = {encoded_name};
const space = await useOrCreateTaskSpace(name);
cliLog("{EGO_RESULT_PREFIX}" + JSON.stringify({{
  task_space_id: String(space.id),
  task_space_name: String(space.name || name)
}}));
"#
    );
    run_helper(&helper, &script).and_then(|stdout| parse_task_space_result(&stdout))
}

fn close_task_space(task_space_id: &str) -> Result<(), String> {
    let helper = ego_browser_helper();
    ensure_agent_owned_task_space(&helper, task_space_id)?;
    let encoded_id = serde_json::to_string(task_space_id)
        .map_err(|error| format!("无法编码旧 Ego Task Space ID：{error}"))?;
    // `completeTaskSpace` 必须独占本次 helper 调用；所有权确认在前一轮已完成。
    let script = format!(
        r#"const result = await completeTaskSpace({encoded_id}, {{ keep: false }});
if (!result || result.done !== true) {{
  throw new Error("Task Space was not closed: " + JSON.stringify(result));
}}
cliLog("{EGO_RESULT_PREFIX}" + "closed");
"#
    );
    let stdout = run_helper(&helper, &script)?;
    stdout
        .lines()
        .any(|line| line == format!("{EGO_RESULT_PREFIX}closed"))
        .then_some(())
        .ok_or_else(|| "ego-browser 未确认关闭旧 Task Space".to_string())
}

fn ensure_agent_owned_task_space(helper: &PathBuf, task_space_id: &str) -> Result<(), String> {
    let encoded_id = serde_json::to_string(task_space_id)
        .map_err(|error| format!("无法编码 Ego Task Space ID：{error}"))?;
    let script = format!(
        r#"const targetId = {encoded_id};
const spaces = await listTaskSpaces();
const task = spaces.find((space) => String(space.id) === targetId);
cliLog("{EGO_RESULT_PREFIX}" + JSON.stringify({{
  found: Boolean(task),
  ownership: task ? String(task.ownership || "") : null
}}));
"#
    );
    let stdout = run_helper(helper, &script)?;
    let ownership = parse_task_space_ownership(&stdout)?;
    validate_agent_owned_task_space(task_space_id, &ownership)
}

fn validate_agent_owned_task_space(
    task_space_id: &str,
    ownership: &EgoTaskSpaceOwnershipResult,
) -> Result<(), String> {
    if !ownership.found {
        return Err(format!("未找到待替换的 Ego Task Space {task_space_id}"));
    }
    if ownership.ownership.as_deref() != Some("agent") {
        return Err(format!(
            "不会关闭 Task Space {task_space_id}：所有权为 {:?}；仅可替换 agent 所有的已完成空间",
            ownership.ownership,
        ));
    }
    Ok(())
}

fn ego_browser_helper() -> PathBuf {
    std::env::var_os("EGO_BROWSER_HELPER")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/bin/ego-browser"))
                .filter(|path| path.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("ego-browser"))
}

fn run_helper(helper: &PathBuf, script: &str) -> Result<String, String> {
    let mut child = Command::new(helper)
        .arg("nodejs")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动 ego-browser：{error}"))?;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("ego-browser stdin 不可用".to_string());
    };
    if let Err(error) = stdin.write_all(script.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("无法发送 Ego helper 脚本：{error}"));
    }
    drop(stdin);

    // timeout = 10_000ms；poll interval = 25ms；最多约 10_000 / 25 = 400 次检查。
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("无法等待 ego-browser：{error}"))?
        {
            break status;
        }
        if started.elapsed() >= EGO_HELPER_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("ego-browser 创建 Task Space 超时（10 秒）".to_string());
        }
        std::thread::sleep(EGO_HELPER_POLL_INTERVAL);
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .map_err(|error| format!("无法读取 ego-browser 输出：{error}"))?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .map_err(|error| format!("无法读取 ego-browser 错误：{error}"))?;
    }
    if status.success() {
        Ok(stdout)
    } else {
        Err(format!(
            "ego-browser 创建 Task Space 失败：{}",
            stderr.trim()
        ))
    }
}

fn parse_task_space_result(stdout: &str) -> Result<EgoTaskSpaceResult, String> {
    let payload = stdout
        .lines()
        .find_map(|line| line.strip_prefix(EGO_RESULT_PREFIX))
        .ok_or_else(|| "ego-browser 未返回可识别的 Task Space 结果".to_string())?;
    serde_json::from_str(payload).map_err(|error| format!("Ego Task Space 结果无效：{error}"))
}

fn parse_task_space_ownership(stdout: &str) -> Result<EgoTaskSpaceOwnershipResult, String> {
    let payload = stdout
        .lines()
        .find_map(|line| line.strip_prefix(EGO_RESULT_PREFIX))
        .ok_or_else(|| "ego-browser 未返回 Task Space 所有权结果".to_string())?;
    serde_json::from_str(payload).map_err(|error| format!("Ego Task Space 所有权结果无效：{error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/ego_controller_tests.rs"]
mod tests;
