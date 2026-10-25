import CryptoKit
import Foundation
import ImageIO

extension Session {
    /// 原始截图不调用 Vision；texts 为空，不能用于 present / absent 断言。
    public func capturePixels() async throws -> Frame {
        try ensureRunning()
        try ensureCaptureEnvironment()
        let captured = try await retryCapture { () async throws -> Frame? in
            let png = FileManager.default.temporaryDirectory.appendingPathComponent("norma-pixels-\(UUID()).png")
            defer { try? FileManager.default.removeItem(at: png) }
            let window = try controlWindow(try await control(["op": "capture", "path": png.path]))
            guard let data = try? Data(contentsOf: png),
                  let source = CGImageSourceCreateWithData(data as CFData, nil),
                  let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
            return Frame(windowID: window.id, windowFrame: window.frame, image: image, pngData: data, texts: [])
        }
        guard let captured else { throw E2EError.driver("连续 3 次无法导出窗口 PNG") }
        return captured
    }

    @discardableResult
    public func visualCheck(_ criteria: [VisualCriterion]) async throws -> Frame {
        let (frame, _) = try await reviewImage(kind: "check", criteria: criteria)
        visualCheckCount += 1
        return frame
    }

    /// 定位成功不是产品验收；场景随后必须检查动作后置状态。
    public func visualClick(_ instruction: String) async throws {
        let (frame, response) = try await reviewImage(kind: "locate", criteria: [VisualCriterion(id: "target", text: instruction)])
        guard let point = response.point else { throw E2EError.driver("模型未返回定位点") }
        recordDriverEvent("visual", "click", "requestID=\(response.requestID)")
        try await click(local: point, on: frame)
        try save(try await capturePixels(), "visual/\(response.requestID)-after-click.png")
    }

    private func reviewImage(kind: String, criteria: [VisualCriterion]) async throws -> (Frame, VisualResponse) {
        try await retryVisualReview { try await self.reviewImageOnce(kind: kind, criteria: criteria) }
    }

    private func reviewImageOnce(kind: String, criteria: [VisualCriterion]) async throws -> (Frame, VisualResponse)? {
        guard visualReview, let output else { throw E2EError.driver("模型看图模式未启用或运行目录未建立") }
        // 上一动作先绘制一帧；仍在加载时模型可返回 wait，不重放动作。
        try await Task.sleep(nanoseconds: 600_000_000)
        let frame = try await capturePixels()
        let directory = output.appendingPathComponent("visual", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let id = UUID().uuidString
        let name = "visual/\(id).png"
        try save(frame, name)
        let request = VisualRequest(id: id, screenshot: output.appendingPathComponent(name).path,
            sha256: SHA256.hash(data: frame.pngData).map { String(format: "%02x", $0) }.joined(), kind: kind,
            width: frame.windowFrame.width, height: frame.windowFrame.height,
            pixelWidth: frame.image.width, pixelHeight: frame.image.height, criteria: criteria)
        let requestURL = directory.appendingPathComponent("\(id).request.json")
        let responseURL = directory.appendingPathComponent("\(id).response.json")
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(request).write(to: requestURL, options: .atomic)
        recordDriverEvent("visual", "requested", requestURL.path)
        fflush(stdout)
        let response = try await awaitVisualResponse(at: responseURL, ensureRunning: { try self.ensureRunning() })
        recordDriverEvent("visual", "response", "id=\(id) reviewer=\(response.reviewer) " + response.findings.map { "\($0.id)=\($0.verdict): \($0.observation)" }.joined(separator: "；"))
        guard try validateVisualResponse(response, for: request) else { return nil }
        let window = try await currentWindow()
        try validateVisualWindow(id: window.id, size: window.frame.size, expectedID: frame.windowID, request: request)
        return (frame, response)
    }
}
