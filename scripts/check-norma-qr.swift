// Isolated test decoder for production Core Image QR pixels. Never used by the app.
import Foundation
import Vision
import ImageIO

guard CommandLine.arguments.count == 3 else { exit(2) }
let request = VNDetectBarcodesRequest()
request.symbologies = [.qr]
let handler = VNImageRequestHandler(url: URL(fileURLWithPath: CommandLine.arguments[1]))
try handler.perform([request])
guard let result = request.results?.first?.payloadStringValue else { exit(1) }
// System Vision diagnostics may write to stdout; the decoded payload has its own protocol.
try Data(result.utf8).write(to: URL(fileURLWithPath: CommandLine.arguments[2]), options: .atomic)
