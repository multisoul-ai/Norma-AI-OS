import Foundation

/// Build-time local acceptance configuration. Invitations cannot change the endpoint or TLS identity.
public struct LocalCloudConfiguration: Sendable {
    public let origin: URL
    public let certificateSHA256: String
    public let namespace: String

    public static func bundled() throws -> Self? {
        try Self(info: Bundle.main.infoDictionary?.compactMapValues { $0 as? String } ?? [:])
    }

    public init?(info: [String: String]) throws {
        let address = info["NormaLocalGatewayOrigin"] ?? ""
        let pin = info["NormaLocalGatewayPin"] ?? ""
        let namespace = info["NormaLocalGatewayNamespace"] ?? ""
        if address.isEmpty && pin.isEmpty && namespace.isEmpty { return nil }
        guard let url = URL(string: address), url.scheme == "https",
              let host = url.host, Self.privateIPv4(host),
              url.user == nil, url.password == nil, url.query == nil, url.fragment == nil,
              url.path.isEmpty || url.path == "/", let port = url.port, (1...65535).contains(port),
              validHex(pin), UUID(uuidString: namespace) != nil else { throw CloudTextError.invalidRequest }
        origin = url
        certificateSHA256 = pin
        self.namespace = namespace
    }

    private static func privateIPv4(_ host: String) -> Bool {
        let parts = host.split(separator: ".", omittingEmptySubsequences: false)
        let bytes = parts.compactMap { UInt8($0) }
        guard bytes.count == 4, zip(parts, bytes).allSatisfy({ $0.0 == String($0.1) }) else { return false }
        return bytes[0] == 10 || bytes[0] == 127 ||
            (bytes[0] == 192 && bytes[1] == 168) || (bytes[0] == 172 && (16...31).contains(bytes[1]))
    }
}
