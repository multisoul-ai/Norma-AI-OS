use super::FeatureGuideTopic;

impl FeatureGuideTopic {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::VoiceDispatch => "用语音把任务交给 Agent",
            Self::ContinueConversation => "在卡片里继续推进任务",
            Self::CanvasManagement => "管理画布与工作卡片",
            Self::TaskStatus => "看懂任务状态与派发回执",
        }
    }

    pub(super) fn eyebrow(self) -> &'static str {
        match self {
            Self::VoiceDispatch => "VOICE DISPATCH",
            Self::ContinueConversation => "AGENT FOLLOW-UP",
            Self::CanvasManagement => "INFINITE CANVAS",
            Self::TaskStatus => "TASK FEEDBACK",
        }
    }

    pub(super) fn summary(self) -> &'static str {
        match self {
            Self::VoiceDispatch => "说出 Agent 和要做的事，Norma 会识别意图、找到卡片并派发任务。",
            Self::ContinueConversation => {
                "选中正在工作的 Agent 卡片，直接补充条件或继续安排下一步。"
            }
            Self::CanvasManagement => "拖动、缩放和重排卡片，把项目空间整理成适合你的工作流。",
            Self::TaskStatus => "通过状态标签、队列和流星回执，快速确认任务是否真正送达。",
        }
    }

    pub(super) fn steps(self) -> [&'static str; 3] {
        match self {
            Self::VoiceDispatch => [
                "按下语音快捷键并说出 Agent 名称",
                "描述任务目标和必要约束",
                "流星命中卡片，表示任务已接收或已排队",
            ],
            Self::ContinueConversation => [
                "点选需要继续沟通的 Agent 卡片",
                "说出补充要求或直接输入消息",
                "新消息进入同一会话并继续执行",
            ],
            Self::CanvasManagement => [
                "拖动空白区域平移整个画布",
                "拖动卡片调整位置，使用布局快速整理",
                "聚焦卡片后继续输入或按 Esc 返回画布",
            ],
            Self::TaskStatus => [
                "Working 表示 Agent 正在执行当前任务",
                "Queued 表示后续任务已进入同一卡片队列",
                "流星命中与回执代表本次派发已被接收",
            ],
        }
    }
}
