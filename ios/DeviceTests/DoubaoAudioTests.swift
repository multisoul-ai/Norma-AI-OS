import XCTest
@testable import Norma

final class DoubaoAudioTests: XCTestCase {
    /// 麦克风真实批量数据必须切成 20ms 帧，不重复、不丢尾部采样。
    /// 数据构造：16k × 2bytes × 20ms = 640bytes；3232bytes 一批产生 5 帧并留下 32bytes。
    /// 执行过程：输入第一批 → 输入 608bytes 补足余帧 → 拼接检查原始 PCM。
    /// 预期结果：总共 6 帧/3840bytes，残余清空，重组与原输入完全一致。
    func testCapturePacketizationPreservesSamplesAcrossBatches() {
        var pending = Data(), frames: [Data] = []
        let first = Data(repeating: 17, count: 3232)
        let second = Data(repeating: 29, count: 608)
        AudioDuplex.packetize(first, pending: &pending) { frames.append($0) }
        XCTAssertEqual(frames.count, 5, "第一次批量回调只能生成五个完整帧")
        XCTAssertEqual(pending.count, 32, "不足20ms的采样必须保留")
        AudioDuplex.packetize(second, pending: &pending) { frames.append($0) }
        XCTAssertEqual(frames.count, 6, "第二批应补全第六帧")
        XCTAssertEqual(frames.reduce(Data(), +), first + second, "分帧不能重复或丢失音频")
        XCTAssertTrue(pending.isEmpty, "恰好完整的音频不能残留到下一段")
        XCTAssertFalse(frames.contains { $0.count != 640 }, "网关只能收到20ms完整帧")
    }
}
