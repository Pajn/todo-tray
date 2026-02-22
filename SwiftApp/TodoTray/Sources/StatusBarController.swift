import Cocoa
import os.log

/// Manages the status bar item and menu
/// This file is compiled together with the UniFFI-generated todo_tray_core.swift
class StatusBarController: NSObject {
    private var statusItem: NSStatusItem!
    private var core: TodoTrayCore!
    private var currentState: AppState?
    private var eventHandler: TodoTrayEventHandler!
    private let logger = OSLog(subsystem: "com.todo-tray.app", category: "StatusBarController")
    
    override init() {
        super.init()
        os_log("StatusBarController init started", log: logger, type: .info)
        
        // Create status bar item
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.title = "..."
        os_log("Status bar item created", log: logger, type: .info)
        
        // Set up notification authorization
        NotificationManager.shared.requestAuthorization()
        os_log("Notification authorization requested", log: logger, type: .info)
        
        // Create event handler
        eventHandler = TodoTrayEventHandler(controller: self)
        os_log("Event handler created", log: logger, type: .info)
        
        // Initialize Rust core
        os_log("Initializing Rust core...", log: logger, type: .info)
        do {
            core = try TodoTrayCore(eventHandler: eventHandler)
            os_log("Rust core initialized successfully", log: logger, type: .info)
        } catch {
            os_log("Failed to initialize Rust core: %{public}@", log: logger, type: .error, error.localizedDescription)
            showError("Failed to initialize: \(error.localizedDescription)")
            return
        }
        
        // Build initial menu
        rebuildMenu()
        os_log("Initial menu built", log: logger, type: .info)
        
        os_log("StatusBarController init completed", log: logger, type: .info)
    }
    
    /// Update the state from Rust
    func updateState(_ state: AppState) {
        os_log("updateState called with %d overdue, %d today", log: logger, type: .info, state.overdueCount, state.todayCount)
        currentState = state
        updateMenuBar()
        rebuildMenu()
    }
    
