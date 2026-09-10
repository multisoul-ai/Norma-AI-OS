// Isolated test decoder for production Core Image QR pixels. Never used by the app.
import Foundation
import Vision
import ImageIO

guard CommandLine.arguments.count == 2 else { exit(2) }
let request = VNDetectBarcodesRequest()
request.symbologies = [.qr]
let handler = VNImageRequestHandler(url: URL(fileURLWithPath: CommandLine.arguments[1]))
try handler.perform([request])
guard let result = request.results?.first?.payloadStringValue else { exit(1) }
print(result)
