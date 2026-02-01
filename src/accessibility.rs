// Accessibility API for Music.app control
// Uses hybrid approach: AppleScript for selection + Accessibility API for Play button
// Music process is hidden (visible=false) - no window appears on screen

use accessibility::{AXAttribute, AXUIElement};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use std::process::Command;

/// Initialize Music app (launch only) at app startup
pub fn init_music_window_offscreen() {
    let _ = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Music\" to launch")
        .output();
}

fn get_music_pid() -> Option<i32> {
    let output = Command::new("pgrep")
        .arg("-x")
        .arg("Music")
        .output()
        .ok()?;
    let pid_str = String::from_utf8_lossy(&output.stdout);
    pid_str.trim().parse().ok()
}

fn get_element_title(element: &AXUIElement) -> Option<String> {
    element
        .attribute(&AXAttribute::title())
        .ok()
        .map(|t| unsafe {
            let cf_str = CFString::wrap_under_get_rule(t.as_CFTypeRef() as _);
            cf_str.to_string()
        })
}

fn get_element_role(element: &AXUIElement) -> Option<String> {
    element
        .attribute(&AXAttribute::role())
        .ok()
        .map(|r| unsafe {
            let cf_str = CFString::wrap_under_get_rule(r.as_CFTypeRef() as _);
            cf_str.to_string()
        })
}

fn get_children(element: &AXUIElement) -> Vec<AXUIElement> {
    let mut result = Vec::new();
    if let Ok(children) = element.attribute(&AXAttribute::children()) {
        let children_array: CFArray<CFType> =
            unsafe { CFArray::wrap_under_get_rule(children.as_CFTypeRef() as _) };
        for i in 0..children_array.len() {
            if let Some(child_ref) = children_array.get(i) {
                let child =
                    unsafe { AXUIElement::wrap_under_get_rule(child_ref.as_CFTypeRef() as _) };
                result.push(child);
            }
        }
    }
    result
}

fn find_element_by_role_and_title(
    element: &AXUIElement,
    role: &str,
    title: Option<&str>,
    depth: usize,
) -> Option<AXUIElement> {
    if depth > 15 {
        return None;
    }

    if let Some(elem_role) = get_element_role(element) {
        if elem_role == role {
            if let Some(expected_title) = title {
                if let Some(elem_title) = get_element_title(element) {
                    if elem_title == expected_title {
                        return Some(element.clone());
                    }
                }
            } else {
                return Some(element.clone());
            }
        }
    }

    for child in get_children(element) {
        if let Some(found) = find_element_by_role_and_title(&child, role, title, depth + 1) {
            return Some(found);
        }
    }

    None
}

fn click_element(element: &AXUIElement) -> Result<(), String> {
    element
        .perform_action(&CFString::new("AXPress"))
        .map_err(|e| format!("{:?}", e))
}

/// Ensure Music window exists but process is hidden
fn ensure_music_hidden_with_window() -> Result<(), String> {
    let script = r#"
        tell application "Music" to launch
        delay 0.1
        tell application "System Events"
            tell process "Music"
                if (count of windows) = 0 then
                    tell application "Music" to reopen
                    delay 0.1
                end if
                set visible to false
            end tell
        end tell
    "#;

    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed: {}", e))?;
    Ok(())
}

/// Select a playlist in the sidebar (process stays hidden)
fn select_sidebar_item(item_name: &str) -> Result<(), String> {
    let script = format!(
        r#"tell application "System Events"
            tell process "Music"
                set visible to false
                set sidebarOutline to outline 1 of scroll area 1 of splitter group 1 of window 1
                repeat with r in rows of sidebarOutline
                    try
                        if name of UI element 1 of r is "{}" then
                            select r
                            return "found"
                        end if
                    end try
                end repeat
                return "not found"
            end tell
        end tell"#,
        item_name.replace('"', "\\\"")
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if result == "found" {
        Ok(())
    } else {
        Err(format!("'{}' not found in sidebar", item_name))
    }
}

/// Click the Play button using Accessibility API
fn click_play_button() -> Result<(), String> {
    let pid = get_music_pid().ok_or("Music.app is not running")?;
    let music_app = AXUIElement::application(pid);

    let main_window = music_app
        .attribute(&AXAttribute::new(&CFString::new("AXMainWindow")))
        .map_err(|_| "No main window")?;

    let window = unsafe { AXUIElement::wrap_under_get_rule(main_window.as_CFTypeRef() as _) };

    let play_button = find_element_by_role_and_title(&window, "AXButton", Some("Play"), 0)
        .ok_or("Play button not found")?;

    click_element(&play_button)
}

/// Play a playlist with proper context (queue, not AutoPlay)
pub fn play_playlist_with_context(playlist_name: &str) -> Result<(), String> {
    ensure_music_hidden_with_window()?;
    select_sidebar_item(playlist_name)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    click_play_button()?;
    Ok(())
}

/// Play an album with proper context (queue, not AutoPlay)
pub fn play_album_with_context(album_name: &str) -> Result<(), String> {
    ensure_music_hidden_with_window()?;
    select_sidebar_item("Albums")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Reveal AND select the album's first track
    let script = format!(
        r#"tell application "Music"
            set albumTracks to (every track of library playlist 1 whose album is "{}")
            if (count of albumTracks) > 0 then
                set firstTrack to item 1 of albumTracks
                reveal firstTrack
                -- Try to select the track for playback context
                try
                    set selection to firstTrack
                end try
            else
                error "Album not found"
            end if
        end tell
        tell application "System Events"
            tell process "Music"
                set visible to false
            end tell
        end tell"#,
        album_name.replace('"', "\\\"")
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("Failed: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{}", err.trim()));
    }

    std::thread::sleep(std::time::Duration::from_millis(150));
    click_play_button()?;
    Ok(())
}
