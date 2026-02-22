import Cocoa

// Entry point for Todo Tray
// We manually set up the application to ensure the delegate is properly connected

// Get the shared application
let app = NSApplication.shared

// Set activation policy for menu bar only app (no dock icon)
app.setActivationPolicy(.accessory)

// Create and set the delegate BEFORE running
// NSApplication retains its delegate, so we don't need to keep a reference
let delegate = AppDelegate()
app.delegate = delegate

// Run the event loop - this will not return until the app terminates
app.run()
