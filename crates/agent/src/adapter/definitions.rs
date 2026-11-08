use super::ToolDefinition;
use serde_json::{json, Value};

pub fn norma_tool_definitions() -> Vec<ToolDefinition> {
    let mut tools = vec![
        definition(
            "task_list",
            "查询当前项目的 Agent 任务。无筛选条件时返回最近的未结束任务。",
            object_schema(
                json!({
                    "task_id": {"type": "integer", "minimum": 1},
                    "agent": {"type": "string", "minLength": 1},
                    "states": {
                        "type": "array",
                        "items": {"type": "string", "enum": ["queued", "running", "cancel_requested", "succeeded", "failed", "cancelled", "interrupted"]},
                        "uniqueItems": true
                    }
                }),
                &[],
            ),
        ),
        definition(
            "agent_create",
            "仅在用户明确要求新建某个内置类型的 Agent 时使用。若同时要求新 Agent 执行任务，把完整任务放入 task，由系统在创建后立即入队，不要再调用 agent_assign。若用户点名的 Agent 画布上已有卡片，不要调用本工具，应 agent_reply 直接对话。",
            object_schema(
                json!({
                    "profile": {"type": "string", "enum": ["codex", "claude", "opencode", "infcode", "cursor", "grok", "kimi", "zcode", "pi", "hermes"]},
                    "task": {"type": "string", "minLength": 1}
                }),
                &["profile"],
            ),
        ),
        definition(
            "agent_assign",
            "派任务。当前项目只填 agent。别的项目只填 workspace_id、area_id、node_id，且必须来自同一次 workspace_snapshot。不要两种地址一起填。已有卡片且用户只是对话时用 agent_reply。",
            object_schema(
                json!({
                    "task": {"type": "string", "minLength": 1},
                    "agent": {"type": "string", "minLength": 1},
                    "workspace_id": {"type": "string", "minLength": 1},
                    "area_id": {"type": "string", "minLength": 1},
                    "node_id": {"type": "integer", "minimum": 1}
                }),
                &["task"],
            ),
        ),
        definition(
            "task_cancel",
            "取消一个明确 task_id 的 AgentCard 任务。",
            object_schema(
                json!({"task_id": {"type": "integer", "minimum": 1}}),
                &["task_id"],
            ),
        ),
        definition(
            "agent_reply",
            "向画布上已有卡片的 Agent 发送会话消息；有卡即可对话，不要求先激活或新建，也不创建新任务。",
            object_schema(
                json!({
                    "agent": {"type": "string", "minLength": 1},
                    "message": {"type": "string", "minLength": 1}
                }),
                &["agent", "message"],
            ),
        ),
        definition(
            "reply_cancel",
            "撤回尚未提交的会话回复；可按 reply_id 或 Agent 名称取消。",
            object_schema(
                json!({
                    "reply_id": {"type": "integer", "minimum": 1},
                    "agent": {"type": "string", "minLength": 1}
                }),
                &[],
            ),
        ),
        definition(
            "terminal_create",
            "直接创建一张 Terminal 卡片。",
            object_schema(json!({}), &[]),
        ),
        definition(
            "card_delete",
            "仅在用户明确要求删除或关闭指定卡片时使用；目标必须且只能是稳定 node_id、用户所说的 1-based 画布位置，或 Agent 呼叫名/语音别名之一。",
            delete_canvas_card_schema(),
        ),
    ];
    tools.extend(crate::memory_access::definitions());
    tools.push(definition("workspace_memory", "只读指定工作空间项目区域的项目或工作树记忆。area_id 必须来自目录；不接受路径。action 为 list/search/read，需要正文时使用 read。",object_schema(json!({"area_id":{"type":"string"},"action":{"type":"string","enum":["list","search","read"]},"scope":{"type":"string","enum":["project","worktree"]},"id":{"type":"string"},"query":{"type":"string"},"offset":{"type":"integer","minimum":0}}),&["area_id","action","scope"])));
    tools.push(definition(
        "workspace_snapshot",
        "只读查询当前桌面工作空间的 Agent 任务状态。默认当前项目用 current_project；用户明确要求整个工作空间用 workspace；指定项目用 area 和快照返回的 area_id。未加载项目返回上次状态和更新时间，待审状态不代表审批仍可操作；仅 can_locate_approval=true 可直达当前审批。",
        object_schema(
            json!({
                "scope": {"type": "string", "enum": ["current_project", "workspace", "area"]},
                "area_id": {"type": "string", "minLength": 1}
            }),
            &["scope"],
        ),
    ));
    tools.push(definition(
        "file_open",
        "打开 workspace_snapshot 返回的任务产物，保持当前画布，并在右侧选中对应文件标签。所有身份字段必须来自同一条产物和所属工作空间。",
        object_schema(
            json!({
                "workspace_id": {"type": "string", "minLength": 1},
                "area_id": {"type": "string", "minLength": 1},
                "node_id": {"type": "integer", "minimum": 1},
                "turn_id": {"type": "string", "minLength": 1},
                "source_path": {"type": "string", "minLength": 1},
                "version": {"type": "string", "minLength": 1}
            }),
            &["workspace_id", "area_id", "node_id", "turn_id", "source_path", "version"],
        ),
    ));
    tools.push(definition(
        "agent_show",
        "切到工作空间内已加载的 Agent 卡片。处理待审项时同时传 workspace_snapshot 中的 approval_run_id 和 approval_turn_id；App 会重新校验待审状态，再切到所属项目。",
        object_schema(
            json!({
                "workspace_id": {"type": "string", "minLength": 1},
                "area_id": {"type": "string", "minLength": 1},
                "node_id": {"type": "integer", "minimum": 1},
                "approval_run_id": {"type": "string", "minLength": 1},
                "approval_turn_id": {"type": "string", "minLength": 1}
            }),
            &["workspace_id", "area_id", "node_id"],
        ),
    ));
    tools.push(definition(
        "idea_save",
        "用户说“记一下/记下来/别忘了/提醒我/加个待办”等，想把一个想法留着以后做时调用。body 写成一句自包含的想法原文，不要自行扩写或拆分。只记录，不执行；用户没有要求现在就做时不要派发。",
        object_schema(json!({"body": {"type": "string", "minLength": 1}}), &["body"]),
    ));
    tools.push(definition(
        "idea_list",
        "只读：列出当前工作空间的想法清单（待办、已指派、已完成）及 idea_id。",
        object_schema(json!({}), &[]),
    ));
    tools.push(definition(
        "handoff_to_brain",
        "把需读代码或文档、整理多个 Agent 进度、派后续任务的请求交给后台大脑。调用后只说“我去看看”，结果稍后播报。简单状态问题不要交接。",
        object_schema(
            json!({"request": {"type": "string", "minLength": 1}}),
            &["request"],
        ),
    ));
    tools.extend(crate::context::tools::definitions());
    tools
}

