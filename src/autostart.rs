//! macOS LaunchAgent management for autostart functionality
//!
//! This module provides functions to enable/disable autostart on macOS
//! by creating/removing a LaunchAgent plist file.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// The bundle identifier for the LaunchAgent
const BUNDLE_ID: &str = "com.todo-tray.app";

/// Get the path to the LaunchAgent plist file
fn plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", BUNDLE_ID)))
}

/// Get the path to the current executable
fn executable_path() -> Result<PathBuf> {
    std::env::current_exe().context("Could not determine current executable path")
}

/// Generate the plist content for the LaunchAgent
fn generate_plist_content(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{bundle_id}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        bundle_id = BUNDLE_ID,
        executable = executable.display()
    )
}

/// Check if autostart is enabled (LaunchAgent plist exists)
pub fn is_enabled() -> bool {
    plist_path().map(|path| path.exists()).unwrap_or(false)
}

/// Enable autostart by creating the LaunchAgent plist file
pub fn enable() -> Result<()> {
    let plist_path = plist_path()?;
    let executable = executable_path()?;

    // Ensure the LaunchAgents directory exists
    if let Some(parent) = plist_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).context("Failed to create LaunchAgents directory")?;
        }
    }

    // Generate and write the plist file
    let content = generate_plist_content(&executable);
    fs::write(&plist_path, content).context("Failed to write LaunchAgent plist file")?;

    tracing::info!("Autostart enabled: created LaunchAgent at {:?}", plist_path);
    Ok(())
}

/// Disable autostart by removing the LaunchAgent plist file
pub fn disable() -> Result<()> {
    let plist_path = plist_path()?;

    if plist_path.exists() {
        fs::remove_file(&plist_path).context("Failed to remove LaunchAgent plist file")?;
        tracing::info!(
            "Autostart disabled: removed LaunchAgent at {:?}",
            plist_path
        );
    } else {
        tracing::debug!("Autostart already disabled: plist file does not exist");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_plist_content() {
        let path = PathBuf::from("/Applications/todo-tray.app/Contents/MacOS/todo-tray");
        let content = generate_plist_content(&path);

        assert!(content.contains("<?xml version=\"1.0\""));
        assert!(content.contains("<string>com.todo-tray.app</string>"));
        assert!(content.contains("/Applications/todo-tray.app/Contents/MacOS/todo-tray"));
        assert!(content.contains("<key>RunAtLoad</key>"));
        assert!(content.contains("<true/>"));
    }

    #[test]
    fn test_plist_path() {
        let path = plist_path().unwrap();
        assert!(path.ends_with("Library/LaunchAgents/com.todo-tray.app.plist"));
    }
}
