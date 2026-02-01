use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use rand::Rng;

use crate::accessibility;
use crate::cache::{CachedTrack, CachedPlaylist, CachedPlaylistTrack, PlaylistCache, TrackCache};
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


pub struct App {
    pub track: TrackInfo,
    pub volume: i32,
    pub shuffle: bool,
    pub repeat: String,
    pub message: Option<String>,
    pub should_quit: bool,

    pub focus: Focus,
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

    pub search_mode: bool,
    pub search_query: String,
    pub search_results: Vec<ListItem>,

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
}

impl App {
    pub fn new() -> Self {
        // Initialize Music window off-screen at startup
        // This ensures the window exists before any playlist playback
        accessibility::init_music_window_offscreen();

        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (resp_tx, resp_rx) = mpsc::channel::<Response>();
        let (cache_resp_tx, cache_resp_rx) = mpsc::channel::<CacheResponse>();

        // キャッシュを読み込み
        let cache = TrackCache::load();

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
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
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
        //
        // Left column:
        // - search: 3 (when not in search mode)
        // - recently_added: 12 (fixed)
        // - playlists: remaining
        //
        // Recently Added card (height 12):
        // - border: 2
        // - title: 1
        // - list: 12 - 2 - 1 = 9
        //
        // Playlists card:
        // - border: 2
        // - title: 1
        // - list: remaining - 3

        let main_height = terminal_height.saturating_sub(8);
        let search_height: u16 = if self.search_mode { 3 } else { 3 };
        let recently_added_height: u16 = 12;
        let playlists_height = main_height.saturating_sub(search_height + recently_added_height);

        // Recently Added: 固定12行のカード
        // カード高さ12 - ボーダー2 - タイトル1 = リスト部分9行
        // 余白を考慮して1引く
        self.recently_added_visible = 8;

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
        self.focus = match self.focus {
            Focus::RecentlyAdded => Focus::Playlists,
            Focus::Playlists => Focus::Content,
            Focus::Content => Focus::RecentlyAdded,
            Focus::Search => Focus::Content,
        };

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


    /// マウスクリックを処理
    /// 戻り値: クリックが処理されたか
    pub fn handle_mouse_click(&mut self, x: u16, y: u16, terminal_height: u16) -> bool {
        // レイアウト計算
        // margin: 1
        // header: 4 (実際のレンダリング位置を考慮して+2調整)
        // footer: 2

        let header_height = 7u16;  // 4 + margin調整
        let footer_height = 2u16;
        let left_column_width = 40u16;

        let main_start_y = header_height;
        let main_end_y = terminal_height.saturating_sub(footer_height + 1);
        
        // クリックがメインエリア外なら無視
        if y < main_start_y || y >= main_end_y {
            return false;
        }
        
        let relative_y = y - main_start_y;
        
        // 左カラム (x < left_column_width + 1 for margin)
        if x < left_column_width + 1 {
            // Search: 3行
            // Recently Added: 12行
            // Playlists: 残り
            let search_height = 3u16;
            let recently_added_height = 12u16;
            
            if relative_y < search_height {
                // Search area - 無視
                return false;
            } else if relative_y < search_height + recently_added_height {
                // Recently Added
                let card_y = relative_y - search_height;
                // カード内: ボーダー1 + タイトル1 = 2行がヘッダー
                if card_y >= 2 {
                    let item_index = (card_y - 2) as usize + self.recently_added_scroll;
                    if item_index < self.recently_added.len() {
                        self.recently_added_selected = item_index;
                        self.focus = Focus::RecentlyAdded;
                        self.load_selected_album_tracks();
                        return true;
                    }
                }
                self.focus = Focus::RecentlyAdded;
                return true;
            } else {
                // Playlists
                let card_start = search_height + recently_added_height;
                let card_y = relative_y - card_start;
                // カード内: ボーダー1 + タイトル1 = 2行がヘッダー
                if card_y >= 2 {
                    let item_index = (card_y - 2) as usize + self.playlists_scroll;
                    if item_index < self.playlists.len() {
                        self.playlists_selected = item_index;
                        self.focus = Focus::Playlists;
                        self.load_selected_playlist_tracks();
                        return true;
                    }
                }
                self.focus = Focus::Playlists;
                return true;
            }
        } else {
            // Right column (Content)
            // ボーダー1 + タイトル1 + ヘッダー1 = 3行がヘッダー
            if relative_y >= 3 {
                let item_index = (relative_y - 3) as usize + self.content_scroll;
                let items = if self.search_mode { &self.search_results } else { &self.content_items };
                if item_index < items.len() {
                    // 既に選択済みの曲をクリックしたら再生
                    if self.content_selected == item_index && self.focus == Focus::Content {
                        self.play_selected();
                    } else {
                        self.content_selected = item_index;
                        self.focus = Focus::Content;
                    }
                    return true;
                }
            }
            self.focus = Focus::Content;
            return true;
        }
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
        self.content_loading = true;
        
        match MusicController::get_playlist_tracks(&playlist_name) {
            Ok(tracks) => {
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
                
                self.content_items = tracks;
                self.message = Some(format!("Refreshed {}", playlist_name));
            }
            Err(e) => {
                self.message = Some(format!("Refresh failed: {}", e));
            }
        }
        self.content_loading = false;
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
        self.search_results.clear();
        self.focus = Focus::Search;
    }

    pub fn cancel_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.focus = Focus::RecentlyAdded;
    }

    pub fn search_input(&mut self, c: char) {
        self.search_query.push(c);
        self.do_search();
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        if self.search_query.is_empty() {
            self.search_results.clear();
        } else {
            self.do_search();
        }
    }

    fn do_search(&mut self) {
        if self.search_query.len() >= 3 {
            // キャッシュから検索（高速・同期）
            let results: Vec<ListItem> = self.cache
                .search(&self.search_query)
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
            self.search_results = results;
            self.content_selected = 0;
            self.content_scroll = 0;
        }
    }

    pub fn confirm_search(&mut self) {
        if !self.search_results.is_empty() {
            // 選択した項目を再生
            self.play_selected();
            // 検索モードを終了
            self.search_mode = false;
            self.focus = Focus::Content;
        }
    }
}
