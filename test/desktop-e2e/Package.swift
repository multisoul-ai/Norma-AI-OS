// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "desktop-e2e",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "norma-e2e", targets: ["norma-e2e"]),
    ],
    targets: [
        .target(
            name: "E2EKit",
            path: "Sources/E2EKit",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("Vision"),
                .linkedLibrary("sqlite3"),
            ]
        ),
        .target(
            name: "Fixtures",
            dependencies: ["E2EKit"],
            path: "Fixtures"
        ),
        .target(
            name: "Scenarios",
            dependencies: ["E2EKit", "Fixtures"],
            path: "Scenarios"
        ),
        .executableTarget(
            name: "norma-e2e",
            dependencies: ["E2EKit", "Scenarios"],
            path: "Sources/norma-e2e"
        ),
    ]
)
