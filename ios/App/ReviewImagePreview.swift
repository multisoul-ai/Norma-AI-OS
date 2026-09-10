import SwiftUI
import ImageIO
import NormaCore

enum ReviewImageError: Error { case unsupported, tooLarge }

struct ReviewImagePreview: View {
    let version: ReviewFileVersion
    let path: String
    @State private var result: Result<CGImage, ReviewImageError>?

    var body: some View {
        Group {
            switch result {
            case .success(let image):
                VStack(spacing: 8) {
                    ReviewImageCanvas(image: image, name: path)
                    Text("静态预览 · 可捏合放大").font(.caption).foregroundStyle(.secondary)
                }
            case .failure(let error):
                ContentUnavailableView(error == .tooLarge ? "图片超过预览上限" : "此文件无法预览",
                    systemImage: "photo", description: Text(error == .tooLarge
                        ? "请在电脑查看原图。手机支持 1 MiB、1677 万像素以内的图片。"
                        : "支持常见栅格图片。文件可能损坏，或需要在电脑上打开。"))
            case nil:
                ProgressView("正在读取图片").frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }.task(id: version.bytes) {
            result = nil
            let worker = Task.detached(priority: .userInitiated) { try Self.decode(version) }
            do {
                let image = try await withTaskCancellationHandler { try await worker.value } onCancel: { worker.cancel() }
                try Task.checkCancellation()
                result = .success(image)
            } catch {
                if !Task.isCancelled { result = .failure(error as? ReviewImageError ?? .unsupported) }
            }
        }
    }

    nonisolated static func decode(_ version: ReviewFileVersion) throws -> CGImage {
        // Reuse the wire's 1MiB limit. 4096² pixels bound the decoded source to ~64MiB;
        // a 2048px thumbnail bounds the displayed RGBA image to 2048²×4 = 16MiB.
        guard version.bytes.count <= 1024 * 1024 else { throw ReviewImageError.tooLarge }
        guard version.kind == "binary",
              let source = CGImageSourceCreateWithData(Data(version.bytes) as CFData,
                  [kCGImageSourceShouldCache: false] as CFDictionary),
              let type = CGImageSourceGetType(source) as String?,
              ["public.png", "public.jpeg", "public.tiff", "com.compuserve.gif", "public.heic", "public.heif", "org.webmproject.webp"].contains(type),
              CGImageSourceGetStatus(source) == .statusComplete,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0 else { throw ReviewImageError.unsupported }
        guard width <= (4096 * 4096) / height else { throw ReviewImageError.tooLarge }
        try Task.checkCancellation()
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 2048,
            kCGImageSourceShouldCacheImmediately: true
        ] as CFDictionary) else { throw ReviewImageError.unsupported }
        return image
    }
}

private struct ReviewImageCanvas: UIViewRepresentable {
    let image: CGImage
    let name: String
    func makeUIView(context: Context) -> ImageScrollView { ImageScrollView() }
    func updateUIView(_ view: ImageScrollView, context: Context) {
        if view.picture.image?.cgImage !== image {
            view.setZoomScale(1, animated: false)
            view.picture.image = UIImage(cgImage: image)
        }
        view.picture.accessibilityLabel = name
    }
}

private final class ImageScrollView: UIScrollView, UIScrollViewDelegate {
    let picture = UIImageView()
    private var viewport = CGSize.zero

    init() {
        super.init(frame: .zero)
        backgroundColor = .black; delegate = self
        minimumZoomScale = 1; maximumZoomScale = 4
        picture.contentMode = .scaleAspectFit
        picture.isAccessibilityElement = true
        picture.accessibilityTraits = .image
        picture.accessibilityIdentifier = "review-image"
        picture.accessibilityCustomActions = [
            UIAccessibilityCustomAction(name: "放大") { [weak self] _ in self?.adjustZoom(2) ?? false },
            UIAccessibilityCustomAction(name: "还原大小") { [weak self] _ in self?.adjustZoom(0) ?? false }
        ]
        addSubview(picture)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func layoutSubviews() {
        super.layoutSubviews()
        if viewport != bounds.size {
            viewport = bounds.size
            setZoomScale(1, animated: false)
            picture.frame = CGRect(origin: .zero, size: viewport)
            contentSize = viewport
        }
    }
    func viewForZooming(in scrollView: UIScrollView) -> UIView? { picture }
    private func adjustZoom(_ factor: CGFloat) -> Bool {
        setZoomScale(min(maximumZoomScale, max(minimumZoomScale, zoomScale * factor)),
                     animated: !UIAccessibility.isReduceMotionEnabled)
        return true
    }
}
