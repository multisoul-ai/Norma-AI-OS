import XCTest
@testable import Norma
@testable import NormaCore

@MainActor final class WorkspaceTerminalModelTests: XCTestCase {
    /// 场景：终端因原身份无效而停止后，回到实时不能清除阻止连接的说明。
    /// 数据构造：未绑定run的原卡；协议校验直接失败，无需网络或真实设备凭据。
    /// 执行过程：启动只读观察 → 身份校验失败并停止 → 用户点回到实时。
    /// 预期结果：明确失败说明保留；连接仍停止；不会变成无说明的连接中状态。
    func testReturnToLivePreservesStoppedConnectionNotice() async throws {
        let machine = try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: [
            "id":"mac-" + String(repeating: "a", count: 64), "name":"测试Mac",
            "certificateSHA256":String(repeating: "b", count: 64), "endpoints":["wss://127.0.0.1:1/mobile"]
        ]))
        let computers = ComputerModel()
        let client = WorkspaceTerminalClient(machine: machine, mobile: computers.client) { throw MobileError.unavailable }
        let target = WorkspaceAgentTarget(areaID: "area-original", definitionID: 1, nodeID: 1, runID: nil, sessionID: nil)
        let model = WorkspaceTerminalModel()
        await model.watch(client, request: .init(workspaceID: "workspace-original", target: target))
        let notice = try XCTUnwrap(model.failure, "无效身份必须留下停止连接的明确说明")
        model.returnToLive()
        XCTAssertEqual(model.failure, notice, "回到实时不能隐藏原身份失效或授权变化的说明")
        XCTAssertFalse(model.connected, "回到实时不能自动建立替代会话连接")
        XCTAssertNil(model.frame, "未通过原身份校验不能产生终端画面")
    }
}
