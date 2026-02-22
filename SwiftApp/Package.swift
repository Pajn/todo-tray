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
            dependencies: ["TodoTrayCore"],
            path: "Sources",
            linkerSettings: [
                .linkedFramework("Cocoa"),
                .linkedFramework("UserNotifications"),
            ]
        ),
        .binaryTarget(
            name: "TodoTrayCore",
            path: "todo_tray_core.xcframework"
        ),
    ]
)
