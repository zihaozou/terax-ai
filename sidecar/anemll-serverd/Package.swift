// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "anemll-serverd",
    platforms: [
        .macOS(.v15)
    ],
    dependencies: [
        .package(path: "../vendor/anemll-swift-cli"),
        .package(url: "https://github.com/swhitty/FlyingFox.git", from: "0.20.0"),
        .package(url: "https://github.com/apple/swift-argument-parser", from: "1.3.0"),
    ],
    targets: [
        .executableTarget(
            name: "anemll-serverd",
            dependencies: [
                .product(name: "AnemllCore", package: "anemll-swift-cli"),
                .product(name: "FlyingFox", package: "FlyingFox"),
                .product(name: "ArgumentParser", package: "swift-argument-parser"),
            ],
            // Glue code; AnemllCore itself is not Sendable-annotated, so avoid
            // Swift 6 strict-concurrency friction in this thin wrapper.
            swiftSettings: [.swiftLanguageMode(.v5)]
        )
    ]
)
