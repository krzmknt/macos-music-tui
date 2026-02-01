# macos-music-tui Architecture Documentation

## Overview

macos-music-tui is a TUI (Terminal User Interface) application for controlling macOS Music.app with keyboard.

## Module Structure

```
src/
├── main.rs          # Entry point, event loop
├── app.rs           # Application state, business logic
├── ui.rs            # UI rendering (ratatui)
├── music.rs         # Music.app control (AppleScript)
├── cache.rs         # Cache management
└── accessibility.rs # Playback control (Accessibility API + AppleScript)
```

## Thread Architecture

```
┌────────────────────────────────────────────────────────────┐
│                      Main Thread                           │
│  - Event loop (50ms polling)                               │
│  - UI rendering                                            │
│  - User input handling                                     │
└────────────────────────────────────────────────────────────┘
              │                              ▲
              │ Command                      │ Response
              ▼                              │
┌────────────────────────────────────────────────────────────┐
│                 Playback Control Thread                    │
│  - AppleScript communication with Music.app                │
│  - Periodic state updates                                  │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                    Cache Thread                            │
│  - Background loading of track metadata                    │
│  - Runs independently without blocking playback            │
└────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│                  Playlist Load Thread                      │
│  - Loading playlist list and track information             │
│  - Only loads playlists not yet cached                     │
└────────────────────────────────────────────────────────────┘
```

### Inter-thread Communication

- **Command/Response pattern**: Instructions from main thread to worker threads and responses
- **Channels (mpsc)**: Asynchronous messaging

```rust
enum Command {
    RefreshPosition,  // Update playback position
    RefreshFull,      // Update full state
}

enum Response {
    PositionUpdated(f64, bool),           // position, is_playing
    StateUpdated(TrackInfo, i32, bool, String),  // track, volume, shuffle, repeat
}
```

## Playback Control Architecture

### Temporary Playlist Approach

In Music.app, simply executing `play track` activates **AutoPlay** mode, which plays random songs after the album/playlist ends. To avoid this, we use the following approach.

#### Processing Flow

```
┌─────────────────────────────────────────────────────────────┐
│              User selects track N and plays                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  1. Create temporary playlist (___TempQueue___)             │
│     - Track order: N, N+1, ..., last, 1, 2, ..., N-1        │
│     - Rotated (circular) order                              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  2. Ensure Music window exists (process hidden)             │
│     - set visible to false                                  │
│     - Window exists but not shown on screen                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  3. Select temporary playlist in sidebar                    │
│     - Manipulate UI elements with System Events             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  4. Click Play button (Accessibility API)                   │
│     - Identify Play button with AXUIElement                 │
│     - Execute AXPress action                                │
│     - This adds all tracks to the queue                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  5. Delete temporary playlist after 500ms                   │
│     - No longer needed after playback starts                │
│     - Keeps library clean                                   │
└─────────────────────────────────────────────────────────────┘
```

#### Why This Approach?

| Approach                       | Problem                                                   |
| ------------------------------ | --------------------------------------------------------- |
| Direct `play track`            | AutoPlay activates, random songs after track ends         |
| `reveal` + Play click          | Previously selected album plays (selection context issue) |
| Sidebar selection + Play click | Correctly adds to queue ✓                                 |

#### Implementation Details

```rust
// accessibility.rs

const TEMP_PLAYLIST_NAME: &str = "___TempQueue___";

pub fn play_album_with_context(album_name: &str, track_index: usize) -> Result<(), String> {
    // 1. Create rotated temporary playlist
    create_rotated_playlist_from_album(album_name, track_index)?;

    // 2. Ensure window exists (hidden)
    ensure_music_hidden_with_window()?;

    // 3. Select in sidebar
    select_sidebar_item(TEMP_PLAYLIST_NAME)?;

    // 4. Click Play button
    std::thread::sleep(std::time::Duration::from_millis(100));
    click_play_button()?;

    // 5. Delete
    std::thread::sleep(std::time::Duration::from_millis(500));
    delete_temp_playlist();

    Ok(())
}
```

### Accessibility API Usage

Uses macOS Accessibility API to manipulate UI elements.

