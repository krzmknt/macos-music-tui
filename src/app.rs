use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::accessibility;
use crate::cache::{CachedTrack, CachedPlaylist, CachedPlaylistTrack, PlaylistCache, Settings, TrackCache};
use crate::music::{ListItem, MusicController, TrackInfo};

// 再生制御用コマンド（メインワーカースレッド）
enum Command {
    RefreshPosition,
    RefreshFull,
}

// 再生制御用レスポンス
enum Response {
    PositionUpdated(f64, bool),
    StateUpdated(TrackInfo, i32, bool, String),
}

// キャッシュ用レスポンス（専用スレッドから）
enum CacheResponse {
    BatchLoaded {
        tracks: Vec<CachedTrack>,
        loaded: usize,
        total: usize,
    },
    Upsert {
        tracks: Vec<CachedTrack>,
        total: usize,
    },
    Complete,
}

// プレイリスト読み込み用レスポンス
enum PlaylistLoadResponse {
    PlaylistList(Vec<ListItem>),  // プレイリスト一覧
    Progress { current: usize, total: usize, name: String },
    PlaylistLoaded(CachedPlaylist),
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    RecentlyAdded,
    Playlists,
    Content,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragTarget {
    ColumnDivider,      // 左右カラムの境界
    CardDivider,        // Recently AddedとPlaylistsの境界
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchSortMode {
    Default,            // 検索結果のデフォルト順
    PlayCount,          // 再生回数降順
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HighlightColor {
    Cyan,
    Green,
    Yellow,
    Orange,
    Pink,
    Magenta,
    Purple,
    Blue,
    Red,
    White,
}

impl HighlightColor {
    pub fn next(&self) -> Self {
        match self {
            HighlightColor::Cyan => HighlightColor::Green,
            HighlightColor::Green => HighlightColor::Yellow,
            HighlightColor::Yellow => HighlightColor::Orange,
            HighlightColor::Orange => HighlightColor::Pink,
            HighlightColor::Pink => HighlightColor::Magenta,
            HighlightColor::Magenta => HighlightColor::Purple,
            HighlightColor::Purple => HighlightColor::Blue,
            HighlightColor::Blue => HighlightColor::Red,
            HighlightColor::Red => HighlightColor::White,
            HighlightColor::White => HighlightColor::Cyan,
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        match self {
            HighlightColor::Cyan => (80, 200, 255),
            HighlightColor::Green => (80, 220, 120),
            HighlightColor::Yellow => (255, 220, 80),
            HighlightColor::Orange => (255, 150, 50),
            HighlightColor::Pink => (255, 130, 180),
            HighlightColor::Magenta => (220, 100, 255),
            HighlightColor::Purple => (160, 100, 255),
            HighlightColor::Blue => (100, 140, 255),
            HighlightColor::Red => (255, 100, 100),
            HighlightColor::White => (255, 255, 255),
        }
    }
}


pub struct App {
    pub track: TrackInfo,
    pub volume: i32,
    pub shuffle: bool,
    pub repeat: String,
    pub message: Option<String>,
    pub should_quit: bool,

    pub focus: Focus,
    pub last_left_focus: Focus,  // h/l移動時に戻る左ペイン
    pub recently_added: Vec<ListItem>,
    pub recently_added_selected: usize,
    pub recently_added_scroll: usize,
    pub content_items: Vec<ListItem>,
    pub content_selected: usize,
    pub content_scroll: usize,
    pub content_loading: bool,
    pub content_title: String,  // アルバム/プレイリスト詳細表示時のタイトル
    pub content_source_name: String,  // 再生用のアルバム/プレイリスト名
    pub is_playlist_detail: bool,  // プレイリスト詳細表示中かどうか

    pub playlists: Vec<ListItem>,
    pub playlists_selected: usize,
    pub playlists_scroll: usize,

    // 可視行数（UIから設定）
    pub recently_added_visible: usize,
    pub playlists_visible: usize,
    pub content_visible: usize,

    // レイアウトサイズ（リサイズ可能）
    pub left_column_width: u16,
    pub recently_added_height: u16,
    pub dragging: Option<DragTarget>,

    pub search_mode: bool,
    pub search_query: String,
    pub search_cursor: usize,  // カーソル位置（文字数）
    pub search_results: Vec<ListItem>,
    pub search_sort_mode: SearchSortMode,
    search_results_all: Vec<ListItem>,      // 全検索結果（遅延読み込み用）
    search_results_unsorted: Vec<ListItem>,  // ソート切替用にオリジナルを保持
    pub search_total_count: usize,           // 検索結果の総数

    // プレイリスト追加モード
    pub add_to_playlist_mode: bool,
    pub track_to_add: Option<ListItem>,
    pub new_playlist_input_mode: bool,
    pub new_playlist_name: String,
    pub playlist_refreshing: Option<String>,  // 更新中のプレイリスト名

    position_pending: bool,
    full_pending: bool,
    pub spinner_frame: usize,
    pub level_meter: [u8; 5],
    cmd_tx: Sender<Command>,
    resp_rx: Receiver<Response>,

    // キャッシュ関連
    pub cache: TrackCache,
    pub cache_loading: bool,
    cache_resp_rx: Receiver<CacheResponse>,
    pub playlist_cache: PlaylistCache,

    // プレイリスト読み込み関連
    pub playlist_loading: bool,
    pub playlist_loading_progress: String,
    playlist_load_rx: Receiver<PlaylistLoadResponse>,

    // プレイリスト更新用
    playlist_refresh_rx: Option<Receiver<(String, Vec<ListItem>)>>,

    // ハイライトカラー
    pub highlight_color: HighlightColor,

    // ウェルカム画面を閉じたかどうか
    pub welcome_dismissed: bool,
}

impl App {
    pub fn new() -> Self {
        // Initialize Music window off-screen in background
        // This ensures the window exists before any playlist playback
        thread::spawn(|| {
            accessibility::init_music_window_offscreen();
        });

        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (resp_tx, resp_rx) = mpsc::channel::<Response>();
        let (cache_resp_tx, cache_resp_rx) = mpsc::channel::<CacheResponse>();

        // キャッシュを読み込み
        let cache = TrackCache::load();

        // 設定を読み込み
        let settings = Settings::load();

        // 再生制御用バックグラウンドスレッド（軽量・高速）
        thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    Command::RefreshPosition => {
                        let (position, is_playing) = MusicController::get_position()
                            .unwrap_or((0.0, false));
                        let _ = resp_tx.send(Response::PositionUpdated(position, is_playing));
                    }
                    Command::RefreshFull => {
                        let state = MusicController::get_all_state();
                        match state {
                            Ok(s) => {
                                let _ = resp_tx.send(Response::StateUpdated(
                                    s.track,
                                    s.volume,
                                    s.shuffle,
                                    s.repeat,
                                ));
                            }
                            Err(_) => {
                                // エラー時もレスポンスを送信してpendingフラグをリセット
                                let _ = resp_tx.send(Response::StateUpdated(
                                    TrackInfo::default(),
                                    50,
                                    false,
                                    "off".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        });

        // キャッシュ専用バックグラウンドスレッド（独立して自動実行・差分更新対応）
        {
            let cache_loaded = cache.loaded_tracks;
            let cache_last_updated = cache.last_updated;
            let cache_is_complete = cache.is_complete();
            thread::spawn(move || {
                let current_total = MusicController::get_total_track_count().unwrap_or(0);

                if current_total == 0 {
                    let _ = cache_resp_tx.send(CacheResponse::Complete);
                    return;
                }

                // キャッシュが完了済みの場合は差分更新（upsert方式）
                if cache_is_complete {
                    if let Some(last_updated) = cache_last_updated {
                        // last_updated の1日前から取得して upsert
                        // これにより、キャッシュ構築中に追加された曲も確実に取得できる
                        let cutoff = last_updated.saturating_sub(86400); // 1日 = 86400秒
                        match MusicController::get_tracks_added_since(cutoff) {
                            Ok(tracks) => {
                                if !tracks.is_empty() {
                                    let cached_tracks: Vec<CachedTrack> = tracks
                                        .into_iter()
                                        .map(|t| CachedTrack::new(
                                            t.name, t.artist, t.album, t.date_added,
                                            t.year, t.track_number, t.disc_number,
                                            t.time, t.played_count, t.favorited,
                                        ))
                                        .collect();
                                    let _ = cache_resp_tx.send(CacheResponse::Upsert {
                                        tracks: cached_tracks,
                                        total: current_total,
                                    });
                                }
                            }
                            Err(_) => {}
                        }
                    }

                    let _ = cache_resp_tx.send(CacheResponse::Complete);
                    return;
                }

                // キャッシュが未完了の場合は続きから読み込む
                let mut cache_offset = cache_loaded;
                const BATCH_SIZE: usize = 50;

                while cache_offset < current_total {
                    match MusicController::get_tracks_batch(cache_offset + 1, BATCH_SIZE) {
                        Ok(tracks) => {
                            let cached_tracks: Vec<CachedTrack> = tracks
                                .into_iter()
                                .map(|t| CachedTrack::new(
                                    t.name, t.artist, t.album, t.date_added,
                                    t.year, t.track_number, t.disc_number,
                                    t.time, t.played_count, t.favorited,
                                ))
                                .collect();
                            let batch_len = cached_tracks.len();
                            cache_offset += batch_len;

                            let _ = cache_resp_tx.send(CacheResponse::BatchLoaded {
                                tracks: cached_tracks,
                                loaded: cache_offset,
                                total: current_total,
                            });

                            thread::sleep(std::time::Duration::from_millis(100));
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
                let _ = cache_resp_tx.send(CacheResponse::Complete);
            });
        }

        // キャッシュからRecently Addedを初期化
        let recently_added = Self::albums_to_list_items(&cache.get_recent_albums(30));
        let cache_complete = cache.is_complete();

        // プレイリスト読み込み用チャンネル
        let (playlist_load_tx, playlist_load_rx) = mpsc::channel::<PlaylistLoadResponse>();

        // プレイリストキャッシュを読み込み
        let playlist_cache = PlaylistCache::load();
        let playlist_cache_clone = playlist_cache.playlists.keys().cloned().collect::<std::collections::HashSet<_>>();

        // プレイリスト読み込み用バックグラウンドスレッド
        thread::spawn(move || {
            // プレイリスト一覧を取得
            let playlists = match MusicController::get_playlists() {
                Ok(p) => p,
                Err(_) => {
                    let _ = playlist_load_tx.send(PlaylistLoadResponse::Complete);
                    return;
                }
            };

            // プレイリスト一覧を送信
            let _ = playlist_load_tx.send(PlaylistLoadResponse::PlaylistList(playlists.clone()));

            // キャッシュされていないプレイリストを抽出
            let uncached: Vec<_> = playlists
                .iter()
                .filter(|p| !playlist_cache_clone.contains(&p.name))
                .collect();

            if uncached.is_empty() {
                let _ = playlist_load_tx.send(PlaylistLoadResponse::Complete);
                return;
            }

            let total = uncached.len();
            for (i, playlist) in uncached.iter().enumerate() {
                let _ = playlist_load_tx.send(PlaylistLoadResponse::Progress {
                    current: i + 1,
                    total,
                    name: playlist.name.clone(),
                });

                // プレイリストのトラックを取得
                if let Ok(tracks) = MusicController::get_playlist_tracks(&playlist.name) {
                    let cached_tracks: Vec<CachedPlaylistTrack> = tracks
                        .iter()
                        .map(|t| CachedPlaylistTrack {
                            name: t.name.clone(),
                            artist: t.artist.clone(),
                            album: t.album.clone(),
                            year: t.year,
                            time: t.time.clone(),
                            played_count: t.played_count,
                            favorited: t.favorited,
                        })
                        .collect();
                    let cached_playlist = CachedPlaylist {
                        name: playlist.name.clone(),
                        tracks: cached_tracks,
                    };
                    let _ = playlist_load_tx.send(PlaylistLoadResponse::PlaylistLoaded(cached_playlist));
                }
            }
            let _ = playlist_load_tx.send(PlaylistLoadResponse::Complete);
        });

        // キャッシュからプレイリスト名を取得（起動時は空、バックグラウンドで読み込まれる）
        let playlists: Vec<ListItem> = playlist_cache.playlists.keys().map(|name| {
            ListItem {
                name: name.clone(),
                artist: String::new(),
                album: String::new(),
                time: String::new(),
                year: 0,
                track_number: 0,
                played_count: 0,
                favorited: false,
            }
        }).collect();

        // 起動時に最初のアルバムを読み込む（content_source_nameを初期化）
        let (initial_content_items, initial_content_title, initial_content_source_name) =
            if let Some(album_item) = recently_added.first() {
                let album_name = &album_item.album;
                let tracks = cache.get_tracks_by_album(album_name);
                let year = tracks.first().map(|t| t.year).unwrap_or(0);
                let year_str = if year > 0 { format!(" ({})", year) } else { String::new() };
                let title = format!("{} - {}{}", album_name, album_item.artist, year_str);
                let items: Vec<ListItem> = tracks
                    .into_iter()
                    .map(|t| ListItem {
                        name: t.name.clone(),
                        artist: t.artist.clone(),
                        album: t.album.clone(),
                        time: t.time.clone(),
                        year: t.year,
                        track_number: t.track_number,
                        played_count: t.played_count,
                        favorited: t.favorited,
                    })
                    .collect();
                (items, title, album_name.clone())
            } else {
                (Vec::new(), String::new(), String::new())
            };

        Self {
            track: TrackInfo::default(),
            volume: 50,
            shuffle: false,
            repeat: "off".to_string(),
            message: None,
            should_quit: false,
            focus: Focus::RecentlyAdded,
            last_left_focus: Focus::RecentlyAdded,
            recently_added,
            recently_added_selected: 0,
            recently_added_scroll: 0,
            content_items: initial_content_items,
            content_selected: 0,
            content_scroll: 0,
            content_loading: false,
            content_title: initial_content_title,
            content_source_name: initial_content_source_name,
            is_playlist_detail: false,
            playlists,
            playlists_selected: 0,
            playlists_scroll: 0,
            recently_added_visible: 10,  // デフォルト値、UIから更新される
            playlists_visible: 10,       // デフォルト値、UIから更新される
            content_visible: 15,         // デフォルト値、UIから更新される
            left_column_width: 40,       // 左カラムの幅
            recently_added_height: 12,   // Recently Addedカードの高さ
            dragging: None,
            search_mode: false,
            search_query: String::new(),
            search_cursor: 0,
            search_results: Vec::new(),
            search_sort_mode: SearchSortMode::Default,
            search_results_all: Vec::new(),
            search_results_unsorted: Vec::new(),
            search_total_count: 0,
            add_to_playlist_mode: false,
            track_to_add: None,
            new_playlist_input_mode: false,
            new_playlist_name: String::new(),
            playlist_refreshing: None,
            position_pending: false,
            full_pending: false,
            spinner_frame: 0,
            level_meter: [0; 5],
            cmd_tx,
            resp_rx,
            cache,
            cache_loading: !cache_complete,
            cache_resp_rx,
            playlist_cache,
            playlist_loading: true,
            playlist_loading_progress: String::new(),
            playlist_load_rx,
            playlist_refresh_rx: None,
            highlight_color: settings.highlight_color,
            welcome_dismissed: false,
        }
    }

    fn albums_to_list_items(albums: &[(String, String)]) -> Vec<ListItem> {
        albums
            .iter()
            .map(|(album, artist)| ListItem {
                name: album.clone(),
                artist: artist.clone(),
                album: album.clone(),
                time: String::new(),
                year: 0,
                track_number: 0,
                played_count: 0,
                favorited: false,
            })
            .collect()
    }

    /// スピナーフレームを更新
    pub fn update_spinner(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % 10;
    }


    /// ターミナルサイズに基づいて可視行数を更新
    pub fn update_visible_heights(&mut self, terminal_height: u16) {
        // レイアウト計算:
        // - margin: 1 (top) + 1 (bottom) = 2
        // - header: 4
        // - footer: 2
        // - main area: terminal_height - 2 - 4 - 2 = terminal_height - 8

        let main_height = terminal_height.saturating_sub(8);
        let search_height: u16 = if self.search_mode { 3 } else { 3 };
        let playlists_height = main_height.saturating_sub(search_height + self.recently_added_height);

        // Recently Added: 動的なサイズ
        // カード高さ - ボーダー2 - タイトル1 - 余白1 = リスト部分
        self.recently_added_visible = self.recently_added_height.saturating_sub(4) as usize;

        // Playlists: 動的なサイズ
        // カード高さ - ボーダー2 - タイトル1 - 余白3 = リスト部分
        self.playlists_visible = playlists_height.saturating_sub(6) as usize;

        // Content (右ペイン): main_height全体を使用
        // ボーダー2 + タイトル1 + ヘッダー行1 + 余白3 = 7を引く
        self.content_visible = main_height.saturating_sub(7) as usize;
    }

    /// レベルメーターを更新（再生中のみアニメーション）
    pub fn update_level_meter(&mut self) {
        if self.track.is_playing {
            let mut rng = rand::thread_rng();
            for i in 0..5 {
                // 現在の値から±2の範囲でランダムに変動（滑らかに）
                let current = self.level_meter[i] as i16;
                let delta: i16 = rng.gen_range(-2..=3);
                let new_val = (current + delta).clamp(0, 7) as u8;
                self.level_meter[i] = new_val;
            }
        } else {
            // 停止中は徐々に下がる
            for i in 0..5 {
                if self.level_meter[i] > 0 {
                    self.level_meter[i] -= 1;
                }
            }
        }
    }

    /// バックグラウンドからのレスポンスを処理（再生制御）
    pub fn poll_responses(&mut self) {
        loop {
            match self.resp_rx.try_recv() {
                Ok(resp) => match resp {
                    Response::PositionUpdated(position, is_playing) => {
                        self.track.position = position;
                        self.track.is_playing = is_playing;
                        self.position_pending = false;
                    }
                    Response::StateUpdated(track, volume, shuffle, repeat) => {
                        self.track = track;
                        self.volume = volume;
                        self.shuffle = shuffle;
                        self.repeat = repeat;
                        self.full_pending = false;
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// キャッシュスレッドからのレスポンスを処理
    pub fn poll_cache_responses(&mut self) {
        loop {
            match self.cache_resp_rx.try_recv() {
                Ok(resp) => match resp {
                    CacheResponse::BatchLoaded { tracks, loaded, total } => {
                        self.cache.add_tracks(tracks);
                        self.cache.total_tracks = total;

                        // Recently Addedを更新（キャッシュから最新30アルバム）
                        self.recently_added = Self::albums_to_list_items(&self.cache.get_recent_albums(30));

                        // 定期的に保存（100曲ごと）、完了時はタイムスタンプも更新
                        if loaded >= total {
                            self.cache.update_timestamp();
                            let _ = self.cache.save();
                        } else if loaded % 100 == 0 {
                            let _ = self.cache.save();
                        }
                    }
                    CacheResponse::Upsert { tracks, total } => {
                        // 差分更新（upsert）
                        let added = self.cache.upsert_tracks(tracks);
                        self.cache.total_tracks = total;

                        // Recently Addedを更新
                        self.recently_added = Self::albums_to_list_items(&self.cache.get_recent_albums(30));

                        if added > 0 {
                            self.message = Some(format!("{} new tracks added", added));
                            // 新規トラックが追加された場合のみタイムスタンプを更新
                            self.cache.update_timestamp();
                            let _ = self.cache.save();
                        }
                    }
                    CacheResponse::Complete => {
                        self.cache_loading = false;
                        // タイムスタンプは更新しない（BatchLoaded/Upsertで更新済み）
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.cache_loading = false;
                    break;
                }
            }
        }
    }

    /// プレイリスト読み込みスレッドからのレスポンスを処理
    pub fn poll_playlist_responses(&mut self) {
        loop {
            match self.playlist_load_rx.try_recv() {
                Ok(resp) => match resp {
                    PlaylistLoadResponse::PlaylistList(items) => {
                        // プレイリスト一覧を更新
                        self.playlists = items;
                    }
                    PlaylistLoadResponse::Progress { current, total, name } => {
                        self.playlist_loading_progress = format!("Loading playlists ({}/{}) {}...", current, total, name);
                    }
                    PlaylistLoadResponse::PlaylistLoaded(playlist) => {
                        self.playlist_cache.insert(playlist);
                    }
                    PlaylistLoadResponse::Complete => {
                        self.playlist_loading = false;
                        self.playlist_loading_progress.clear();
                        // キャッシュを保存
                        let _ = self.playlist_cache.save();
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.playlist_loading = false;
                    break;
                }
            }
        }
    }

    /// 状態を非同期で更新
    pub fn refresh_position(&mut self) {
        if !self.position_pending {
            self.position_pending = true;
            let _ = self.cmd_tx.send(Command::RefreshPosition);
        }
    }

    pub fn refresh_full(&mut self) {
        // pendingチェックを削除 - 2秒ごとにしか呼ばれないので常に送信
        let _ = self.cmd_tx.send(Command::RefreshFull);
    }


    pub fn play_pause(&mut self) {
        self.track.is_playing = !self.track.is_playing;
        if let Err(e) = MusicController::play_pause() {
            self.message = Some(format!("Error: {}", e));
        }
    }

    pub fn next_track(&mut self) {
        if let Err(e) = MusicController::next_track() {
            self.message = Some(format!("Error: {}", e));
        }
    }

    pub fn previous_track(&mut self) {
        if let Err(e) = MusicController::previous_track() {
            self.message = Some(format!("Error: {}", e));
        }
    }

    pub fn toggle_shuffle(&mut self) {
        // 同期的に実行して即座にフィードバック
        match MusicController::toggle_shuffle() {
            Ok(state) => {
                self.shuffle = state;
            }
            Err(e) => {
                self.message = Some(format!("Error: {}", e));
            }
        }
    }

    pub fn cycle_repeat(&mut self) {
        // 同期的に実行して即座にフィードバック
        match MusicController::cycle_repeat() {
            Ok(mode) => {
                self.repeat = mode;
            }
            Err(e) => {
                self.message = Some(format!("Error: {}", e));
            }
        }
    }

    pub fn should_show_welcome(&self) -> bool {
        !self.welcome_dismissed && self.cache.is_fresh_build && !self.cache.is_complete()
    }

    pub fn dismiss_welcome(&mut self) {
        self.welcome_dismissed = true;
    }

    pub fn cycle_highlight_color(&mut self) {
        self.highlight_color = self.highlight_color.next();
        // 設定を保存
        let settings = Settings {
            highlight_color: self.highlight_color,
        };
        let _ = settings.save();
    }

    pub fn seek_backward(&mut self) {
        self.track.position = (self.track.position - 10.0).max(0.0);
        if let Err(e) = MusicController::seek_backward() {
            self.message = Some(format!("Error: {}", e));
        }
    }

    pub fn seek_forward(&mut self) {
        self.track.position = (self.track.position + 10.0).min(self.track.duration);
        if let Err(e) = MusicController::seek_forward() {
            self.message = Some(format!("Error: {}", e));
        }
    }

    pub fn focus_next(&mut self) {
        // Tab: Recently Added <-> Playlists のみ切り替え
        self.focus = match self.focus {
            Focus::RecentlyAdded => Focus::Playlists,
            Focus::Playlists => Focus::RecentlyAdded,
            Focus::Content => Focus::Content,  // Contentでは何もしない
            Focus::Search => Focus::Search,
        };

        // 左ペイン間の移動時は last_left_focus を更新
        match self.focus {
            Focus::RecentlyAdded | Focus::Playlists => {
                self.last_left_focus = self.focus;
            }
            _ => {}
        }

        // Reload content when focus changes to ensure content_source_name matches current selection
        match self.focus {
            Focus::RecentlyAdded => {
                self.load_selected_album_tracks();
            }
            Focus::Playlists => {
                if !self.playlists.is_empty() {
                    self.load_selected_playlist_tracks();
                }
            }
            _ => {}
        }
    }

    /// h: 左カラムへ移動（元いた左ペインに戻り、詳細を再読み込み）
    pub fn focus_left(&mut self) {
        match self.focus {
            Focus::Content => {
                self.focus = self.last_left_focus;
                // 戻り先に応じて詳細画面を再読み込み
                match self.last_left_focus {
                    Focus::RecentlyAdded => {
                        self.load_selected_album_tracks();
                    }
                    Focus::Playlists => {
                        if !self.playlists.is_empty() {
                            self.load_selected_playlist_tracks();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// l: 右カラム（詳細）へ移動、またはプレイリスト曲からアルバム全曲表示へ切替
    pub fn focus_right(&mut self) {
        match self.focus {
            Focus::RecentlyAdded | Focus::Playlists => {
                self.last_left_focus = self.focus;  // 元の左ペインを記憶
                self.focus = Focus::Content;
                self.content_selected = 0;
                self.content_scroll = 0;
            }
            Focus::Content => {
                // プレイリスト詳細表示中の場合、選択中の曲のアルバム全曲を表示
                if self.is_playlist_detail {
                    if let Some(item) = self.content_items.get(self.content_selected) {
                        let album_name = item.album.clone();
                        self.show_album_tracks(&album_name);
                    }
                }
            }
            _ => {}
        }
    }

    /// アルバム名からアルバム全曲を詳細画面に表示（フォーカスはContentのまま）
    pub fn show_album_tracks(&mut self, album_name: &str) {
        let tracks = self.cache.get_tracks_by_album(album_name);
        if !tracks.is_empty() {
            let year = tracks.first().map(|t| t.year).unwrap_or(0);
            let year_str = if year > 0 { format!(" ({})", year) } else { String::new() };
            let artist = tracks.first().map(|t| t.artist.as_str()).unwrap_or("");
            
            self.content_title = format!("{} - {}{}", album_name, artist, year_str);
            self.content_source_name = album_name.to_string();
            self.is_playlist_detail = false;
            self.content_items = tracks
                .into_iter()
                .map(|t| ListItem {
                    name: t.name.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    time: t.time.clone(),
                    year: t.year,
                    track_number: t.track_number,
                    played_count: t.played_count,
                    favorited: t.favorited,
                })
                .collect();
            self.content_selected = 0;
            self.content_scroll = 0;
        }
    }


    /// マウスクリックを処理
    /// 戻り値: クリックが処理されたか
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, terminal_height: u16) -> bool {
        let header_height = 7u16;
        let footer_height = 2u16;

        let main_start_y = header_height;
        let main_end_y = terminal_height.saturating_sub(footer_height + 1);

        // クリックがメインエリア外なら無視
        if y < main_start_y || y >= main_end_y {
            return false;
        }

        let relative_y = y - main_start_y;
        let search_height = 3u16;

        // カラム境界のドラッグ検出 (左カラム幅 ±2 の範囲)
        let column_divider_x = self.left_column_width + 1;
        if x >= column_divider_x.saturating_sub(1) && x <= column_divider_x + 1 {
            self.dragging = Some(DragTarget::ColumnDivider);
            return true;
        }

        // カード境界のドラッグ検出 (左カラム内、Recently AddedとPlaylistsの境界 ±1)
        if x < column_divider_x {
            let card_divider_y = search_height + self.recently_added_height;
            if relative_y >= card_divider_y.saturating_sub(1) && relative_y <= card_divider_y {
                self.dragging = Some(DragTarget::CardDivider);
                return true;
            }
        }

        // 通常のクリック処理
        if x < column_divider_x {
            if relative_y < search_height {
                return false;
            } else if relative_y < search_height + self.recently_added_height {
                // Recently Added
                let card_y = relative_y - search_height;
                if card_y >= 2 {
                    let item_index = (card_y - 2) as usize + self.recently_added_scroll;
                    if item_index < self.recently_added.len() {
                        self.recently_added_selected = item_index;
                        self.focus = Focus::RecentlyAdded;
                        self.last_left_focus = Focus::RecentlyAdded;
                        self.load_selected_album_tracks();
                        return true;
                    }
                }
                self.focus = Focus::RecentlyAdded;
                self.last_left_focus = Focus::RecentlyAdded;
                return true;
            } else {
                // Playlists
                let card_start = search_height + self.recently_added_height;
                let card_y = relative_y - card_start;
                if card_y >= 2 {
                    let item_index = (card_y - 2) as usize + self.playlists_scroll;
                    if item_index < self.playlists.len() {
                        self.playlists_selected = item_index;
                        self.focus = Focus::Playlists;
                        self.last_left_focus = Focus::Playlists;
                        self.load_selected_playlist_tracks();
                        return true;
                    }
                }
                self.focus = Focus::Playlists;
                self.last_left_focus = Focus::Playlists;
                return true;
            }
        } else {
            // Right column (Content)
            if relative_y >= 3 {
                let item_index = (relative_y - 3) as usize + self.content_scroll;
                let items = if self.search_mode { &self.search_results } else { &self.content_items };
                if item_index < items.len() {
                    self.content_selected = item_index;
                    self.focus = Focus::Content;
                    return true;
                }
            }
            self.focus = Focus::Content;
            return true;
        }
    }


    /// マウスドラッグを処理
    pub fn handle_mouse_drag(&mut self, x: u16, y: u16, terminal_height: u16) {
        let Some(target) = self.dragging else {
            return;
        };

        match target {
            DragTarget::ColumnDivider => {
                // 左カラム幅を調整 (最小20、最大でターミナル幅の70%)
                let min_width = 20u16;
                let max_width = 100u16;  // 実際のターミナル幅に依存しないよう固定上限
                self.left_column_width = x.clamp(min_width, max_width);
            }
            DragTarget::CardDivider => {
                // Recently Addedの高さを調整
                let header_height = 7u16;
                let search_height = 3u16;
                let main_start_y = header_height;
                let main_height = terminal_height.saturating_sub(10);  // header + footer + margin

                if y > main_start_y + search_height {
                    let new_height = y - main_start_y - search_height;
                    // 最小5行、最大で main_height - 5 (Playlistsに最低5行残す)
                    let min_height = 5u16;
                    let max_height = main_height.saturating_sub(5);
                    self.recently_added_height = new_height.clamp(min_height, max_height);
                }
            }
        }
    }

    /// マウスボタンを離したときの処理
    pub fn handle_mouse_up(&mut self) {
        self.dragging = None;
    }

    pub fn recently_added_up(&mut self) {
        if self.recently_added_selected > 0 {
            self.recently_added_selected -= 1;
            self.adjust_recently_added_scroll();
            self.load_selected_album_tracks();
        }
    }

    pub fn recently_added_down(&mut self) {
        if self.recently_added_selected < self.recently_added.len().saturating_sub(1) {
            self.recently_added_selected += 1;
            self.adjust_recently_added_scroll();
            self.load_selected_album_tracks();
        }
    }


    pub fn recently_added_top(&mut self) {
        if !self.recently_added.is_empty() {
            self.recently_added_selected = 0;
            self.recently_added_scroll = 0;
            self.load_selected_album_tracks();
        }
    }

    pub fn recently_added_bottom(&mut self) {
        if !self.recently_added.is_empty() {
            self.recently_added_selected = self.recently_added.len() - 1;
            self.adjust_recently_added_scroll();
            self.load_selected_album_tracks();
        }
    }

    /// 選択中のアルバムのトラックを読み込む
    pub fn load_selected_album_tracks(&mut self) {
        if let Some(album_item) = self.recently_added.get(self.recently_added_selected) {
            let album_name = &album_item.album;
            let tracks = self.cache.get_tracks_by_album(album_name);

            // 年を取得（最初のトラックから）
            let year = tracks.first().map(|t| t.year).unwrap_or(0);
            let year_str = if year > 0 { format!(" ({})", year) } else { String::new() };

            self.content_title = format!("{} - {}{}", album_name, album_item.artist, year_str);
            self.content_source_name = album_name.clone();
            self.is_playlist_detail = false;
            self.content_items = tracks
                .into_iter()
                .map(|t| ListItem {
                    name: t.name.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    time: t.time.clone(),
                    year: t.year,
                    track_number: t.track_number,
                    played_count: t.played_count,
                    favorited: t.favorited,
                })
                .collect();
            self.content_selected = 0;
            self.content_scroll = 0;
        }
    }

    /// 選択中のプレイリストのトラックを読み込む
    pub fn load_selected_playlist_tracks(&mut self) {
        if let Some(playlist_item) = self.playlists.get(self.playlists_selected) {
            let playlist_name = playlist_item.name.clone();
            self.content_title = playlist_name.clone();
            self.content_source_name = playlist_name.clone();
            self.is_playlist_detail = true;

            // キャッシュを確認
            if let Some(cached) = self.playlist_cache.get(&playlist_name) {
                // キャッシュから読み込み
                self.content_items = cached.tracks.iter().map(|t| ListItem {
                    name: t.name.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    year: t.year,
                    time: t.time.clone(),
                    played_count: t.played_count,
                    favorited: t.favorited,
                    track_number: 0,
                }).collect();
            } else {
                // キャッシュになければAppleScriptで取得
                self.content_loading = true;
                match MusicController::get_playlist_tracks(&playlist_name) {
                    Ok(tracks) => {
                        // キャッシュに保存
                        let cached_tracks: Vec<CachedPlaylistTrack> = tracks.iter().map(|t| {
                            CachedPlaylistTrack {
                                name: t.name.clone(),
                                artist: t.artist.clone(),
                                album: t.album.clone(),
                                year: t.year,
                                time: t.time.clone(),
                                played_count: t.played_count,
                                favorited: t.favorited,
                            }
                        }).collect();
                        let cached_playlist = CachedPlaylist {
                            name: playlist_name.clone(),
                            tracks: cached_tracks,
                        };
                        self.playlist_cache.insert(cached_playlist);
                        let _ = self.playlist_cache.save();

                        self.content_items = tracks;
                    }
                    Err(_) => {
                        self.content_items = Vec::new();
                    }
                }
                self.content_loading = false;
            }
            self.content_selected = 0;
            self.content_scroll = 0;
        }
    }


    /// プレイリストを強制リフレッシュ（キャッシュを無視して再取得）
    pub fn refresh_current_playlist(&mut self) {
        if !self.is_playlist_detail {
            self.message = Some("Not viewing a playlist".to_string());
            return;
        }
        
        let playlist_name = self.content_source_name.clone();
        if playlist_name.is_empty() {
            return;
        }
        
        self.message = Some(format!("Refreshing {}...", playlist_name));
        // 非同期でリフレッシュ（スピナー表示）
        self.refresh_playlist_cache(&playlist_name);
    }

    fn adjust_recently_added_scroll(&mut self) {
        let visible = self.recently_added_visible;
        if visible == 0 {
            return;
        }
        if self.recently_added_selected < self.recently_added_scroll {
            self.recently_added_scroll = self.recently_added_selected;
        } else if self.recently_added_selected >= self.recently_added_scroll + visible {
            self.recently_added_scroll = self.recently_added_selected - visible + 1;
        }
    }

    pub fn playlists_up(&mut self) {
        if self.playlists_selected > 0 {
            self.playlists_selected -= 1;
            self.adjust_playlists_scroll();
            self.load_selected_playlist_tracks();
        }
    }

    pub fn playlists_down(&mut self) {
        if self.playlists_selected < self.playlists.len().saturating_sub(1) {
            self.playlists_selected += 1;
            self.adjust_playlists_scroll();
            self.load_selected_playlist_tracks();
        }
    }


    pub fn playlists_top(&mut self) {
        if !self.playlists.is_empty() {
            self.playlists_selected = 0;
            self.playlists_scroll = 0;
            self.load_selected_playlist_tracks();
        }
    }

    pub fn playlists_bottom(&mut self) {
        if !self.playlists.is_empty() {
            self.playlists_selected = self.playlists.len() - 1;
            self.adjust_playlists_scroll();
            self.load_selected_playlist_tracks();
        }
    }

    fn adjust_playlists_scroll(&mut self) {
        let visible = self.playlists_visible;
        if visible == 0 {
            return;
        }
        if self.playlists_selected < self.playlists_scroll {
            self.playlists_scroll = self.playlists_selected;
        } else if self.playlists_selected >= self.playlists_scroll + visible {
            self.playlists_scroll = self.playlists_selected - visible + 1;
        }
    }

    pub fn content_up(&mut self) {
        let items = if self.search_mode { &self.search_results } else { &self.content_items };
        if self.content_selected > 0 {
            self.content_selected -= 1;
        }
        self.adjust_scroll(items.len());
    }

    pub fn content_down(&mut self) {
        let items = if self.search_mode { &self.search_results } else { &self.content_items };
        let len = items.len();
        if self.content_selected < len.saturating_sub(1) {
            self.content_selected += 1;
        }
        self.adjust_scroll(len);

        // 検索モードで残り20件以下になったら追加読み込み
        if self.search_mode && self.content_selected + 20 >= self.search_results.len() {
            self.load_more_search_results();
        }
    }


    pub fn content_top(&mut self) {
        self.content_selected = 0;
        self.content_scroll = 0;
    }

    pub fn content_bottom(&mut self) {
        let items = if self.search_mode { &self.search_results } else { &self.content_items };
        let len = items.len();
        if len > 0 {
            self.content_selected = len - 1;
            self.adjust_scroll(len);
        }
    }

    fn adjust_scroll(&mut self, _len: usize) {
        let visible = self.content_visible;
        if visible == 0 {
            return;
        }
        if self.content_selected < self.content_scroll {
            self.content_scroll = self.content_selected;
        } else if self.content_selected >= self.content_scroll + visible {
            self.content_scroll = self.content_selected - visible + 1;
        }
    }

    pub fn play_selected(&mut self) {
        if self.search_mode {
            // 検索結果からの再生
            if let Some(item) = self.search_results.get(self.content_selected) {
                let result = MusicController::play_track(&item.name, &item.artist);
                match result {
                    Ok(_) => {
                        self.message = Some(format!("▶ {}", item.name));
                    }
                    Err(e) => {
                        self.message = Some(format!("Error: {}", e));
                    }
                }
            }
        } else if self.is_playlist_detail {
            // プレイリスト詳細からの再生 - 選択した曲から巡回再生
            let playlist_name = self.content_source_name.clone();
            let track_index = self.content_selected;
            if !playlist_name.is_empty() {
                if let Some(item) = self.content_items.get(track_index) {
                    self.message = Some(format!("▶ {}", item.name));
                }
                // 同期的に実行（競合を避けるため）
                if let Err(e) = accessibility::play_playlist_with_context(&playlist_name, track_index) {
                    self.message = Some(format!("Error: {}", e));
                }
            }
        } else {
            // アルバム詳細からの再生 - 選択した曲から巡回再生
            let album_name = self.content_items
                .first()
                .map(|item| item.album.clone())
                .unwrap_or_else(|| self.content_source_name.clone());
            let track_index = self.content_selected;
            if !album_name.is_empty() {
                if let Some(item) = self.content_items.get(track_index) {
                    self.message = Some(format!("▶ {}", item.name));
                }
                // 同期的に実行（競合を避けるため）
                if let Err(e) = accessibility::play_album_with_context(&album_name, track_index) {
                    self.message = Some(format!("Error: {}", e));
                }
            }
        }
    }

    pub fn start_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_cursor = 0;
        self.focus = Focus::Search;
        self.do_search();  // 空クエリで全曲表示
    }

    pub fn cancel_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_cursor = 0;
        self.search_results.clear();
        self.search_results_all.clear();
        self.search_results_unsorted.clear();
        self.search_total_count = 0;
        self.focus = Focus::RecentlyAdded;
    }

    pub fn search_input(&mut self, c: char) {
        // カーソル位置に文字を挿入
        let byte_pos = self.search_query.chars().take(self.search_cursor).map(|c| c.len_utf8()).sum();
        self.search_query.insert(byte_pos, c);
        self.search_cursor += 1;
        self.do_search();
    }

    pub fn search_backspace(&mut self) {
        // カーソル位置の前の文字を削除 (Ctrl+H)
        if self.search_cursor > 0 {
            let char_indices: Vec<_> = self.search_query.char_indices().collect();
            if let Some(&(byte_pos, _)) = char_indices.get(self.search_cursor - 1) {
                self.search_query.remove(byte_pos);
                self.search_cursor -= 1;
            }
        }
        self.do_search();
    }

    /// カーソルを行頭に移動 (Ctrl+A)
    pub fn search_cursor_start(&mut self) {
        self.search_cursor = 0;
    }

    /// カーソルを行末に移動 (Ctrl+E)
    pub fn search_cursor_end(&mut self) {
        self.search_cursor = self.search_query.chars().count();
    }

    /// カーソルを1文字進める (Ctrl+F)
    pub fn search_cursor_forward(&mut self) {
        let len = self.search_query.chars().count();
        if self.search_cursor < len {
            self.search_cursor += 1;
        }
    }

    /// カーソルを1文字戻す (Ctrl+B)
    pub fn search_cursor_backward(&mut self) {
        if self.search_cursor > 0 {
            self.search_cursor -= 1;
        }
    }

    /// カーソル位置から行末まで削除 (Ctrl+K)
    pub fn search_kill_line(&mut self) {
        let byte_pos: usize = self.search_query.chars().take(self.search_cursor).map(|c| c.len_utf8()).sum();
        self.search_query.truncate(byte_pos);
        self.do_search();
    }

    const SEARCH_PAGE_SIZE: usize = 200;

    fn do_search(&mut self) {
        // キャッシュから検索（高速・同期）
        let mut results: Vec<_> = self.cache
            .search(&self.search_query)
            .into_iter()
            .collect();

        // Artist昇順, Year昇順, Album昇順, Disc昇順, Track昇順 でソート
        results.sort_by(|a, b| {
            a.artist.cmp(&b.artist)
                .then_with(|| a.year.cmp(&b.year))
                .then_with(|| a.album.cmp(&b.album))
                .then_with(|| a.disc_number.cmp(&b.disc_number))
                .then_with(|| a.track_number.cmp(&b.track_number))
        });

        // 全結果をListItemに変換
        self.search_results_all = results
            .into_iter()
            .map(|t| ListItem {
                name: t.name.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                time: t.time.clone(),
                year: t.year,
                track_number: t.track_number,
                played_count: t.played_count,
                favorited: t.favorited,
            })
            .collect();

        self.search_total_count = self.search_results_all.len();

        // 最初の200件のみ表示
        let initial_count = self.search_results_all.len().min(Self::SEARCH_PAGE_SIZE);
        self.search_results = self.search_results_all[..initial_count].to_vec();
        self.search_results_unsorted = self.search_results.clone();
        self.search_sort_mode = SearchSortMode::Default;
        self.content_selected = 0;
        self.content_scroll = 0;
    }

    /// 検索結果をさらに読み込む（スクロール時に呼び出し）
    pub fn load_more_search_results(&mut self) {
        if self.search_results.len() >= self.search_results_all.len() {
            return; // すでに全て読み込み済み
        }

        let current_len = self.search_results.len();
        let next_len = (current_len + Self::SEARCH_PAGE_SIZE).min(self.search_results_all.len());

        // ソートモードに応じて追加
        match self.search_sort_mode {
            SearchSortMode::Default => {
                self.search_results = self.search_results_all[..next_len].to_vec();
                self.search_results_unsorted = self.search_results.clone();
            }
            SearchSortMode::PlayCount => {
                // 再生回数順の場合は全体をソートしてから取得
                let mut sorted = self.search_results_all.clone();
                sorted.sort_by(|a, b| b.played_count.cmp(&a.played_count));
                self.search_results = sorted[..next_len].to_vec();
            }
        }
    }

    pub fn confirm_search(&mut self) {
        if !self.search_results.is_empty() {
            // 検索結果（Detailカード）にフォーカス移動
            self.focus = Focus::Content;
            self.content_selected = 0;
            self.content_scroll = 0;
        }
    }

    /// 検索結果のソートモードを切り替え (s key)
    pub fn toggle_search_sort(&mut self) {
        if self.search_results_all.is_empty() {
            return;
        }

        match self.search_sort_mode {
            SearchSortMode::Default => {
                // 再生回数降順でソート（全結果に適用）
                let mut sorted = self.search_results_all.clone();
                sorted.sort_by(|a, b| b.played_count.cmp(&a.played_count));
                let initial_count = sorted.len().min(Self::SEARCH_PAGE_SIZE);
                self.search_results = sorted[..initial_count].to_vec();
                self.search_sort_mode = SearchSortMode::PlayCount;
            }
            SearchSortMode::PlayCount => {
                // デフォルト順に戻す（最初の200件）
                let initial_count = self.search_results_all.len().min(Self::SEARCH_PAGE_SIZE);
                self.search_results = self.search_results_all[..initial_count].to_vec();
                self.search_results_unsorted = self.search_results.clone();
                self.search_sort_mode = SearchSortMode::Default;
            }
        }
        self.content_selected = 0;
        self.content_scroll = 0;
    }

    /// 検索結果で次のアルバムにジャンプ (Shift+J)
    pub fn search_next_album(&mut self) {
        if self.search_results.is_empty() {
            return;
        }

        let current_album = match self.search_results.get(self.content_selected) {
            Some(item) => &item.album,
            None => return,
        };

        // 現在位置から次の異なるアルバムを探す
        for i in (self.content_selected + 1)..self.search_results.len() {
            if &self.search_results[i].album != current_album {
                self.content_selected = i;
                // スクロール調整
                if self.content_selected >= self.content_scroll + self.content_visible {
                    self.content_scroll = self.content_selected.saturating_sub(self.content_visible - 1);
                }
                return;
            }
        }
    }

    /// 検索結果で前のアルバムにジャンプ (Shift+K)
    pub fn search_prev_album(&mut self) {
        if self.search_results.is_empty() || self.content_selected == 0 {
            return;
        }

        let current_album = match self.search_results.get(self.content_selected) {
            Some(item) => &item.album,
            None => return,
        };

        // 現在位置から前の異なるアルバムを探す
        for i in (0..self.content_selected).rev() {
            if &self.search_results[i].album != current_album {
                // そのアルバムの最初のトラックを探す
                let target_album = &self.search_results[i].album;
                let mut first_of_album = i;
                for j in (0..i).rev() {
                    if &self.search_results[j].album == target_album {
                        first_of_album = j;
                    } else {
                        break;
                    }
                }
                self.content_selected = first_of_album;
                // スクロール調整
                if self.content_selected < self.content_scroll {
                    self.content_scroll = self.content_selected;
                }
                return;
            }
        }

        // 見つからなければ先頭へ
        self.content_selected = 0;
        self.content_scroll = 0;
    }


    // ========== プレイリスト追加モード ==========

    /// プレイリスト追加モードを開始
    pub fn start_add_to_playlist(&mut self) {
        // Content にフォーカスがあり、曲が選択されている場合のみ
        if self.focus != Focus::Content {
            return;
        }
        
        let items = if self.search_mode { &self.search_results } else { &self.content_items };
        if let Some(item) = items.get(self.content_selected) {
            self.track_to_add = Some(item.clone());
            self.add_to_playlist_mode = true;
            self.focus = Focus::Playlists;
            self.playlists_selected = 0;
            self.playlists_scroll = 0;
        }
    }

    /// プレイリスト追加モードをキャンセル
    pub fn cancel_add_to_playlist(&mut self) {
        self.add_to_playlist_mode = false;
        self.track_to_add = None;
        self.new_playlist_input_mode = false;
        self.new_playlist_name.clear();
        self.focus = Focus::Content;
    }

    /// 選択したプレイリストに曲を追加
    pub fn confirm_add_to_playlist(&mut self) {
        // "+ New playlist" が選択された場合
        if self.playlists_selected >= self.playlists.len() {
            self.new_playlist_input_mode = true;
            return;
        }

        let Some(track) = &self.track_to_add else {
            self.cancel_add_to_playlist();
            return;
        };

        let Some(playlist) = self.playlists.get(self.playlists_selected) else {
            self.cancel_add_to_playlist();
            return;
        };

        let playlist_name = playlist.name.clone();
        let track_name = track.name.clone();
        let track_album = track.album.clone();

        // AppleScriptでプレイリストに曲を追加
        match Self::add_track_to_playlist(&track_name, &track_album, &playlist_name) {
            Ok(_) => {
                self.message = Some(format!("Added to '{}'", playlist_name));
                // プレイリストキャッシュを更新
                self.refresh_playlist_cache(&playlist_name);
            }
            Err(e) => {
                self.message = Some(format!("Error: {}", e));
            }
        }

        self.add_to_playlist_mode = false;
        self.track_to_add = None;
        self.focus = Focus::Content;
    }

    /// 新規プレイリスト名の入力
    pub fn new_playlist_input(&mut self, c: char) {
        self.new_playlist_name.push(c);
    }

    /// 新規プレイリスト名のバックスペース
    pub fn new_playlist_backspace(&mut self) {
        self.new_playlist_name.pop();
    }

    /// 新規プレイリストを作成して曲を追加
    pub fn confirm_new_playlist(&mut self) {
        if self.new_playlist_name.is_empty() {
            return;
        }

        let Some(track) = &self.track_to_add else {
            self.cancel_add_to_playlist();
            return;
        };

        let playlist_name = self.new_playlist_name.clone();
        let track_name = track.name.clone();
        let track_album = track.album.clone();

        // AppleScriptで新規プレイリストを作成して曲を追加
        match Self::create_playlist_and_add_track(&playlist_name, &track_name, &track_album) {
            Ok(_) => {
                self.message = Some(format!("Created '{}' and added track", playlist_name));
                // プレイリスト一覧に追加
                self.playlists.push(ListItem {
                    name: playlist_name.clone(),
                    artist: String::new(),
                    album: String::new(),
                    time: String::new(),
                    year: 0,
                    track_number: 0,
                    played_count: 0,
                    favorited: false,
                });
                // プレイリストキャッシュを更新
                self.refresh_playlist_cache(&playlist_name);
            }
            Err(e) => {
                self.message = Some(format!("Error: {}", e));
            }
        }

        self.add_to_playlist_mode = false;
        self.track_to_add = None;
        self.new_playlist_input_mode = false;
        self.new_playlist_name.clear();
        self.focus = Focus::Content;
    }

    /// AppleScript: プレイリストに曲を追加
    fn add_track_to_playlist(track_name: &str, track_album: &str, playlist_name: &str) -> Result<(), String> {
        let script = format!(
            r#"tell application "Music"
                set targetTrack to (first track of library playlist 1 whose name is "{}" and album is "{}")
                set targetPlaylist to (first playlist whose name is "{}")
                duplicate targetTrack to targetPlaylist
            end tell"#,
            track_name.replace('"', "\\\""),
            track_album.replace('"', "\\\""),
            playlist_name.replace('"', "\\\"")
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("Failed to run osascript: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(err.trim().to_string())
        }
    }

    /// AppleScript: 新規プレイリストを作成して曲を追加
    fn create_playlist_and_add_track(playlist_name: &str, track_name: &str, track_album: &str) -> Result<(), String> {
        let script = format!(
            r#"tell application "Music"
                set newPlaylist to make new playlist with properties {{name:"{}"}}
                set targetTrack to (first track of library playlist 1 whose name is "{}" and album is "{}")
                duplicate targetTrack to newPlaylist
            end tell"#,
            playlist_name.replace('"', "\\\""),
            track_name.replace('"', "\\\""),
            track_album.replace('"', "\\\"")
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("Failed to run osascript: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(err.trim().to_string())
        }
    }

    /// プレイリスト追加モード用のプレイリスト数（+ New playlist を含む）
    pub fn playlists_count_with_new(&self) -> usize {
        self.playlists.len() + 1
    }


    /// 指定したプレイリストのキャッシュを非同期で更新
    fn refresh_playlist_cache(&mut self, playlist_name: &str) {
        let name = playlist_name.to_string();
        self.playlist_refreshing = Some(name.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        self.playlist_refresh_rx = Some(rx);

        std::thread::spawn(move || {
            if let Ok(tracks) = MusicController::get_playlist_tracks(&name) {
                let _ = tx.send((name, tracks));
            }
        });
    }


    /// プレイリスト更新の完了をポーリング
    pub fn poll_playlist_refresh(&mut self) {
        if let Some(rx) = &self.playlist_refresh_rx {
            if let Ok((playlist_name, tracks)) = rx.try_recv() {
                // キャッシュを更新
                let cached_tracks: Vec<CachedPlaylistTrack> = tracks.iter().map(|t| {
                    CachedPlaylistTrack {
                        name: t.name.clone(),
                        artist: t.artist.clone(),
                        album: t.album.clone(),
                        year: t.year,
                        time: t.time.clone(),
                        played_count: t.played_count,
                        favorited: t.favorited,
                    }
                }).collect();
                let cached_playlist = CachedPlaylist {
                    name: playlist_name.clone(),
                    tracks: cached_tracks,
                };
                self.playlist_cache.insert(cached_playlist);
                let _ = self.playlist_cache.save();

                // 現在表示中のプレイリストなら content_items も更新
                if self.is_playlist_detail && self.content_source_name == playlist_name {
                    self.content_items = tracks;
                }

                self.playlist_refreshing = None;
                self.playlist_refresh_rx = None;
            }
        }
    }
}
