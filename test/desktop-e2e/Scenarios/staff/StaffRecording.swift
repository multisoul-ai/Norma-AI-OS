import AVFoundation
import CoreGraphics
import E2EKit
import Foundation

/// Continuously samples the live debug window, including all input and intermediate states.
/// Frames are encoded immediately at their monotonic capture time, never assembled from checkpoints.
@MainActor
final class StaffRecording {
    private let writer: AVAssetWriter
    private let input: AVAssetWriterInput
    private let adaptor: AVAssetWriterInputPixelBufferAdaptor
    private let width: Int
    private let height: Int
    private let started = ProcessInfo.processInfo.systemUptime
    private var loop: Task<Void, Never>?
    private var error: Error?
    private var frames = 0
    private var longestGap = 0.0
    private var previousTime = 0.0
    private let url: URL

    init(session: Session, first: Frame, url: URL) throws {
        self.url = url
        width = first.image.width / 2 * 2
        height = first.image.height / 2 * 2
        writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: width, AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 3_000_000, AVVideoMaxKeyFrameIntervalKey: 20]
        ])
        input.expectsMediaDataInRealTime = true
        adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB,
            kCVPixelBufferWidthKey as String: width, kCVPixelBufferHeightKey as String: height,
            kCVPixelBufferCGImageCompatibilityKey as String: true,
            kCVPixelBufferCGBitmapContextCompatibilityKey as String: true
        ])
        writer.add(input)
        guard writer.startWriting() else { throw writer.error ?? E2EError.driver("录像编码器无法启动") }
        writer.startSession(atSourceTime: .zero)
        try append(first.image)
        loop = Task { [weak self, weak session] in
            while !Task.isCancelled {
                do {
                    try await Task.sleep(nanoseconds: 250_000_000)
                    guard let self, let session else { return }
                    let frame = try await session.capturePixels()
                    try self.append(frame.image)
                } catch is CancellationError { return }
                catch { self?.error = error; return }
            }
        }
    }

    func finish() async throws {
        loop?.cancel()
        await loop?.value
        loop = nil
        input.markAsFinished()
        await writer.finishWriting()
        let stats: [String: Any] = ["frames": frames, "seconds": ProcessInfo.processInfo.systemUptime - started,
            "longestCaptureGapSeconds": longestGap, "source": "continuous-live-window", "requestedFPS": 4]
        try JSONSerialization.data(withJSONObject: stats, options: [.prettyPrinted, .sortedKeys])
            .write(to: url.deletingPathExtension().appendingPathExtension("json"), options: .atomic)
        if let error { throw error }
        guard writer.status == .completed, frames > 1 else { throw writer.error ?? E2EError.driver("录像没有有效连续帧") }
    }

    private func append(_ image: CGImage) throws {
        guard input.isReadyForMoreMediaData else { return }
        guard let pool = adaptor.pixelBufferPool else { throw E2EError.driver("录像像素池不可用") }
        var buffer: CVPixelBuffer?
        guard CVPixelBufferPoolCreatePixelBuffer(nil, pool, &buffer) == kCVReturnSuccess, let buffer else {
            throw E2EError.driver("无法分配录像帧")
        }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let context = CGContext(data: CVPixelBufferGetBaseAddress(buffer), width: width, height: height,
            bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(buffer), space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue) else { throw E2EError.driver("无法创建录像绘制上下文") }
        context.setFillColor(CGColor(gray: 0, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        let scale = min(Double(width) / Double(image.width), Double(height) / Double(image.height))
        let rect = CGRect(x: (Double(width) - Double(image.width) * scale) / 2,
            y: (Double(height) - Double(image.height) * scale) / 2,
            width: Double(image.width) * scale, height: Double(image.height) * scale)
        context.draw(image, in: rect)
        let now = ProcessInfo.processInfo.systemUptime - started
        guard adaptor.append(buffer, withPresentationTime: CMTime(seconds: now, preferredTimescale: 600)) else {
            throw writer.error ?? E2EError.driver("无法写入录像帧")
        }
        longestGap = max(longestGap, now - previousTime)
        previousTime = now
        frames += 1
    }
}
