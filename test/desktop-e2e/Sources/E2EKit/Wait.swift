import Foundation

public struct Expectation {
    public var anyOf: [String]
    public var allOf: [String]
    public var absent: [String]
    public var timeout: TimeInterval

    public init(anyOf: [String] = [], allOf: [String] = [], absent: [String] = [], timeout: TimeInterval = 5) {
        self.anyOf = anyOf
        self.allOf = allOf
        self.absent = absent
        self.timeout = timeout
    }

    public func satisfied(by texts: [VisibleText]) -> Bool {
        if anyOf.isEmpty && allOf.isEmpty && absent.isEmpty { return false }
        func hit(_ needle: String) -> Bool {
            texts.contains { $0.value.localizedCaseInsensitiveContains(needle) }
        }
        if !anyOf.isEmpty && !anyOf.contains(where: hit) { return false }
        if !allOf.allSatisfy(hit) { return false }
        if absent.contains(where: hit) { return false }
        return true
    }
}

extension Session {
    public func wait(_ expect: Expectation, saveFailure: Bool = true) async throws -> Frame {
        let deadline = Date().addingTimeInterval(expect.timeout)
        var last: Frame?
        var lastError = ""
        while Date() < deadline {
            do {
                let frame = try await capture()
                last = frame
                if frame.satisfies(expect) { return frame }
            } catch let error as E2EError where error.kind == .env {
                throw error
            } catch {
                lastError = "\(error)"
                if app?.isRunning != true { throw deathError() }
            }
            try await Task.sleep(nanoseconds: 250_000_000)
        }
        if saveFailure, let last {
            let name = "failure-\(Int(Date().timeIntervalSince1970)).png"
            try? save(last, name)
            print("可见文字：\(last.summary)")
        }
        let wanted = (expect.allOf + expect.anyOf).joined(separator: " / ")
        let blocked = expect.absent.joined(separator: " / ")
        throw E2EError.product("等待画面超时。需要出现[\(wanted)]，需要消失[\(blocked)]。可见：\(last?.summary ?? lastError)")
    }

    /// 动作前先看期望是否已经成立；否则执行动作并等待。点击类动作应在闭包内重新 OCR。
    @discardableResult
    public func act(
        _ name: String,
        attempts: Int = 4,
        expect: Expectation,
        action: () async throws -> Void
    ) async throws -> Frame {
        try await retryAction(name, attempts: attempts, peek: {
            let frame = try await peek()
            return frame.satisfies(expect) ? frame : nil
        }, action: action, wait: { lastAttempt in
            try await wait(expect, saveFailure: lastAttempt)
        }, record: { attempt, stage, detail in
            recordDriverEvent("action", stage, "\(name) attempt=\(attempt): \(detail)")
        })
    }

    private func peek() async throws -> Frame {
        do { return try await capture() }
        catch let error as E2EError where error.kind == .env { throw error }
        catch {
            if app?.isRunning != true { throw deathError() }
            throw error
        }
    }
}

/// The retry boundary joins an observed precondition, actual input, and its postcondition.
@MainActor
func retryAction<Result>(
    _ name: String,
    attempts: Int,
    peek: () async throws -> Result?,
    action: () async throws -> Void,
    wait: (Bool) async throws -> Result,
    record: (Int, String, String) -> Void = { _, _, _ in }
) async throws -> Result {
    var last: Error?
    for attempt in 1...attempts {
        do {
            if let ready = try await peek() { return ready }
        } catch {
            record(attempt, "observation-failed", "\(error)")
            if let failure = error as? E2EError, failure.kind == .env {
                record(attempt, "action-not-started", "本次 actionInvoked=false；\(error)")
                throw error
            }
            last = error
        }
        do {
            try await action()
            record(attempt, "action-completed", "动作已完成，开始核对后置状态")
        } catch {
            record(attempt, "action-failed", "\(error)")
            if let failure = error as? E2EError, failure.kind == .env { throw error }
            last = error
            continue
        }
        do {
            return try await wait(attempt == attempts)
        } catch {
            record(attempt, "postcondition-failed", "\(error)")
            if let failure = error as? E2EError, failure.kind == .env { throw error }
            last = error
        }
    }
    throw last ?? E2EError.product("\(name) 在 \(attempts) 次尝试后仍未达到期望")
}
