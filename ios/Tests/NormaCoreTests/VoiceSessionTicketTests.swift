import XCTest
@testable import NormaCore

final class VoiceSessionTicketTests: XCTestCase {
    /// 子路径部署必须在同一挂载目录内换票、通话及连接Mac工具。
    /// 数据构造：now=1000，expires=1000+30=1030；16k输入/24k输出，工具token为256位的43字符编码。
    /// 执行过程：解析新服务地址及媒体/工具票据 → 改成根路径、相邻目录、查询和编码斜杠。
    /// 预期结果：两条合法通道可用；不能把短期票据发送到挂载目录外，也不能接受含秘密的URL。
    func testMountedGatewayKeepsMediaAndToolsInsideMount() throws {
        let endpoint = try XCTUnwrap(VoiceSessionTicket.endpoint("https://soulvoiceai.com/norma/voice/"), "服务地址应支持挂载前缀")
        XCTAssertEqual(endpoint.absoluteString, "https://soulvoiceai.com/norma/voice/v1/session", "换票不能跳到域名根目录")
        var wire: [String: Any] = ["provider":"doubao", "transport":"websocket", "token":"temporary-ticket",
            "expiresAt":1030, "url":"wss://soulvoiceai.com/norma/voice/v1/voice", "inputRate":16000, "outputRate":24000]
        func media() throws -> VoiceSessionTicket {
            try VoiceSessionTicket(data: JSONSerialization.data(withJSONObject: wire), endpoint: endpoint, provider: .doubao, now: 1000)
        }
        XCTAssertEqual(try media().url.path, "/norma/voice/v1/voice", "新部署媒体路径应通过校验")
        wire["url"] = "wss://soulvoiceai.com/v1/voice"
        XCTAssertThrowsError(try media(), "不能把新服务票据交给根目录中的其他服务")
        wire["url"] = "wss://soulvoiceai.com/norma/push/v1/voice"
        XCTAssertThrowsError(try media(), "同域名不同服务也应拒绝")
        var tool: [String: Any] = ["call_id":"doubao_A", "url":"wss://soulvoiceai.com/norma/voice/v1/tools",
            "token":String(repeating:"b",count:43), "expires_at":1030]
        func tools() throws -> RealtimeGatewayTicket {
            try RealtimeGatewayTicket(data: JSONSerialization.data(withJSONObject: tool), endpoint: endpoint, callID: "doubao_A", now: 1000)
        }
        XCTAssertEqual(try tools().url.path, "/norma/voice/v1/tools", "Mac工具通道必须使用同一挂载前缀")
        tool["url"] = "wss://soulvoiceai.com/v1/tools"
        XCTAssertThrowsError(try tools(), "不能转交根目录工具服务")
        XCTAssertNil(VoiceSessionTicket.endpoint("https://soulvoiceai.com/norma/voice?token=bad"), "服务地址不得含查询秘密")
        XCTAssertNil(VoiceSessionTicket.endpoint("https://soulvoiceai.com/norma%2Fvoice"), "拒绝代理可能不同解码的斜杠")
    }

    /// 豆包票据只能连接同源语音路径，拒绝过期、跨站和不匹配的音频协议。
    /// 数据构造：固定 now=1000 秒，expiresAt=1030 即 30 秒有效期；16k 输入、24k 输出。
    /// 执行过程：解码正常票据 → 逐项替换目标/过期时间/供应商/采样率 → 检查拒绝。
    /// 预期结果：有效票据保留 token 和 URL；任何边界错误都不能创建传输。
    func testDoubaoTicketValidatesOriginExpiryAndFormat() throws {
        let origin = URL(string: "https://voice.example/v1/session")!
        var wire: [String: Any] = ["provider": "doubao", "transport": "websocket", "token": "temporary-ticket",
                                  "expiresAt": 1030, "url": "wss://voice.example/v1/voice", "inputRate": 16000, "outputRate": 24000]
        func decode() throws -> VoiceSessionTicket {
            try VoiceSessionTicket(data: JSONSerialization.data(withJSONObject: wire), endpoint: origin, provider: .doubao, now: 1000)
        }
        let valid = try decode()
        XCTAssertEqual(valid.token, "temporary-ticket", "应使用网关发放的临时票据")
        XCTAssertEqual(valid.url.absoluteString, "wss://voice.example/v1/voice", "只能连接网关的语音路径")
        wire["url"] = "wss://other.example/v1/voice"
        XCTAssertThrowsError(try decode(), "不能把票据发到其他站点")
        wire["url"] = "wss://voice.example/v1/voice?token=leak"
        XCTAssertThrowsError(try decode(), "查询串不能携带会话秘密")
        wire["url"] = "wss://voice.example/v1/session"
        XCTAssertThrowsError(try decode(), "同源的其他路径也不是语音传输")
        wire["url"] = "wss://voice.example/v1/voice"; wire["expiresAt"] = 1000
        XCTAssertThrowsError(try decode(), "到期时刻不能继续使用票据")
        wire["expiresAt"] = 1030; wire["provider"] = "openai"
        XCTAssertThrowsError(try decode(), "请求豆包不能接受 GPT 票据")
        wire["provider"] = "doubao"; wire["inputRate"] = 24000
        XCTAssertThrowsError(try decode(), "上传采样率不匹配会造成语速错误，必须拒绝")
    }
}
