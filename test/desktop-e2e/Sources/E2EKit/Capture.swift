import AppKit
import CoreGraphics
import Foundation
import ImageIO
import ScreenCaptureKit
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

extension Session {
    public func capture() async throws -> Frame {
        try ensureRunning()
        try ensureCaptureEnvironment()
        let window = try await currentWindow()
        let png = FileManager.default.temporaryDirectory.appendingPathComponent("norma-e2e-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: png) }
        let shot = Process()
        shot.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        shot.arguments = ["-x", "-o", "-l", String(window.id), png.path]
        try shot.run()
        shot.waitUntilExit()
        guard shot.terminationStatus == 0,
              let source = CGImageSourceCreateWithURL(png as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            try ensureCaptureEnvironment()
            // A restarted app can publish its window before the first image is available.
            // The caller retries with a fresh window; missing permissions still stop immediately.
            throw E2EError.product("窗口\(window.id)图像尚不可用；截图权限有效，等待新窗口就绪")
        }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["zh-Hans", "en-US"]
        request.usesLanguageCorrection = false
        try VNImageRequestHandler(cgImage: image).perform([request])
        let texts = (request.results ?? []).compactMap { item -> VisibleText? in
            guard let value = item.topCandidates(1).first?.string else { return nil }
            let box = item.boundingBox
            return VisibleText(value: value, rect: CGRect(
                x: box.minX * window.frame.width,
                y: (1 - box.maxY) * window.frame.height,
                width: box.width * window.frame.width,
                height: box.height * window.frame.height
            ))
        }
        return Frame(windowID: window.id, windowFrame: window.frame, image: image, texts: texts)
    }

    private func ensureCaptureEnvironment() throws {
        guard CGPreflightScreenCaptureAccess() else {
            throw E2EError.env("屏幕录制权限已不可用，请允许 Norma Desktop E2E 后重跑。")
        }
        if let state = CGSessionCopyCurrentDictionary() as NSDictionary?,
           (state["CGSSessionScreenIsLocked"] as? NSNumber)?.boolValue == true {
            throw E2EError.env("屏幕已锁定，请解锁后重跑。")
        }
    }

    func currentWindow() async throws -> (id: CGWindowID, frame: CGRect) {
        try ensureRunning()
        guard let pid = app?.processIdentifier else { throw deathError() }
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        let windows = content.windows.filter {
            $0.owningApplication?.processID == pid && $0.frame.width > 400 && $0.frame.height > 300
        }
        guard let window = windows.max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }) else {
            throw E2EError.product("找不到 PID \(pid) 的 Norma 窗口")
        }
        return (CGWindowID(window.windowID), window.frame)
    }

    public func save(_ frame: Frame, _ name: String) throws {
        guard let output else { throw E2EError.env("运行目录尚未建立") }
        let image = NSBitmapImageRep(cgImage: frame.image)
        guard let data = image.representation(using: .png, properties: [:]) else {
            throw E2EError.product("无法编码截图 \(name)")
        }
        try data.write(to: output.appendingPathComponent(name))
        screenshotNames.append(name)
        print("截图 \(name)：\(frame.summary)")
    }
}
