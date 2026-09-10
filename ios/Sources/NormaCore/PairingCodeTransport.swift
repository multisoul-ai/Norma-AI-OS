import Foundation
import Security

/// Used ONLY for one public PAKE exchange before the certificate identity is known.
/// This session never receives a PIN, device token, QR secret, or application command.
private final class CodeBootstrapTrust: NSObject, URLSessionDelegate, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping @Sendable (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let trust = challenge.protectionSpace.serverTrust,
              let certificates = SecTrustCopyCertificateChain(trust) as? [SecCertificate], let leaf = certificates.first,
              SecTrustSetAnchorCertificates(trust, [leaf] as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust, true) == errSecSuccess,
              SecTrustSetPolicies(trust, SecPolicyCreateSSL(true, nil)) == errSecSuccess,
              SecTrustEvaluateWithError(trust, nil) else {
            completionHandler(.cancelAuthenticationChallenge, nil); return
        }
        completionHandler(.useCredential, URLCredential(trust: trust))
    }
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

func exchangePairingCode(_ url: URL, _ request: Data) async throws -> Data {
    let config = URLSessionConfiguration.ephemeral
    config.tlsMinimumSupportedProtocolVersion = .TLSv13
    config.urlCache = nil; config.httpCookieStorage = nil; config.urlCredentialStorage = nil
    // One user submission = one attempt, never retry a guess or follow a redirect automatically.
    config.timeoutIntervalForRequest = 8; config.timeoutIntervalForResource = 8
    let session = URLSession(configuration: config, delegate: CodeBootstrapTrust(), delegateQueue: nil)
    let socket = session.webSocketTask(with: url)
    socket.maximumMessageSize = 12 * 1024 // 4124 ciphertext bytes * 2 hex + envelope < 12KiB.
    defer { socket.cancel(with: .goingAway, reason: nil); session.invalidateAndCancel() }
    socket.resume()
    return try await withTaskCancellationHandler {
        try await withThrowingTaskGroup(of: Data.self) { group in
            group.addTask {
                try await socket.send(.string(String(decoding: request, as: UTF8.self)))
                guard case .string(let text) = try await socket.receive(), text.utf8.count <= 12 * 1024 else {
                    throw MobileError.invalidResponse
                }
                return Data(text.utf8)
            }
            group.addTask {
                try await Task.sleep(for: .seconds(8))
                socket.cancel(with: .goingAway, reason: nil)
                throw MobileError.timedOut
            }
            defer { group.cancelAll() }
            guard let result = try await group.next() else { throw CancellationError() }
            return result
        }
    } onCancel: { socket.cancel(with: .goingAway, reason: nil); session.invalidateAndCancel() }
}
