import XCTest
@testable import NormaCore

final class RealtimeCallTests: XCTestCase {
    /// SDP Location 只接受固定 OpenAI 会话路径，不能把外站或函数调用标识当作媒体会话。
    /// 数据构造：同一 rtc_A-1 会话的绝对/相对路径，以及外站、额外路径和查询串对照。
    /// 执行过程：逐一解析 → 检查媒体会话 ID → 拒绝不属于 API 的位置。
    /// 预期结果：两个合法 Location 解析一致，外站、额外路径与查询串均不生成绑定。
    func testCallLocationRequiresExactAPIOriginAndPath() {
        XCTAssertEqual(RealtimeCallLocation.callID("https://api.openai.com/v1/realtime/calls/rtc_A-1"), "rtc_A-1", "绝对位置应保留媒体会话身份")
        XCTAssertEqual(RealtimeCallLocation.callID("/v1/realtime/calls/rtc_A-1"), "rtc_A-1", "相对位置应绑定相同会话")
        XCTAssertNil(RealtimeCallLocation.callID("https://evil.example/v1/realtime/calls/rtc_A-1"), "外站位置不能获得 Mac 绑定")
        XCTAssertNil(RealtimeCallLocation.callID("/v1/realtime/calls/rtc_A-1/other"), "多余路径不能被截断成合法会话")
        XCTAssertNil(RealtimeCallLocation.callID("/v1/realtime/calls/rtc_A-1?key=value"), "查询串不能进入会话标识")
        XCTAssertNil(RealtimeCallLocation.callID("function_call_A"), "函数调用 ID 不能作为媒体 Location")
    }
}
