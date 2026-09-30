import Foundation

/// The SDP response identifies the media session, independently of function call IDs.
public enum RealtimeCallLocation {
    public static func callID(_ location: String) -> String? {
        let base = URL(string: "https://api.openai.com")!
        guard let url = URL(string: location, relativeTo: base)?.absoluteURL,
              url.scheme == "https", url.host == "api.openai.com", url.port == nil || url.port == 443,
              url.user == nil, url.password == nil, url.query == nil, url.fragment == nil else { return nil }
        let prefix = "/v1/realtime/calls/"
        guard url.path.hasPrefix(prefix) else { return nil }
        let id = String(url.path.dropFirst(prefix.count))
        guard !id.isEmpty, id.utf8.count <= 128,
              id.utf8.allSatisfy({ (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || $0 == 45 || $0 == 95 }) else { return nil }
        return id
    }
}
