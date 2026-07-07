use crate::app::wallpaper_resolvers::bundle_resources_dir;
use card_task::{compile_voice_plan_json_for_card_task, CardTaskPlannerError};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use voice::{CompiledVoicePlan, VoicePlanningRequest};

#[derive(Clone)]
pub(super) struct OpenAiCardTaskPlanningJob {
    pub(super) request: VoicePlanningRequest,
    pub(super) api_key: String,
    pub(super) model: String,
}

pub(super) fn run_openai_card_task_planning_job(
    job: OpenAiCardTaskPlanningJob,
) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
    let plan_json = request_openai_plan_json(&job)?;
    compile_voice_plan_json_for_card_task(&job.request, &plan_json)
}

fn request_openai_plan_json(
    job: &OpenAiCardTaskPlanningJob,
) -> Result<String, CardTaskPlannerError> {
    let helper_input = serde_json::json!({
        "model": job.model,
        "request": job.request,
    });
    let helper_input =
        serde_json::to_vec(&helper_input).map_err(|error| CardTaskPlannerError::Provider {
            message: format!("failed to encode card planner request: {error}"),
        })?;

    let mut child = Command::new(node_executable())
        .arg(card_planner_helper_path())
        .env("OPENAI_API_KEY", &job.api_key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| CardTaskPlannerError::Provider {
            message: format!("failed to start card planner helper: {error}"),
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CardTaskPlannerError::Provider {
            message: "card planner helper stdin unavailable".to_string(),
        })?;
    stdin
        .write_all(&helper_input)
        .map_err(|error| CardTaskPlannerError::Provider {
            message: format!("failed to write card planner request: {error}"),
        })?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|error| CardTaskPlannerError::Provider {
            message: format!("card planner helper failed: {error}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CardTaskPlannerError::Provider {
            message: sanitize_planner_error(&stderr, &job.api_key),
        });
    }

    let stdout =
        String::from_utf8(output.stdout).map_err(|error| CardTaskPlannerError::Provider {
            message: format!("card planner helper returned non-utf8 output: {error}"),
        })?;
    let plan_json = stdout.trim();
    if plan_json.is_empty() {
        return Err(CardTaskPlannerError::Provider {
            message: "card planner helper returned empty output".to_string(),
        });
    }
    Ok(plan_json.to_string())
}

fn node_executable() -> PathBuf {
    std::env::var_os("NODE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("node"))
}

fn card_planner_helper_path() -> PathBuf {
    std::env::var_os("NORMA_OPENAI_CARD_PLANNER_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| card_planner_helper_path_from_sources(bundle_resources_dir()))
}

fn card_planner_helper_path_from_sources(bundle_resources: Option<PathBuf>) -> PathBuf {
    if let Some(resources) = bundle_resources {
        let bundled = resources.join("openai-card-planner.mjs");
        if bundled.is_file() {
            return bundled;
        }
    }
    PathBuf::from("sidecars/openai-card-planner/openai-card-planner.mjs")
}

fn sanitize_planner_error(message: &str, api_key: &str) -> String {
    let sanitized = message.replace(api_key, "[redacted]");
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        "card planner helper exited with an error".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{card_planner_helper_path_from_sources, OpenAiCardTaskPlanningJob};
    use std::fs;
    use std::path::PathBuf;
    use voice::VoicePlanningRequest;

    /// CTP-1：packaged app 的 card planner helper 应优先从 bundle Resources 解析。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   temp_root = std::env::temp_dir()/norma-card-planner-helper-test-<pid>
    ///   resources = temp_root/Resources
    ///   helper    = resources/openai-card-planner.mjs
    ///   fallback  = sidecars/openai-card-planner/openai-card-planner.mjs
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在临时 Resources 目录写入 openai-card-planner.mjs
    ///   2. 调用 card_planner_helper_path_from_sources(Some(resources))
    ///   3. 删除 helper 后再次调用同一 resolver
    ///
    /// 预期结果：
    ///   - 正断言：bundle helper 存在时返回 Resources/openai-card-planner.mjs
    ///   - 负断言：bundle helper 缺失时不得返回不存在的 Resources 路径，应回退 repo 相对路径
    #[test]
    fn test_card_planner_helper_path_prefers_packaged_resource_when_present() {
        let temp_root = std::env::temp_dir().join(format!(
            "norma-card-planner-helper-test-{}",
            std::process::id()
        ));
        let resources = temp_root.join("Resources");
        fs::create_dir_all(&resources)
            .expect("测试应能创建临时 Resources 目录用于模拟 .app bundle");
        let helper = resources.join("openai-card-planner.mjs");
        fs::write(&helper, "console.log('{}')\n").expect("测试应能写入临时 planner helper 文件");

        let packaged = card_planner_helper_path_from_sources(Some(resources.clone()));

        assert_eq!(
            packaged, helper,
            "packaged helper 存在时 resolver 必须返回 Contents/Resources/openai-card-planner.mjs"
        );

        fs::remove_file(&helper).expect("测试应能删除临时 planner helper 文件");
        let fallback = card_planner_helper_path_from_sources(Some(resources));

        assert_eq!(
            fallback,
            PathBuf::from("sidecars/openai-card-planner/openai-card-planner.mjs"),
            "packaged helper 缺失时 resolver 不应返回不存在的 Resources 路径"
        );

        let _ = fs::remove_dir_all(temp_root);
    }

    /// CTP-2：OpenAI planner job 不应实现 Debug，避免 api_key 被未来日志误打印。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   source = 当前源码文本
    ///   secret field = api_key: String
    ///   forbidden derive = #[derive(Clone, Debug)]
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 用 include_str! 读取当前文件源码
    ///   2. 构造一个 OpenAiCardTaskPlanningJob，证明 Clone 仍可用
    ///   3. 检查源码中不再存在 Clone + Debug 派生组合
    ///
    /// 预期结果：
    ///   - 正断言：job 仍能 clone，后台 spawn 所需所有权语义不变
    ///   - 负断言：secret-bearing job 不应派生 Debug
    #[test]
    fn test_openai_card_task_planning_job_does_not_derive_debug() {
        let source = include_str!("card_task_planning.rs");
        let job = OpenAiCardTaskPlanningJob {
            request: VoicePlanningRequest {
                transcript: "列出卡片".to_string(),
                cards: Vec::new(),
                focused_node_id: None,
                selected_node_ids: Vec::new(),
                max_steps: 8,
            },
            api_key: "sk-secret".to_string(),
            model: "gpt-card-planner".to_string(),
        };

        let cloned = job.clone();

        assert_eq!(
            cloned.model, "gpt-card-planner",
            "移除 Debug 派生后仍必须保留 Clone 行为，供后台规划任务移动使用"
        );
        assert!(
            !source
                .contains("#[derive(Clone, Debug)]\npub(super) struct OpenAiCardTaskPlanningJob"),
            "OpenAiCardTaskPlanningJob 包含 api_key，不能派生 Debug"
        );
    }
}