```rust
fn click_play_button() -> Result<(), String> {
    let pid = get_music_pid()?;
    let music_app = AXUIElement::application(pid);

    // Get main window
    let main_window = music_app
        .attribute(&AXAttribute::new(&CFString::new("AXMainWindow")))?;

    // Search for Play button
    let play_button = find_element_by_role_and_title(&window, "AXButton", Some("Play"), 0)?;

    // Execute click
    play_button.perform_action(&CFString::new("AXPress"))
}
```

## Cache System

### Purpose

- AppleScript calls to Music.app are slow (can take several seconds)
- Keep cache in memory for fast search
- Persist to file for faster startup

### Cache Files

```
~/Library/Caches/macos-music-tui/
├── tracks.json      # All track metadata
├── playlists.json   # Playlist information
└── settings.json    # User settings (highlight color)
```

### Track Cache Structure

```json
{
  "total_tracks": 30968,
  "loaded_tracks": 30968,
  "last_updated": 1706612400,
  "tracks": [
    {
      "name": "Yesterday",
      "artist": "The Beatles",
      "album": "Help!",
      "date_added": "Sunday, September 13, 2015 at 3:44:42",
      "year": 1965,
      "track_number": 13,
      "disc_number": 1,
      "time": "2:05",
      "played_count": 42,
      "favorited": true
    }
  ]
}
```

### Cache Loading Flow

```
┌─────────────────────────────────────────────────────────────┐
│                        On Startup                           │
├─────────────────────────────────────────────────────────────┤
│  1. Load cache file                                         │
│  2. Display Recently Added immediately if data exists       │
│  3. Start background cache update                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  Background Processing                      │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────┐    100ms    ┌─────────┐    100ms    ┌────────┐│
│  │ Batch 1 │ ──────────▶ │ Batch 2 │ ──────────▶ │ Batch N││
│  │ 50 trks │             │ 50 trks │             │ rest   ││
│  └─────────┘             └─────────┘             └────────┘│
│       │                       │                       │    │
│       ▼                       ▼                       ▼    │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              TrackCache (in memory)                 │   │
│  └─────────────────────────────────────────────────────┘   │
│                              │                              │
│                    Save every 100 tracks                    │
│                              ▼                              │
│                    ~/Library/Caches/.../tracks.json        │
└─────────────────────────────────────────────────────────────┘
```

### Incremental Update (Upsert)

When cache is complete, only fetches tracks added within 1 day of last update.

```rust
// If cache is complete, do incremental update
if cache_is_complete {
    if let Some(last_updated) = cache_last_updated {
        // Fetch from 1 day before last_updated
        let cutoff = last_updated.saturating_sub(86400);
        let new_tracks = MusicController::get_tracks_added_since(cutoff)?;
        cache.upsert_tracks(new_tracks);
    }
}
```

### Search

Search is performed on cache, so it's fast.

```rust
pub fn search(&mut self, query: &str) -> Vec<CachedTrack> {
    // Lazy initialization of search keys for performance
    self.ensure_search_keys();

    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    self.tracks
        .iter()
        .filter(|track| {
            // Check if all words are in search_key
            query_words.iter().all(|word| track.search_key.contains(word))
        })
        .cloned()
        .collect()
}
```

- Search starts at 3+ characters
- Space-separated AND search
- Case insensitive

### Limitations and Trade-offs

#### Why Caching is Necessary

AppleScript communication with Music.app is **extremely slow**:

| Operation             | Approximate Time |
| --------------------- | ---------------- |
| Get single track info | 50-100ms         |
| Get 1000 tracks       | 50-100 seconds   |
| Get 30000 tracks      | 25-50 minutes    |

Without caching, the TUI would be unusable. Users would wait minutes just to see their library.

#### Cache Staleness

**Problem**: Cached data becomes stale over time.

| Data Type            | Staleness Impact                                     |
| -------------------- | ---------------------------------------------------- |
| **Play count**       | May be outdated (shows count from last cache update) |
| **Last played date** | May not reflect recent plays                         |
| **Favorited status** | May not reflect recent changes in Music.app          |
| **Track ratings**    | May not reflect recent changes                       |

