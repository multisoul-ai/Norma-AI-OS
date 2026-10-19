import AppKit
import CoreGraphics
import Foundation
import ImageIO
import Vision

public struct VisibleText {
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

/// Recover from Vision's occasional compute model failure without turning it into a product verdict.
func recoverVisionRecognition<T>(
    accurate: () throws -> T,
    fast: () throws -> T
) throws -> T {
    do {
        return try accurate()
    } catch let accurateError {
        do {
            return try fast()
        } catch let fastError {
            throw E2EError.env("macOS Vision 文字识别不可用；精确模式：\(accurateError)；快速模式：\(fastError)")
        }
    }
}

private func recognizeText(_ image: CGImage, level: VNRequestTextRecognitionLevel) throws -> [VNRecognizedTextObservation] {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = level
    request.recognitionLanguages = ["zh-Hans", "en-US"]
    request.usesLanguageCorrection = false
    try VNImageRequestHandler(cgImage: image).perform([request])
    return request.results ?? []
}

extension Session {
    public func capture() async throws -> Frame {
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
            let observations = try recoverVisionRecognition(
                accurate: { try recognizeText(image, level: .accurate) },
                fast: { try recognizeText(image, level: .fast) }
            )
            let texts = observations.compactMap { item -> VisibleText? in
                guard let value = item.topCandidates(1).first?.string else { return nil }
                let box = item.boundingBox
                return VisibleText(value: value, rect: CGRect(
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

    private func ensureCaptureEnvironment() throws {
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
