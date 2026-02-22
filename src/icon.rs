/// Generate tray icon
/// Returns RGBA pixel data for a simple circle icon
pub fn generate_tray_icon() -> Vec<u8> {
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    // Create a simple circle icon
    let center = size as f32 / 2.0;
    let radius = size as f32 / 2.0 - 2.0;

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;

            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Inside circle - use a dark gray for visibility in both light/dark mode
                let alpha = if dist > radius - 1.5 {
                    // Anti-aliasing edge
                    ((radius - dist) / 1.5).clamp(0.0, 1.0)
                } else {
                    1.0
                };

                // Gray color that works in both modes
                rgba[idx] = (80.0 * alpha) as u8; // R
                rgba[idx + 1] = (80.0 * alpha) as u8; // G
                rgba[idx + 2] = (80.0 * alpha) as u8; // B
                rgba[idx + 3] = (255.0 * alpha) as u8; // A
            }
        }
    }

    rgba
}

/// Format task display for menu item
/// Uses tab to right-align the time in a separate column
pub fn format_task_menu_item(task: &crate::todoist::Task) -> String {
    let time = task.display_time();

    if time != "no due date" {
        // Use tab to right-align time in a separate column
        format!("{}\t{}", truncate(&task.content, 35), time)
    } else {
        truncate(&task.content, 40)
    }
}

/// Truncate string with ellipsis if too long
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Format the tray icon title (shown in menubar)
pub fn format_tray_title(overdue_count: usize, today_count: usize) -> String {
    if overdue_count > 0 {
        format!("! {}", overdue_count)
    } else if today_count > 0 {
        format!("{}", today_count)
    } else {
        "0".to_string()
    }
}
