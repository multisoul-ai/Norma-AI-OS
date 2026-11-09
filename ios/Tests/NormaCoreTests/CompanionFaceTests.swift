import XCTest
@testable import NormaCore

final class CompanionFaceTests: XCTestCase {
    /// 待机和倾听时角色自己换表情，并缓慢转头；云端指定、说话、思考和减少动态效果优先。
    /// 数据构造：20 秒一轮，[0,4) 平静、[4,8) 好奇 0.7、[8,12) 平静、[12,16) 微笑 0.6、[16,20) 平静。
    ///   转头 yaw = sin(t×0.31)×0.05，pitch = sin(t×0.17)×0.018。t=2 时 yaw≈0.029052、pitch≈0.006003。
    /// 执行过程：采样周期边界、倾听、云端表情、说话、思考、用户说话、减少动态效果和手势占用。
    /// 预期结果：待机会在好奇和轻微笑之间换脸并转头；指定表情为满权重且不再叠加自主脸；减少动态效果和用户说话时脸与头都停住。
    func testIdleFaceChangesOnItsOwnAndYieldsToDirection() {
        let calm = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                         userSpeaking: false, reduceMotion: false, ornament: true, time: 2)
        XCTAssertEqual(calm.happy, 0, accuracy: 0.0001, "前 4 秒保持冷静，不自行微笑")
        XCTAssertEqual(calm.curious, 0, accuracy: 0.0001, "前 4 秒不自行抬眉")
        XCTAssertEqual(calm.concern, 0, accuracy: 0.0001, "自主待机不使用懊恼")
        XCTAssertEqual(calm.headYaw, 0.029052, accuracy: 0.0001, "t=2 的水平转头为 sin(0.62)×0.05")
        XCTAssertEqual(calm.headPitch, 0.006003, accuracy: 0.0001, "t=2 的点头为 sin(0.34)×0.018")

        let curious = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                            userSpeaking: false, reduceMotion: false, ornament: true, time: 4)
        XCTAssertEqual(curious.curious, 0.7, accuracy: 0.0001, "第 4 秒进入好奇")
        XCTAssertEqual(curious.happy, 0, accuracy: 0.0001, "好奇时不叠加微笑")
        let stillCurious = CompanionFace.resolve(phase: .listening, emotion: "neutral", directed: false,
                                                 userSpeaking: false, reduceMotion: false, ornament: true, time: 7.999)
        XCTAssertEqual(stillCurious.curious, 0.7, accuracy: 0.0001, "倾听与待机共用同一轮表情，8 秒前仍好奇")

        let settled = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                            userSpeaking: false, reduceMotion: false, ornament: true, time: 8)
        XCTAssertEqual(settled.curious, 0, accuracy: 0.0001, "第 8 秒好奇结束")
        XCTAssertEqual(settled.happy, 0, accuracy: 0.0001, "回到平静后没有残留微笑")

        let smile = CompanionFace.resolve(phase: .listening, emotion: "neutral", directed: false,
                                          userSpeaking: false, reduceMotion: false, ornament: true, time: 14)
        XCTAssertEqual(smile.happy, 0.6, accuracy: 0.0001, "第 14 秒是看得见的微笑，仍低于指定表情的 1")
        XCTAssertEqual(smile.curious, 0, accuracy: 0.0001, "微笑时不抬眉")
        XCTAssertLessThan(abs(smile.headYaw), 0.06, "转头保持在约 3 度以内")

        let rest = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                         userSpeaking: false, reduceMotion: false, ornament: true, time: 16)
        XCTAssertEqual(rest.happy, 0, accuracy: 0.0001, "第 16 秒微笑结束")
        let wrapped = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                            userSpeaking: false, reduceMotion: false, ornament: true, time: 20)
        XCTAssertEqual(wrapped.happy, 0, accuracy: 0.0001, "20 秒后回到平静，不把转头周期当成表情")
        XCTAssertEqual(wrapped.curious, 0, accuracy: 0.0001, "新一轮开始时不残留好奇")

        let directed = CompanionFace.resolve(phase: .idle, emotion: "happy", directed: true,
                                             userSpeaking: false, reduceMotion: false, ornament: true, time: 14)
        XCTAssertEqual(directed.happy, 1, accuracy: 0.0001, "云端高兴覆盖自主微笑")
        XCTAssertEqual(directed.curious, 0, accuracy: 0.0001, "指定高兴时不混入好奇")
        XCTAssertEqual(directed.headYaw, 0, accuracy: 0.0001, "指定表情时不再自行转头")
        XCTAssertEqual(directed.headPitch, 0, accuracy: 0.0001, "指定表情时不再自行点头")

        let concern = CompanionFace.resolve(phase: .idle, emotion: "concerned", directed: true,
                                            userSpeaking: false, reduceMotion: false, ornament: true, time: 14)
        XCTAssertEqual(concern.concern, 1, accuracy: 0.0001, "指定懊恼保持满权重")
        XCTAssertEqual(concern.happy, 0, accuracy: 0.0001, "懊恼不退回自主微笑")

        let heldNeutral = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: true,
                                                userSpeaking: false, reduceMotion: false, ornament: true, time: 14)
        XCTAssertEqual(heldNeutral.happy, 0, accuracy: 0.0001, "被要求保持平静时，不在微笑时段自行笑")

        let speaking = CompanionFace.resolve(phase: .speaking, emotion: "neutral", directed: true,
                                             userSpeaking: false, reduceMotion: false, ornament: true, time: 14)
        XCTAssertEqual(speaking.happy, 0.25, accuracy: 0.0001, "中性说话只用轻一点的投入表情")
        XCTAssertNotEqual(speaking.happy, 0.6, "说话时不走待机微笑")
        XCTAssertEqual(speaking.headYaw, 0, accuracy: 0.0001, "说话的头部交给动作片段")

        let thinking = CompanionFace.resolve(phase: .thinking, emotion: "neutral", directed: false,
                                             userSpeaking: false, reduceMotion: false, ornament: true, time: 6)
        XCTAssertEqual(thinking.curious, 0.35, accuracy: 0.0001, "思考使用固定的轻微好奇")
        XCTAssertNotEqual(thinking.curious, 0.7, "思考不走待机好奇幅度")

        let listening = CompanionFace.resolve(phase: .idle, emotion: "happy", directed: true,
                                              userSpeaking: true, reduceMotion: false, ornament: true, time: 14)
        XCTAssertEqual(listening, .still, "用户说话时她停住听，不继续笑或转头")

        let reduced = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                            userSpeaking: false, reduceMotion: true, ornament: true, time: 14)
        XCTAssertEqual(reduced, .still, "减少动态效果去掉自主表情和转头")
        let reducedDirected = CompanionFace.resolve(phase: .idle, emotion: "curious", directed: true,
                                                    userSpeaking: false, reduceMotion: true, ornament: true, time: 10)
        XCTAssertEqual(reducedDirected.curious, 1, accuracy: 0.0001, "减少动态效果仍保留被指定的表情")
        XCTAssertEqual(reducedDirected.headYaw, 0, accuracy: 0.0001, "减少动态效果不转头")

        let gesture = CompanionFace.resolve(phase: .idle, emotion: "neutral", directed: false,
                                            userSpeaking: false, reduceMotion: false, ornament: false, time: 2)
        XCTAssertEqual(gesture.headYaw, 0, accuracy: 0.0001, "挥手、点头或摸头时不再叠加转头")
        XCTAssertEqual(gesture.headPitch, 0, accuracy: 0.0001, "手势进行时头部只留给这个手势")
    }
}
