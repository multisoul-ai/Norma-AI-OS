use super::{localized_live_action, progress_detail, running_detail};
use norma::mobile::TaskProgress;
use runtime::AgentTaskState;
use ui::i18n::Locale;

/// 场景：后台结构化状态与进度投影为当前语言，用户步骤/工具名保持原文。
/// 数据构造：开始于 0、当前 120000ms，即 2 分钟；3 步完成 1 步，正在第 2 步。
/// 执行过程：对同一进度及运行状态分别使用英文、中文呈现，最后恢复原语言。
/// 预期结果：产品状态与时间双语正确；步骤名称保持中文；结束态不伪造运行提示。
#[test]
fn drawer_i18n_localizes_runtime_status_without_rewriting_user_content() {
    let previous = Locale::current();
    let progress = TaskProgress {
        completed: 1,
        total: 3,
        current_step: Some("用户原始步骤".into()),
    };
    for (locale, thinking, detail, step, calling) in [
        (
            Locale::En,
            "Thinking",
            "Running · Thinking · Running for 2 min",
            "Step 2/3 · 用户原始步骤 · Running for 2 min",
            "Calling 用户工具",
        ),
        (
            Locale::ZhCn,
            "思考中",
            "运行中 · 思考中 · 已运行 2 分",
            "第 2/3 项 · 用户原始步骤 · 已运行 2 分",
            "调用 用户工具",
        ),
    ] {
        locale.apply();
        let action = localized_live_action(&AgentTaskState::Thinking).expect("运行状态应有文案");
        assert_eq!(action, thinking, "后台思考状态必须在展示边界翻译");
        assert_eq!(
            running_detail(Some(&action), Some(0), 120_000),
            detail,
            "时长与状态应使用同一语言"
        );
        assert_eq!(
            progress_detail(&progress, Some(0), 120_000).0,
            step,
            "用户步骤原文必须保留"
        );
        assert_eq!(
            localized_live_action(&AgentTaskState::RunningTool {
                tool_name: Some("用户工具".into())
            })
            .as_deref(),
            Some(calling),
            "工具名保持原文"
        );
        assert!(
            localized_live_action(&AgentTaskState::Done).is_none(),
            "结束状态不能显示运行中动作"
        );
    }
    previous.apply();
}
