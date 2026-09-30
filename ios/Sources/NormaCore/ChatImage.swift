import Foundation
import CryptoKit

public enum ChatImageError: Error, LocalizedError {
    case invalid, tooMany, storageFull, unavailable, unsupported
    public var errorDescription: String? {
        switch self {
        case .invalid: "图片无法读取或超过大小限制，请重新选择。"
        case .tooMany: "每条消息最多添加 4 张图片。"
        case .storageFull: "图片存储空间已满，请清理不需要的图片草稿。"
        case .unavailable: "图片暂时无法读取，请检查电脑连接后重试。"
        case .unsupported: "请更新电脑端 Norma 后发送图片。"
        }
    }
}

public struct ChatImage: Codable, Sendable, Equatable, Identifiable {
    public static let maximumBytes = 2 * 1024 * 1024
    public let id: String
    public let byteCount: Int
    // Unique local allocation prevents a delayed draft write from deleting a newly re-added image.
    var localKey: String? = nil
    var fileKey: String { localKey ?? id }
    public init(data: Data) throws {
        guard !data.isEmpty, data.count <= Self.maximumBytes else { throw ChatImageError.invalid }
        id = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        byteCount = data.count
    }
    func validate() throws {
        guard validHex(id), localKey == nil || validID(localKey!), byteCount > 0, byteCount <= Self.maximumBytes else { throw ChatImageError.invalid }
    }
    func verify(_ data: Data) throws {
        let actual = try ChatImage(data: data)
        guard actual.id == id, actual.byteCount == byteCount else { throw ChatImageError.invalid }
    }
}

/// File references are plain input to the existing Agent session, never shell commands.
public enum ChatImageMessage {
    private static let opening = "[[norma-images]]\n请先读取并查看以下图片，再结合上面的文字回复：\n"
    private static let closing = "\n[[/norma-images]]"
    public static func compose(text: String, paths: [String]) throws -> String {
        guard !paths.isEmpty, paths.count <= 4, paths.allSatisfy({ imageID(path: $0) != nil }) else { throw ChatImageError.invalid }
        // JSON keeps image paths literal: Claude otherwise replaces pasted bare paths with [Image #N], losing exact receipts/history.
        let encoder = JSONEncoder(); encoder.outputFormatting = .withoutEscapingSlashes
        let references = String(decoding: try encoder.encode(paths), as: UTF8.self)
        return text + "\n\n" + opening + references + closing
    }
    public static func parse(_ text: String) -> (text: String, paths: [String])? {
        guard text.hasSuffix(closing), let range = text.range(of: opening, options: .backwards) else { return nil }
        let prefix = String(text[..<range.lowerBound])
        guard prefix.isEmpty || prefix.hasSuffix("\n\n") else { return nil }
        let references = String(text[range.upperBound...].dropLast(closing.count))
        let paths = (try? JSONDecoder().decode([String].self, from: Data(references.utf8))) ?? references.components(separatedBy: "\n")
        guard !paths.isEmpty, paths.count <= 4, paths.allSatisfy({ imageID(path: $0) != nil }) else { return nil }
        return (prefix.isEmpty ? "" : String(prefix.dropLast(2)), paths)
    }
    public static func imageID(path: String) -> String? {
        guard path.hasPrefix("/"), path.utf8.count <= 4096, !path.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              path.contains(".mobile-images/"), ["jpg", "png"].contains(URL(fileURLWithPath: path).pathExtension) else { return nil }
        let id = URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
        return validHex(id) ? id : nil
    }
}

extension ChatDraftStore {
    private var imageFolder: URL { url.deletingPathExtension().appendingPathExtension("images") }
    func saveImage(_ data: Data) throws -> ChatImage {
        var image = try ChatImage(data: data)
        image.localKey = UUID().uuidString
        try FileManager.default.createDirectory(at: imageFolder, withIntermediateDirectories: true)
        let target = imageFolder.appendingPathComponent(image.fileKey)
        //128MiB bounds selected, unsent images; bytes live separately so typing never rewrites images.
        let files = try FileManager.default.contentsOfDirectory(at: imageFolder, includingPropertiesForKeys: [.fileSizeKey])
        let total = try files.reduce(0) { try $0 + ($1.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) }
        guard total + data.count <= 128 * 1024 * 1024 else { throw ChatImageError.storageFull }
        #if os(iOS)
        try data.write(to: target, options: [.atomic, .completeFileProtection])
        #else
        try data.write(to: target, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: target.path)
        #endif
        return image
    }
    func readImage(_ image: ChatImage) throws -> Data {
        try image.validate()
        let file = imageFolder.appendingPathComponent(image.fileKey)
        guard try file.resourceValues(forKeys: [.fileSizeKey]).fileSize == image.byteCount else { throw ChatImageError.invalid }
        let data = try Data(contentsOf: file); try image.verify(data); return data
    }
    func removeUnusedImages(previous: [ChatDraft], next: [ChatDraft]) {
        let removed = Set(previous.flatMap(\.imageAttachments).map(\.fileKey)).subtracting(next.flatMap(\.imageAttachments).map(\.fileKey))
        for id in removed { try? FileManager.default.removeItem(at: imageFolder.appendingPathComponent(id)) }
    }
    func discardImage(_ image: ChatImage) { try? FileManager.default.removeItem(at: imageFolder.appendingPathComponent(image.fileKey)) }
}
