// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TodoTray",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(
            name: "TodoTray",
            targets: ["TodoTray"]
        )
    ],
    dependencies: [],
    targets: [
        .executableTarget(
            name: "TodoTray",
            dependencies: [],
            path: "Sources",
            linkerSettings: [
                .linkedFramework("Cocoa"),
                .linkedFramework("UserNotifications"),
            ]
        ),
    ]
)
