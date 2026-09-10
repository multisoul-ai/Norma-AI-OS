import XCTest
@testable import NormaCore

final class ApprovalTests: XCTestCase {
    /// 决策详情只打开同项目、窗口、Provider与执行身份的原卡，不要求开启手机控制模式。
    /// 数据构造：原生Claude节点7，run-a/session-a/turn-a；mobile_control=false仍允许只读查看。
    /// 执行过程：准确匹配→替换项目/窗口/Provider/运行/会话/回合→分别拒绝。
    /// 预期结果：返回原卡而非构造新卡；所有同名但不同身份的目标均不能被打开。
    func testApprovalOpensOnlyItsOriginalConversation() throws {
        let target = try JSONDecoder().decode(ApprovalTarget.self, from: Data(#"{"node_id":7,"profile":"Claude","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"request-a"}"#.utf8))
        let raw = #"{"workspace_id":"workspace-a","display_name":"A","state":"open","route_id":"route-a","card_count":1,"cards":[{"node_id":7,"profile":"Claude","title":"原卡","mobile_control":false,"state":"waiting_approval","run_id":"run-a","session_id":"session-a","turn_id":"turn-a"}]}"#
        func project(_ from: String = "unused", _ to: String = "unused") throws -> ProjectSnapshot {
            try JSONDecoder().decode(ProjectSnapshot.self, from: Data(raw.replacingOccurrences(of: from, with: to).utf8))
        }
        let card = try target.card(in: project(), workspaceID: "workspace-a", routeID: "route-a")
        XCTAssertEqual(card.title, "原卡", "必须返回电脑原卡内容")
        XCTAssertFalse(card.mobileControl, "查看不能替原卡切换模式")
        XCTAssertThrowsError(try target.card(in: project("workspace-a", "workspace-b"), workspaceID: "workspace-a", routeID: "route-a"), "不能跨项目打开相同节点")
        XCTAssertThrowsError(try target.card(in: project("route-a", "route-b"), workspaceID: "workspace-a", routeID: "route-a"), "旧窗口不能认领重开窗口")
        XCTAssertThrowsError(try target.card(in: project("Claude", "Codex"), workspaceID: "workspace-a", routeID: "route-a"), "相同数字身份不能跨Provider")
        XCTAssertThrowsError(try target.card(in: project("run-a", "run-b"), workspaceID: "workspace-a", routeID: "route-a"), "重开运行不能认领旧请求")
        XCTAssertThrowsError(try target.card(in: project("session-a", "session-b"), workspaceID: "workspace-a", routeID: "route-a"), "不能打开不同会话")
        XCTAssertThrowsError(try target.card(in: project("turn-a", "turn-b"), workspaceID: "workspace-a", routeID: "route-a"), "打开前已改变回合应明确过期")
    }

    /// 审批保留完整工具上下文，仅支持当前请求提供的选择，回执不能认领另一会话或另一决定。
    ///
    /// 数据构造：node7/run-a/session-a/turn-a 的 Codex run-a:1；命令含换行，u64最大值验证数字不会舍入。
    /// 执行过程：解析审批 → 保存本机预览和待发命令 → 重读 → 验证submitted/closed → 改错身份与决定。
    /// 预期结果：完整上下文仅留本机预览，网络只发目标；已提交不等于工具执行，伪造或冲突结果被拒绝。
    func testApprovalContextChoicesAndExactReceipts() async throws {
        let raw = #"{"target":{"node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"run-a:1"},"card_title":"实现登录","tool":"item/commandExecution/requestApproval","details":{"command":"printf first\nprintf second","limit":18446744073709551615},"decisions":["deny"]}"#
        let snapshot = try JSONDecoder().decode(ApprovalSnapshot.self, from: Data(raw.utf8))
        try snapshot.validate()
        let external = try JSONDecoder().decode(ApprovalSnapshot.self, from: Data(raw.replacingOccurrences(of: "item/commandExecution/requestApproval", with: "external_permission_hook").replacingOccurrences(of: #"["deny"]"#, with: "[]").utf8))
        let native = try JSONDecoder().decode(ApprovalSnapshot.self, from: Data(raw.replacingOccurrences(of: "item/commandExecution/requestApproval", with: "native_terminal").replacingOccurrences(of: #"["deny"]"#, with: "[]").utf8))
        XCTAssertNoThrow(try native.validate(), "原生终端确认提示应被接受")
        XCTAssertTrue(native.requiresDesktop, "原生终端确认必须指向电脑")
        XCTAssertTrue(native.decisions.isEmpty, "不显示未经接通的批准按钮")
        XCTAssertNoThrow(try external.validate(), "外部等待可以展示但没有动作")
        XCTAssertThrowsError(try JSONDecoder().decode(ApprovalSnapshot.self, from: Data(raw.replacingOccurrences(of: #"["deny"]"#, with: "[]").utf8)).validate(), "真正的Provider请求缺少选项必须报错")
        XCTAssertTrue(try snapshot.details.formatted().contains("18446744073709551615"), "大整数不能被舍入成另一个参数")
        let machine = "mac-" + String(repeating: "a", count: 64)
        let pending = try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, approving: snapshot, decision: .deny, operationID: "decision-a")
        XCTAssertTrue(pending.matchesApproval(machineID: machine, workspaceID: "project-a", routeID: "route-a", target: snapshot.target), "原来源必须关联已保存的决定")
        XCTAssertFalse(pending.matchesApproval(machineID: "mac-" + String(repeating: "b", count: 64), workspaceID: "project-a", routeID: "route-a", target: snapshot.target), "另一电脑相同局部身份不能被隐藏或认领")
        XCTAssertFalse(pending.matchesApproval(machineID: machine, workspaceID: "project-b", routeID: "route-a", target: snapshot.target), "另一项目不能认领决定")
        XCTAssertFalse(pending.matchesApproval(machineID: machine, workspaceID: "project-a", routeID: "route-b", target: snapshot.target), "重开窗口必须重新核验来源")
        XCTAssertThrowsError(try PendingMobileCommand(machineID: machine, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, approving: snapshot, decision: .allow), "不能创造Provider没提供的批准选项")
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = PendingCommandStore(url: directory.appendingPathComponent("commands.json"))
        try await store.save(pending)
        let saved = try await store.load()
        XCTAssertEqual(saved, [pending], "重启仅回查，预览和身份须保留")
        XCTAssertEqual(saved.first?.approvalPreview, snapshot, "请求关闭后仍能核对决定内容")
        let wire = String(decoding: try JSONEncoder().encode(pending.request), as: UTF8.self)
        XCTAssertTrue(wire.contains("decide_approval"), "必须使用正式决定接口")
        XCTAssertFalse(wire.contains("printf first"), "上下文仅用于预览，手机不能改写Provider的工具输入")
        let rawReceipt = #"{"device_id":"phone-a","operation_id":"decision-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"run-a:1","decision":"deny","status":"submitted"},"error":null}"#
        func receipt(_ text: String) throws -> MobileCommandReceipt {
            let value = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(text.utf8))
            try value.validate(for: pending); return value
        }
        XCTAssertEqual(try receipt(rawReceipt).result?.status, "submitted", "只确认本地提交，不宣称工具执行")
        XCTAssertEqual(try receipt(rawReceipt.replacingOccurrences(of: "submitted", with: "request_closed")).result?.status, "request_closed", "关闭不可冒充决定被采用")
        XCTAssertThrowsError(try receipt(rawReceipt.replacingOccurrences(of: "turn-a", with: "turn-b")), "旧回合不能认领新回合")
        XCTAssertThrowsError(try receipt(rawReceipt.replacingOccurrences(of: "deny", with: "allow")), "批准不能认领为拒绝")
        XCTAssertThrowsError(try receipt(rawReceipt.replacingOccurrences(of: "run-a:1", with: "run-a:2")), "同卡另一审批调用不得认领")
        XCTAssertThrowsError(try receipt(rawReceipt.replacingOccurrences(of: "submitted", with: "executed")), "现有Provider信号不证明执行成功")
    }
}
