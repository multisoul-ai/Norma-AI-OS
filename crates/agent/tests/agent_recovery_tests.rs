mod support;

use agent::{AgentCommand, AgentEvent, AgentSystem, AuditEntry, CardTaskBudget, CardTaskJournal};
use support::{task_id_from, RecordingHost, ScriptedProvider};

/// A-6b：项目级 AgentSystem 重建时应公开上次未完成任务的中断事件，并延续 task id。
///
/// 数据构造（含关键数值的推导过程）：
///   recover_incomplete = [7]（Journal 中唯一未完成任务）；
///   next_task_id = 8（最大已知 task id 7 加一）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 RecoveryJournal 创建 project-a 的 AgentSystem；
///   2. 初始化恢复 task_id=7，并公开 TaskInterrupted；
///   3. 提交一个新的 DirectCardTask，系统从 next_task_id=8 分配标识。
///
/// 预期结果：
///   - 断言 A：初始事件包含且只包含 TaskInterrupted(7)，旧任务不会伪装为完成；
///   - 断言 B：新任务使用 task_id=8，不复用已恢复任务的 id。
#[test]
fn startup_surfaces_recovered_tasks_and_continues_task_ids() {
    struct RecoveryJournal;

    impl CardTaskJournal for RecoveryJournal {
        fn append(&mut self, _entry: AuditEntry) {}

        fn next_task_id(&self, project_id: &str) -> u64 {
            assert_eq!(project_id, "project-a", "应按当前项目查询下一个 task id");
            8
        }

        fn recover_incomplete(&mut self, project_id: &str) -> Vec<u64> {
            assert_eq!(project_id, "project-a", "应只恢复当前项目的未完成任务");
            vec![7]
        }
    }

    let mut system = AgentSystem::new(
        "project-a",
        ScriptedProvider::default(),
        ScriptedProvider::default(),
        RecordingHost::default(),
        RecoveryJournal,
        CardTaskBudget::default(),
    );

    assert_eq!(
        system.events(),
        &[AgentEvent::TaskInterrupted { task_id: 7 }],
        "启动时应公开恢复出的中断任务，且不得遗漏或伪造其他事件"
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask("新任务".to_string()))
        .expect("恢复后仍应接受新任务");
    assert_eq!(
        task_id_from(&queued),
        8,
        "新任务必须使用 next_task_id=8，不得复用已恢复的 task id=7"
    );
}
