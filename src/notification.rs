use anyhow::Result;

/// Send a macOS notification for overdue tasks
pub fn notify_overdue(count: usize, task_names: &[String]) -> Result<()> {
    if count == 0 {
        return Ok(());
    }

    let title = if count == 1 {
        "Task Overdue".to_string()
    } else {
        format!("{} Tasks Overdue", count)
    };

    let subtitle = if count == 1 && !task_names.is_empty() {
        task_names[0].clone()
    } else {
        format!("{} tasks need attention", count)
    };

    // Send notification using mac-notification-sys
    let _ = mac_notification_sys::send_notification(
        &title,
        Some(&subtitle),
        "Click to view in Todo Tray",
        None,
    );

    Ok(())
}

/// Send notification when task is completed
pub fn notify_task_completed(task_name: &str) -> Result<()> {
    let truncated = truncate(task_name, 50);
    let _ = mac_notification_sys::send_notification(
        "Task Completed",
        Some(&truncated),
        "Great job!",
        None,
    );

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}
