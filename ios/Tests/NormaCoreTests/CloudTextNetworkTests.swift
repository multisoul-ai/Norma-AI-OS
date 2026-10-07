import Foundation
import XCTest
@testable import NormaCore

final class CloudTextNetworkTests: XCTestCase {
    /// Swift 客户端通过正式本地网关获取完整回复，失败与撤销不伪造消息或重复调用。
    ///
    /// 数据构造：1 台隔离设备；2 个回合 = 成功轮用户/Norma 两条 + 中断轮用户一条。
    ///   成功回复两段「先把想法」+「留下。」；所有 HTTP 仅使用 fixture 指定的回环端口。
    /// 执行过程：兑换邀请 → 落用户原文/接收真实 HTTP 流 → 保存与重开 → 重放拒绝 → 中断 → 撤销。
    /// 预期结果：成功正文和增量一致，重启可搜索；失败只有原文，撤销后的令牌不能再获得回复。
    @MainActor func testInvitedClientStreamsThroughLocalGateway() async throws {
        guard let fixturePath = ProcessInfo.processInfo.environment["NORMA_CLOUD_TEXT_FIXTURE"] else {
            throw XCTSkip("由 services/norma-cloud/tests/check-text-native.mjs 启动隔离本机网关后运行")
        }
        struct Fixture: Decodable { let origin: URL; let admin: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: fixturePath)))
        guard fixture.origin.scheme == "http", fixture.origin.host == "127.0.0.1",
              fixture.origin.port != nil, fixture.origin.user == nil, fixture.origin.password == nil else {
            XCTFail("网络测试只允许指定的本地回环网关，不能请求正式服务")
            return
        }
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        #if DEBUG
        let client = try CloudTextClient(localPort: UInt16(try XCTUnwrap(fixture.origin.port)))
        #else
        let client = CloudTextClient(origin: fixture.origin, session: session)
        #endif
        func admin(_ path: String, body: [String: String] = [:]) async throws -> Data {
            var request = URLRequest(url: fixture.origin.appending(path: "v1/admin/devices/" + path))
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.setValue("Bearer \(fixture.admin)", forHTTPHeaderField: "Authorization")
            request.httpBody = try JSONEncoder().encode(body)
            let (data, response) = try await session.data(for: request)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, "隔离管理员请求必须成功")
            return data
        }
        struct Invitation: Decodable { let code: String }
        let invitation = try JSONDecoder().decode(Invitation.self, from: await admin("invite"))
        let credential = try await client.claim(code: invitation.code, name: "本地 Swift 验收")
        XCTAssertNotNil(UUID(uuidString: credential.deviceID), "网关必须返回稳定的设备 UUID")
        XCTAssertNotEqual(credential.token, invitation.code, "设备凭据不能复用邀请")
        do {
            _ = try await client.claim(code: invitation.code, name: "重复兑换")
            XCTFail("一次性邀请不能登记第二台设备")
        } catch CloudTextError.invitationInvalid { }

        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = directory.appendingPathComponent("chat.sqlite")
        let store = CloudChatStore(databaseURL: database)
        let turnID = UUID().uuidString
        let messages = [CloudTextRequestMessage(role: "user", content: "怎么整理想法？")]
        _ = try await store.appendUser(turnID: turnID, text: messages[0].content)
        let deltas = TextDeltas()
        let reply = try await client.reply(turnID: turnID, token: credential.token, messages: messages) {
            await deltas.append($0)
        }
        XCTAssertEqual(reply, "先把想法留下。", "真实 HTTP 分段正文不得丢字或乱码")
        let received = await deltas.values
        XCTAssertEqual(received, ["先把想法", "先把想法留下。"], "客户端必须逐段报告累计正文")
        _ = try await store.complete(turnID: turnID, text: reply)
        let reopened = CloudChatStore(databaseURL: database)
        let history = try await reopened.all()
        XCTAssertEqual(history.count, 2, "成功轮重启后必须仍保留用户与 Norma 两条消息")
        XCTAssertEqual(history.last?.text, reply, "落盘内容须与网关完整回复一致")
        let matches = try await reopened.search("留下")
        XCTAssertEqual(matches.map(\.text), [reply], "重开数据库后仍可搜索完整回复")
        do {
            _ = try await client.reply(turnID: turnID, token: credential.token, messages: messages) { _ in }
            XCTFail("相同 turn ID 不能再次得到模型回复")
        } catch CloudTextError.duplicateTurn { }

        let interruptedID = UUID().uuidString
        _ = try await store.appendUser(turnID: interruptedID, text: "模拟中断")
        do {
            _ = try await client.reply(turnID: interruptedID, token: credential.token,
                                       messages: [.init(role: "user", content: "模拟中断")]) { _ in }
            XCTFail("只有部分增量而缺少结束事件时不能交付完整回复")
        } catch CloudTextError.providerUnavailable {
            try await store.fail(turnID: interruptedID, code: "provider_unavailable")
        }
        let failedHistory = try await reopened.all()
        XCTAssertEqual(failedHistory.count, 3, "中断只增加用户原文，不增加虚假的 Norma 消息")
        XCTAssertEqual(failedHistory.last?.state, .failed, "失败回合必须可重试且保留明确状态")
        XCTAssertFalse(failedHistory.contains { $0.turnID == interruptedID && $0.role == .norma },
                       "半句回复不能作为完整历史保存")
        _ = try await admin("revoke", body: ["device_id": credential.deviceID])
        do {
            _ = try await client.reply(turnID: UUID().uuidString, token: credential.token, messages: messages) { _ in }
            XCTFail("撤销后的设备不能继续对话")
        } catch CloudTextError.revoked { }
    }
}

private actor TextDeltas {
    private(set) var values: [String] = []
    func append(_ text: String) { values.append(text) }
}
