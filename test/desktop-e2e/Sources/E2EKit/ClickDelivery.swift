import CoreGraphics
import Foundation

/// Resolve against a fresh frame before sending any input; transport completion means ACK.
@MainActor
func deliverClick(
    resolve: () async throws -> CGPoint,
    send: (String, CGPoint) async throws -> Void,
    pause: (UInt64) async throws -> Void = { try await Task.sleep(nanoseconds: $0) },
    record: (String, String) -> Void = { _, _ in }
) async throws {
    let point: CGPoint
    do {
        point = try await resolve()
    } catch {
        record("not-sent", "mouseRequests=0；\(error)")
        throw error
    }
    var acknowledged: [String] = []
    for (phase, delay) in [("move", UInt64(160_000_000)), ("down", 40_000_000), ("up", 0)] {
        let target = "phase=\(phase) x=\(point.x) y=\(point.y)"
        record("dispatch-attempted", target)
        do {
            try await send(phase, point)
        } catch {
            let detail = "\(target) delivery=unknown acknowledged=\(acknowledged)；原始错误：\(error)"
            record("delivery-unknown", detail)
            if let failure = error as? E2EError, failure.kind == .product { throw failure }
            throw E2EError.driver("点击控制通道未确认，不能判断本次请求是否已送达；\(detail)")
        }
        acknowledged.append(phase)
        record("acknowledged", target)
        if delay > 0 {
            do { try await pause(delay) }
            catch {
                let detail = "acknowledged=\(acknowledged)；原始错误：\(error)"
                record("sequence-interrupted", detail)
                throw E2EError.driver("点击序列中断；\(detail)")
            }
        }
    }
    record("completed", "move/down/up 均获控制通道确认；仍须核对产品后置状态")
}
