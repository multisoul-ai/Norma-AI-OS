import XCTest
@testable import NormaCore

final class CloudTextStreamTests: XCTestCase {
    #if DEBUG
    /// 本地网关配置必须与隔离命名空间同时提供，错误配置不能静默连正式服务。
    /// 数据构造：端口 58477（合法非零 UInt16）；0 与 65536 = UInt16.max + 1 为边界反例；1 个 UUID 隔离空间。
    /// 执行过程：读取空配置、合法配置、缺命名空间、越界端口、伪 URL 端口配置。
    /// 预期结果：默认无覆盖；合法配置只含回环端口及隔离身份；所有非法显式覆盖均报错。
    func testLocalDevelopmentRequiresIsolatedLoopbackConfiguration() throws {
        let namespace = UUID().uuidString
        XCTAssertNil(try CloudTextClient.developmentSettings(in: [:]), "普通启动必须保持正式地址")
        let settings = try XCTUnwrap(CloudTextClient.developmentSettings(in: [
            "NORMA_CLOUD_LOCAL_PORT": "58477", "NORMA_CLOUD_TEST_NAMESPACE": namespace,
        ]))
        XCTAssertEqual(settings.port, 58477, "本地联调端口必须精确保留")
        XCTAssertEqual(settings.namespace, namespace, "本地凭据和数据库必须使用隔离身份")
        XCTAssertThrowsError(try CloudTextClient.developmentSettings(in: ["NORMA_CLOUD_LOCAL_PORT": "58477"]),
                             "没有隔离空间不能用正式历史或凭据访问本机服务")
        for port in ["0", "65536", "https://example.com", "", "-1"] {
            XCTAssertThrowsError(try CloudTextClient.developmentSettings(in: [
                "NORMA_CLOUD_LOCAL_PORT": port, "NORMA_CLOUD_TEST_NAMESPACE": namespace,
            ]), "非法端口 \(port) 必须显式失败，不能接受任意目标或回退正式服务")
        }
        XCTAssertThrowsError(try CloudTextClient(localPort: 0), "底层本地客户端也必须拒绝零端口")
        XCTAssertEqual(CloudTextClient.serviceOrigin.absoluteString, "https://soulvoiceai.com/norma/voice/",
                       "本地配置不能改写正式服务基址")
    }
    #endif

    /// 文字流只在匹配当前回合的 done 后完成，途中断开或服务端报错不能变成完整回复。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 个 UUID 回合；两段增量「你好」+「，我在。」= 完整回复「你好，我在。」。
    ///   另一个 UUID 的 done 与空流是反例，均不能让当前回合完成。
    ///
    /// 执行过程：依次读 start、两条 delta、done；重新建解析器只读 start/delta；再输入错误和错回合 done。
    ///
    /// 预期结果：完整回复准确拼接；无 done 的部分文字不能提交；错身份和错误事件被拒绝。
    func testTextStreamRequiresMatchingFinalEvent() throws {
        let turnID = "00000000-0000-0000-0000-000000000001"
        var stream = CloudTextStream(turnID: turnID)
        try stream.consume(#"{"type":"start","turn_id":"00000000-0000-0000-0000-000000000001"}"#)
        try stream.consume(#"{"type":"delta","text":"你好"}"#)
        try stream.consume(#"{"type":"delta","text":"，我在。"}"#)
        XCTAssertEqual(stream.text, "你好，我在。", "增量必须按到达顺序拼接且不丢字")
        XCTAssertFalse(stream.completed, "只有收到终止事件后才能保存完整云回复")
        try stream.consume(#"{"type":"done","turn_id":"00000000-0000-0000-0000-000000000001"}"#)
        XCTAssertTrue(stream.completed, "匹配当前 turn ID 的 done 必须完成回复")

        var interrupted = CloudTextStream(turnID: turnID)
        try interrupted.consume(#"{"type":"start","turn_id":"00000000-0000-0000-0000-000000000001"}"#)
        try interrupted.consume(#"{"type":"delta","text":"半句"}"#)
        XCTAssertFalse(interrupted.completed, "断线前只有半句时不能展示为完整回答")
        XCTAssertThrowsError(try interrupted.consume(#"{"type":"done","turn_id":"00000000-0000-0000-0000-000000000002"}"#),
                             "另一轮的 done 不能完成当前回复")
        XCTAssertThrowsError(try interrupted.consume(#"{"type":"error","code":"provider_unavailable"}"#),
                             "云服务错误必须显式失败而不能提交半句")
    }
}
