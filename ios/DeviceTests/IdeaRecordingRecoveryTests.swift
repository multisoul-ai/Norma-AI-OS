import AVFoundation
import SwiftUI
import XCTest
@testable import Norma

@MainActor final class IdeaRecordingRecoveryTests: XCTestCase {
    /// 波形只随真实录音电平变化，待录音及减少动态效果下不产生伪语音跳动。
    ///
    /// 数据构造：宽 350pt = iPhone 390pt - 两侧各 20pt；高 90pt 沿用 P02 波形区域。
    ///   电平 0 表示静音、1 表示采集器归一化上限；每张图使用同尺寸和 1x 比例。
    /// 执行过程：渲染录音中的低/高电平，再渲染减少动态效果和待录音的同两种电平。
    /// 预期结果：录音高低电平像素不同；减少动态效果和待录音像素不随电平改变。
    func testWaveformRespondsToLevelAndHonorsReducedMotion() throws {
        func render(_ level: Float, recording: Bool = true, reduced: Bool = false) throws -> Data {
            let view = IdeaVoiceWaveform(recording: recording, level: level, reduceMotion: reduced)
                .frame(width: 350, height: 90)
            let renderer = ImageRenderer(content: view)
            renderer.scale = 1
            return try XCTUnwrap(renderer.uiImage?.pngData(), "波形必须能由系统渲染器生成原生图像")
        }
        XCTAssertNotEqual(try render(0), try render(1), "真实录音波形必须响应音量变化")
        XCTAssertEqual(try render(0, reduced: true), try render(1, reduced: true),
                       "减少动态效果时不能继续按音量缩放波形")
        XCTAssertEqual(try render(0, recording: false), try render(1, recording: false),
                       "等待录音不能展示仿造的语音活动")
    }

    /// 崩溃或保存失败后留下的本地 CAF 可恢复为待转写录音，明确放弃才删除。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1600 帧 / 16000 Hz = 0.1 秒单声道静音 CAF；目录是单次测试专属临时位置。
    ///
    /// 执行过程：写录音文件 → 新建采集器并恢复文件 → 检查时长与路径 → 明确放弃。
    ///
    /// 预期结果：恢复不启动麦克风而进入已停止状态；音频在放弃前存在，放弃后消失。
    func testInterruptedRecordingCanBeRecoveredWithoutMicrophone() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("interrupted.caf")
        let format = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: 16_000, channels: 1))
        let file = try AVAudioFile(forWriting: url, settings: format.settings)
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 1_600))
        buffer.frameLength = 1_600
        buffer.floatChannelData?[0].update(repeating: 0, count: 1_600)
        try file.write(from: buffer)

        let capture = IdeaVoiceCapture(directory: directory)
        try capture.recover(url)
        XCTAssertEqual(capture.phase, .stopped, "恢复录音后不应重新打开麦克风")
        XCTAssertEqual(capture.recordingURL, url, "恢复必须指向原始录音文件")
        XCTAssertEqual(capture.duration, 0.1, accuracy: 0.01, "1600帧录音应恢复约0.1秒时长")
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path), "用户未明确放弃前不得删除录音")
        capture.discard()
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path), "明确放弃后才删除未保存录音")
    }
}
