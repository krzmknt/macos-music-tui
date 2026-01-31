use anyhow::Result;
use std::process::Command;

pub struct MusicController;

impl MusicController {
    fn run_script(script: &str) -> Result<String> {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("AppleScript error: {}", err)
        }
    }

    pub fn play_pause() -> Result<()> {
        Self::run_script("tell application \"Music\" to playpause")?;
        Ok(())
    }

    pub fn get_position() -> Result<(f64, bool)> {
        let result = Self::run_script(
            "tell application \"Music\"
                if player state is stopped then
                    return \"0|||false\"
                else
                    return (player position as string) & \"|||\" & (player state is playing)
                end if
            end tell"
        )?;
        let parts: Vec<&str> = result.split("|||").collect();
        let position = parts.get(0).unwrap_or(&"0").parse().unwrap_or(0.0);
        let is_playing = parts.get(1).unwrap_or(&"false") == &"true";
        Ok((position, is_playing))
    }

    pub fn next_track() -> Result<()> {
        Self::run_script("tell application \"Music\" to next track")?;
        Ok(())
    }

    pub fn previous_track() -> Result<()> {
        Self::run_script("tell application \"Music\" to previous track")?;
        Ok(())
    }

    pub fn toggle_shuffle() -> Result<bool> {
        let result = Self::run_script(
            "tell application \"Music\"
                set shuffle enabled to not shuffle enabled
                return shuffle enabled
            end tell"
        )?;
        Ok(result == "true")
    }

    pub fn cycle_repeat() -> Result<String> {
        let result = Self::run_script(
            "tell application \"Music\"
                if song repeat is off then
                    set song repeat to all
                    return \"all\"
                else if song repeat is all then
                    set song repeat to one
                    return \"one\"
                else
                    set song repeat to off
                    return \"off\"
                end if
            end tell"
        )?;
        Ok(result)
    }

    pub fn get_all_state() -> Result<PlayerState> {
        let script = r#"
            tell application "Music"
                set vol to sound volume
                set shuf to shuffle enabled
                set rep to song repeat as string

                if player state is not stopped then
                    try
                        set trackName to name of current track
                        set trackArtist to artist of current track
                        set trackAlbum to album of current track
                        set trackDuration to duration of current track
                        set currentPos to player position
                        set isPlaying to (player state is playing)
                        return trackName & "|||" & trackArtist & "|||" & trackAlbum & "|||" & trackDuration & "|||" & currentPos & "|||" & isPlaying & "|||" & vol & "|||" & shuf & "|||" & rep
                    on error
                        -- current track にアクセスできない場合（ラジオ等）は空を返す
                        try
                            set currentPos to player position
                        on error
                            set currentPos to 0
                        end try
                        set isPlaying to (player state is playing)
                        return "" & "|||" & "" & "|||" & "" & "|||" & "0" & "|||" & currentPos & "|||" & isPlaying & "|||" & vol & "|||" & shuf & "|||" & rep
                    end try
                else
                    return "||||||false|||" & vol & "|||" & shuf & "|||" & rep
                end if
            end tell
        "#;

        let result = Self::run_script(script)?;
        let parts: Vec<&str> = result.split("|||").collect();

        if parts.len() >= 9 {
            Ok(PlayerState {
                track: TrackInfo {
                    name: parts[0].to_string(),
                    artist: parts[1].to_string(),
                    album: parts[2].to_string(),
                    duration: parts[3].parse().unwrap_or(0.0),
                    position: parts[4].parse().unwrap_or(0.0),
                    is_playing: parts[5] == "true",
                },
                volume: parts[6].parse().unwrap_or(50),
                shuffle: parts[7] == "true",
                repeat: parts[8].to_string(),
            })
        } else {
            Ok(PlayerState::default())
        }
    }

    pub fn seek_backward() -> Result<()> {
        Self::run_script(
            "tell application \"Music\" to set player position to (player position - 10)"
        )?;
        Ok(())
    }

    pub fn seek_forward() -> Result<()> {
        Self::run_script(
            "tell application \"Music\" to set player position to (player position + 10)"
        )?;
        Ok(())
    }

    pub fn get_playlists() -> Result<Vec<ListItem>> {
        let script = r#"
            tell application "Music"
                set output to ""
                set allPlaylists to user playlists
                repeat with p in allPlaylists
                    set pName to name of p
                    set pCount to count of tracks of p
                    set output to output & pName & ":::" & pCount & "|||"
                end repeat
                return output
            end tell
        "#;
        let result = Self::run_script(script)?;

        let excluded = ["Music", "Music Videos", "Favorite Songs"];
        let playlists: Vec<ListItem> = result
            .split("|||")
            .filter(|s| !s.is_empty())
            .filter_map(|s| {
                let parts: Vec<&str> = s.split(":::").collect();
                let name = parts.get(0).unwrap_or(&"").to_string();
                if excluded.contains(&name.as_str()) {
                    None
                } else {
                    Some(ListItem {
                        name,
                        artist: format!("{} tracks", parts.get(1).unwrap_or(&"0")),
                        album: String::new(),
                        time: String::new(),
                        year: 0,
                        track_number: 0,
                        played_count: 0,
                        favorited: false,
                    })
                }
            })
            .collect();

        Ok(playlists)
    }

    /// プレイリストのトラックを取得
    pub fn get_playlist_tracks(name: &str) -> Result<Vec<ListItem>> {
        let script = format!(
            r#"tell application "Music"
                set output to ""
                set trackList to every track of playlist "{}"
                repeat with t in trackList
                    set trackName to name of t
                    set trackArtist to artist of t
                    set trackAlbum to album of t
                    set trackYear to year of t
                    set trackTime to time of t
                    set trackPlays to played count of t
                    set trackFav to favorited of t
                    set output to output & trackName & ":::" & trackArtist & ":::" & trackAlbum & ":::" & trackYear & ":::" & trackTime & ":::" & trackPlays & ":::" & trackFav & "|||"
                end repeat
                return output
            end tell"#,
            name.replace("\"", "\\\"")
        );
        let result = Self::run_script(&script)?;
        let tracks: Vec<ListItem> = result
            .split("|||")
            .filter(|s| !s.is_empty())
            .map(|item| {
                let parts: Vec<&str> = item.split(":::").collect();
                ListItem {
                    name: parts.get(0).unwrap_or(&"").to_string(),
                    artist: parts.get(1).unwrap_or(&"").to_string(),
                    album: parts.get(2).unwrap_or(&"").to_string(),
                    year: parts.get(3).unwrap_or(&"0").parse().unwrap_or(0),
                    time: parts.get(4).unwrap_or(&"").to_string(),
                    played_count: parts.get(5).unwrap_or(&"0").parse().unwrap_or(0),
                    favorited: *parts.get(6).unwrap_or(&"false") == "true",
                    track_number: 0,
                }
            })
            .collect();
        Ok(tracks)
    }

    /// 曲を再生
    pub fn play_track(name: &str, artist: &str) -> Result<()> {
        let escaped_name = name.replace("\"", "\\\"");
        let escaped_artist = artist.replace("\"", "\\\"");
        let script = format!(
            r#"tell application "Music"
                set matchingTracks to (every track of library playlist 1 whose name is "{}" and artist is "{}")
                if (count of matchingTracks) > 0 then
                    play item 1 of matchingTracks
                end if
            end tell"#,
            escaped_name, escaped_artist
        );
        Self::run_script(&script)?;
        Ok(())
    }

    /// アルバムの特定トラックを再生
    /// 注意: AutoPlay モードになるため、n/p はアルバム内ではなくライブラリ全体から選曲される
    pub fn play_album_from_track(album: &str, track_name: &str, track_artist: &str) -> Result<()> {
        let escaped_album = album.replace("\"", "\\\"");
        let escaped_name = track_name.replace("\"", "\\\"");
        let escaped_artist = track_artist.replace("\"", "\\\"");

        let script = format!(
            r#"tell application "Music"
    set albumTracks to (every track of library playlist 1 whose album is "{}" and name is "{}" and artist is "{}")
    if (count of albumTracks) > 0 then
        play item 1 of albumTracks
    else
        -- 完全一致しない場合はアルバム名とトラック名のみで検索
        set albumTracks to (every track of library playlist 1 whose album is "{}" and name is "{}")
        if (count of albumTracks) > 0 then
            play item 1 of albumTracks
        end if
    end if
end tell"#,
            escaped_album, escaped_name, escaped_artist, escaped_album, escaped_name
        );
        Self::run_script(&script)?;
        Ok(())
    }

    /// プレイリストの特定トラックを再生
    /// 注意: AutoPlay モードになるため、n/p はプレイリスト内ではなくライブラリ全体から選曲される
    pub fn play_playlist_from_track(playlist_name: &str, track_index: usize) -> Result<()> {
        let escaped = playlist_name.replace("\"", "\\\"");
        let script = format!(
            r#"tell application "Music"
    set pl to playlist "{}"
    set trackCount to count of tracks of pl
    set targetIndex to {}
    if targetIndex > trackCount then
        set targetIndex to trackCount
    end if
    if targetIndex < 1 then
        set targetIndex to 1
    end if
    play track targetIndex of pl
end tell"#,
            escaped, track_index + 1
        );
        Self::run_script(&script)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct TrackInfo {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub position: f64,
    pub is_playing: bool,
}

#[derive(Debug, Default, Clone)]
pub struct PlayerState {
    pub track: TrackInfo,
    pub volume: i32,
    pub shuffle: bool,
    pub repeat: String,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub name: String,
    pub artist: String,
    pub album: String,
    // 詳細表示用の追加フィールド
    pub time: String,
    pub year: u32,
    pub track_number: u32,
    pub played_count: u32,
    pub favorited: bool,
}

impl TrackInfo {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }

    pub fn format_time(seconds: f64) -> String {
        let mins = (seconds as i32) / 60;
        let secs = (seconds as i32) % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

/// キャッシュ用のシンプルなトラック情報
#[derive(Debug, Clone)]
pub struct SimpleTrack {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub date_added: String,
    pub year: u32,
    pub track_number: u32,
    pub disc_number: u32,
    pub time: String,
    pub played_count: u32,
    pub favorited: bool,
}

impl MusicController {
    /// ライブラリの総曲数を取得
    pub fn get_total_track_count() -> Result<usize> {
        let script = r#"
            tell application "Music"
                return count of tracks of library playlist 1
            end tell
        "#;
        let result = Self::run_script(script)?;
        Ok(result.parse().unwrap_or(0))
    }

    /// 指定範囲のトラックを取得（1-indexed）
    pub fn get_tracks_batch(start: usize, count: usize) -> Result<Vec<SimpleTrack>> {
        let script = format!(
            r#"tell application "Music"
                set output to ""
                set trackList to every track of library playlist 1
                set totalCount to count of trackList
                set endIndex to {} + {} - 1
                if endIndex > totalCount then
                    set endIndex to totalCount
                end if
                if {} > totalCount then
                    return ""
                end if
                repeat with i from {} to endIndex
                    set t to item i of trackList
                    set dateStr to (date added of t) as string
                    set yr to year of t
                    set tn to track number of t
                    set dn to disc number of t
                    set tm to time of t
                    set pc to played count of t
                    set fav to favorited of t
                    set output to output & name of t & ":::" & artist of t & ":::" & album of t & ":::" & dateStr & ":::" & yr & ":::" & tn & ":::" & dn & ":::" & tm & ":::" & pc & ":::" & fav & "|||"
                end repeat
                return output
            end tell"#,
            start, count, start, start
        );
        let result = Self::run_script(&script)?;

        let tracks: Vec<SimpleTrack> = result
            .split("|||")
            .filter(|s| !s.is_empty())
            .map(|s| {
                let parts: Vec<&str> = s.split(":::").collect();
                SimpleTrack {
                    name: parts.get(0).unwrap_or(&"").to_string(),
                    artist: parts.get(1).unwrap_or(&"").to_string(),
                    album: parts.get(2).unwrap_or(&"").to_string(),
                    date_added: parts.get(3).unwrap_or(&"").to_string(),
                    year: parts.get(4).unwrap_or(&"0").parse().unwrap_or(0),
                    track_number: parts.get(5).unwrap_or(&"0").parse().unwrap_or(0),
                    disc_number: parts.get(6).unwrap_or(&"0").parse().unwrap_or(0),
                    time: parts.get(7).unwrap_or(&"").to_string(),
                    played_count: parts.get(8).unwrap_or(&"0").parse().unwrap_or(0),
                    favorited: *parts.get(9).unwrap_or(&"false") == "true",
                }
            })
            .collect();

        Ok(tracks)
    }

    /// 指定日時以降に追加されたトラックを取得
    pub fn get_tracks_added_since(unix_timestamp: u64) -> Result<Vec<SimpleTrack>> {
        // Unix timestamp を AppleScript の日付形式に変換
        // AppleScriptは現在時刻と基準日の差分を使って正確なオフセットを計算
        let script = format!(
            r#"tell application "Music"
                set output to ""
                set unixTs to {}
                set baseDate to date "Monday, January 1, 2001 at 12:00:00 AM"
                -- 現在の Unix timestamp と AppleScript の (current date - baseDate) の差分からオフセットを計算
                set currentUnix to (do shell script "date +%s") as integer
                set currentAppleSeconds to (current date) - baseDate
                set offsetVal to currentAppleSeconds - (currentUnix - 978307200)
                -- 正しい cutoffDate を計算
                set cutoffDate to baseDate + (unixTs - 978307200) + offsetVal
                set recentTracks to (every track of library playlist 1 whose date added > cutoffDate)
                repeat with t in recentTracks
                    try
                        set dateStr to (date added of t) as string
                        set yr to year of t
                        set tn to track number of t
                        set dn to disc number of t
                        set tm to time of t
                        set pc to played count of t
                        set fav to favorited of t
                        set output to output & name of t & ":::" & artist of t & ":::" & album of t & ":::" & dateStr & ":::" & yr & ":::" & tn & ":::" & dn & ":::" & tm & ":::" & pc & ":::" & fav & "|||"
                    end try
                end repeat
                return output
            end tell"#,
            unix_timestamp
        );
        let result = Self::run_script(&script)?;

        let tracks: Vec<SimpleTrack> = result
            .split("|||")
            .filter(|s| !s.is_empty())
            .map(|s| {
                let parts: Vec<&str> = s.split(":::").collect();
                SimpleTrack {
                    name: parts.get(0).unwrap_or(&"").to_string(),
                    artist: parts.get(1).unwrap_or(&"").to_string(),
                    album: parts.get(2).unwrap_or(&"").to_string(),
                    date_added: parts.get(3).unwrap_or(&"").to_string(),
                    year: parts.get(4).unwrap_or(&"0").parse().unwrap_or(0),
                    track_number: parts.get(5).unwrap_or(&"0").parse().unwrap_or(0),
                    disc_number: parts.get(6).unwrap_or(&"0").parse().unwrap_or(0),
                    time: parts.get(7).unwrap_or(&"").to_string(),
                    played_count: parts.get(8).unwrap_or(&"0").parse().unwrap_or(0),
                    favorited: *parts.get(9).unwrap_or(&"false") == "true",
                }
            })
            .collect();

        Ok(tracks)
    }
}
