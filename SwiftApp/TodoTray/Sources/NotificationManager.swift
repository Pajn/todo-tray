import Foundation
import UserNotifications

/// Manages notifications using UNUserNotificationCenter
class NotificationManager: NSObject {
    static let shared = NotificationManager()
    
    private override init() {
        super.init()
    }
    
    func requestAuthorization() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
            if granted {
                print("Notification authorization granted")
            } else if let error = error {
                print("Notification authorization error: \(error)")
            }
        }
    }
    
    func showOverdue(count: Int, taskNames: [String]) {
        let content = UNMutableNotificationContent()
        
        if count == 1 {
            content.title = "Task Overdue"
            content.subtitle = taskNames.first ?? "Task needs attention"
        } else {
            content.title = "\(count) Tasks Overdue"
            content.subtitle = "\(count) tasks need attention"
        }
        content.body = "Click to view in Todo Tray"
        content.sound = .default
        
        let request = UNNotificationRequest(
            identifier: "overdue-\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        
        UNUserNotificationCenter.current().add(request)
    }
    
    func showTaskCompleted(taskName: String) {
        let content = UNMutableNotificationContent()
        content.title = "Task Completed"
        content.subtitle = truncate(taskName, maxLength: 50)
        content.body = "Great job!"
        content.sound = .default
        
        let request = UNNotificationRequest(
            identifier: "completed-\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        
        UNUserNotificationCenter.current().add(request)
    }
    
    private func truncate(_ string: String, maxLength: Int) -> String {
        if string.count <= maxLength {
            return string
        }
        let index = string.index(string.startIndex, offsetBy: maxLength - 1)
        return String(string[..<index]) + "…"
    }
}
