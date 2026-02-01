# Playback System Details

> **Tested with**: Music.app Version 1.6.0.151 (macOS Sequoia, 2025-02-01)

## Architecture Decision Record (ADR)

### Context

We needed to implement playback functionality that:

1. Plays a specific track when selected in the TUI
2. Queues the remaining tracks in the album/playlist
3. Does not trigger AutoPlay mode
4. Keeps the Music.app GUI hidden

### Decision

We adopted the **Temporary Playlist + Sidebar Selection** approach.

### Alternatives Considered

| Approach                    | Result | Why Rejected                                          |
| --------------------------- | ------ | ----------------------------------------------------- |
| AppleScript `play track`    | ❌     | Only queues ONE track, then AutoPlay activates        |
| AppleScript `play playlist` | ❌     | Still triggers AutoPlay after playlist ends           |
| AppleScript `reveal` + Play | ❌     | Plays previously selected content, not revealed track |
| Direct album selection      | ❌     | Albums don't appear in sidebar (impossible)           |
| **Temp playlist + sidebar** | ✅     | **Adopted** - queues all tracks properly              |

### Consequences

- **Positive**: Reliable queue behavior, no AutoPlay, works while hidden
- **Negative**: Slightly complex implementation, requires Accessibility API permission

---

## Background and Challenges

The macOS Music.app AppleScript API has important constraints regarding playback.

### Fundamental Problem: Single Track Queuing

The most straightforward approach using AppleScript does **not** queue multiple tracks:

```applescript
tell application "Music"
    play track 3 of library playlist 1 whose album is "Help!"
end tell
```

**Expected behavior:**

```
Queue:
├─ Track 3 (now playing)
├─ Track 4
├─ Track 5
└─ ...
```

**Actual behavior:**

```
Queue:
└─ Track 3 (now playing) ← Only this ONE track!

After track ends:
└─ AutoPlay activates → Random songs based on listening history
```

This is the root cause of why we cannot use simple AppleScript playback.

### AutoPlay Problem

```applescript
-- This method triggers AutoPlay
tell application "Music"
    play track 1 of library playlist 1
end tell
```

When playing with this method, Music.app enters "AutoPlay" mode, which automatically plays random songs based on user's listening history after the current song or album ends.

### The reveal Problem

```applescript
tell application "Music"
    reveal track 1 of library playlist 1
end tell
```

The `reveal` command only displays the track in the UI but does not "select" it. Clicking the Play button afterwards will play whatever was previously selected.

### Why Albums Cannot Use Direct Sidebar Selection

Music.app's sidebar has a specific structure that treats playlists and albums differently:

```
┌─────────────────────────────────────────────────────────────┐
│  Music.app Sidebar                                          │
├─────────────────────────────────────────────────────────────┤
│  Library                                                    │
│    ├─ Recently Added      ← Category (not selectable)      │
│    ├─ Artists             ← Category (not selectable)      │
│    ├─ Albums              ← Category (not selectable)      │
│    └─ Songs               ← Category (not selectable)      │
│                                                             │
│  Playlists                                                  │
│    ├─ My Playlist 1       ← Individual playlist (SELECTABLE)│
│    ├─ My Playlist 2       ← Individual playlist (SELECTABLE)│
│    └─ ___TempQueue___     ← Temp playlist (SELECTABLE)     │
└─────────────────────────────────────────────────────────────┘
```

**Key difference:**

- **Playlists**: Individual playlists appear as sidebar items → can be selected via System Events
- **Albums**: Only the "Albums" category appears → individual albums (e.g., "Help!") are NOT in sidebar

This is why:

- For **playlists**: Direct sidebar selection works (but we still use temp playlist for rotation)
- For **albums**: We MUST create a temporary playlist to make it appear in the sidebar

## Solution: Temporary Playlist Approach

### Discovery

When selecting a playlist in the sidebar and clicking the Play button:

- All tracks are added to the queue
- AutoPlay is not triggered
- GUI can be operated while hidden

Based on this discovery, we adopted the following approach.

### Circular Playlist

When the user selects track N of an album, we create a temporary playlist containing tracks in the following order:

```
Original album: [1, 2, 3, 4, 5]
If N = 3 is selected:

Temporary playlist: [3, 4, 5, 1, 2]
                    ↑ Start from selected track
                          ↑ Play to end
                             ↑ Wrap around to beginning
```

### Processing Steps

```
1. create_rotated_playlist_from_album()
   ┌────────────────────────────────────────┐
   │ Create temp playlist with AppleScript  │
   │ - Delete existing ___TempQueue___      │
   │ - Create new playlist                  │
   │ - Add tracks from N to end             │
   │ - Add tracks from 1 to N-1             │
   └────────────────────────────────────────┘
                    │
                    ▼
2. ensure_music_hidden_with_window()
   ┌────────────────────────────────────────┐
   │ Prepare Music.app window               │
   │ - launch Music                         │
   │ - reopen if no window                  │
   │ - set visible to false (hidden)        │
   └────────────────────────────────────────┘
                    │
                    ▼
3. select_sidebar_item("___TempQueue___")
   ┌────────────────────────────────────────┐
   │ Manipulate sidebar with System Events  │
   │ - Get outline 1 of scroll area 1       │
   │ - Search through rows                  │
   │ - Select matching row                  │
   └────────────────────────────────────────┘
                    │
                    ▼
4. click_play_button()
   ┌────────────────────────────────────────┐
   │ Operate Play button via Accessibility  │
   │ - Get Music.app with AXUIElement       │
   │ - Get AXMainWindow                     │
   │ - Search for AXButton "Play"           │
   │ - Execute AXPress action               │
   └────────────────────────────────────────┘
                    │
                    ▼
5. delete_temp_playlist()
   ┌────────────────────────────────────────┐
   │ Delete temporary playlist              │
   │ - Wait 500ms (confirm playback start)  │
   │ - Delete ___TempQueue___               │
   └────────────────────────────────────────┘
```

