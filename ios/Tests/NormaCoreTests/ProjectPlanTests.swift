import XCTest
@testable import NormaCore

final class ProjectPlanTests: XCTestCase {
    /// 方向与优先事项使用同一持久命令，兼容未选择优先项的旧请求。
    /// 数据构造：plan-a的第二项index1；非法index8=最多8项的首个越界位置。
    /// 执行过程：编码/重开带优先项命令 → 验证原文与来源 → 空ID/越界 → 编码旧版无优先项。
    /// 预期结果：选择不能变成Agent输入或丢失身份，旧请求不新增字段。
    func testPlanPriorityPreservesSourceAndValidatesBounds() throws {
        func make(_ priority: PlanPriority?) throws -> PendingMobileCommand {
            try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a",
                workspaceID: "project-a", routeID: "route-a", revision: 2, planning: "先稳定连接",
                priority: priority, operationID: "priority-op")
        }
        let command = try make(PlanPriority(planID: "plan-a", todoIndex: 1))
        let data = try JSONEncoder().encode(command)
        let restored = try JSONDecoder().decode(PendingMobileCommand.self, from: data)
        XCTAssertEqual(restored, command, "重开必须保留方向和准确优先项")
        XCTAssertFalse(restored.request.command.isMessage, "调整建议不能冒充已派发任务")
        let raw = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        let request = try XCTUnwrap(raw["request"] as? [String: Any])
        let payload = try XCTUnwrap(request["command"] as? [String: Any])
        let priority = try XCTUnwrap(payload["priority"] as? [String: Any])
        XCTAssertEqual(priority["plan_id"] as? String, "plan-a", "优先项须绑定来源计划")
        XCTAssertEqual(priority["todo_index"] as? Int, 1, "必须是用户选中的第二项")
        XCTAssertEqual(payload["direction"] as? String, "先稳定连接", "不要把结构化偏好拼进用户原文")
        XCTAssertThrowsError(try make(PlanPriority(planID: "", todoIndex: 1)), "空计划身份不能提交")
        XCTAssertThrowsError(try make(PlanPriority(planID: "plan-a", todoIndex: 8)), "优先项越界不能提交")
        let legacy = String(decoding: try JSONEncoder().encode(make(nil)), as: UTF8.self)
        XCTAssertFalse(legacy.contains("\"priority\":"), "无优先项仍使用旧版命令格式")
    }

    /// 计划命令保存完整方向，成功回执必须是建议身份，不得伪造卡片创建或执行成功。
    ///
    /// 数据构造：plan-op绑定phone-a/project-a/route-a；8KiB上限加一个字节构造超限方向。
    /// 执行过程：保存/重开 → 接受plan-a/proposed → 拒绝缺计划、混合卡片、completed与空/超限方向。
    /// 预期结果：原方向不变，无执行命令；只有准确建议回执被接受。
    func testPlanCommandPersistenceAndReceiptKind() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        func make(_ direction: String) throws -> PendingMobileCommand {
            try PendingMobileCommand(machineID: "mac-" + String(repeating: "a", count: 64), deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 2, planning: direction, operationID: "plan-op")
        }
        let command = try make("优先验证登录\n保留原有入口")
        let url = directory.appendingPathComponent("commands.json")
        try await PendingCommandStore(url: url).save(command)
        let saved = try await PendingCommandStore(url: url).load()
        XCTAssertEqual(saved, [command], "重开应保留原始方向和操作ID")
        XCTAssertFalse(command.request.command.isMessage, "计划生成不能派发Agent消息")
        XCTAssertThrowsError(try make(" \n"), "空方向不可提交")
        XCTAssertThrowsError(try make(String(repeating: "a", count: 8 * 1024 + 1)), "超出8KiB一个字节必须拒绝")
        let raw = #"{"device_id":"phone-a","operation_id":"plan-op","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"plan_id":"plan-a","status":"proposed"},"error":null}"#
        func read(_ input: String) throws -> MobileCommandReceipt {
            let receipt = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(input.utf8))
            try receipt.validate(for: command)
            return receipt
        }
        XCTAssertEqual(try read(raw).result?.planID, "plan-a", "建议回执应返回真实计划身份")
        XCTAssertNil(try read(raw).result?.nodeID, "计划不假造执行节点")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "proposed", with: "completed")), "建议不能声称完成")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: #""plan_id":"plan-a""#, with: #""node_id":7"#)), "卡片回执不能冒充计划回执")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: #""status":"proposed""#, with: #""status":"proposed","node_id":7"#)), "混合结果应拒绝")
    }

    /// 计划分页必须保留完整建议并按指定项目/游标隔离，不能展示其他项目的内容。
    ///
    /// 数据构造：sequence=5、空闲受控Codex node=7；before=6合法，before=5越界；一项非满页。
    /// 执行过程：解析完整计划 → 校验作用域/目标 → 换项目、顺序、非受控目标与伪造下一页。
    /// 预期结果：方向/指令保留且无完成标记；错项目、超页和不合法目标不进入UI。
    func testPageValidatesScopeTargetsAndCursor() throws {
        let raw = #"{"items":[{"sequence":5,"plan_id":"plan-a","workspace_id":"project-a","route_id":"route-a","direction":"先验证登录","created_at_unix":1,"draft":{"summary":"下一步补登录测试","todos":[{"target":{"node_id":7,"profile":"Codex","title":"登录","mobile_control":true,"state":"idle","run_id":null,"session_id":null,"turn_id":null,"task_title":null},"title":"补测试","instruction":"检查登录边界。"}]}}],"next_cursor":null}"#
        func read(_ text: String, before: UInt64? = 6) throws -> ProjectPlanPage {
            let page = try JSONDecoder().decode(ProjectPlanPage.self, from: Data(text.utf8))
            try page.validate(workspaceID: "project-a", before: before)
            return page
        }
        let page = try read(raw)
        XCTAssertEqual(page.items.first?.draft.todos.first?.instruction, "检查登录边界。", "具体执行指令不能被摘要替代")
        XCTAssertEqual(page.items.first?.direction, "先验证登录", "保留用户方向")
        XCTAssertNil(page.nextCursor, "没有历史页时不虚构游标")
        let native = raw.replacingOccurrences(of: #""run_id":null"#, with: #""run_id":"native-run""#)
            .replacingOccurrences(of: #""state":"idle""#, with: #""state":"ready""#)
        let todo = try XCTUnwrap(read(native).items.first?.draft.todos.first)
        XCTAssertEqual(todo.acceptance(for: todo.target), .accept(target: todo.target, queue: false), "原生待输入卡片可接收首次任务")
        XCTAssertThrowsError(try todo.validate(choice: .accept(target: todo.target, queue: true)), "尚无会话不能排队")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: #""session_id":null"#, with: #""session_id":"orphan""#)), "会话身份不能脱离运行身份")
        XCTAssertThrowsError(try read(raw, before: 5), "严格早于游标，不能重复上一页")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: "project-a", with: "project-b")), "其他项目不得混入")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: #""mobile_control":true"#, with: #""mobile_control":false"#)), "建议不能指向未受控卡片")
        XCTAssertThrowsError(try read(raw.replacingOccurrences(of: #""next_cursor":null"#, with: #""next_cursor":5"#)), "不足四条的页不能声明有下一页")
    }
}
