import Foundation

public struct E2EError: Error, CustomStringConvertible {
    public enum Kind: String {
        case env
        case product
    }

    public let kind: Kind
    public let message: String
    public let layer: String

    public init(_ kind: Kind, _ message: String, layer: String? = nil) {
        self.kind = kind
        self.message = message
        self.layer = layer ?? (kind == .env ? "environment" : "product")
    }

    public var description: String { "[\(kind.rawValue.uppercased())] \(message)" }

    public static func env(_ message: String) -> E2EError { E2EError(.env, message) }
    public static func product(_ message: String) -> E2EError { E2EError(.product, message) }
    public static func driver(_ message: String) -> E2EError { E2EError(.env, message, layer: "driver") }
}

struct StepRecord {
    var name: String
    var passed: Bool
    var seconds: Double
    var detail: String
}

extension Session {
    /// Keep recovery causes and input acknowledgements in both the console and final report.
    func recordDriverEvent(_ category: String, _ stage: String, _ detail: String) {
        driverEvents.append([
            "seconds": Date().timeIntervalSince(started), "category": category,
            "stage": stage, "detail": detail,
        ])
        print("DRIVER \(category) \(stage): \(detail)")
    }

    func record(_ name: String, _ passed: Bool, since start: Date, _ detail: String) {
        let seconds = Date().timeIntervalSince(start)
        steps.append(StepRecord(name: name, passed: passed, seconds: seconds, detail: detail))
        let mark = passed ? "PASS" : "FAIL"
        print("\(mark) \(name) \(String(format: "%.2f", seconds))s \(detail)")
    }

    @discardableResult
    public func step(_ name: String, _ body: () async throws -> String) async throws -> String {
        let start = Date()
        do {
            let detail = try await body()
            record(name, true, since: start, detail)
            return detail
        } catch {
            record(name, false, since: start, "\(error)")
            throw error
        }
    }

    public func finishSuccess() throws {
        try writeReport(result: "pass", kind: nil, message: "场景通过")
        let seconds = Date().timeIntervalSince(started)
        print("PASS \(scenario) \(String(format: "%.2f", seconds))s 证据 \(output?.path ?? artifactRoot.path)")
    }

    public func finishFailure(_ error: Error) {
        let kind = (error as? E2EError)?.kind ?? .product
        try? writeReport(result: "fail", kind: kind, message: "\(error)", layer: (error as? E2EError)?.layer)
        fputs("FAIL \(scenario): \(error)\n", stderr)
        if let output {
            fputs("证据 \(output.path)\n", stderr)
        }
    }

    func writeReport(result: String, kind: E2EError.Kind?, message: String, layer: String? = nil) throws {
        let root = output ?? artifactRoot
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let seconds = Date().timeIntervalSince(started)
        var payload: [String: Any] = [
            "scenario": scenario,
            "result": result,
            "seconds": seconds,
            "message": message,
            "screenshots": screenshotNames,
            "driverEvents": driverEvents,
            "steps": steps.map { step in
                [
                    "name": step.name,
                    "passed": step.passed,
                    "seconds": step.seconds,
                    "detail": step.detail,
                ]
            },
        ]
        if let kind { payload["kind"] = kind.rawValue }
        if let layer { payload["failureLayer"] = layer }
        if let output { payload["output"] = output.path }
        if let dataRoot { payload["dataRoot"] = dataRoot.path }
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: root.appendingPathComponent("report.json"))
    }

    func tailLog(maxLines: Int = 30) -> String {
        guard let output else { return "" }
        let url = output.appendingPathComponent("app.log")
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return "" }
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false).suffix(maxLines)
        return lines.joined(separator: "\n")
    }
}
