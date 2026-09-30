import XCTest
@testable import Norma
@testable import NormaCore

final class CloudChatSendingTests: XCTestCase {
    /// 连点发送只创建一个云回合，落盘期间新输入的下一条草稿不能被清空。
    /// 数据构造：一个隔离数据库/Keychain、2 次同时发送；固定 503 HTTP 替身，不访问公网、不调用模型。
    /// 执行过程：加载设备 → 主队列安排两次发送和一次草稿编辑 → 等待网络失败 → 读取真实 SQLite。
    /// 预期结果：仅 1 条用户原文且为失败；无 Norma 回复；下一条草稿保留，可继续编辑。
    @MainActor func testRepeatedSendKeepsNextDraftAndOnlyOneTurn() async throws {
        let (model, store) = try await makeModel()
        await model.load()
        model.draft = "先梳理首屏"
        let first = Task { await model.send() }
        let repeated = Task { await model.send() }
        let editing = Task { model.draft = "接着讨论导航" }
        await first.value; await repeated.value; await editing.value
        let messages = try await store.all()
        XCTAssertEqual(messages.count, 1, "连点不得在首次持久化等待期间创建第二个回合")
        XCTAssertEqual(messages.first?.text, "先梳理首屏", "失败后原文仍须保留")
        XCTAssertEqual(messages.first?.state, .failed, "503 必须终止发送状态")
        XCTAssertFalse(messages.contains { $0.role == .norma }, "失败不能伪造 Norma 回答")
        XCTAssertEqual(model.draft, "接着讨论导航", "持久化前后新增输入不能被旧回合清空")
        XCTAssertFalse(model.sending, "请求结束后必须释放发送状态")
    }

    /// 首页启动和邀请深链同时加载时，只恢复一次上次未完成的回合。
    /// 数据构造：1 条 pending 原文、1 个有效测试设备令牌；两个 load 同时进入。
    /// 执行过程：写 pending → 两次加载 → 重读数据库与模型错误。
    /// 预期结果：pending 变 failed 且只有一条；不因重复恢复出现 invalidTurn，也不产生云端调用。
    @MainActor func testConcurrentLoadRecoversInterruptedTurnOnce() async throws {
        let (model, store) = try await makeModel()
        _ = try await store.appendUser(turnID: UUID().uuidString, text: "上次没聊完")
        let first = Task { await model.load() }
        let second = Task { await model.load() }
        await first.value; await second.value
        let messages = try await store.all()
        XCTAssertTrue(model.loaded, "并发恢复后历史应可读")
        XCTAssertNil(model.error, "重复加载不应对同一回合执行两次失败恢复")
        XCTAssertEqual(messages.map(\.text), ["上次没聊完"], "不能丢失或复制恢复的原文")
        XCTAssertEqual(messages.first?.state, .failed, "中断回合须转为失败")
        XCTAssertFalse(messages.contains { $0.state == .pending }, "恢复后不能残留虚假的发送中状态")
    }

    @MainActor private func makeModel() async throws -> (CloudChatModel, CloudChatStore) {
        let id = UUID().uuidString
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(id)
        let store = CloudChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
        let vault = CloudCredentialVault(service: "com.norma-ai.os.ios.cloud.send-tests.\(id)")
        let credential = try JSONDecoder().decode(CloudDeviceCredential.self, from: JSONSerialization.data(withJSONObject: [
            "deviceID": id, "token": String(repeating: "A", count: 43), // 32 随机字节的 base64url 长度为 ceil(256/6) = 43。
        ]))
        try await vault.save(credential)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [CloudUnavailableProtocol.self]
        let session = URLSession(configuration: configuration)
        let client = CloudTextClient(origin: URL(string: "https://cloud-test.invalid/")!, session: session)
        addTeardownBlock {
            session.invalidateAndCancel()
            try await vault.remove()
            try FileManager.default.removeItem(at: directory)
        }
        return (CloudChatModel(store: store, vault: vault, client: client), store)
    }
}

private final class CloudUnavailableProtocol: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        guard let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: 503, httpVersion: nil,
                                             headerFields: ["Content-Type": "application/json"]) else { return }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(#"{"error":"provider_unavailable"}"#.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}
