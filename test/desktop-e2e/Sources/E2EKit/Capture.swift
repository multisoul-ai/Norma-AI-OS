import AppKit
import CoreGraphics
import CoreML
import Foundation
import ImageIO
import Vision

public struct VisibleText: Codable {
    public let value: String
    public let rect: CGRect

    public var center: CGPoint { CGPoint(x: rect.midX, y: rect.midY) }
}

public enum TextPick {
    case top, bottom, left, right
}

public struct Frame {
    public let windowID: CGWindowID
    public let windowFrame: CGRect
    public let image: CGImage
    public let pngData: Data
    public let texts: [VisibleText]

    public var summary: String {
        texts.map(\.value).joined(separator: " | ")
    }

    public func contains(_ needle: String) -> Bool {
        texts.contains { $0.value.localizedCaseInsensitiveContains(needle) }
    }

    public func satisfies(_ expect: Expectation) -> Bool {
        expect.satisfied(by: texts)
    }

    public func pick(_ candidates: [String], _ selection: TextPick) -> VisibleText? {
        let matches = texts.filter { text in
            candidates.contains { text.value.localizedCaseInsensitiveContains($0) }
        }
        switch selection {
        case .top: return matches.min { $0.center.y < $1.center.y }
        case .bottom: return matches.max { $0.center.y < $1.center.y }
        case .left: return matches.min { $0.center.x < $1.center.x }
        case .right: return matches.max { $0.center.x < $1.center.x }
        }
    }

    public func exact(_ value: String) -> VisibleText? {
        texts.first {
            $0.value.trimmingCharacters(in: .whitespacesAndNewlines)
                .localizedCaseInsensitiveCompare(value) == .orderedSame
        }
    }
}

/// A window may briefly stop producing a CGImage while macOS changes focus or redraws it.
/// A missing frame is retried; errors such as a vanished app still propagate immediately.
func retryCapture<T>(
    attempts: Int = 3,
    intervalNanoseconds: UInt64 = 120_000_000,
    operation: () async throws -> T?
) async throws -> T? {
    for attempt in 0..<attempts {
        if let result = try await operation() { return result }
        if attempt + 1 < attempts {
            try await Task.sleep(nanoseconds: intervalNanoseconds)
        }
    }
    return nil
}

/// Only accurate recognition can support presence/absence assertions. Fast results are diagnostic.
func recoverVisionRecognition<T>(
    accurate: () throws -> T,
    cpuAccurate: (() throws -> T)? = nil,
    isolatedAccurate: (() throws -> T)? = nil,
    record: (String, String) -> Void = { _, _ in },
    describe: (T) -> String = { String(describing: $0) },
    fast: () throws -> T
) throws -> T {
    var failures: [String] = []
    do {
        return try accurate()
    } catch {
        failures.append("精确模式：\(error)")
        record("accurate-failed", "\(error)")
    }
    if let cpuAccurate {
        do {
            let result = try cpuAccurate()
            record("cpu-accurate-recovered", describe(result))
            return result
        } catch {
            failures.append("CPU 精确模式：\(error)")
            record("cpu-accurate-failed", "\(error)")
        }
    }
    if let isolatedAccurate {
        do {
            let result = try isolatedAccurate()
            record("isolated-accurate-recovered", describe(result))
            return result
        } catch {
            failures.append("独立进程精确模式：\(error)")
            record("isolated-accurate-failed", "\(error)")
        }
    }
    do {
        failures.append("快速模式（仅诊断，不参与断言）：\(describe(try fast()))")
    } catch {
        failures.append("快速模式：\(error)")
    }
    let detail = failures.joined(separator: "；")
    record("recognition-unavailable", detail)
    throw E2EError.driver("macOS Vision 精确文字识别不可用；\(detail)")
}

