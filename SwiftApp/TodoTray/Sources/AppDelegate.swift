import Cocoa
import os.log

class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusBarController: StatusBarController?
    private let logger = OSLog(subsystem: "com.todo-tray.app", category: "AppDelegate")
    
    func applicationDidFinishLaunching(_ notification: Notification) {
        os_log("applicationDidFinishLaunching called", log: logger, type: .info)
        
        // Set up as an accessory app (no dock icon)
        NSApp.setActivationPolicy(.accessory)
        os_log("Set activation policy to accessory", log: logger, type: .info)
        
        // Initialize the status bar controller
        os_log("Creating StatusBarController...", log: logger, type: .info)
        statusBarController = StatusBarController()
        os_log("StatusBarController created", log: logger, type: .info)
    }
    
    func applicationWillTerminate(_ notification: Notification) {
        os_log("applicationWillTerminate", log: logger, type: .info)
        // Cleanup
        statusBarController = nil
    }
}
