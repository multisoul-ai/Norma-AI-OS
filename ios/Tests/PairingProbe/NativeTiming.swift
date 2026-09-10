import Foundation
import Darwin

/// Only compiled into the isolated native probe/test host; no telemetry or app configuration.
enum NativeTiming {
    static func milliseconds(_ value: Duration) -> Double {
        Double(value.components.seconds) * 1000 + Double(value.components.attoseconds) / 1e15
    }

    static func report(_ name: String, provider: String, samples: [Duration], scope: String) throws {
        let raw = samples.map(milliseconds), ordered = raw.sorted()
        guard !raw.isEmpty else { return }
        // Nearest-rank: ceil(n × p) - 1. With20 reads p50=index9 and p95=index18.
        // Approval submits have only2 observations: report them without claiming a stable distribution.
        let result: [String: Any] = ["name": name, "provider": provider, "samples_ms": raw,
            "sample_count": raw.count, "p50_ms": ordered[Int(ceil(Double(raw.count) * 0.5)) - 1],
            "p95_ms": ordered[Int(ceil(Double(raw.count) * 0.95)) - 1], "max_ms": ordered.last!,
            "scope": scope, "os": ProcessInfo.processInfo.operatingSystemVersionString]
        print("NATIVE_LATENCY=" + String(decoding: try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), as: UTF8.self))
    }

    static func idleChat(provider: String) async throws {
        let cpu = try cpuSeconds(), started = ContinuousClock.now
        //6s = three existing2s heartbeat periods, after the real chat subscription baseline.
        try await Task.sleep(for: .seconds(6))
        let elapsed = milliseconds(ContinuousClock.now - started) / 1000
        let consumed = try cpuSeconds() - cpu
        let result: [String: Any] = ["provider": provider, "wall_seconds": elapsed,
            "client_cpu_seconds": consumed, "client_one_core_percent": consumed / elapsed * 100,
            "scope": "native probe/test host with one idle conversation subscription; no SwiftUI rendering or server CPU",
            "network_bytes": "not measured"]
        print("NATIVE_IDLE=" + String(decoding: try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), as: UTF8.self))
    }

    static func cpuSeconds() throws -> Double {
        var value = rusage()
        guard getrusage(RUSAGE_SELF, &value) == 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
        return Double(value.ru_utime.tv_sec + value.ru_stime.tv_sec)
            + Double(value.ru_utime.tv_usec + value.ru_stime.tv_usec) / 1_000_000
    }
}
