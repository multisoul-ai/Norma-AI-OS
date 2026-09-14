import XCTest
@testable import NormaCore

final class VoiceSessionTicketTests: XCTestCase {
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