    /// Show an error
    func showError(_ message: String) {
        os_log("Showing error: %{public}@", log: logger, type: .error, message)
        statusItem.button?.title = "!"
        statusItem.button?.toolTip = "Todo Tray - Error: \(message)"
        
        // Build error menu
        let menu = NSMenu()
        menu.addItem(withTitle: "Error", action: nil, keyEquivalent: "")
        menu.addItem(withTitle: message, action: nil, keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(createMenuItem("Quit", action: #selector(quit), keyEquivalent: "q"))
        statusItem.menu = menu
    }
    
    /// Update the menu bar title
    private func updateMenuBar() {
        guard let state = currentState else {
            statusItem.button?.title = "..."
            return
        }
        
        if state.overdueCount > 0 {
            statusItem.button?.title = "! \(state.overdueCount)"
        } else if state.todayCount > 0 {
            statusItem.button?.title = "\(state.todayCount)"
        } else {
            statusItem.button?.title = "0"
        }
        
        statusItem.button?.toolTip = "Todo Tray - \(state.overdueCount) overdue, \(state.todayCount) today"
        os_log("Menu bar title updated to: %{public}@", log: logger, type: .info, statusItem.button?.title ?? "nil")
    }
    
    /// Rebuild the menu
    private func rebuildMenu() {
        let menu = NSMenu()
        
        guard let state = currentState else {
            // Loading menu
            menu.addItem(withTitle: "Loading...", action: nil, keyEquivalent: "")
            menu.addItem(.separator())
            menu.addItem(createMenuItem("Quit", action: #selector(quit), keyEquivalent: "q"))
            statusItem.menu = menu
            return
        }
        
        // Check if we should show tomorrow section (after noon)
        let showTomorrow = Calendar.current.component(.hour, from: Date()) >= 12
        
        // Overdue section
        if !state.tasks.overdue.isEmpty {
            menu.addItem(createHeader("Overdue"))
            for task in state.tasks.overdue {
                menu.addItem(createTaskItem(task))
            }
            menu.addItem(.separator())
        }
        
        // Today section
        if !state.tasks.today.isEmpty {
            menu.addItem(createHeader("Today"))
            for task in state.tasks.today {
                menu.addItem(createTaskItem(task))
            }
            menu.addItem(.separator())
        }
        
        // Tomorrow section (only after noon)
        if showTomorrow && !state.tasks.tomorrow.isEmpty {
            menu.addItem(createHeader("Tomorrow"))
            for task in state.tasks.tomorrow {
                menu.addItem(createTaskItem(task))
            }
            menu.addItem(.separator())
        }
        
        // No tasks message
        if state.tasks.overdue.isEmpty && state.tasks.today.isEmpty && (!showTomorrow || state.tasks.tomorrow.isEmpty) {
            let item = menu.addItem(withTitle: "No tasks for today", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(.separator())
        }
        
        // Controls
        menu.addItem(createMenuItem("Refresh", action: #selector(refresh), keyEquivalent: "r"))
        menu.addItem(createAutostartItem(state.autostartEnabled))
        menu.addItem(.separator())
        menu.addItem(createMenuItem("Quit", action: #selector(quit), keyEquivalent: "q"))
        
        statusItem.menu = menu
    }
    
    /// Create a simple menu item with target set to self
    private func createMenuItem(_ title: String, action: Selector, keyEquivalent: String = "") -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: keyEquivalent)
        item.target = self
        return item
    }
    
    /// Create a header menu item
    private func createHeader(_ title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }
    
    /// Create a task menu item with custom view (task name + right-aligned greyed time)
    private func createTaskItem(_ task: TodoTask) -> NSMenuItem {
        let item = NSMenuItem(title: task.content, action: #selector(completeTask(_:)), keyEquivalent: "")
        item.target = self
        
        // Create custom view with right-aligned time
        let taskView = TaskMenuItemView(title: task.content, time: task.displayTime)
        item.view = taskView
        item.representedObject = task.id
        
        return item
    }
    
    /// Create autostart toggle menu item
    private func createAutostartItem(_ enabled: Bool) -> NSMenuItem {
        let title = enabled ? "✓ Autostart" : "Autostart"
        let item = NSMenuItem(title: title, action: #selector(toggleAutostart), keyEquivalent: "")
        item.target = self
        return item
    }
    
    // MARK: - Actions
    
    @objc func refresh() {
        os_log("Refresh triggered", log: logger, type: .info)
        // Use async task wrapper to avoid confusion with Swift.Task
        refreshAsync()
    }
    
    private func refreshAsync() {
        Task { @MainActor in
            do {
                try core.refresh()
            } catch {
                showError("Failed to refresh: \(error.localizedDescription)")
            }
        }
    }
    
    @objc func completeTask(_ sender: NSMenuItem) {
        guard let taskId = sender.representedObject as? String else { return }
        os_log("Complete task: %{public}@", log: logger, type: .info, taskId)
        
        // Close the menu immediately for better UX
        statusItem.menu?.cancelTracking()
        
        // Optimistically remove the task from local state and rebuild menu
        if var state = currentState {
            state.tasks.overdue.removeAll { $0.id == taskId }
            state.tasks.today.removeAll { $0.id == taskId }
            state.tasks.tomorrow.removeAll { $0.id == taskId }
            state.overdueCount = UInt32(state.tasks.overdue.count)
            state.todayCount = UInt32(state.tasks.today.count)
            currentState = state
            updateMenuBar()
            rebuildMenu()
        }
        
        Task { @MainActor in
            do {
                try core.complete(taskId: taskId)
                // The complete method already calls refresh, which will update the state
            } catch {
                showError("Failed to complete task: \(error.localizedDescription)")
            }
        }
    }
    
    @objc func toggleAutostart() {
        os_log("Toggle autostart", log: logger, type: .info)
        do {
            _ = try core.toggleAutostart()
        } catch {
            showError("Failed to toggle autostart: \(error.localizedDescription)")
        }
    }
    
    @objc func quit() {
        os_log("Quit requested", log: logger, type: .info)
        NSApp.terminate(nil)
    }
}
