import AVFoundation
import CoreGraphics
import CoreVideo
import Foundation
import ImageIO
import E2EKit

public enum ContentMediaFixture {
    public static func install(_ root: URL) throws {
        let writer = try AVAssetWriter(outputURL: root.appendingPathComponent("ClipE2E.mp4"), fileType: .mp4)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: 320, AVVideoHeightKey: 180,
        ])
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB,
            kCVPixelBufferWidthKey as String: 320, kCVPixelBufferHeightKey as String: 180,
        ])
        writer.add(input)
        guard writer.startWriting() else { throw E2EError.env("无法生成视频夹具：\(String(describing: writer.error))") }
        writer.startSession(atSourceTime: .zero)
        for index in 0..<144 {
            let deadline = Date().addingTimeInterval(5)
            while !input.isReadyForMoreMediaData && Date() < deadline { Thread.sleep(forTimeInterval: 0.005) }
            guard input.isReadyForMoreMediaData else { throw E2EError.env("视频夹具编码超时") }
            var pixel: CVPixelBuffer?
            guard CVPixelBufferCreate(kCFAllocatorDefault, 320, 180, kCVPixelFormatType_32ARGB, nil, &pixel) == kCVReturnSuccess, let pixel else { throw E2EError.env("视频帧分配失败") }
            CVPixelBufferLockBaseAddress(pixel, [])
            if let context = CGContext(data: CVPixelBufferGetBaseAddress(pixel), width: 320, height: 180, bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(pixel), space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue) {
                context.setFillColor(CGColor(red: CGFloat(index % 24) / 24, green: 0.3, blue: 0.8, alpha: 1))
                context.fill(CGRect(x: 0, y: 0, width: 320, height: 180))
                context.setFillColor(CGColor(red: 1, green: 0.8, blue: 0.2, alpha: 1))
                context.fill(CGRect(x: (index % 24) * 10, y: 55, width: 60, height: 70))
                if index == 0, let image = context.makeImage(), let destination = CGImageDestinationCreateWithURL(root.appendingPathComponent("PatternE2E.png") as CFURL, "public.png" as CFString, 1, nil) {
                    CGImageDestinationAddImage(destination, image, nil)
                    guard CGImageDestinationFinalize(destination) else { throw E2EError.env("图片夹具写入失败") }
                }
            }
            CVPixelBufferUnlockBaseAddress(pixel, [])
            guard adaptor.append(pixel, withPresentationTime: CMTime(value: Int64(index), timescale: 12)) else { throw E2EError.env("视频夹具帧写入失败") }
        }
        input.markAsFinished()
        let completed = DispatchSemaphore(value: 0)
        writer.finishWriting { completed.signal() }
        guard completed.wait(timeout: .now() + 15) == .success, writer.status == .completed else { throw E2EError.env("视频夹具未完成") }
    }
}
