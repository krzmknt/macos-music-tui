use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTrack {
    pub name: String,
    pub artist: String,
    pub album: String,
    #[serde(default)]
    pub date_added: String,
    #[serde(default)]
    pub year: u32,
    #[serde(default)]
    pub track_number: u32,
    #[serde(default)]
    pub disc_number: u32,
    #[serde(default)]
    pub time: String,  // "3:08" 形式
    #[serde(default)]
    pub played_count: u32,
    #[serde(default)]
    pub favorited: bool,
    // 検索用に小文字化した文字列
    #[serde(skip)]
    pub search_key: String,
}

impl CachedTrack {
    pub fn new(
        name: String,
        artist: String,
        album: String,
        date_added: String,
        year: u32,
        track_number: u32,
        disc_number: u32,
        time: String,
        played_count: u32,
        favorited: bool,
    ) -> Self {
        let search_key = format!("{} {} {}", name, artist, album).to_lowercase();
        Self {
            name,
            artist,
            album,
            date_added,
            year,
            track_number,
            disc_number,
            time,
            played_count,
            favorited,
            search_key,
        }
    }

    pub fn init_search_key(&mut self) {
        self.search_key = format!("{} {} {}", self.name, self.artist, self.album).to_lowercase();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackCache {
    pub total_tracks: usize,
    pub loaded_tracks: usize,
    pub last_updated: Option<u64>,  // Unix timestamp
    pub tracks: Vec<CachedTrack>,
    #[serde(skip)]
    search_keys_initialized: bool,
    #[serde(skip)]
    pub is_fresh_build: bool,  // true if no prior cache existed
}

impl TrackCache {
    fn cache_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|p| p.join("macos-music-tui").join("tracks.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::cache_path() else {
            return Self { is_fresh_build: true, ..Self::default() };
        };

        if !path.exists() {
            return Self { is_fresh_build: true, ..Self::default() };
        }

        match fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str::<TrackCache>(&content).unwrap_or_default()
            }
            Err(_) => Self { is_fresh_build: true, ..Self::default() },
        }
    }

    pub fn save(&mut self) -> Result<()> {
        let Some(path) = Self::cache_path() else {
            anyhow::bail!("Could not determine cache directory");
        };

        // ディレクトリを作成
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// last_updated を現在時刻に更新
    pub fn update_timestamp(&mut self) {
        self.last_updated = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
    }

    pub fn add_tracks(&mut self, new_tracks: Vec<CachedTrack>) {
        self.tracks.extend(new_tracks);
        self.loaded_tracks = self.tracks.len();
    }

    /// トラックを upsert（既存なら更新、なければ追加）
    /// キーは name + artist + album
    pub fn upsert_tracks(&mut self, new_tracks: Vec<CachedTrack>) -> usize {
        let mut added_count = 0;
        for new_track in new_tracks {
            // 既存トラックを検索
            if let Some(existing) = self.tracks.iter_mut().find(|t| {
                t.name == new_track.name && t.artist == new_track.artist && t.album == new_track.album
            }) {
                // 既存トラックを更新
                existing.date_added = new_track.date_added;
                existing.year = new_track.year;
                existing.track_number = new_track.track_number;
                existing.disc_number = new_track.disc_number;
                existing.time = new_track.time;
                existing.played_count = new_track.played_count;
                existing.favorited = new_track.favorited;
                existing.init_search_key();
            } else {
                // 新規トラックを追加
                self.tracks.push(new_track);
                added_count += 1;
            }
        }
        self.loaded_tracks = self.tracks.len();
        added_count
    }

    pub fn is_complete(&self) -> bool {
        self.total_tracks > 0 && self.loaded_tracks >= self.total_tracks
    }

    /// 最終更新日を "Last updated: yyyy/MM/dd hh:mm" 形式で返す（JST）
    pub fn format_last_updated(&self) -> Option<String> {
        self.last_updated.map(|ts| {
            // Unix timestamp (UTC) を JST (UTC+9) に変換
            let ts_local = ts as i64 + 9 * 3600;

            // 時刻を計算
            let seconds_in_day = ((ts_local % 86400) + 86400) % 86400; // 負数対応
            let hour = seconds_in_day / 3600;
            let minute = (seconds_in_day % 3600) / 60;

            // 日付を計算
            let mut days = ts_local / 86400;
            if ts_local < 0 && ts_local % 86400 != 0 {
                days -= 1;
            }

            // 1970年1月1日からの日数を年月日に変換
            let mut year = 1970i32;
            if days >= 0 {
                loop {
                    let days_in_year = if is_leap_year(year) { 366 } else { 365 };
                    if days < days_in_year {
                        break;
                    }
                    days -= days_in_year;
                    year += 1;
                }
            }

            let mut month = 1u32;
            loop {
                let dim = days_in_month(year, month) as i64;
                if days < dim {
                    break;
                }
                days -= dim;
                month += 1;
            }

            let day = days + 1;
            format!("Last updated: {}/{:02}/{:02} {:02}:{:02}", year, month, day, hour, minute)
        })
    }

    /// 検索キーを遅延初期化
    fn ensure_search_keys(&mut self) {
        if !self.search_keys_initialized {
            for track in &mut self.tracks {
                track.init_search_key();
            }
            self.search_keys_initialized = true;
        }
    }

    /// あいまい検索 - クエリの各単語がトラック情報に含まれているか
    /// 全て小文字の場合は case insensitive、大文字が含まれる場合は case sensitive
    /// 高度な検索: "Name:{key} Artist:{key} Album:{key}" でフィールド指定検索
    /// フィールド名は大文字小文字を区別しない (name:, Name:, NAME: など)
    /// "" または '' で囲むと完全一致検索 (例: artist:"IO")
    pub fn search(&mut self, query: &str) -> Vec<CachedTrack> {
        self.ensure_search_keys();

        // フィールド指定フィルタと一般検索語を分離
        // SearchFilter: (value, is_exact_match)
        let mut name_filters: Vec<(String, bool)> = Vec::new();
        let mut artist_filters: Vec<(String, bool)> = Vec::new();
        let mut album_filters: Vec<(String, bool)> = Vec::new();
        let mut general_words: Vec<&str> = Vec::new();

        for word in query.split_whitespace() {
            let word_lower = word.to_lowercase();
            if word_lower.starts_with("name:") {
                if let Some((value, exact)) = Self::parse_filter_value(&word[5..]) {
                    name_filters.push((value, exact));
                }
            } else if word_lower.starts_with("artist:") {
                if let Some((value, exact)) = Self::parse_filter_value(&word[7..]) {
                    artist_filters.push((value, exact));
                }
            } else if word_lower.starts_with("album:") {
                if let Some((value, exact)) = Self::parse_filter_value(&word[6..]) {
                    album_filters.push((value, exact));
                }
            } else {
                general_words.push(word);
            }
        }

        self.tracks
            .iter()
            .filter(|track| {
                // Name フィルタ (AND条件)
                for (key, exact) in &name_filters {
                    if !Self::field_match(&track.name, key, *exact) {
                        return false;
                    }
                }

                // Artist フィルタ (AND条件)
                for (key, exact) in &artist_filters {
                    if !Self::field_match(&track.artist, key, *exact) {
                        return false;
                    }
                }

                // Album フィルタ (AND条件)
                for (key, exact) in &album_filters {
                    if !Self::field_match(&track.album, key, *exact) {
                        return false;
                    }
                }

                // 一般検索語 (各語がname/artist/albumのいずれかに含まれる)
                for word in &general_words {
                    let has_uppercase = word.chars().any(|c| c.is_uppercase());
                    let matched = if has_uppercase {
                        let search_target = format!("{} {} {}", track.name, track.artist, track.album);
                        search_target.contains(*word)
                    } else {
                        track.search_key.contains(&word.to_lowercase())
                    };
                    if !matched {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    /// フィルタ値をパース: 引用符で囲まれていれば (値, true)、そうでなければ (値, false)
    fn parse_filter_value(value: &str) -> Option<(String, bool)> {
        if value.is_empty() {
            return None;
        }

        // "" または '' で囲まれている場合は完全一致
        if value.len() >= 2 &&
           ((value.starts_with('"') && value.ends_with('"')) ||
            (value.starts_with('\'') && value.ends_with('\''))) {
            let inner = &value[1..value.len()-1];
            if inner.is_empty() {
                return None;
            }
            Some((inner.to_string(), true))
        } else {
            // 引用符が閉じていない場合はそのまま部分一致として扱う
            Some((value.to_string(), false))
        }
    }

    /// フィールドマッチ: exact=true なら完全一致、false ならスマートケース部分一致
    fn field_match(target: &str, key: &str, exact: bool) -> bool {
        if exact {
            // 完全一致
            target == key
        } else {
            // スマートケース部分一致
            Self::smart_case_match(target, key)
        }
    }

    /// スマートケースマッチ: キーが全て小文字なら case insensitive、大文字を含むなら case sensitive
    fn smart_case_match(target: &str, key: &str) -> bool {
        let has_uppercase = key.chars().any(|c| c.is_uppercase());
        if has_uppercase {
            target.contains(key)
        } else {
            target.to_lowercase().contains(&key.to_lowercase())
        }
    }

    /// アルバム名でトラックを取得（トラック番号順）
    pub fn get_tracks_by_album(&self, album_name: &str) -> Vec<&CachedTrack> {
        let mut tracks: Vec<_> = self.tracks
            .iter()
            .filter(|t| t.album == album_name)
            .collect();
        // ディスク番号 → トラック番号でソート
        tracks.sort_by(|a, b| {
            a.disc_number.cmp(&b.disc_number)
                .then(a.track_number.cmp(&b.track_number))
        });
        tracks
    }

    /// 最近追加された曲からユニークなアルバムを取得（追加日順）
    pub fn get_recent_albums(&self, limit: usize) -> Vec<(String, String)> {
        // 追加日でソート（降順 = 最新が先）
        let mut sorted_tracks: Vec<_> = self.tracks.iter().collect();
        sorted_tracks.sort_by(|a, b| {
            let date_a = parse_date_to_sortable(&a.date_added);
            let date_b = parse_date_to_sortable(&b.date_added);
            date_b.cmp(&date_a)
        });

        let mut seen = std::collections::HashSet::new();
        sorted_tracks
            .iter()
            .filter_map(|t| {
                if !t.album.is_empty() && seen.insert(t.album.clone()) {
                    Some((t.album.clone(), t.artist.clone()))
                } else {
                    None
                }
            })
            .take(limit)
            .collect()
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 30,
    }
}

/// AppleScript日付文字列 "Weekday, Month DD, YYYY at HH:MM:SS" をソート可能な形式に変換
fn parse_date_to_sortable(date_str: &str) -> String {
    if date_str.is_empty() {
        return String::new();
    }

    // "Sunday, September 13, 2015 at 3:44:42" のような形式をパース
    let parts: Vec<&str> = date_str.split(", ").collect();
    if parts.len() < 2 {
        return date_str.to_string();
    }

    // "September 13" と "2015 at 3:44:42" を取得
    let month_day = parts.get(1).unwrap_or(&"");
    let year_time = parts.get(2).unwrap_or(&"");

    // 月と日を分離
    let md_parts: Vec<&str> = month_day.split_whitespace().collect();
    let month_name = md_parts.get(0).unwrap_or(&"");
    let day: u32 = md_parts.get(1).unwrap_or(&"1").parse().unwrap_or(1);

    // 年と時刻を分離
    let yt_parts: Vec<&str> = year_time.split(" at ").collect();
    let year: u32 = yt_parts.get(0).unwrap_or(&"1970").parse().unwrap_or(1970);
    let time = yt_parts.get(1).unwrap_or(&"00:00:00");

    // 月名を数字に変換
    let month = match *month_name {
        "January" => 1, "February" => 2, "March" => 3, "April" => 4,
        "May" => 5, "June" => 6, "July" => 7, "August" => 8,
        "September" => 9, "October" => 10, "November" => 11, "December" => 12,
        _ => 1,
    };

    // YYYY-MM-DD HH:MM:SS 形式で返す
    format!("{:04}-{:02}-{:02} {}", year, month, day, time)
}

/// プレイリストのトラック情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPlaylistTrack {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub year: u32,
    pub time: String,
    pub played_count: u32,
    pub favorited: bool,
}

/// キャッシュされたプレイリスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPlaylist {
    pub name: String,
    pub tracks: Vec<CachedPlaylistTrack>,
}

/// プレイリストキャッシュ
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistCache {
    pub playlists: std::collections::HashMap<String, CachedPlaylist>,
}

impl PlaylistCache {
    fn cache_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|p| p.join("macos-music-tui").join("playlists.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::cache_path() else {
            return Self::default();
        };

        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str(&content).unwrap_or_default()
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = Self::cache_path() else {
            anyhow::bail!("Could not determine cache directory");
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn get(&self, playlist_name: &str) -> Option<&CachedPlaylist> {
        self.playlists.get(playlist_name)
    }

    pub fn insert(&mut self, playlist: CachedPlaylist) {
        self.playlists.insert(playlist.name.clone(), playlist);
    }
}

// アプリケーション設定
use crate::app::HighlightColor;

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub highlight_color: HighlightColor,
}

impl Default for HighlightColor {
    fn default() -> Self {
        HighlightColor::Cyan
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            highlight_color: HighlightColor::Cyan,
        }
    }
}

impl Settings {
    fn settings_path() -> Option<PathBuf> {
        dirs::cache_dir().map(|p| p.join("macos-music-tui").join("settings.json"))
    }

    pub fn load() -> Self {
        let path = match Self::settings_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::settings_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine settings path"))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
