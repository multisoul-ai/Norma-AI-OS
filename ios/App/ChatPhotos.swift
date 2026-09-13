import SwiftUI
import PhotosUI
import ImageIO
import UniformTypeIdentifiers
import NormaCore

enum ChatPhotoEncoding {
    nonisolated static func preview(_ data: Data, size: Int = 2048) throws -> CGImage {
        // Bound encoded input to32MiB and source pixels to64MP; only decode a2048px thumbnail.
        guard data.count <= 32 * 1024 * 1024,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache:false] as CFDictionary),
              let info = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString:Any],
              let width = info[kCGImagePropertyPixelWidth] as? Int, let height = info[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0, width <= 64_000_000 / height,
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways:true,
                  kCGImageSourceCreateThumbnailWithTransform:true, kCGImageSourceThumbnailMaxPixelSize:size,
                  kCGImageSourceShouldCacheImmediately:true] as CFDictionary) else { throw ChatImageError.invalid }
        return image
    }
    nonisolated static func prepare(_ data: Data) throws -> Data {
        let image = try preview(data)
        // JPEG rebuild normalizes orientation and drops location/EXIF metadata.
        for quality in [0.85, 0.7, 0.5] {
            try Task.checkCancellation()
            let bytes = NSMutableData()
            guard let destination = CGImageDestinationCreateWithData(bytes, UTType.jpeg.identifier as CFString, 1, nil) else { throw ChatImageError.invalid }
            CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality:quality] as CFDictionary)
            guard CGImageDestinationFinalize(destination) else { throw ChatImageError.invalid }
            if bytes.length <= ChatImage.maximumBytes { return bytes as Data }
        }
        throw ChatImageError.invalid
    }
}

struct ChatPhotoPicker: View {
    let book: ChatDraftBook
    let scope: ChatDraft.Scope
    let label: String
    let disabled: Bool
    @Binding var loading: Bool
    @Binding var error: String?
    @State private var selection: [PhotosPickerItem] = []
    var body: some View {
        PhotosPicker(selection: $selection, maxSelectionCount: max(1, 4 - (book.draft(for: scope)?.imageAttachments.count ?? 0)), matching: .images) {
            Image(systemName: "plus").font(.system(size: 19, weight: .medium))
                .frame(width: 44, height: 44).foregroundStyle(.white)
                .background(WorkspaceStyle.surface, in: Circle())
        }
        .disabled(disabled || loading || (book.draft(for: scope)?.imageAttachments.count ?? 0) >= 4)
        .accessibilityLabel("添加图片").accessibilityIdentifier("chat-add-images")
        .task(id: selection) {
            guard !selection.isEmpty else { return }
            let target = scope
            loading = true; error = nil
            defer { loading = false; selection = [] }
            do {
                for item in selection {
                    guard let data = try await item.loadTransferable(type: Data.self) else { throw ChatImageError.invalid }
                    let prepared = try await Task.detached(priority: .userInitiated) { try ChatPhotoEncoding.prepare(data) }.value
                    try Task.checkCancellation()
                    try await book.addImage(prepared, for: target, label: label)
                }
            } catch { if !Task.isCancelled { self.error = error.localizedDescription } }
        }
    }
}

struct ChatPhotoTile: View {
    let id: String
    let label: String
    let load: () async throws -> Data
    var remove: (() -> Void)? = nil
    @State private var thumbnail: CGImage?
    @State private var preview: CGImage?
    @State private var failure: String?
    @State private var showing = false
    @State private var retry = 0
    var body: some View {
        Button { showing = true } label: {
            Group {
                if let thumbnail { Image(decorative: thumbnail, scale: 1).resizable().scaledToFill() }
                else if failure != nil { Image(systemName: "photo.badge.exclamationmark").foregroundStyle(.secondary) }
                else { ProgressView() }
            }.frame(width: 80, height: 80).background(WorkspaceStyle.raised)
                .clipShape(RoundedRectangle(cornerRadius: 14))
        }.buttonStyle(WorkspacePressStyle()).accessibilityLabel(label).accessibilityIdentifier("chat-photo-" + id)
        .overlay(alignment: .topTrailing) {
            if let remove {
                Button(action: remove) {
                    Image(systemName: "xmark.circle.fill").symbolRenderingMode(.palette).foregroundStyle(.white, .black.opacity(0.8))
                        .font(.system(size: 20)).frame(width: 44, height: 44)
                }.accessibilityLabel("移除" + label).accessibilityIdentifier("chat-remove-photo-" + id)
            }
        }
        .task(id: "\(id)-\(retry)") {
            do {
                let data = try await load()
                let image = try await Task.detached { try ChatPhotoEncoding.preview(data, size: 256) }.value
                try Task.checkCancellation(); thumbnail = image; failure = nil
            } catch { if !Task.isCancelled { failure = error.localizedDescription } }
        }
        .sheet(isPresented: $showing) {
            NavigationStack {
                Group {
                    if let preview { ReviewImageCanvas(image: preview, name: label) }
                    else if let failure {
                        ContentUnavailableView { Label("图片暂不可用", systemImage: "photo") } description: { Text(failure) } actions: {
                            Button("重试") { retry += 1 }
                        }
                    } else { WorkspaceLoadingView(message: "正在读取图片…") }
                }.background(.black).navigationTitle(label).navigationBarTitleDisplayMode(.inline)
                    .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { showing = false } } }
                    .task(id: retry) {
                        do {
                            let data = try await load()
                            let image = try await Task.detached { try ChatPhotoEncoding.preview(data) }.value
                            try Task.checkCancellation(); preview = image; failure = nil
                        } catch { if !Task.isCancelled { failure = error.localizedDescription } }
                    }
            }.preferredColorScheme(.dark)
        }
    }
}
