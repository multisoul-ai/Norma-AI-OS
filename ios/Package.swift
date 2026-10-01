// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "NormaCore",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "NormaCore", targets: ["NormaCore"]),
               .executable(name: "PairingProbe", targets: ["PairingProbe"])],
    targets: [
        .binaryTarget(name: "NormaPairing", path: "Libraries/NormaPairing.xcframework"),
        .target(name: "NormaCore", dependencies: ["NormaPairing"], linkerSettings: [.linkedLibrary("sqlite3")]),
        .executableTarget(name: "PairingProbe", dependencies: ["NormaCore"], path: "Tests/PairingProbe"),
        .testTarget(name: "NormaCoreTests", dependencies: ["NormaCore"], resources: [.copy("Fixtures/relay-peer.cjs")])
    ]
)
