import Foundation

public struct UploadedChatImage: Decodable, Sendable {
    public let id: String
    public let path: String
    public let byteCount: Int
    let data: String?
    enum CodingKeys: String, CodingKey { case id, path, byteCount = "byte_count", data }
    func validate(_ image: ChatImage) throws {
        guard id == image.id, byteCount == image.byteCount, ChatImageMessage.imageID(path: path) == id else { throw ChatImageError.invalid }
    }
}

extension MobileClient {
    public func uploadImage(_ data: Data, to machine: PairedMachine, workspaceID: String, revision: UInt64,
                            progress: @Sendable (Double) async -> Void = { _ in }) async throws -> UploadedChatImage {
        try machine.validate()
        let image = try ChatImage(data: data)
        guard validID(workspaceID), revision > 0 else { throw ChatImageError.invalid }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let id = UUID().uuidString
        let action: [String: Any] = ["type":"upload_image", "device_id":credential.deviceID, "token":credential.token,
            "workspace_id":workspaceID, "grant_revision":revision, "image_id":image.id, "byte_count":data.count]
        let body = try JSONSerialization.data(withJSONObject: ["version":1, "request_id":id, "machine_id":machine.id, "action":action])
        let connection = try await PinnedConnection.open(machine)
        defer { connection.close() }
        let ready = try await connection.exchange(body)
        if WireReply.rejectsNewRead(ready) { throw ChatImageError.unsupported }
        let reply = try WireReply.decode(ready, expectedID: id)
        guard reply.result?.status == "image_upload", reply.result?.imageOffset == 0 else { throw ChatImageError.invalid }
        var offset = 0
        while offset < data.count {
            try Task.checkCancellation()
            //64KiB binary -> at most87384 base64 characters, below the existing256KiB wire frame.
            let end = min(offset + 64 * 1024, data.count)
            let chunk = try JSONSerialization.data(withJSONObject: ["offset":offset, "data":data[offset..<end].base64EncodedString()])
            let raw = try await connection.exchange(chunk, timeout: .seconds(10))
            let reply = try WireReply.decode(raw, expectedID: id)
            offset = end
            await progress(Double(offset) / Double(data.count))
            if offset == data.count {
                guard reply.result?.status == "image_saved", let saved = reply.result?.image else { throw ChatImageError.invalid }
                try saved.validate(image); return saved
            }
            guard reply.result?.status == "image_upload", reply.result?.imageOffset == offset else { throw ChatImageError.invalid }
        }
        throw ChatImageError.invalid
    }

    public func chatImage(_ imageID: String, on machine: PairedMachine, workspaceID: String) async throws -> Data {
        try machine.validate()
        guard validHex(imageID), validID(workspaceID) else { throw ChatImageError.invalid }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let id = UUID().uuidString
        let body = try JSONSerialization.data(withJSONObject: ["version":1, "request_id":id, "machine_id":machine.id,
            "action":["type":"chat_image", "device_id":credential.deviceID, "token":credential.token,
                      "workspace_id":workspaceID, "image_id":imageID]])
        let connection = try await PinnedConnection.open(machine)
        defer { connection.close() }
        var raw = try await connection.exchange(body)
        var transfer = ConversationTransfer()
        while true {
            try Task.checkCancellation()
            if WireReply.rejectsNewRead(raw) { throw ChatImageError.unsupported }
            let reply = try WireReply.decode(raw, expectedID: id)
            let result: UploadedChatImage?
            if reply.result?.status == "chat_image" { result = reply.result?.image }
            else if reply.result?.status == "image_fragment", let fragment = reply.result?.fragment,
                    fragment.totalBytes <= 3 * 1024 * 1024, fragment.transferID == imageID {
                result = try transfer.append(fragment, as: UploadedChatImage.self)
            } else { throw ChatImageError.invalid }
            if let result {
                guard let encoded = result.data, let data = Data(base64Encoded: encoded) else { throw ChatImageError.invalid }
                let image = try ChatImage(data: data)
                guard image.id == imageID else { throw ChatImageError.invalid }
                try result.validate(image); return data
            }
            raw = try await connection.receive()
        }
    }
}
