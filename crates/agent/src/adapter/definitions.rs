use super::ToolDefinition;
use serde_json::{json, Value};

pub fn norma_tool_definitions() -> Vec<ToolDefinition> {
    let mut tools = vec![
        definition(
            "find_agent_tasks",
            "查询 AgentCard 任务；无筛选条件时返回最近的未结束任务。",
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
            "create_agent",
            "仅在用户明确要求新建某个内置类型的 Agent 时使用。若同时要求新 Agent 执行任务，把完整任务放入 task，由系统在创建后立即入队，不要再调用 assign_agent_task。若用户点名的 Agent 画布上已有卡片，不要调用本工具，应 reply_agent 直接对话。",
            object_schema(
                json!({
                    "profile": {"type": "string", "enum": ["codex", "claude", "opencode", "cursor", "grok"]},
                    "task": {"type": "string", "minLength": 1}
                }),
                &["profile"],
            ),
        ),
        definition(
            "assign_agent_task",
            "把任务派发给已启用的 Agent；画布上没有卡片时系统会自动打开唯一一张。目标明确时直接执行，无需确认；已有卡片且用户只是对话时用 reply_agent。",
            object_schema(
                json!({
                    "agent": {"type": "string", "minLength": 1},
                    "task": {"type": "string", "minLength": 1}
                }),
                &["agent", "task"],
            ),
        ),
        definition(
            "cancel_agent_task",
            "取消一个明确 task_id 的 AgentCard 任务。",
            object_schema(
                json!({"task_id": {"type": "integer", "minimum": 1}}),
                &["task_id"],
            ),
        ),
        definition(
            "reply_agent",
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
            "cancel_agent_reply",
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
            "create_terminal",
            "直接创建一张 Terminal 卡片。",
            object_schema(json!({}), &[]),
        ),
        definition(
            "create_browser",
            "在桌面工作空间右侧标签打开网页；用户未指定网址时省略 url，使用产品默认首页。",
            object_schema(json!({"url": {"type": "string", "minLength": 1}}), &[]),
        ),
        definition(
            "create_preview",
            "在桌面工作空间右侧标签打开只读文件预览（用户也可能称 REVIEW）；path 可使用当前工作区中的相对路径或绝对路径。",
            object_schema(
                json!({
                    "path": {"type": "string", "minLength": 1},
                    "preview_type": {
                        "type": "string",
                        "enum": ["markdown", "text", "image"]
                    }
                }),
                &["path"],
            ),
        ),
        definition(
            "focus_card",
            "聚焦一张已存在的 Canvas 卡片。",
            object_schema(
                json!({"node_id": {"type": "integer", "minimum": 1}}),
                &["node_id"],
            ),
        ),
        definition(
            "send_terminal_input",
            "向 Terminal 卡片发送文本；不能用于 AgentCard。",
            object_schema(
                json!({
                    "node_id": {"type": "integer", "minimum": 1},
                    "text": {"type": "string"},
                    "submit": {"type": "boolean"}
                }),
                &["node_id", "text", "submit"],
            ),
        ),
        definition(
            "delete_canvas_card",
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
        "assign_workspace_task",
        "向当前桌面工作空间内一张已加载的 Agent 卡片派发任务。先查 workspace_snapshot，使用返回的 workspace_id、area_id 和 node_id；每次只派一个目标，按真实回执答复。",
        object_schema(
            json!({
                "workspace_id": {"type": "string", "minLength": 1},
                "area_id": {"type": "string", "minLength": 1},
                "node_id": {"type": "integer", "minimum": 1},
                "task": {"type": "string", "minLength": 1}
            }),
            &["workspace_id", "area_id", "node_id", "task"],
        ),
    ));
    tools.push(definition(
        "open_workspace_artifact",
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
        "locate_workspace_agent",
        "定位当前工作空间内已加载的 Agent 卡片。处理待审项时同时传 workspace_snapshot 中的 approval_run_id 和 approval_turn_id；App 会重新校验待审状态，再切到所属项目。",
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
