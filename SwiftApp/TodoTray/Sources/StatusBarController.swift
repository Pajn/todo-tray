import Cocoa
import os.log

private final class GitHubNotificationMenuPayload: NSObject {
    let accountName: String
    let threadId: String
    let webUrl: String
    
    init(accountName: String, threadId: String, webUrl: String) {
        self.accountName = accountName
        self.threadId = threadId
        self.webUrl = webUrl
    }
}

private final class TodoistSnoozeMenuPayload: NSObject {
    let taskId: String
    let durationLabel: String
    
    init(taskId: String, durationLabel: String) {
        self.taskId = taskId
        self.durationLabel = durationLabel
    }
}

private final class CalendarEventMenuPayload: NSObject {
    let webUrl: String

    init(webUrl: String) {
        self.webUrl = webUrl
    }
}

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
        os_log(
            "updateState called with %d overdue, %d today, %d linear in progress, %d github notifications, %d calendar events",
            log: logger,
            type: .info,
            state.overdueCount,
            state.todayCount,
            state.inProgressCount,
            state.githubNotificationCount,
            state.calendarEventCount
        )
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
        
        let overdue = Int(state.overdueCount)
        let github = Int(state.githubNotificationCount)
        let today = Int(state.todayCount)
        let linear = Int(state.inProgressCount)
        let calendar = Int(state.calendarEventCount)
        
        var title: String
        
        if overdue > 0 && github > 0 {
            title = "!\(overdue) + \(github)"
        } else if overdue > 0 {
            title = "!\(overdue)"
        } else if github > 0 {
            title = "0 + \(github)"
        } else if today > 0 {
            title = "\(today)"
        } else if linear > 0 {
            title = "L\(linear)"
        } else if calendar > 0 {
            title = "C\(calendar)"
        } else {
            title = "0"
        }
        
        statusItem.button?.title = title
        statusItem.button?.toolTip = "Todo Tray - \(overdue) overdue, \(today) today, \(linear) linear in progress, \(github) GitHub notifications, \(calendar) calendar events"
        os_log("Menu bar title updated to: %{public}@", log: logger, type: .info, title)
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

        // Linear in-progress section
        if !state.tasks.inProgress.isEmpty {
            menu.addItem(createHeader("Linear · In Progress"))
            for task in state.tasks.inProgress {
                menu.addItem(createTaskItem(task))
            }
            menu.addItem(.separator())
        }
        
        // GitHub notifications grouped by account
        for section in state.githubNotifications where !section.notifications.isEmpty {
            menu.addItem(createHeader("GitHub · \(section.accountName)"))
            for notification in section.notifications {
                menu.addItem(createGitHubNotificationItem(notification, accountName: section.accountName))
            }
            menu.addItem(.separator())
        }

        // Calendar events grouped by feed/account
        for section in state.calendarEvents where !section.events.isEmpty {
            menu.addItem(createHeader("Calendar · \(section.accountName)"))
            for event in section.events {
                menu.addItem(createCalendarEventItem(event))
            }
            menu.addItem(.separator())
        }
        
        // No tasks message
        if state.tasks.overdue.isEmpty
            && state.tasks.today.isEmpty
            && (!showTomorrow || state.tasks.tomorrow.isEmpty)
            && state.tasks.inProgress.isEmpty
            && state.githubNotifications.allSatisfy({ $0.notifications.isEmpty })
            && state.calendarEvents.allSatisfy({ $0.events.isEmpty })
        {
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
        if task.source == "todoist" && task.canComplete {
            return createTodoistTaskSubmenu(task)
        }

        let action: Selector? = if task.canComplete {
            #selector(completeTask(_:))
        } else if task.source == "linear", task.openUrl != nil {
            #selector(openLinearTask(_:))
        } else {
            nil
        }

        let item = NSMenuItem(title: task.content, action: action, keyEquivalent: "")
        item.target = action != nil ? self : nil
        item.isEnabled = action != nil
        
        // Create custom view with right-aligned time
        let taskView = TaskMenuItemView(title: task.content, time: task.displayTime)
        item.view = taskView
        if task.canComplete {
            item.representedObject = task.id
        } else if let openUrl = task.openUrl {
            item.representedObject = openUrl
        }
        
        return item
    }

    private func createTodoistTaskSubmenu(_ task: TodoTask) -> NSMenuItem {
        let item = NSMenuItem(title: "\(task.content) · \(task.displayTime)", action: nil, keyEquivalent: "")
        let submenu = NSMenu(title: task.content)

        let resolve = NSMenuItem(title: "Resolve", action: #selector(completeTask(_:)), keyEquivalent: "")
        resolve.target = self
        resolve.representedObject = task.id
        submenu.addItem(resolve)

        let durations = (currentState?.snoozeDurations.isEmpty == false)
            ? (currentState?.snoozeDurations ?? [])
            : ["30m", "1d"]
        for duration in durations {
            let snooze = NSMenuItem(
                title: "Snooze \(duration)",
                action: #selector(snoozeTodoistTask(_:)),
                keyEquivalent: ""
            )
            snooze.target = self
            snooze.representedObject = TodoistSnoozeMenuPayload(taskId: task.id, durationLabel: duration)
            submenu.addItem(snooze)
        }

        item.submenu = submenu
        return item
    }
    
    /// Create a GitHub notification item that opens in browser and resolves it.
    private func createGitHubNotificationItem(_ notification: GithubNotification, accountName: String) -> NSMenuItem {
        let item = NSMenuItem(title: notification.title, action: #selector(openGitHubNotification(_:)), keyEquivalent: "")
        item.target = self
        
        let view = TaskMenuItemView(
            title: "\(notification.title) (\(notification.reason))",
            time: notification.repository
        )
        item.view = view
        item.representedObject = GitHubNotificationMenuPayload(
            accountName: accountName,
            threadId: notification.threadId,
            webUrl: notification.webUrl
        )
        
        return item
    }
    
    /// Create autostart toggle menu item
    private func createAutostartItem(_ enabled: Bool) -> NSMenuItem {
        let title = enabled ? "✓ Autostart" : "Autostart"
        let item = NSMenuItem(title: title, action: #selector(toggleAutostart), keyEquivalent: "")
        item.target = self
        return item
    }

    private func createCalendarEventItem(_ event: CalendarEvent) -> NSMenuItem {
        let action: Selector? = event.openUrl != nil ? #selector(openCalendarEvent(_:)) : nil
        let item = NSMenuItem(title: event.title, action: action, keyEquivalent: "")
        item.target = action != nil ? self : nil
        item.isEnabled = action != nil
        item.view = TaskMenuItemView(title: event.title, time: event.displayTime)
        if let url = event.openUrl {
            item.representedObject = CalendarEventMenuPayload(webUrl: url)
        }
        return item
    }
    
    // MARK: - Actions
    
    @objc func refresh() {
        os_log("Refresh triggered", log: logger, type: .info)
        // Use async task wrapper to avoid confusion with Swift.Task
        refreshAsync()
    }
    
    private func refreshAsync() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self else { return }
            do {
                try self.core.refresh()
            } catch {
                DispatchQueue.main.async { [weak self] in
                    self?.showError("Failed to refresh: \(error.localizedDescription)")
                }
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
            state.tasks.inProgress.removeAll { $0.id == taskId }
            state.overdueCount = UInt32(state.tasks.overdue.count)
            state.todayCount = UInt32(state.tasks.today.count)
            state.inProgressCount = UInt32(state.tasks.inProgress.count)
            currentState = state
            updateMenuBar()
            rebuildMenu()
        }
        
        guard let core else { return }
        DispatchQueue.global(qos: .utility).async { [weak self] in
            do {
                try core.complete(taskId: taskId)
            } catch {
                DispatchQueue.main.async { [weak self] in
                    self?.showError("Failed to complete task: \(error.localizedDescription)")
                }
            }
        }
    }

    @objc func openLinearTask(_ sender: NSMenuItem) {
        guard let openUrl = sender.representedObject as? String else { return }
        os_log("Open Linear issue: %{public}@", log: logger, type: .info, openUrl)
        
        // Close the menu immediately for better UX
        statusItem.menu?.cancelTracking()
        
        guard let url = URL(string: openUrl) else {
            showError("Invalid Linear issue URL")
            return
        }
        NSWorkspace.shared.open(url)
    }

    @objc func snoozeTodoistTask(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? TodoistSnoozeMenuPayload else { return }
        os_log(
            "Snooze Todoist task %{public}@ by %{public}@",
            log: logger,
            type: .info,
            payload.taskId,
            payload.durationLabel
        )

        // Close the menu immediately for better UX
        statusItem.menu?.cancelTracking()

        guard let core else { return }
        DispatchQueue.global(qos: .utility).async { [weak self] in
            do {
                try core.snoozeTask(taskId: payload.taskId, durationLabel: payload.durationLabel)
            } catch {
                DispatchQueue.main.async { [weak self] in
                    self?.showError("Failed to snooze task: \(error.localizedDescription)")
                }
            }
        }
    }
    
    @objc func openGitHubNotification(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? GitHubNotificationMenuPayload else { return }
        os_log(
            "Open GitHub notification account=%{public}@ thread=%{public}@",
            log: logger,
            type: .info,
            payload.accountName,
            payload.threadId
        )
        
        // Close the menu immediately for better UX
        statusItem.menu?.cancelTracking()
        
        guard let url = URL(string: payload.webUrl) else {
            showError("Invalid GitHub notification URL")
            return
        }
        NSWorkspace.shared.open(url)
        
        // Optimistically remove the notification and update counts.
        if var state = currentState {
            for index in state.githubNotifications.indices {
                if state.githubNotifications[index].accountName == payload.accountName {
                    state.githubNotifications[index].notifications.removeAll { $0.threadId == payload.threadId }
                }
            }
            state.githubNotifications.removeAll { $0.notifications.isEmpty }
            state.githubNotificationCount = UInt32(
                state.githubNotifications.reduce(0) { partial, section in
                    partial + section.notifications.count
                }
            )
            currentState = state
            updateMenuBar()
            rebuildMenu()
        }
        
        guard let core else { return }
        let accountName = payload.accountName
        let threadId = payload.threadId
        DispatchQueue.global(qos: .utility).async { [weak self] in
            do {
                try core.resolveGithubNotification(
                    accountName: accountName,
                    threadId: threadId
                )
            } catch {
                DispatchQueue.main.async { [weak self] in
                    self?.showError("Failed to resolve GitHub notification: \(error.localizedDescription)")
                }
            }
        }
    }

    @objc func openCalendarEvent(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? CalendarEventMenuPayload else { return }
        os_log("Open calendar event URL: %{public}@", log: logger, type: .info, payload.webUrl)

        // Close the menu immediately for better UX
        statusItem.menu?.cancelTracking()

        guard let url = URL(string: payload.webUrl) else {
            showError("Invalid calendar event URL")
            return
        }
        NSWorkspace.shared.open(url)
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
