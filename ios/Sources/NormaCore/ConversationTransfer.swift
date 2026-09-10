import Foundation

struct ConversationFragment: Decodable, Sendable {
    let transferID: String
    let offset: Int
    let totalBytes: Int
    let text: String
    enum CodingKeys: String, CodingKey { case transferID = "transfer_id", offset, totalBytes = "total_bytes", text }
}

/// A single immutable read, held offscreen until every byte and the exact request validate.
struct ConversationTransfer {
    private var id: String?
    private var total = 0
    private var data = Data()
    mutating func append<T: Decodable>(_ part: ConversationFragment, as type: T.Type) throws -> T? {
        do {
            let bytes = Data(part.text.utf8)
            // Same32MiB page ceiling and64KiB fragment ceiling as the Rust transport.
            guard validID(part.transferID), part.totalBytes > 0, part.totalBytes <= 32 * 1024 * 1024,
                  !bytes.isEmpty, bytes.count <= 64 * 1024, part.offset == data.count,
                  bytes.count <= part.totalBytes - data.count else { throw MobileError.invalidResponse }
            if let id {
                guard id == part.transferID, total == part.totalBytes else { throw MobileError.invalidResponse }
            } else { id = part.transferID; total = part.totalBytes }
            data.append(bytes)
            if data.count == total { return try JSONDecoder().decode(type, from: data) }
            return nil
        } catch { id = nil; total = 0; data.removeAll(keepingCapacity: false); throw error }
    }
}
