import Darwin
import Foundation
import ImageIO

/// The CLI worker has fresh Vision state and reads exactly the PNG captured by the parent.
public func writeAccurateRecognition(pngPath: String) throws {
    guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: pngPath) as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        throw E2EError.driver("独立 OCR 无法读取当前截图：\(pngPath)")
    }
    let texts = try recognizeText(image, level: .accurate, cpuOnly: true)
    try FileHandle.standardOutput.write(contentsOf: JSONEncoder().encode(texts))
}

func isolatedAccurateRecognition(png: URL) throws -> [VisibleText] {
    guard let executable = Bundle.main.executableURL else {
        throw E2EError.driver("无法定位独立 OCR 驱动程序")
    }
    return try runRecognitionWorker(executable: executable, arguments: ["recognize", png.path])
}

/// Synchronous OCR boundary in the external test driver; the tested GPUI App stays responsive.
/// Files avoid pipe-buffer deadlocks, and a deadline bounds and reaps only this worker process.
func runRecognitionWorker(executable: URL, arguments: [String], timeout: TimeInterval = 20) throws -> [VisibleText] {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("norma-ocr-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let output = root.appendingPathComponent("result.json")
    let errorLog = root.appendingPathComponent("stderr.log")
    FileManager.default.createFile(atPath: output.path, contents: nil)
    FileManager.default.createFile(atPath: errorLog.path, contents: nil)
    let stdout = try FileHandle(forWritingTo: output)
    let stderr = try FileHandle(forWritingTo: errorLog)
    defer { try? stdout.close(); try? stderr.close() }
    let process = Process()
    process.executableURL = executable
    process.arguments = arguments
    process.standardOutput = stdout
    process.standardError = stderr
    let ended = DispatchSemaphore(value: 0)
    process.terminationHandler = { _ in ended.signal() }
    try process.run()
    let timedOut = ended.wait(timeout: .now() + timeout) == .timedOut
    if timedOut {
        if process.isRunning { process.terminate() }
        if ended.wait(timeout: .now() + 1) == .timedOut, process.isRunning {
            kill(process.processIdentifier, SIGKILL)
        }
    }
    process.waitUntilExit()
    let original = (try? String(contentsOf: errorLog, encoding: .utf8)) ?? ""
    guard !timedOut else {
        throw E2EError.driver("独立 OCR 超时（\(timeout)s），已回收子进程；stderr：\(original)")
    }
    guard process.terminationStatus == 0 else {
        throw E2EError.driver("独立 OCR status=\(process.terminationStatus)；stderr：\(original)")
    }
    do {
        return try JSONDecoder().decode([VisibleText].self, from: Data(contentsOf: output))
    } catch {
        throw E2EError.driver("独立 OCR JSON 无效：\(error)；stderr：\(original)")
    }
}
