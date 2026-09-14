import XCTest
@testable import NormaCore

final class ChatVoiceDraftTests: XCTestCase {
    /// 一次按住录音在松开后才能合并转写，且原有文字草稿不会被覆盖。
    /// 数据构造：原草稿“检查登录”，转写“补充测试”；二者以一个换行分隔。
    /// 执行过程：按下 → 准备完成 → 录音中的结果被拒绝 → 松开 → 合并最终结果。
    /// 预期结果：得到完整的两段文字；录音中不提交，重复最终回调不重复插入。
    func testReleaseAppendsTranscriptExactlyOnce() throws {
        var voice = ChatVoiceDraft()
        let token = try XCTUnwrap(voice.begin(), "空闲时必须能开始一次录音")
        XCTAssertTrue(voice.didStart(token), "当前录音准备完成后应进入录音状态")
        XCTAssertNil(voice.complete("补充测试", token: token, draft: "检查登录"), "按住时不能提前修改草稿")
        XCTAssertEqual(voice.release(), token, "松开应结束准确的当前录音")
        XCTAssertEqual(voice.complete(" 补充测试\n", token: token, draft: "检查登录"), "检查登录\n补充测试", "转写必须追加并保留原草稿")
        XCTAssertEqual(voice.phase, .idle, "完成后不能停留在识别中")
        XCTAssertNil(voice.complete("补充测试", token: token, draft: "检查登录\n补充测试"), "迟到的重复回调不得再次追加")
    }

    /// 准备期间松开及取消后迟到的识别结果不得进入新录音或其他卡片。
    /// 数据构造：两次独立录音；第一条在权限准备阶段松开，第二条在录音阶段取消。
    /// 执行过程：开始第一条 → 提前松开 → 开始第二条 → 投递第一条回调 → 取消第二条。
    /// 预期结果：旧准备回调被拒绝；两条最终结果均不修改草稿；取消后空闲。
    func testReleaseDuringPreparationAndCancellationRejectLateResults() throws {
        var voice = ChatVoiceDraft()
        let first = try XCTUnwrap(voice.begin(), "第一条录音应能准备")
        XCTAssertNil(voice.release(), "尚未开始收音时松开不能提交空录音")
        let second = try XCTUnwrap(voice.begin(), "提前松开后必须能重试")
        XCTAssertFalse(voice.didStart(first), "旧权限回调不能启动新一轮麦克风")
        XCTAssertTrue(voice.didStart(second), "第二条应正常开始")
        XCTAssertNil(voice.complete("旧任务", token: first, draft: "保留"), "旧结果不能写入新草稿")
        voice.cancel()
        XCTAssertEqual(voice.phase, .idle, "取消必须结束录音状态")
        XCTAssertNil(voice.complete("取消的任务", token: second, draft: "保留"), "取消后不能接收迟到转写")
    }
}
