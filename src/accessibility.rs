// Music.app initialization
// Launches Music in background at TUI startup

use std::process::Command;

/// Initialize Music app (launch only, no window) at app startup
pub fn init_music_window_offscreen() {
    // Just launch Music without creating window
    let _ = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Music\" to launch")
        .output();
}