## AppleScript Details

### Creating Temporary Playlist

```applescript
tell application "Music"
    -- Get album tracks
    set allTracks to (every track of library playlist 1 whose album is "AlbumName")
    set trackCount to count of allTracks

    -- Delete existing temp playlist
    try
        delete (first playlist whose name is "___TempQueue___")
    end try

    -- Create new playlist
    set tempPlaylist to make new playlist with properties {name:"___TempQueue___"}

    -- Add from track N to end
    repeat with i from N to trackCount
        duplicate (item i of allTracks) to tempPlaylist
    end repeat

    -- Add from track 1 to N-1
    if N > 1 then
        repeat with i from 1 to (N - 1)
            duplicate (item i of allTracks) to tempPlaylist
        end repeat
    end if
end tell
```

### Sidebar Selection

```applescript
tell application "System Events"
    tell process "Music"
        set visible to false
        set sidebarOutline to outline 1 of scroll area 1 of splitter group 1 of window 1
        repeat with r in rows of sidebarOutline
            try
                if name of UI element 1 of r is "___TempQueue___" then
                    select r
                    return "found"
                end if
            end try
        end repeat
        return "not found"
    end tell
end tell
```

## Accessibility API Details

### UI Element Search

```rust
fn find_element_by_role_and_title(
    element: &AXUIElement,
    role: &str,           // "AXButton"
    title: Option<&str>,  // Some("Play")
    depth: usize,
) -> Option<AXUIElement> {
    // Depth limit (prevent infinite loop)
    if depth > 15 {
        return None;
    }

    // Check current element
    if get_element_role(element) == Some(role.to_string()) {
        if let Some(expected) = title {
            if get_element_title(element) == Some(expected.to_string()) {
                return Some(element.clone());
            }
        }
    }

    // Recursively search children
    for child in get_children(element) {
        if let Some(found) = find_element_by_role_and_title(&child, role, title, depth + 1) {
            return Some(found);
        }
    }

    None
}
```

### Button Click

```rust
fn click_element(element: &AXUIElement) -> Result<(), String> {
    element
        .perform_action(&CFString::new("AXPress"))
        .map_err(|e| format!("{:?}", e))
}
```

## Timing and Delays

Appropriate delays are placed between each operation:

| Operation               | Delay | Reason                           |
| ----------------------- | ----- | -------------------------------- |
| After window setup      | 100ms | Wait for UI initialization       |
| After sidebar selection | 100ms | Wait for view update             |
| After Play click        | 500ms | Confirm playback before deletion |

## Error Handling

```rust
pub fn play_album_with_context(album_name: &str, track_index: usize) -> Result<(), String> {
    // Early return if any step fails
    create_rotated_playlist_from_album(album_name, track_index)?;
    ensure_music_hidden_with_window()?;
    select_sidebar_item(TEMP_PLAYLIST_NAME)?;
    click_play_button()?;

    // Deletion can fail silently
    std::thread::sleep(std::time::Duration::from_millis(500));
    delete_temp_playlist();  // Ignore errors

    Ok(())
}
```

## Limitations and Trade-offs

### System Requirements

| Limitation                   | Description                                                                                  |
| ---------------------------- | -------------------------------------------------------------------------------------------- |
| **Accessibility Permission** | Requires accessibility permission in System Preferences → Security & Privacy → Accessibility |
| **Music.app State**          | Automatically launches Music.app if not running                                              |
| **Temp Playlist Conflict**   | Overwrites any existing playlist named `___TempQueue___`                                     |

### Sidebar Visibility Requirement (Resolved)

**Problem** (now fixed): If the "Playlists" section in Music.app's sidebar was collapsed, playback would fail.

**Solution**: The application now automatically expands the Playlists section by setting the `AXDisclosing` attribute before selecting the temporary playlist.

```applescript
-- Automatically expand Playlists section if collapsed
set isDisclosing to value of attribute "AXDisclosing" of r
if isDisclosing is false then
    set value of attribute "AXDisclosing" of r to true
    delay 0.3
end if
```

This works for both English ("Playlists") and Japanese ("プレイリスト") locales.

### Circular Playlist and Repeat Mode Issue

**Problem**: When `Repeat: OFF` in Music.app, the circular playlist causes unexpected behavior.

```
Original album: [1, 2, 3, 4, 5]
User selects track 3

Temporary playlist created: [3, 4, 5, 1, 2]

Expected with Repeat OFF:
  Play: 3 → 4 → 5 → STOP (end of original album)

Actual with Repeat OFF:
  Play: 3 → 4 → 5 → 1 → 2 → STOP (plays wrapped tracks too!)
```

**Cause**: The temporary playlist is a flat list; Music.app doesn't know where the "original end" was.

**Trade-off accepted**: This is a known compromise. To avoid AutoPlay, we accept that tracks 1 to N-1 may play when Repeat is OFF.

**Potential future solutions**:

- Create a non-circular playlist (N to end only), accepting that 1 to N-1 won't be queued
- Implement queue monitoring to stop playback at the right time (complex)

### Processing Time

For albums with many tracks, creating the temporary playlist may take noticeable time due to AppleScript's per-track `duplicate` operation.
