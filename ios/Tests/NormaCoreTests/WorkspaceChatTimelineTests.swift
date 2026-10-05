import Foundation
import Testing
@testable import NormaCore

/// 场景：桌面卡片生命周期的状态必须在手机上有明确中文反馈。
/// 数据构造：ready/preparing/interrupting/ended 分别来自正式 AgentCard 状态机；未知状态作为负例。
/// 执行过程：逐一映射卡片状态 → 核对可用、准备、中断和结束显示。
/// 预期结果：正式状态不会落入“状态待核查”；未知值不能显示为可用。
@Test func desktopCardStatesHaveExplicitChineseFeedback() {
    #expect(workspaceChatStatus("ready") == "可用", "已就绪卡片应显示可用")
    #expect(workspaceChatStatus("preparing") == "正在准备", "准备中的卡片不能显示状态未知")
    #expect(workspaceChatStatus("interrupting") == "正在中断", "中断过程应区别于已中断")
    #expect(workspaceChatStatus("ended") == "会话已结束", "已结束会话不得暗示可以追问")
    #expect(workspaceChatStatus("unrecognized") != "可用", "未知卡片状态不能默认为可用")
}

/// 场景：重启、授权撤销与模型缺失必须说明原因及原操作处理方式。
/// 数据构造：3个正式服务错误码和1个未知结果码，不包括真实凭据。
/// 执行过程：将Mac事件错误转换为用户文案 → 核对下一步与不重发说明。
/// 预期结果：用户能区分重新配对、核查旧操作和在Mac设置模型；不能把未知结果称为成功。
@Test func workspaceFailuresExplainRecoveryWithoutRawProtocolCodes() {
    #expect(WorkspaceChatError.server("mac_restarted").localizedDescription.contains("Mac 已重启"), "重启应说明会话可能失效")
    #expect(WorkspaceChatError.server("unauthorized").localizedDescription.contains("授权"), "撤销授权不能归为普通离线")
    #expect(WorkspaceChatError.server("norma_model_unconfigured").localizedDescription.contains("模型"), "缺模型配置应指向Mac设置")
    #expect(!WorkspaceChatError.server("outcome_unknown").localizedDescription.contains("outcome_unknown"), "未知结果应使用可理解的中文说明")
    #expect(!WorkspaceChatError.server("outcome_unknown").localizedDescription.contains("已完成"), "未知结果不得伪装完成")
}

/// 场景：两个操作交错执行时，迟到成果应在时间线尾部，完成信封不能重复展示同一成果。
/// 数据构造：游标1=A接收，2=B接收，3=A成果，4=A完成信封；数组输入故意乱序。
/// 执行过程：解析事件 → 生成显示时间线 → 核对顺序与重复事件。
/// 预期结果：A成果在B之后；完成信封不再展示，三条事件全部保留原游标。
@Test func lateResultRemainsAfterNewerMessageWithoutDuplicateEnvelope() throws {
    let events = try JSONDecoder().decode([WorkspaceChatEvent].self, from: Data(#"""
    [
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":4,"device_id":"phone","operation_id":"a","kind":"succeeded","payload":{"task_ref":"t","text":"完成"}},
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":1,"device_id":"phone","operation_id":"a","kind":"received","payload":{"text":"任务A"}},
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":3,"device_id":"phone","operation_id":"a","kind":"task_result","payload":{"task_ref":"t","text":"完成"}},
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":2,"device_id":"phone","operation_id":"b","kind":"received","payload":{"text":"讨论B"}}
    ]
    """#.utf8))
    let timeline = workspaceChatTimeline(events)
    #expect(timeline.map(\.cursor) == [1, 2, 3], "显示顺序必须跟随服务端事件，不得按操作回插成果")
    #expect(timeline.last?.payload.text == "完成", "迟到成果应在时间线尾部")
    #expect(!timeline.contains { $0.cursor == 4 }, "任务成果不应被完成信封重复展示")
}

/// 场景：空闲卡片的直接消息没有task_ref，仍只能显示一次真实成果。
/// 数据构造：操作A的成果与完成信封各1条，操作B有同文但没有成果；游标1/2/3递增。
/// 执行过程：解码3条事件 → 构造时间线 → 核对A信封删除、B独立完成保留。
/// 预期结果：A成果存在、A重复信封不存在；不能按文本误删B的回复。
@Test func directCardResultWithoutTaskReferenceIsNotDuplicated() throws {
    let events = try JSONDecoder().decode([WorkspaceChatEvent].self, from: Data(#"""
    [
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":1,"device_id":"phone","operation_id":"a","kind":"task_result","payload":{"text":"同一回复"}},
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":2,"device_id":"phone","operation_id":"a","kind":"succeeded","payload":{"text":"同一回复"}},
      {"mac_id":"mac","desktop_workspace_id":"space","cursor":3,"device_id":"phone","operation_id":"b","kind":"succeeded","payload":{"text":"同一回复"}}
    ]
    """#.utf8))
    let timeline = workspaceChatTimeline(events)
    #expect(timeline.contains { $0.cursor == 1 }, "无task_ref的真实成果必须保留")
    #expect(!timeline.contains { $0.cursor == 2 }, "相同操作的完成信封不能重复显示成果")
    #expect(timeline.contains { $0.cursor == 3 }, "其他操作的同文回复不能被误删")
}
