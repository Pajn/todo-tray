//! Todo Tray Core
//!
//! This library provides the core functionality for Todo Tray,
//! a macOS menu bar application for Todoist tasks.

uniffi::setup_scaffolding!();

mod autostart;
mod config;
mod core;
mod linear;
mod task;
mod todoist;

pub use core::{AppState, EventHandler, TodoTrayCore, TodoTrayError};
pub use task::{TaskList, TodoTask};
