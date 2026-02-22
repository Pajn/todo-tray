import Foundation
import UserNotifications

/// Implementation of the Rust EventHandler protocol
/// This file is compiled together with the UniFFI-generated todo_tray_core.swift
class TodoTrayEventHandler: EventHandler {
    weak var controller: StatusBarController?
    
    init(controller: StatusBarController) {
        self.controller = controller
    }
    
    func onStateChanged(state: AppState) {
        DispatchQueue.main.async { [weak self] in
            self?.controller?.updateState(state)
        }
    }
    
    func onTaskCompleted(taskName: String) {
        DispatchQueue.main.async {
            NotificationManager.shared.showTaskCompleted(taskName: taskName)
        }
    }
    
    func onError(error: String) {
        DispatchQueue.main.async { [weak self] in
            self?.controller?.showError(error)
        }
    }
}