**Trade-off accepted**: We prioritize responsiveness over real-time accuracy. The incremental update (checking tracks added within 1 day) helps, but play counts and other metadata for existing tracks are not refreshed.

**Workaround**: Users can manually trigger a full cache refresh (though this is slow).

#### Playlist Content Not Real-time

**Problem**: Playlist track lists are cached and not fetched in real-time.

```
Timeline:
┌──────────────────────────────────────────────────────────────┐
│ 10:00  User adds song X to "My Playlist" in Music.app        │
│ 10:05  TUI shows "My Playlist" (cached, X not visible)       │
│ 10:10  Cache updates in background                           │
│ 10:15  TUI now shows song X in "My Playlist"                 │
└──────────────────────────────────────────────────────────────┘
```

**Cause**: Fetching playlist contents via AppleScript is slow. We cache playlist data to maintain UI responsiveness.

**Trade-off accepted**: Playlist changes made in Music.app may not appear immediately in the TUI. Playlists are only updated when:

- The TUI is restarted
- Background cache update runs
- User navigates away and back to the playlist

#### Summary of Trade-offs

| Optimization        | Benefit                      | Cost                                          |
| ------------------- | ---------------------------- | --------------------------------------------- |
| Track caching       | Fast search, instant display | Stale play counts, metadata                   |
| Playlist caching    | Fast navigation              | Delayed sync with Music.app changes           |
| Incremental updates | Faster startup               | Only catches new tracks, not metadata changes |

## UI Structure

### Layout

```
┌────────────────────────────────────────────────────────────┐
│ Header: Now Playing, Progress Bar, Controls                │
├────────────────────┬───────────────────────────────────────┤
│ Recently Added     │                                       │
│ (left column top)  │        Content                        │
├────────────────────┤        (track list / search results)  │
│ Playlists          │                                       │
│ (left column bot)  │                                       │
├────────────────────┴───────────────────────────────────────┤
│ Footer: Key bindings help                                  │
└────────────────────────────────────────────────────────────┘
```

### Focus Management

```rust
pub enum Focus {
    RecentlyAdded,  // Left column top
    Playlists,      // Left column bottom
    Content,        // Right main area
    Search,         // Search mode
}
```

- `Tab` switches between left panes: RecentlyAdded ↔ Playlists
- `h` / `l` switches between columns: left pane ↔ Content

### Scrolling

Each pane scrolls independently.

```rust
pub struct App {
    // Recently Added
    pub recently_added_selected: usize,
    pub recently_added_scroll: usize,

    // Playlists
    pub playlists_selected: usize,
    pub playlists_scroll: usize,

    // Content
    pub content_selected: usize,
    pub content_scroll: usize,
}
```

## Key Bindings

| Key               | Function                                               |
| ----------------- | ------------------------------------------------------ |
| `Space`           | Play/Pause                                             |
| `n`               | Next track                                             |
| `p`               | Previous track                                         |
| `←` `→`           | Seek 10 seconds                                        |
| `s`               | Toggle shuffle                                         |
| `r`               | Cycle repeat mode                                      |
| `c`               | Cycle highlight color                                  |
| `R`               | Refresh current playlist (force reload from Music.app) |
| `j` `k` / `↑` `↓` | Navigate list                                          |
| `g` `G`           | Jump to top / bottom of list                           |
| `h` `l`           | Switch column (left pane ↔ content)                    |
| `Tab`             | Switch pane (Recently Added ↔ Playlists)               |
| `Enter`           | Play / Show details                                    |
| `/`               | Start search mode                                      |
| `Esc`             | Cancel search                                          |
| `a`               | Add selected track to playlist                         |
| `d`               | Delete playlist / remove track from playlist           |
| `q`               | Quit                                                   |

## Dependencies

```toml
[dependencies]
anyhow = "1.0"          # Error handling
crossterm = "0.29.0"    # Terminal control
ratatui = "0.29.0"      # TUI framework
rand = "0.8"            # Level meter animation
serde = "1.0"           # Serialization
serde_json = "1.0"      # JSON cache
dirs = "5.0"            # Cache directory
unicode-width = "0.1"   # Character width calculation
accessibility = "0.2.0" # macOS Accessibility API
core-foundation = "0.10.1" # macOS Core Foundation
```