func recognizeText(_ image: CGImage, level: VNRequestTextRecognitionLevel, cpuOnly: Bool = false) throws -> [VisibleText] {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = level
    request.recognitionLanguages = ["zh-Hans", "en-US"]
    request.usesLanguageCorrection = false
    if cpuOnly {
        for (stage, devices) in try request.supportedComputeStageDevices {
            guard let cpu = devices.first(where: { if case .cpu = $0 { return true }; return false }) else {
                throw E2EError.driver("Vision 的 \(stage) 阶段没有 CPU 计算设备")
            }
            request.setComputeDevice(cpu, for: stage)
        }
    }
    try VNImageRequestHandler(cgImage: image).perform([request])
    return (request.results ?? []).compactMap { item in
        guard let value = item.topCandidates(1).first?.string else { return nil }
        return VisibleText(value: value, rect: item.boundingBox)
    }
}

extension Session {
    public func capture() async throws -> Frame {
        guard !visualReview else { throw E2EError.driver("模型模式禁止隐式 OCR；请使用 capturePixels / visualCheck / visualClick") }
        try ensureRunning()
        try ensureCaptureEnvironment()
        var lastEmptyFrame: Frame?
        let captured = try await retryCapture { () async throws -> Frame? in
            let png = FileManager.default.temporaryDirectory.appendingPathComponent("norma-e2e-\(UUID().uuidString).png")
            defer { try? FileManager.default.removeItem(at: png) }
            let window = try controlWindow(try await control(["op": "capture", "path": png.path]))
            guard let source = CGImageSourceCreateWithURL(png as CFURL, nil),
                  let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
                  let pngData = try? Data(contentsOf: png) else {
                return nil
            }
            let observations: [VisibleText]
            do {
                observations = try recoverVisionRecognition(
                    accurate: { try recognizeText(image, level: .accurate) },
                    cpuAccurate: { try recognizeText(image, level: .accurate, cpuOnly: true) },
                    isolatedAccurate: { try isolatedAccurateRecognition(png: png) },
                    record: { mode, detail in self.recordDriverEvent("ocr", mode, detail) },
                    describe: { $0.map(\.value).joined(separator: " | ") },
                    fast: { try recognizeText(image, level: .fast) }
                )
            } catch {
                let diagnostic = Frame(windowID: window.id, windowFrame: window.frame, image: image, pngData: pngData, texts: [])
                do { try save(diagnostic, "failure-ocr-\(screenshotNames.count).png") }
                catch { recordDriverEvent("ocr", "failed-frame-save-failed", "\(error)") }
                throw error
            }
            let texts = observations.map { item in
                let box = item.rect
                return VisibleText(value: item.value, rect: CGRect(
                    x: box.minX * window.frame.width,
                    y: (1 - box.maxY) * window.frame.height,
                    width: box.width * window.frame.width,
                    height: box.height * window.frame.height
                ))
            }
            let frame = Frame(windowID: window.id, windowFrame: window.frame, image: image, pngData: pngData, texts: texts)
            if texts.isEmpty {
                lastEmptyFrame = frame
                return nil
            }
            return frame
        }
        if let captured { return captured }
        if let lastEmptyFrame { return lastEmptyFrame }
        throw E2EError.env("App 控制通道连续 3 次未能导出窗口画面。请检查隔离窗口是否可见。")
    }

    func ensureCaptureEnvironment() throws {
        if let state = CGSessionCopyCurrentDictionary() as NSDictionary?,
           (state["CGSSessionScreenIsLocked"] as? NSNumber)?.boolValue == true {
            throw E2EError.env("屏幕已锁定，请解锁后重跑。")
        }
    }

    /// 窗口坐标原点固定为 (0, 0)：点击、拖拽和截图都使用窗口内坐标。
    func currentWindow() async throws -> (id: CGWindowID, frame: CGRect) {
        let window = try controlWindow(try await control(["op": "window"]))
        return (window.id, window.frame)
    }

    public func save(_ frame: Frame, _ name: String) throws {
        guard let output else { throw E2EError.env("运行目录尚未建立") }
        try frame.pngData.write(to: output.appendingPathComponent(name))
        screenshotNames.append(name)
        print("截图 \(name)：\(frame.summary)")
    }
}
