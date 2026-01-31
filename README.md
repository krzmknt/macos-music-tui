# macos-music-tui

A TUI (Terminal User Interface) application for controlling macOS Music.app with keyboard.

## Features

- Full keyboard control of Music.app
- Fast search with background caching
- Resumable cache (continues from where it left off on next launch)

## Installation

```bash
cargo build --release
```

## Usage

```bash
cargo run
```

### Key Bindings

| Key               | Function                              |
| ----------------- | ------------------------------------- |
| `Space`           | Play/Pause                            |
| `n`               | Next track                            |
| `p`               | Previous track                        |
| `←` `→`           | Seek 10 seconds                       |
| `s`               | Toggle shuffle                        |
| `r`               | Cycle repeat mode (off → all → one)   |
| `j` `k` / `↑` `↓` | Navigate list                         |
| `Tab`             | Switch focus                          |
| `Enter`           | Play selected item                    |
| `/`               | Start search mode                     |
| `Esc`             | Cancel search                         |
| `q`               | Quit                                  |

## Architecture

### Cache System

All track metadata is cached locally for fast search.

```
~/Library/Caches/macos-music-tui/tracks.json
~/Library/Caches/macos-music-tui/playlists.json
```

#### How Caching Works

```
┌─────────────────────────────────────────────────────────────┐
│                        On Startup                           │
├─────────────────────────────────────────────────────────────┤
│  1. Load cache file                                         │
│  2. Display Recently Added immediately if data exists       │
│  3. Continue cache loading in background thread             │
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
│  │  - tracks: Vec<CachedTrack>                         │   │
│  │  - loaded_tracks: usize                             │   │
│  │  - total_tracks: usize                              │   │
│  └─────────────────────────────────────────────────────┘   │
│                              │                              │
│                    Save every 100 tracks                    │
│                              ▼                              │
│                    ~/Library/Caches/.../tracks.json        │
└─────────────────────────────────────────────────────────────┘
```

#### Cache Data Structure

```json
{
  "total_tracks": 30968,
  "loaded_tracks": 30968,
  "last_updated": 1706612400,
  "tracks": [...]
}
```

#### Track Information

Each track contains the following information:

| Field          | Type   | Description   | Example                                 |
| -------------- | ------ | ------------- | --------------------------------------- |
| `name`         | String | Track name    | "Yesterday"                             |
| `artist`       | String | Artist name   | "The Beatles"                           |
| `album`        | String | Album name    | "Help!"                                 |
| `date_added`   | String | Date added    | "Sunday, September 13, 2015 at 3:44:42" |
| `year`         | u32    | Release year  | 1965                                    |
| `track_number` | u32    | Track number  | 13                                      |
| `disc_number`  | u32    | Disc number   | 1                                       |
| `time`         | String | Duration      | "2:05"                                  |
| `played_count` | u32    | Play count    | 42                                      |
| `favorited`    | bool   | Favorited     | true                                    |

```json
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
```

### Search

Search is always performed against the local cache (requires 3+ characters).

```
┌──────────────────────────────────────────────────────┐
│              Search Query Input (3+ chars)           │
└──────────────────────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────┐
│                    Cache Search                      │
│            (Instant results from memory)             │
└──────────────────────────────────────────────────────┘
```

**Search Logic:**

- Split query by whitespace
- Check if each word is contained in "track name + artist + album"
- Case insensitive
- No limit on result count

Example: `beatles abbey` → Matches "Abbey Road" by "The Beatles"

### Module Structure

```
src/
├── main.rs      # Entry point, event loop
├── app.rs       # Application state, business logic
├── ui.rs        # UI rendering (ratatui)
├── music.rs     # Music.app control (AppleScript)
└── cache.rs     # Cache management
```

### Thread Architecture

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
│  - Background batch loading of track metadata              │
│  - Runs independently without blocking playback            │
└────────────────────────────────────────────────────────────┘
```

## Technical Notes

### Playlist/Album Playback Context

When playing a playlist or album from this TUI, we use a hybrid approach combining AppleScript and macOS Accessibility API.

#### The Problem

Music.app's AppleScript API has a significant limitation: any track played via AppleScript goes into "AutoPlay" mode, where the next track is selected from the entire library rather than from the album or playlist context.

```applescript
-- All of these result in AutoPlay mode:
play track 1 of playlist "My Playlist"
play (every track whose album is "Album Name")
play playlist "My Playlist"
```

This means after a song ends, the next song is not from the same album/playlist, breaking the expected listening experience.

#### The Workaround

To achieve proper playlist/album context playback:

1. **AppleScript (System Events)**: Select the playlist in Music.app's sidebar
   ```applescript
   tell application "System Events"
       tell process "Music"
           select row (matching playlist name) of sidebar
       end tell
   end tell
   ```

2. **Accessibility API**: Click the "Play" button in the playlist view
   ```rust
   // Using macOS Accessibility framework
   play_button.perform_action("AXPress")
   ```

This mimics what a user would do: select a playlist, then click Play. The result is proper playlist context where the next track comes from the same playlist.

#### Trade-offs

- **Requires Music.app window to exist** (but not be visible or frontmost)
- **Slightly slower** than direct AppleScript playback
- **More complex implementation** using two different APIs

This is a workaround for a limitation in Music.app's AppleScript API and may break if Apple changes the UI structure.

## Dependencies

- [ratatui](https://github.com/ratatui-org/ratatui) - TUI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal control
- [serde](https://github.com/serde-rs/serde) - Serialization/Deserialization
- [anyhow](https://github.com/dtolnay/anyhow) - Error handling
- [accessibility](https://crates.io/crates/accessibility) - macOS Accessibility API bindings (for playlist context playback)

## License

MIT
