import XCTest
@testable import NormaCore

final class SnapshotSearchTests: XCTestCase {
    /// 任务列表排除仅创建的空卡；按真实状态筛选并跨项目、任务、Agent匹配查询。
    ///
    /// 数据构造：1张空Codex卡，1张运行中的登录任务，1张Claude待确认任务，1张完成任务。
    /// 执行过程：全量任务筛选 → 按运行/待处理/完成取交集 → 跨字段搜索 → 核对原始状态未变。
    /// 预期结果：空卡不冒充任务；完成不能由等待或运行推断；其他项目/Agent不能混入搜索。
    func testTaskListUsesRealCardTasksAndStates() {
        let empty = card(1, title: "空卡", state: "ready")
        let running = card(2, title: "Backend", task: "修复登录", state: "thinking")
        let approval = card(3, title: "UI", profile: "Claude", task: "检查首页", state: "waiting_approval")
        let done = card(4, title: "Tests", task: "验证连接", state: "done")
        XCTAssertFalse(empty.matchesTaskSearch("", projectName: "Norma", filter: .all), "创建卡片不是派发任务")
        XCTAssertTrue(running.matchesTaskSearch("NÓRMA Codex 登录", projectName: "Norma iOS", filter: .running), "项目、Agent和任务应共同匹配")
        XCTAssertFalse(running.matchesTaskSearch("其他项目", projectName: "Norma", filter: .all), "不能搜索到无关项目")
        XCTAssertFalse(approval.matchesTaskSearch("Codex", projectName: "Norma", filter: .attention), "不同Agent不能混入")
        XCTAssertTrue(approval.matchesTaskSearch("", projectName: "Norma", filter: .attention), "等待确认需要用户决策")
        XCTAssertFalse(approval.matchesTaskSearch("", projectName: "Norma", filter: .completed), "等待确认不能算完成")
        XCTAssertTrue(done.matchesTaskSearch("连接", projectName: "Norma", filter: .completed), "准确完成状态应可检索")
        XCTAssertFalse(done.matchesTaskSearch("", projectName: "Norma", filter: .running), "已完成不能仍显示运行")
        XCTAssertEqual(running.state, "thinking", "筛选不能修改原卡片运行状态")
    }

    /// 卡片搜索同时匹配任务标题与 Agent，状态/Agent 筛选取交集，清除后保留原始顺序。
    ///
    /// 数据构造：卡1=Codex/运行/登录，卡2=Claude/待确认/登录，卡3=Codex/空闲/Café。
    /// 执行过程：跨字段多词搜索 → 加入状态与Agent条件 → 清空条件 → 检查完整原数组。
    /// 预期结果：只出现条件对应的准确卡片；不能串入其他状态、其他Agent或无关任务，也不修改原数组。
    func testCardSearchCombinesTermsStatusAndProvider() {
        let cards = [
            card(1, title: "Backend", task: "修复登录", state: "thinking"),
            card(2, title: "登录界面", profile: "Claude", state: "waiting_approval"),
            card(3, title: "Café", state: "idle")
        ]
        XCTAssertEqual(cards.filter { $0.matchesSearch("  CODEX\n登录 ") }.map(\.id), [1], "空白分词、大小写与任务标题应一起匹配")
        XCTAssertFalse(cards[1].matchesSearch("Codex 登录"), "其他Agent不能因任务标题相同而混入")
        XCTAssertEqual(cards.filter { $0.matchesSearch("登录", state: "waiting_approval", profile: "Claude") }.map(\.id), [2], "状态和Agent应与文本取交集")
        XCTAssertFalse(cards[0].matchesSearch("登录", state: "waiting_approval"), "运行卡不能出现在待确认筛选中")
        XCTAssertFalse(cards[1].matchesSearch("登录", profile: "Codex"), "明确的Agent条件不能被文本覆盖")
        XCTAssertTrue(cards[2].matchesSearch("cafe"), "系统本地化搜索应忽略重音和大小写")
        XCTAssertEqual(cards.filter { $0.matchesSearch(" \n") }.map(\.id), [1, 2, 3], "清除条件应恢复全部卡片及原始顺序")
        XCTAssertEqual(cards.map(\.state), ["thinking", "waiting_approval", "idle"], "本地筛选不得修改真实快照状态")
    }

    private func card(_ id: UInt64, title: String, profile: String = "Codex", task: String? = nil, state: String) -> CardSnapshot {
        CardSnapshot(nodeID: id, profile: profile, title: title, mobileControl: true, state: state,
                     runID: nil, sessionID: nil, taskTitle: task, turnID: nil)
    }

    /// 项目搜索只筛选传入电脑的目录，按名称与打开状态匹配，不合并同名项目。
    ///
    /// 数据构造：电脑A有“Norma iOS”打开及“文档”关闭；电脑B有同名关闭项目，三者独立ID。
    /// 执行过程：对A目录搜索并筛选打开状态 → 对B独立搜索 → 清除A条件。
    /// 预期结果：准确返回各电脑现有目录中的项目；关闭项目不混入打开筛选，清除不扩大电脑范围。
    func testProjectSearchRemainsWithinSuppliedMachineCatalog() {
        let a = ProjectSnapshot(workspaceID: "project-a", displayName: "Norma iOS", state: "open", routeID: "route-a", cardCount: 0, cards: nil)
        let closed = ProjectSnapshot(workspaceID: "docs-a", displayName: "文档", state: "closed", routeID: nil, cardCount: nil, cards: nil)
        let b = ProjectSnapshot(workspaceID: "project-b", displayName: "Norma iOS", state: "closed", routeID: nil, cardCount: nil, cards: nil)
        XCTAssertEqual([a, closed].filter { $0.matchesSearch(" IOS norma ", state: "open") }.map(\.id), ["project-a"], "多词顺序不影响名称匹配，但必须保持电脑A身份")
        XCTAssertEqual([b].filter { $0.matchesSearch("norma") }.map(\.id), ["project-b"], "电脑B同名项目应保持自己的身份")
        XCTAssertFalse(b.matchesSearch("norma", state: "open"), "同名不能将关闭项目误判为打开")
        XCTAssertFalse(closed.matchesSearch("norma"), "无关项目不得出现在结果中")
        XCTAssertEqual([a, closed].filter { $0.matchesSearch("") }.map(\.id), ["project-a", "docs-a"], "清除条件只恢复当前目录，不能混入电脑B")
    }
}
