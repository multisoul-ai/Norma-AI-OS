import CoreGraphics
import Foundation

public struct VisualCriterion: Codable {
    public let id: String
    public let text: String
    public init(id: String, text: String) { self.id = id; self.text = text }
}

struct VisualRequest: Encodable {
    let id: String
    let screenshot: String
    let sha256: String
    let kind: String
    let width: Double
    let height: Double
    let pixelWidth: Int
    let pixelHeight: Int
    let criteria: [VisualCriterion]
    let coordinateSpace = "window-points-top-left"
}

struct VisualFinding: Codable {
    let id: String
    var verdict: String
    let observation: String
}

struct VisualResponse: Codable {
    var requestID: String
    var screenshotSHA256: String
    let reviewer: String
    var findings: [VisualFinding]
    var point: CGPoint?
}

/// 每项判断只对当前 PNG 有效；wait 仅请求新截图，不算通过。
@discardableResult
func validateVisualResponse(_ response: VisualResponse, for request: VisualRequest) throws -> Bool {
    guard response.requestID == request.id, response.screenshotSHA256 == request.sha256 else {
        throw E2EError.driver("模型响应与当前截图不匹配（requestID / SHA256）")
    }
    let expected = request.criteria.map(\.id)
    guard ["check", "locate"].contains(request.kind), !expected.isEmpty,
          Set(expected).count == expected.count,
          response.findings.map(\.id).sorted() == expected.sorted(),
          !response.reviewer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
          response.findings.allSatisfy({ !$0.observation.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) else {
        throw E2EError.driver("模型响应必须逐项提供唯一结论、观察理由和 reviewer")
    }
    guard response.findings.allSatisfy({ ["pass", "fail", "uncertain", "wait"].contains($0.verdict) }) else {
        throw E2EError.driver("模型 verdict 必须为 pass / fail / uncertain / wait")
    }
    if let failed = response.findings.first(where: { $0.verdict == "fail" }) {
        throw E2EError.product("模型看图失败 [\(failed.id)]：\(failed.observation)")
    }
    if let uncertain = response.findings.first(where: { $0.verdict == "uncertain" }) {
        throw E2EError.driver("模型无法判断 [\(uncertain.id)]：\(uncertain.observation)")
    }
    if response.findings.contains(where: { $0.verdict == "wait" }) {
        guard response.point == nil else { throw E2EError.driver("尚未就绪不能附带输入坐标") }
        return false
    }
    if request.kind == "locate" {
        guard let point = response.point, point.x.isFinite, point.y.isFinite,
              point.x >= 0, point.x < request.width, point.y >= 0, point.y < request.height else {
            throw E2EError.driver("模型定位缺少窗口点坐标或坐标越界")
        }
    } else if response.point != nil {
        throw E2EError.driver("视觉检查不能附带输入坐标")
    }
    return true
}

func validateVisualWindow(id: CGWindowID, size: CGSize, expectedID: CGWindowID, request: VisualRequest) throws {
    guard id == expectedID, abs(size.width - request.width) <= 2, abs(size.height - request.height) <= 2 else {
        throw E2EError.driver("模型判断期间窗口已切换或改变尺寸，旧截图不能用于输入或验收")
    }
}

/// 只重新观察；闭包不包含输入或 Present。明确失败不重试，持续 loading 有三次预算。
func retryVisualReview<T>(intervalNanoseconds: UInt64 = 600_000_000, observe: () async throws -> T?) async throws -> T {
    for attempt in 0..<3 {
        if let value = try await observe() { return value }
        if attempt < 2 { try await Task.sleep(nanoseconds: intervalNanoseconds) }
    }
    throw E2EError.driver("模型连续三次观察仍未就绪；未重放任何动作")
}

/// 当前多模态 Agent 原子写入响应；没有网络调用或默认通过分支。
func awaitVisualResponse(at url: URL, timeout: TimeInterval = 300, ensureRunning: () throws -> Void) async throws -> VisualResponse {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        try Task.checkCancellation()
        try ensureRunning()
        if FileManager.default.fileExists(atPath: url.path) {
            do { return try JSONDecoder().decode(VisualResponse.self, from: Data(contentsOf: url)) }
            catch { throw E2EError.driver("模型响应 JSON 无效：\(error)") }
        }
        try await Task.sleep(nanoseconds: 200_000_000)
    }
    throw E2EError.driver("等待模型看图响应超时（\(timeout)s）：\(url.path)")
}
