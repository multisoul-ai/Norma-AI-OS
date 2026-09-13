import SwiftUI
import VisionKit
import AVFoundation
import PhotosUI
import CoreImage
import ImageIO

struct ScannerScreen: View {
    let onCode: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var ready = false
    @State private var unavailable = false
    @State private var photo: PhotosPickerItem?
    @State private var decoding = false
    @State private var imageError: String?

    var body: some View {
        NavigationStack {
            Group {
                if decoding {
                    WorkspaceLoadingView(message: "正在识别二维码…")
                } else if ready {
                    QRScanner(onCode: onCode, onFailure: { ready = false; unavailable = true })
                        .ignoresSafeArea(edges: .bottom)
                } else if unavailable {
                    VStack(spacing: 16) {
                        Image(systemName: "camera").font(.largeTitle).foregroundStyle(.secondary)
                        Text("此设备暂不可扫码").font(.headline)
                        Text("可以从图片识别电脑上的二维码，或在 iPhone 上允许相机访问后扫描。")
                            .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                        if AVCaptureDevice.authorizationStatus(for: .video) == .denied,
                           let url = URL(string: UIApplication.openSettingsURLString) {
                            Link("打开系统设置", destination: url).buttonStyle(.bordered)
                        }
                    }.padding(32)
                } else { WorkspaceLoadingView(message: "准备相机…") }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity).background(.black)
            .safeAreaInset(edge: .bottom) {
                VStack(spacing: 12) {
                    if let imageError { Text(imageError).font(.subheadline).foregroundStyle(.orange) }
                    PhotosPicker(selection: $photo, matching: .images) {
                        Label("从图片识别二维码", systemImage: "photo").frame(minHeight: 32)
                    }
                    .buttonStyle(.bordered).disabled(decoding).accessibilityIdentifier("scan-qr-photo")
                }.multilineTextAlignment(.center).padding().frame(maxWidth: .infinity).background(.black)
            }
            .navigationTitle("扫描 Norma 二维码").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("返回") { dismiss() }.accessibilityIdentifier("scanner-close") }
            }
        }
        .task {
            guard DataScannerViewController.isSupported else { unavailable = true; return }
            let allowed = await AVCaptureDevice.requestAccess(for: .video)
            guard !Task.isCancelled else { return }
            ready = allowed && DataScannerViewController.isAvailable
            unavailable = !ready
        }
        .task(id: photo) {
            guard let photo else { return }
            decoding = true; imageError = nil
            do {
                guard let data = try await photo.loadTransferable(type: Data.self) else { throw PairingImageError.invalidImage }
                try Task.checkCancellation()
                let worker = Task.detached(priority: .userInitiated) { try PairingImageCode.decode(data) }
                let code = try await withTaskCancellationHandler { try await worker.value } onCancel: { worker.cancel() }
                try Task.checkCancellation()
                onCode(code)
            } catch {
                guard !Task.isCancelled else { return }
                imageError = error.localizedDescription
            }
            decoding = false; self.photo = nil
        }
    }
}

enum PairingImageError: Error, LocalizedError {
    case invalidImage, tooLarge, noCode, multipleCodes
    var errorDescription: String? {
        switch self {
        case .invalidImage: "无法读取这张图片，请重新选择。"
        case .tooLarge: "图片过大，请截取二维码后重试。"
        case .noCode: "未找到二维码，请选择清晰的电脑配对二维码。"
        case .multipleCodes: "图片中有多个二维码，请只保留要连接的电脑二维码。"
        }
    }
}

enum PairingImageCode {
    nonisolated static func decode(_ data: Data) throws -> String {
        // 20MiB covers screenshots; 8192² bounds source pixels, and the 4096px
        // thumbnail bounds the detector's RGBA input to 4096²×4 = 64MiB.
        guard data.count <= 20 * 1024 * 1024 else { throw PairingImageError.tooLarge }
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetStatus(source) == .statusComplete,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0 else { throw PairingImageError.invalidImage }
        guard width <= (8192 * 8192) / height else { throw PairingImageError.tooLarge }
        try Task.checkCancellation()
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 4096
        ] as CFDictionary) else { throw PairingImageError.invalidImage }
        // Core Image's QR detector also works without a camera or an ML inference device.
        guard let detector = CIDetector(ofType: CIDetectorTypeQRCode, context: CIContext(),
            options: [CIDetectorAccuracy: CIDetectorAccuracyHigh]) else { throw PairingImageError.invalidImage }
        let codes = detector.features(in: CIImage(cgImage: image)).compactMap { ($0 as? CIQRCodeFeature)?.messageString }
        try Task.checkCancellation()
        guard !codes.isEmpty else { throw PairingImageError.noCode }
        guard codes.count == 1 else { throw PairingImageError.multipleCodes }
        return codes[0]
    }
}

/// VisionKit owns camera capture, focus, highlighting and accessible scanning guidance.
private struct QRScanner: UIViewControllerRepresentable {
    let onCode: (String) -> Void
    let onFailure: () -> Void
    func makeCoordinator() -> Coordinator { Coordinator(onCode: onCode, onFailure: onFailure) }
    func makeUIViewController(context: Context) -> DataScannerViewController {
        let controller = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced, recognizesMultipleItems: false, isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true, isGuidanceEnabled: true, isHighlightingEnabled: true)
        controller.delegate = context.coordinator
        do { try controller.startScanning() } catch { Task { @MainActor in onFailure() } }
        return controller
    }
    func updateUIViewController(_ uiViewController: DataScannerViewController, context: Context) {}
    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) { controller.stopScanning() }

    @MainActor final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onCode: (String) -> Void
        let onFailure: () -> Void
        private var delivered = false
        init(onCode: @escaping (String) -> Void, onFailure: @escaping () -> Void) { self.onCode = onCode; self.onFailure = onFailure }
        func dataScanner(_ scanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            for item in addedItems {
                guard !delivered, case .barcode(let barcode) = item, let text = barcode.payloadStringValue else { continue }
                delivered = true; scanner.stopScanning(); onCode(text); return
            }
        }
        func dataScanner(_ scanner: DataScannerViewController, becameUnavailableWithError error: DataScannerViewController.ScanningUnavailable) {
            scanner.stopScanning(); onFailure()
        }
    }
}
