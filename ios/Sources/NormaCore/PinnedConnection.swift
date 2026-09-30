import Foundation
import CryptoKit
import Security
import os

final class PinDelegate: NSObject, URLSessionDelegate, URLSessionTaskDelegate {
    private let rejected = OSAllocatedUnfairLock(initialState: false)
    var rejectedIdentity: Bool { rejected.withLock { $0 } }
    let pin: String
    init(pin: String) { self.pin = pin }

    func urlSession(_ session: URLSession, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping @Sendable (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let trust = challenge.protectionSpace.serverTrust,
              let certificates = SecTrustCopyCertificateChain(trust) as? [SecCertificate],
              let leaf = certificates.first else { completionHandler(.cancelAuthenticationChallenge, nil); return }
        let digest = SHA256.hash(data: SecCertificateCopyData(leaf) as Data).map { String(format: "%02x", $0) }.joined()
        // QR pins identity independently of changing LAN/Tailscale IPs. Still validate lifetime,
        // signature and server usage, with ONLY this exact pinned certificate as an anchor.
        guard digest == pin,
              SecTrustSetAnchorCertificates(trust, [leaf] as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust, true) == errSecSuccess,
              SecTrustSetPolicies(trust, SecPolicyCreateSSL(true, nil)) == errSecSuccess,
              SecTrustEvaluateWithError(trust, nil) else {
            rejected.withLock { $0 = true }
            completionHandler(.cancelAuthenticationChallenge, nil); return
        }
        completionHandler(.useCredential, URLCredential(trust: trust))
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

struct PinnedConnection: Sendable {
    let session: URLSession
    let socket: URLSessionWebSocketTask

    static func open(_ machine: PairedMachine, streaming: Bool = false) async throws -> Self {
        try machine.validate()
        return try await withThrowingTaskGroup(of: Result<Self, any Error>.self) { group in
            // At most four pinned routes. Probe transport only; send the application request once,
            // after choosing a verified winner. A stale LAN/VPN address cannot delay a healthy route.
            for endpoint in machine.endpoints {
                group.addTask {
                    do { return .success(try await open(endpoint, pin: machine.certificateSHA256, streaming: streaming)) }
                    catch { return .failure(error) }
                }
            }
            var selected: Self?
            var identityRejected = false
            for try await result in group {
                switch result {
                case .success(let connection):
                    if selected == nil && !Task.isCancelled {
                        selected = connection
                        group.cancelAll()
                    } else { connection.close() }
                case .failure(let error):
                    if (error as? MobileError) == .wrongComputer { identityRejected = true }
                }
            }
            if Task.isCancelled { selected?.close(); throw CancellationError() }
            if let selected { return selected }
            throw identityRejected ? MobileError.wrongComputer : MobileError.connectionFailed
        }
    }

    private static func open(_ endpoint: URL, pin: String, streaming: Bool) async throws -> Self {
        try Task.checkCancellation()
        let config = URLSessionConfiguration.ephemeral
        config.tlsMinimumSupportedProtocolVersion = .TLSv13
        config.urlCache = nil; config.httpCookieStorage = nil; config.urlCredentialStorage = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        // All candidate routes share this 5s ceiling instead of accumulating up to 20s.
        config.timeoutIntervalForRequest = 5
        if !streaming { config.timeoutIntervalForResource = 5 }
        let verifier = PinDelegate(pin: pin)
        let session = URLSession(configuration: config, delegate: verifier, delegateQueue: nil)
        let socket = session.webSocketTask(with: endpoint)
        socket.maximumMessageSize = 512 * 1024
        let connection = Self(session: session, socket: socket)
        socket.resume()
        do {
            try await connection.bounded {
                try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
                    socket.sendPing { error in
                        if let error { continuation.resume(throwing: error) }
                        else { continuation.resume() }
                    }
                }
            }
            try Task.checkCancellation()
            return connection
        } catch {
            connection.close()
            try Task.checkCancellation()
            if verifier.rejectedIdentity { throw MobileError.wrongComputer }
            if let urlError = error as? URLError,
               [.userCancelledAuthentication, .serverCertificateUntrusted, .serverCertificateHasBadDate,
                .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid].contains(urlError.code) {
                throw MobileError.wrongComputer
            }
            throw error
        }
    }

    func exchange(_ data: Data, timeout: Duration = .seconds(5)) async throws -> Data {
        try await bounded(timeout: timeout) {
            try await socket.send(.string(String(decoding: data, as: UTF8.self)))
            return try await readMessage()
        }
    }
    func receive() async throws -> Data { try await bounded { try await readMessage() } }
    private func readMessage() async throws -> Data {
        let message: URLSessionWebSocketTask.Message
        do { message = try await socket.receive() }
        catch {
            try Task.checkCancellation()
            // URLSession reports peer shutdown as NSPOSIXErrorDomain on macOS and URLError on iOS.
            // Both mean the authenticated transport ended; malformed frames remain invalidResponse.
            throw MobileError.connectionFailed
        }
        guard case .string(let text) = message, text.utf8.count <= 512 * 1024 else { throw MobileError.invalidResponse }
        return Data(text.utf8)
    }
    func close() { socket.cancel(with: .goingAway, reason: nil); session.invalidateAndCancel() }

    func bounded<T: Sendable>(timeout: Duration = .seconds(5), _ operation: @escaping @Sendable () async throws -> T) async throws -> T {
        try await withTaskCancellationHandler {
            try await withThrowingTaskGroup(of: T.self) { group in
                group.addTask { try await operation() }
                group.addTask {
                    try await Task.sleep(for: timeout)
                    close()
                    throw MobileError.timedOut
                }
                defer { group.cancelAll() }
                guard let result = try await group.next() else { throw CancellationError() }
                return result
            }
        } onCancel: { close() }
    }
}