fn delete_canvas_card_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "node_id": {"type": "integer", "minimum": 1},
            "position": {"type": "integer", "minimum": 1},
            "agent": {"type": "string", "minLength": 1}
        },
        "oneOf": [
            {"required": ["node_id"]},
            {"required": ["position"]},
            {"required": ["agent"]}
        ],
        "additionalProperties": false
    })
}

/// 当前会话能不能把这个工具交给模型。文字会话、豆包、语音和 GPT-Live 后台用同一份判断。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelToolScope {
    pub personal: bool,
    pub desktop: bool,
    pub shared: bool,
    pub mobile: bool,
}

pub fn model_tool_visible(name: &str, scope: ModelToolScope) -> bool {
    match name {
        "memory_list" | "memory_search" | "memory_read" | "memory_write" => true,
        "context_read" | "context_update" => scope.shared,
        "workspace_memory" => scope.mobile,
        "workspace_snapshot" | "agent_show" | "file_open" | "idea_save" | "idea_list" => {
            scope.desktop
        }
        "agent_assign" => scope.desktop || !scope.personal,
        "agent_reply" | "agent_create" | "task_list" | "task_cancel" | "reply_cancel"
        | "terminal_create" | "card_delete" | "handoff_to_brain" => !scope.personal,
        _ => false,
    }
}

fn definition(name: &str, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}
