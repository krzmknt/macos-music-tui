use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus};
use crate::music::{ListItem, TrackInfo};

const BG_ACCENT: Color = Color::Rgb(60, 60, 80);
const BG_SELECTED: Color = Color::Rgb(50, 50, 60);
const BORDER_DIM: Color = Color::Rgb(60, 60, 75);
const TEXT_PRIMARY: Color = Color::Rgb(255, 255, 255);
const TEXT_SECONDARY: Color = Color::Rgb(160, 160, 180);
const TEXT_DIM: Color = Color::Rgb(100, 100, 120);
const ACCENT_GREEN: Color = Color::Rgb(80, 220, 120);

fn accent_color(app: &App) -> Color {
    let (r, g, b) = app.highlight_color.rgb();
    Color::Rgb(r, g, b)
}

fn border_focus_color(app: &App) -> Color {
    accent_color(app)
}

pub fn draw(frame: &mut Frame, app: &App) {
    // 初回起動時（キャッシュなし）はウェルカム画面を表示
    if app.should_show_welcome() {
        draw_welcome(frame, app);
        return;
    }

    // ヘルプ画面表示
    if app.show_help {
        draw_help(frame, app);
        return;
    }

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(4),  // Header (2 lines + border)
            Constraint::Min(10),    // Body (2 columns)
            Constraint::Length(2),  // Footer (command guide)
        ])
        .split(frame.area());

    draw_header(frame, app, main_chunks[0]);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(app.left_column_width),  // Left column (resizable)
            Constraint::Min(30),     // Right column (Content)
        ])
        .split(main_chunks[1]);

    draw_left_column(frame, app, body_chunks[0]);
    draw_content(frame, app, body_chunks[1]);
    draw_footer(frame, app, main_chunks[2]);
}

fn draw_welcome(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // カードサイズ
    let card_width = 50u16;
    let card_height = 12u16;

    // 中央に配置
    let card_x = area.x + (area.width.saturating_sub(card_width)) / 2;
    let card_y = area.y + (area.height.saturating_sub(card_height)) / 2;

    let card_area = Rect {
        x: card_x,
        y: card_y,
        width: card_width.min(area.width),
        height: card_height.min(area.height),
    };

    // カード背景
    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color(app)))
        .title(" Welcome ")
        .title_style(Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD));
    frame.render_widget(card, card_area);

    let inner = inner_area(card_area, 2, 1);

    // タイトル
    let title = Paragraph::new("macos-music-tui")
        .style(Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD))
        .alignment(ratatui::layout::Alignment::Center);
    let title_area = Rect { height: 1, ..inner };
    frame.render_widget(title, title_area);

    // スピナーと進捗
    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner_char = spinner_frames[app.spinner_frame];
    let total_str = if app.cache.total_tracks > 0 {
        app.cache.total_tracks.to_string()
    } else {
        "?".to_string()
    };
    let progress_text = format!(
        "{} Building cache: {}/{}",
        spinner_char,
        app.cache.loaded_tracks,
        total_str
    );
    let progress = Paragraph::new(progress_text)
        .style(Style::default().fg(accent_color(app)))
        .alignment(ratatui::layout::Alignment::Center);
    let progress_area = Rect { y: inner.y + 2, height: 1, ..inner };
    frame.render_widget(progress, progress_area);

    // 注意書き1
    let notice1 = Paragraph::new("Keep this window open while caching")
        .style(Style::default().fg(Color::Rgb(255, 200, 100)))
        .alignment(ratatui::layout::Alignment::Center);
    let notice1_area = Rect { y: inner.y + 4, height: 1, ..inner };
    frame.render_widget(notice1, notice1_area);

    // 注意書き2
    let notice2 = Paragraph::new("Progress is saved if you close")
        .style(Style::default().fg(TEXT_DIM))
        .alignment(ratatui::layout::Alignment::Center);
    let notice2_area = Rect { y: inner.y + 5, height: 1, ..inner };
    frame.render_widget(notice2, notice2_area);

    // カラー変更の案内
    let color_hint = Paragraph::new("Press 'c' to change highlight color")
        .style(Style::default().fg(accent_color(app)))
        .alignment(ratatui::layout::Alignment::Center);
    let color_hint_area = Rect { y: inner.y + 7, height: 1, ..inner };
    frame.render_widget(color_hint, color_hint_area);

    // フッター
    let footer = Paragraph::new("Press any key to continue")
        .style(Style::default().fg(TEXT_DIM))
        .alignment(ratatui::layout::Alignment::Center);
    let footer_area = Rect { y: inner.y + 9, height: 1, ..inner };
    frame.render_widget(footer, footer_area);
}

fn draw_help(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // カードサイズ
    let card_width = 60u16;
    let card_height = 20u16;

    // 中央に配置
    let card_x = area.x + (area.width.saturating_sub(card_width)) / 2;
    let card_y = area.y + (area.height.saturating_sub(card_height)) / 2;

    let card_area = Rect {
        x: card_x,
        y: card_y,
        width: card_width.min(area.width),
        height: card_height.min(area.height),
    };

    // カード背景
    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color(app)))
        .title(" Search Help ")
        .title_style(Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD));
    frame.render_widget(card, card_area);

    let inner = inner_area(card_area, 2, 1);

    let help_text = vec![
        Line::from(vec![
            Span::styled("Advanced Search", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Field Filters", Style::default().fg(accent_color(app))),
            Span::styled(" (case-insensitive prefix)", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  name:", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("xxx    ", Style::default().fg(TEXT_DIM)),
            Span::styled("Search in track name", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  artist:", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("xxx  ", Style::default().fg(TEXT_DIM)),
            Span::styled("Search in artist name", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  album:", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("xxx   ", Style::default().fg(TEXT_DIM)),
            Span::styled("Search in album name", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Exact Match", Style::default().fg(accent_color(app))),
            Span::styled(" (use quotes)", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  artist:\"ABC\"", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("   Artist is exactly \"IO\"", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  name:'OK'", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("     Name is exactly \"OK\"", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Smart Case", Style::default().fg(accent_color(app))),
            Span::styled(" (without quotes)", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  lowercase", Style::default().fg(TEXT_PRIMARY)),
            Span::styled(" → case-insensitive", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("  hasUppercase", Style::default().fg(TEXT_PRIMARY)),
            Span::styled(" → case-sensitive", Style::default().fg(TEXT_DIM)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press any key to close", Style::default().fg(TEXT_DIM)),
        ]),
    ];

    let help = Paragraph::new(help_text);
    frame.render_widget(help, inner);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM));
    frame.render_widget(card, area);

    let inner = inner_area(area, 2, 1);

    // 2 lines layout
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Line 1: Track info + controls
            Constraint::Length(1),  // Line 2: Progress bar
        ])
        .split(inner);

    // Line 1: {icon} {level_meter} {song} - {artist} - {album}  [right: Shuffle/Repeat/Vol]
    let (name, artist, album) = if !app.track.is_playing && app.track.is_empty() {
        ("Not Playing".to_string(), "—".to_string(), "—".to_string())
    } else {
        let name = if app.track.name.is_empty() { "(No title)".to_string() } else { app.track.name.clone() };
        let artist = if app.track.artist.is_empty() { "(No artist)".to_string() } else { app.track.artist.clone() };
        let album = if app.track.album.is_empty() { "(No album)".to_string() } else { app.track.album.clone() };
        (name, artist, album)
    };

    let status_icon = if app.track.is_playing { "▶" } else { "⏸" };

    // Level meter bars using braille (thinner)
    let bar_chars = ['⠀', '⡀', '⡄', '⡆', '⡇', '⣇', '⣧', '⣿'];
    let level_meter: String = app.level_meter.iter()
        .map(|&v| bar_chars[v as usize])
        .collect();

    // Shuffle display
    let shuffle_display = if app.shuffle { "on ".to_string() } else { "off".to_string() };
    let shuffle_style = if app.shuffle {
        Style::default().fg(ACCENT_GREEN)
    } else {
        Style::default().fg(TEXT_SECONDARY)
    };

    // Repeat display
    let repeat_display = format!("{:<3}", &app.repeat);
    let repeat_style = match app.repeat.as_str() {
        "all" | "one" => Style::default().fg(ACCENT_GREEN),
        _ => Style::default().fg(TEXT_SECONDARY),
    };

    // Build controls string for right side (fixed width)
    let controls_len = 30; // "Shuffle(s): OFF  Repeat(r): off"

    // Calculate track info max width
    let track_max = (inner.width as usize).saturating_sub(controls_len + 5);

    // Split line 1 into left (track) and right (controls)
    let line1_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length(controls_len as u16 + 1),
        ])
        .split(lines[0]);

    // 各フィールドに最大幅を設定（より緩やかな制限）
    let name_max = track_max * 40 / 100;
    let artist_max = track_max * 30 / 100;
    let album_max = track_max * 30 / 100;

    let track_info = Paragraph::new(Line::from(vec![
        Span::styled(format!("{} ", status_icon), Style::default().fg(ACCENT_GREEN)),
        Span::styled(format!("{} ", level_meter), Style::default().fg(accent_color(app))),
        Span::styled(truncate(&name, name_max), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::styled(" - ", Style::default().fg(TEXT_DIM)),
        Span::styled(truncate(&artist, artist_max), Style::default().fg(accent_color(app))),
        Span::styled(" - ", Style::default().fg(TEXT_DIM)),
        Span::styled(truncate(&album, album_max), Style::default().fg(TEXT_SECONDARY)),
    ]));
    frame.render_widget(track_info, line1_layout[0]);

    let controls = Paragraph::new(Line::from(vec![
        Span::styled("Shuffle(s): ", Style::default().fg(TEXT_DIM)),
        Span::styled(&shuffle_display, shuffle_style),
        Span::styled("  Repeat(r): ", Style::default().fg(TEXT_DIM)),
        Span::styled(&repeat_display, repeat_style),
    ]));
    frame.render_widget(controls, line1_layout[1]);

    // Line 2: {mm:ss} {seekbar} {mm:ss}
    let ratio = if app.track.duration > 0.0 {
        (app.track.position / app.track.duration).min(1.0)
    } else {
        0.0
    };
    let current = TrackInfo::format_time(app.track.position);
    let total = TrackInfo::format_time(app.track.duration);

    let time_width = 14; // "00:00  00:00 "
    let bar_width = (inner.width as usize).saturating_sub(time_width);
    let filled = (ratio * bar_width as f64) as usize;
    let empty = bar_width.saturating_sub(filled);

    let line2 = Paragraph::new(Line::from(vec![
        Span::styled(&current, Style::default().fg(TEXT_DIM)),
        Span::styled(" ", Style::default()),
        Span::styled("━".repeat(filled), Style::default().fg(accent_color(app))),
        Span::styled("─".repeat(empty), Style::default().fg(BG_ACCENT)),
        Span::styled(" ", Style::default()),
        Span::styled(&total, Style::default().fg(TEXT_DIM)),
    ]));
    frame.render_widget(line2, lines[1]);
}

fn draw_left_column(frame: &mut Frame, app: &App, area: Rect) {
    // 読み込み状態に応じてSearchカードの高さを変える
    // - プレイリスト読み込み中: 6行（入力 + キャッシュ状態 + 日付 + プレイリスト読み込み）
    // - 通常: 5行（入力 + 曲数 + 日付）
    let search_height = if app.playlist_loading { 6 } else { 5 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(search_height),  // Search
            Constraint::Length(app.recently_added_height),  // Recently Added (resizable)
            Constraint::Min(5),                 // Playlists
        ])
        .split(area);

    draw_search_box(frame, app, chunks[0]);
    draw_recently_added(frame, app, chunks[1]);
    draw_playlists(frame, app, chunks[2]);
}

fn draw_search_box(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::Search;
    let border_color = if is_focused { border_focus_color(app) } else { BORDER_DIM };

    // キャッシュ中は高さを増やす
    let is_caching = !app.cache.is_complete();

    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(card, area);

    let inner = inner_area(area, 2, 1);

    // 検索入力行
    let search_line = if app.search_mode {
        if app.search_query.is_empty() {
            Line::from(vec![
                Span::styled("Type to search...", Style::default().fg(TEXT_DIM)),
            ])
        } else {
            Line::from(vec![
                Span::styled(&app.search_query, Style::default().fg(TEXT_PRIMARY)),
            ])
        }
    } else {
        Line::from(vec![
            Span::styled("/ Search", Style::default().fg(TEXT_DIM)),
        ])
    };

    let search_area = Rect { height: 1, ..inner };
    frame.render_widget(Paragraph::new(search_line), search_area);

    // 検索モード時はカーソルを検索入力位置に配置（IME対応）
    if app.search_mode && app.focus == Focus::Search {
        // カーソル位置までの表示幅を計算
        let cursor_width: usize = app.search_query.chars()
            .take(app.search_cursor)
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        let cursor_x = search_area.x + cursor_width as u16;
        let cursor_y = search_area.y;
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // キャッシュ状態表示
    if is_caching {
        // キャッシュ中: 進捗と注意書き
        if inner.height >= 2 {
            let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let spinner_char = spinner_frames[app.spinner_frame];
            let total_str = if app.cache.total_tracks > 0 {
                app.cache.total_tracks.to_string()
            } else {
                "? (calculating)".to_string()
            };
            let progress_text = format!(
                "{} Caching: {}/{}",
                spinner_char,
                app.cache.loaded_tracks,
                total_str
            );
            let cache_area = Rect {
                y: inner.y + 1,
                height: 1,
                ..inner
            };
            frame.render_widget(
                Paragraph::new(progress_text).style(Style::default().fg(TEXT_DIM)),
                cache_area,
            );
        }

        if inner.height >= 3 {
            let notice_area = Rect {
                y: inner.y + 2,
                height: 1,
                ..inner
            };
            frame.render_widget(
                Paragraph::new("Search on cached data only")
                    .style(Style::default().fg(TEXT_DIM)),
                notice_area,
            );
        }
    } else {
        // キャッシュ完了: 曲数（2行目）と日付（3行目）
        if inner.height >= 2 {
            let count_text = format!("{} tracks cached", app.cache.loaded_tracks);
            let count_area = Rect {
                y: inner.y + 1,
                height: 1,
                ..inner
            };
            frame.render_widget(
                Paragraph::new(count_text).style(Style::default().fg(TEXT_DIM)),
                count_area,
            );
        }
        if inner.height >= 3 {
            if let Some(date_str) = app.cache.format_last_updated() {
                let date_area = Rect {
                    y: inner.y + 2,
                    height: 1,
                    ..inner
                };
                frame.render_widget(
                    Paragraph::new(date_str).style(Style::default().fg(TEXT_DIM)),
                    date_area,
                );
            }
        }
    }

    // プレイリスト読み込み中の表示
    if app.playlist_loading && inner.height >= 4 {
        let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let spinner_char = spinner_frames[app.spinner_frame];
        let playlist_text = if app.playlist_loading_progress.is_empty() {
            format!("{} Loading playlists...", spinner_char)
        } else {
            format!("{} {}", spinner_char, app.playlist_loading_progress)
        };
        let playlist_area = Rect {
            y: inner.y + 3,
            height: 1,
            ..inner
        };
        frame.render_widget(
            Paragraph::new(playlist_text).style(Style::default().fg(accent_color(app))),
            playlist_area,
        );
    }
}

fn draw_recently_added(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::RecentlyAdded && !app.search_mode;
    let border_color = if is_focused { border_focus_color(app) } else { BORDER_DIM };

    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(card, area);

    let inner = inner_area(area, 2, 1);

    // Title
    let title_area = Rect { height: 1, ..inner };
    let title = Paragraph::new(Line::from(vec![
        Span::styled("Recently Added", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
    ]));
    frame.render_widget(title, title_area);

    let list_area = Rect {
        y: inner.y + 1,
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let visible_count = list_area.height as usize;

    if app.recently_added.is_empty() {
        let empty = Paragraph::new(Span::styled("No items", Style::default().fg(TEXT_DIM)));
        frame.render_widget(empty, list_area);
    } else {
        for (i, item) in app.recently_added.iter().enumerate().skip(app.recently_added_scroll).take(visible_count) {
            let y = list_area.y + (i - app.recently_added_scroll) as u16;
            if y >= list_area.y + list_area.height {
                break;
            }
            let line_area = Rect { x: list_area.x, y, width: list_area.width, height: 1 };
            let is_selected = i == app.recently_added_selected;

            // 選択行の背景色
            let bg_style = if is_selected && is_focused {
                Style::default().bg(BG_SELECTED)
            } else {
                Style::default()
            };

            let (album_style, artist_style, prefix) = if is_selected && is_focused {
                (Style::default().fg(TEXT_PRIMARY).bg(BG_SELECTED),
                 Style::default().fg(TEXT_SECONDARY).bg(BG_SELECTED),
                 "▎")
            } else {
                (Style::default().fg(TEXT_SECONDARY),
                 Style::default().fg(TEXT_DIM),
                 " ")
            };

            let max_len = list_area.width.saturating_sub(2) as usize;

            // アルバム名とアーティスト名を別々のスタイルで表示
            let line = if !item.artist.is_empty() {
                let separator = " - ";
                let album_max = max_len.saturating_sub(separator.len() + item.artist.width()).min(max_len * 60 / 100);
                let artist_max = max_len.saturating_sub(album_max + separator.len());
                let remaining = max_len.saturating_sub(truncate(&item.album, album_max).width() + separator.len() + truncate(&item.artist, artist_max).width());

                Paragraph::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(accent_color(app)).bg(if is_selected && is_focused { BG_SELECTED } else { Color::Reset })),
                    Span::styled(truncate(&item.album, album_max), album_style),
                    Span::styled(separator, Style::default().fg(TEXT_DIM).bg(if is_selected && is_focused { BG_SELECTED } else { Color::Reset })),
                    Span::styled(truncate(&item.artist, artist_max), artist_style),
                    Span::styled(" ".repeat(remaining), bg_style),
                ]))
            } else {
                let remaining = max_len.saturating_sub(truncate(&item.name, max_len).width());
                Paragraph::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(accent_color(app)).bg(if is_selected && is_focused { BG_SELECTED } else { Color::Reset })),
                    Span::styled(truncate(&item.name, max_len), album_style),
                    Span::styled(" ".repeat(remaining), bg_style),
                ]))
            };
            frame.render_widget(line, line_area);
        }
    }
}

fn draw_playlists(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::Playlists && !app.search_mode;
    let border_color = if is_focused { border_focus_color(app) } else { BORDER_DIM };

    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(card, area);

    let inner = inner_area(area, 2, 1);

    // スピナーフレーム
    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner_char = spinner_frames[app.spinner_frame];

    // タイトル
    let title_area = Rect { height: 1, ..inner };
    if app.new_playlist_input_mode {
        // 新規プレイリスト名入力モード
        let prefix = "New: ";
        let input_display = format!("{}{}", prefix, app.new_playlist_name);
        let title = Paragraph::new(Line::from(vec![
            Span::styled(input_display, Style::default().fg(accent_color(app))),
        ]));
        frame.render_widget(title, title_area);
        // カーソル位置を設定（IME対応）
        let cursor_x = title_area.x + prefix.width() as u16 + app.new_playlist_name.width() as u16;
        let cursor_y = title_area.y;
        frame.set_cursor_position((cursor_x, cursor_y));
    } else if app.add_to_playlist_mode {
        // プレイリスト追加モード
        let title = Paragraph::new(Line::from(vec![
            Span::styled("Add to which playlist?", Style::default().fg(accent_color(app)).add_modifier(Modifier::BOLD)),
        ]));
        frame.render_widget(title, title_area);
    } else {
        // 通常モード
        let playlist_count = app.playlists.len();
        let title = Paragraph::new(Line::from(vec![
            Span::styled("Playlists", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" ({})", playlist_count), Style::default().fg(TEXT_DIM)),
        ]));
        frame.render_widget(title, title_area);
    }

    if app.playlists.is_empty() && !app.add_to_playlist_mode {
        let empty_area = Rect { y: inner.y + 1, height: 1, ..inner };
        let empty = Paragraph::new(Span::styled("Loading...", Style::default().fg(TEXT_DIM)));
        frame.render_widget(empty, empty_area);
    } else {
        let visible_height = (inner.height.saturating_sub(1)) as usize; // -1 for title
        
        // プレイリスト追加モード時は "+ New playlist" を含めた総数
        let total_items = if app.add_to_playlist_mode {
            app.playlists_count_with_new()
        } else {
            app.playlists.len()
        };

        for idx in app.playlists_scroll..(app.playlists_scroll + visible_height).min(total_items) {
            let y = inner.y + 1 + (idx - app.playlists_scroll) as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let line_area = Rect { x: inner.x, y, width: inner.width, height: 1 };
            let is_selected = idx == app.playlists_selected;
            let row_width = inner.width as usize;

            if idx < app.playlists.len() {
                // 通常のプレイリスト
                let item = &app.playlists[idx];
                let is_refreshing = app.playlist_refreshing.as_ref() == Some(&item.name);

                let (style, prefix, bg) = if is_selected && is_focused {
                    (Style::default().fg(TEXT_PRIMARY).bg(BG_SELECTED), "▎", BG_SELECTED)
                } else {
                    (Style::default().fg(TEXT_SECONDARY), " ", Color::Reset)
                };

                let mut spans = vec![
                    Span::styled(prefix, Style::default().fg(accent_color(app)).bg(bg)),
                    Span::styled(&item.name, style),
                ];
                if is_refreshing {
                    spans.push(Span::styled(format!(" {}", spinner_char), Style::default().fg(accent_color(app)).bg(bg)));
                }
                // 行末まで背景色を埋める
                let content_len = 1 + item.name.width() + if is_refreshing { 2 } else { 0 };
                let remaining = row_width.saturating_sub(content_len);
                spans.push(Span::styled(" ".repeat(remaining), Style::default().bg(bg)));

                let line = Paragraph::new(Line::from(spans));
                frame.render_widget(line, line_area);
            } else if app.add_to_playlist_mode {
                // "+ New playlist" 項目
                let (style, prefix, bg) = if is_selected && is_focused {
                    (Style::default().fg(ACCENT_GREEN).bg(BG_SELECTED), "▎", BG_SELECTED)
                } else {
                    (Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::DIM), " ", Color::Reset)
                };
                let text = "+ New playlist";
                let remaining = row_width.saturating_sub(1 + text.width());
                let line = Paragraph::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(accent_color(app)).bg(bg)),
                    Span::styled(text, style),
                    Span::styled(" ".repeat(remaining), Style::default().bg(bg)),
                ]));
                frame.render_widget(line, line_area);
            }
        }
    }
}

fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::Content;
    let border_color = if is_focused { border_focus_color(app) } else { BORDER_DIM };

    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(card, area);

    let inner = inner_area(area, 2, 1);

    // 詳細モード判定
    let is_album_detail = !app.search_mode && !app.content_title.is_empty() && !app.is_playlist_detail;
    let is_playlist_detail = !app.search_mode && app.is_playlist_detail;

    // Title
    let title_area = Rect { height: 1, ..inner };
    let max_title_width = inner.width as usize - 2;

    if app.search_mode {
        let title_text = format!("{} results", app.search_results.len());
        let title = Paragraph::new(Line::from(vec![
            Span::styled(truncate(&title_text, max_title_width), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]));
        frame.render_widget(title, title_area);
    } else if is_album_detail {
        // アルバム詳細: "Album - Artist Year" の形式をパースして別スタイルで表示
        let total_time = calculate_total_time(&app.content_items);
        let time_suffix = format!(" [{}]", total_time);
        let parts: Vec<&str> = app.content_title.splitn(2, " - ").collect();
        if parts.len() == 2 {
            let album = parts[0];
            let artist_year = parts[1];
            let separator = " - ";
            let available = max_title_width.saturating_sub(time_suffix.len());
            let album_max = available.saturating_sub(separator.len() + artist_year.width()).min(available * 50 / 100);
            let artist_max = available.saturating_sub(album_max + separator.len());

            let title = Paragraph::new(Line::from(vec![
                Span::styled(truncate(album, album_max), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(separator, Style::default().fg(TEXT_DIM)),
                Span::styled(truncate(artist_year, artist_max), Style::default().fg(TEXT_DIM)),
                Span::styled(&time_suffix, Style::default().fg(TEXT_DIM)),
            ]));
            frame.render_widget(title, title_area);
        } else {
            let title = Paragraph::new(Line::from(vec![
                Span::styled(truncate(&app.content_title, max_title_width.saturating_sub(time_suffix.len())), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(&time_suffix, Style::default().fg(TEXT_DIM)),
            ]));
            frame.render_widget(title, title_area);
        }
    } else if is_playlist_detail {
        // プレイリスト詳細: プレイリスト名 + 合計時間を表示
        let total_time = calculate_total_time(&app.content_items);
        let time_suffix = format!(" [{}]", total_time);
        let title = Paragraph::new(Line::from(vec![
            Span::styled(truncate(&app.content_title, max_title_width.saturating_sub(time_suffix.len())), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(&time_suffix, Style::default().fg(TEXT_DIM)),
        ]));
        frame.render_widget(title, title_area);
    } else {
        let title_text = if !app.content_title.is_empty() {
            app.content_title.clone()
        } else {
            "Content".to_string()
        };
        let title = Paragraph::new(Line::from(vec![
            Span::styled(truncate(&title_text, max_title_width), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]));
        frame.render_widget(title, title_area);
    }

    // Content list
    let items = if app.search_mode { &app.search_results } else { &app.content_items };
    let list_area = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(2),
        ..inner
    };

    let visible_count = list_area.height as usize;

    let is_loading = app.content_loading;

    if is_loading {
        let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let spinner_char = spinner_frames[app.spinner_frame];
        let loading = Paragraph::new(format!("{} Loading...", spinner_char))
            .style(Style::default().fg(accent_color(app)));
        frame.render_widget(loading, list_area);
    } else if items.is_empty() {
        let empty_msg = if app.search_mode {
            "No results found"
        } else {
            "No items"
        };
        let empty = Paragraph::new(empty_msg)
            .style(Style::default().fg(TEXT_DIM));
        frame.render_widget(empty, list_area);
    } else if app.search_mode {
        // 検索モード: テーブル形式で表示
        let total_width = list_area.width as usize;

        // 列幅の計算 (#, Name, Artist, Album, Time, Year, Plays)
        // プレフィックス用に1を引く
        let available = total_width.saturating_sub(1);
        let col_track = 4;
        let col_time = 6;
        let col_year = 5;
        let col_plays = 6;
        let track_name_gap = 2;  // # と Name の間隔
        let fixed_cols = col_track + track_name_gap + col_time + col_year + col_plays;
        let flex_total = available.saturating_sub(fixed_cols);
        let col_name = flex_total * 30 / 100;
        let col_artist = flex_total * 30 / 100;
        let col_album = flex_total.saturating_sub(col_name + col_artist);

        // ヘッダー行
        let col_gap = 2;
        let header_area = Rect { height: 1, ..list_area };
        let header = Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(pad_right("#", col_track), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_left("Name", col_name), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(pad_left("Artist", col_artist), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(pad_left("Album", col_album), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(pad_right("Time", col_time), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(pad_right("Year", col_year), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(pad_right("Plays", col_plays), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]));
        frame.render_widget(header, header_area);

        // 罫線
        let separator_area = Rect { y: list_area.y + 1, height: 1, ..list_area };
        let separator = Paragraph::new("─".repeat(total_width))
            .style(Style::default().fg(BORDER_DIM));
        frame.render_widget(separator, separator_area);

        // データ行
        let data_area = Rect {
            y: list_area.y + 2,
            height: list_area.height.saturating_sub(2),
            ..list_area
        };
        let data_visible = data_area.height as usize;

        for (i, item) in items.iter().enumerate().skip(app.content_scroll).take(data_visible) {
            let y = data_area.y + (i - app.content_scroll) as u16;
            if y >= data_area.y + data_area.height {
                break;
            }

            let line_area = Rect { x: data_area.x, y, width: data_area.width, height: 1 };
            let is_selected = i == app.content_selected;

            let (name_style, sub_style, prefix, bg) = if is_selected && is_focused {
                (Style::default().fg(TEXT_PRIMARY).bg(BG_SELECTED),
                 Style::default().fg(TEXT_SECONDARY).bg(BG_SELECTED),
                 "▎", BG_SELECTED)
            } else if is_selected {
                (Style::default().fg(TEXT_PRIMARY), Style::default().fg(TEXT_DIM), " ", Color::Reset)
            } else {
                (Style::default().fg(TEXT_SECONDARY), Style::default().fg(TEXT_DIM), " ", Color::Reset)
            };

            let seq_num = (i + 1).to_string();  // 通し番号 (1-indexed)
            let year_str = if item.year > 0 { item.year.to_string() } else { String::new() };
            let plays_str = if item.played_count > 0 { item.played_count.to_string() } else { String::new() };

            let line = Paragraph::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(accent_color(app)).bg(bg)),
                Span::styled(pad_right(&seq_num, col_track), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_left(&truncate(&item.name, col_name.saturating_sub(1)), col_name), name_style),
                Span::styled(pad_left(&truncate(&item.artist, col_artist.saturating_sub(1)), col_artist), sub_style),
                Span::styled(pad_left(&truncate(&item.album, col_album.saturating_sub(1)), col_album), sub_style),
                Span::styled(pad_right(&item.time, col_time), sub_style),
                Span::styled(pad_right(&year_str, col_year), sub_style),
                Span::styled(pad_right(&plays_str, col_plays), sub_style),
            ]));
            frame.render_widget(line, line_area);
        }
    } else if is_album_detail {
        // アルバム詳細モード: テーブル形式で表示 (#, Name, Time, Plays)
        let total_width = list_area.width as usize;

        // 列幅の計算
        let available = total_width.saturating_sub(1); // プレフィックス用
        let col_track = 4;   // #
        let col_gap = 2;     // 列間の間隔
        let col_time = 5;    // Time
        let col_plays = 5;   // Plays
        // 間隔: # - Name - Time - Plays (3つの間隔)
        let fixed_cols = col_track + col_time + col_plays + (col_gap * 3);
        let col_name = available.saturating_sub(fixed_cols); // Name gets the rest

        // ヘッダー行
        let header_area = Rect { height: 1, ..list_area };
        let header = Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(pad_right("#", col_track), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_left("Name", col_name), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_right("Time", col_time), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_right("Plays", col_plays), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]));
        frame.render_widget(header, header_area);

        // 罫線
        let separator_area = Rect { y: list_area.y + 1, height: 1, ..list_area };
        let separator = Paragraph::new("─".repeat(total_width))
            .style(Style::default().fg(BORDER_DIM));
        frame.render_widget(separator, separator_area);

        // データ行
        let data_area = Rect {
            y: list_area.y + 2,
            height: list_area.height.saturating_sub(2),
            ..list_area
        };
        let data_visible = data_area.height as usize;

        for (i, item) in items.iter().enumerate().skip(app.content_scroll).take(data_visible) {
            let y = data_area.y + (i - app.content_scroll) as u16;
            if y >= data_area.y + data_area.height {
                break;
            }

            let line_area = Rect { x: data_area.x, y, width: data_area.width, height: 1 };
            let is_selected = i == app.content_selected;

            let (name_style, sub_style, prefix, bg) = if is_selected && is_focused {
                (Style::default().fg(TEXT_PRIMARY).bg(BG_SELECTED),
                 Style::default().fg(TEXT_SECONDARY).bg(BG_SELECTED),
                 "▎", BG_SELECTED)
            } else if is_selected {
                (Style::default().fg(TEXT_PRIMARY), Style::default().fg(TEXT_DIM), " ", Color::Reset)
            } else {
                (Style::default().fg(TEXT_SECONDARY), Style::default().fg(TEXT_DIM), " ", Color::Reset)
            };

            let track_str = if item.track_number > 0 { item.track_number.to_string() } else { String::new() };
            let plays_str = if item.played_count > 0 { item.played_count.to_string() } else { String::new() };

            let line = Paragraph::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(accent_color(app)).bg(bg)),
                Span::styled(pad_right(&track_str, col_track), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_left(&truncate(&item.name, col_name.saturating_sub(1)), col_name), name_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_right(&item.time, col_time), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_right(&plays_str, col_plays), sub_style),
            ]));
            frame.render_widget(line, line_area);
        }
    } else if is_playlist_detail {
        // プレイリスト詳細モード: テーブル形式で表示 (#, Name, Artist, Album, Year, Time, Plays)
        let total_width = list_area.width as usize;

        // 列幅の計算
        let available = total_width.saturating_sub(1); // プレフィックス用
        let col_gap = 2;     // 列間の間隔
        let col_track = 4;   // #
        let col_year = 5;    // Year
        let col_time = 5;    // Time
        let col_plays = 5;   // Plays
        // 間隔: # - Name - Artist - Album - Year - Time - Plays (6つの間隔)
        let fixed_cols = col_track + col_year + col_time + col_plays + (col_gap * 6);
        let flex_total = available.saturating_sub(fixed_cols);
        let col_name = flex_total * 35 / 100;
        let col_artist = flex_total * 30 / 100;
        let col_album = flex_total.saturating_sub(col_name + col_artist);

        // ヘッダー行
        let header_area = Rect { height: 1, ..list_area };
        let header = Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(pad_right("#", col_track), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_left("Name", col_name), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_left("Artist", col_artist), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_left("Album", col_album), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_right("Year", col_year), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_right("Time", col_time), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(col_gap), Style::default()),
            Span::styled(pad_right("Plays", col_plays), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]));
        frame.render_widget(header, header_area);

        // 罫線
        let separator_area = Rect { y: list_area.y + 1, height: 1, ..list_area };
        let separator = Paragraph::new("─".repeat(total_width))
            .style(Style::default().fg(BORDER_DIM));
        frame.render_widget(separator, separator_area);

        // データ行
        let data_area = Rect {
            y: list_area.y + 2,
            height: list_area.height.saturating_sub(2),
            ..list_area
        };
        let data_visible = data_area.height as usize;

        for (i, item) in items.iter().enumerate().skip(app.content_scroll).take(data_visible) {
            let y = data_area.y + (i - app.content_scroll) as u16;
            if y >= data_area.y + data_area.height {
                break;
            }

            let line_area = Rect { x: data_area.x, y, width: data_area.width, height: 1 };
            let is_selected = i == app.content_selected;

            let (name_style, sub_style, prefix, bg) = if is_selected && is_focused {
                (Style::default().fg(TEXT_PRIMARY).bg(BG_SELECTED),
                 Style::default().fg(TEXT_SECONDARY).bg(BG_SELECTED),
                 "▎", BG_SELECTED)
            } else if is_selected {
                (Style::default().fg(TEXT_PRIMARY), Style::default().fg(TEXT_DIM), " ", Color::Reset)
            } else {
                (Style::default().fg(TEXT_SECONDARY), Style::default().fg(TEXT_DIM), " ", Color::Reset)
            };

            let track_num = (i + 1).to_string();  // 1-indexed track number
            let display_name = if item.name.is_empty() { "(No title)" } else { &item.name };
            let display_artist = if item.artist.is_empty() { "(No artist)" } else { &item.artist };
            let display_album = if item.album.is_empty() { "(No album)" } else { &item.album };
            let year_str = if item.year > 0 { item.year.to_string() } else { String::new() };
            let plays_str = if item.played_count > 0 { item.played_count.to_string() } else { String::new() };

            let line = Paragraph::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(accent_color(app)).bg(bg)),
                Span::styled(pad_right(&track_num, col_track), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_left(&truncate(display_name, col_name.saturating_sub(1)), col_name), name_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_left(&truncate(display_artist, col_artist.saturating_sub(1)), col_artist), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_left(&truncate(display_album, col_album.saturating_sub(1)), col_album), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_right(&year_str, col_year), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_right(&item.time, col_time), sub_style),
                Span::styled(" ".repeat(col_gap), Style::default().bg(bg)),
                Span::styled(pad_right(&plays_str, col_plays), sub_style),
            ]));
            frame.render_widget(line, line_area);
        }
    } else {
        // 通常モード: リスト形式
        for (i, item) in items.iter().enumerate().skip(app.content_scroll).take(visible_count) {
            let y = list_area.y + (i - app.content_scroll) as u16;
            if y >= list_area.y + list_area.height {
                break;
            }

            let line_area = Rect {
                x: list_area.x,
                y,
                width: list_area.width,
                height: 1,
            };

            let is_selected = i == app.content_selected;
            let (prefix, name_style, sub_style, bg) = if is_selected && is_focused {
                ("▎ ", Style::default().fg(TEXT_PRIMARY).bg(BG_SELECTED),
                 Style::default().fg(TEXT_SECONDARY).bg(BG_SELECTED), BG_SELECTED)
            } else if is_selected {
                ("  ", Style::default().fg(TEXT_PRIMARY), Style::default().fg(TEXT_DIM), Color::Reset)
            } else {
                ("  ", Style::default().fg(TEXT_SECONDARY), Style::default().fg(TEXT_DIM), Color::Reset)
            };

            let total_width = list_area.width as usize;
            let name_max = total_width * 40 / 100;
            let artist_max = total_width * 30 / 100;
            let album_max = total_width * 25 / 100;

            let display_name = if item.name.is_empty() { "(No title)" } else { &item.name };
            let display_artist = if item.artist.is_empty() { "(No artist)" } else { &item.artist };
            let display_album = if item.album.is_empty() { "(No album)" } else { &item.album };

            let mut spans = vec![
                Span::styled(prefix, Style::default().fg(accent_color(app)).bg(bg)),
                Span::styled(truncate(display_name, name_max), name_style),
            ];

            spans.push(Span::styled(" - ", Style::default().fg(TEXT_DIM).bg(bg)));
            spans.push(Span::styled(truncate(display_artist, artist_max), sub_style));

            spans.push(Span::styled(" - ", Style::default().fg(TEXT_DIM).bg(bg)));
            spans.push(Span::styled(truncate(display_album, album_max), sub_style));

            // 行末まで背景を埋める
            let content_len = prefix.width() + truncate(display_name, name_max).width() + 3 +
                truncate(display_artist, artist_max).width() + 3 + truncate(display_album, album_max).width();
            let remaining = total_width.saturating_sub(content_len);
            spans.push(Span::styled(" ".repeat(remaining), Style::default().bg(bg)));

            let line = Paragraph::new(Line::from(spans));
            frame.render_widget(line, line_area);
        }
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let key_style = Style::default().fg(accent_color(app));
    let sep_style = Style::default().fg(TEXT_DIM);

    let commands: Vec<(&str, &str)> = if app.new_playlist_input_mode {
        // 新規プレイリスト名入力モード
        vec![
            ("Return", "create"),
            ("Esc", "cancel"),
        ]
    } else if app.add_to_playlist_mode {
        // プレイリスト追加モード
        vec![
            ("Return", "add"),
            ("j/k/g/G", "nav"),
            ("Esc", "cancel"),
        ]
    } else if app.search_mode {
        if app.focus == Focus::Content {
            // 検索結果にフォーカス中
            vec![
                ("Return", "play"),
                ("j/k/g/G", "nav"),
                ("h", "back"),
                ("l", "album"),
                ("a", "add"),
                ("Esc", "cancel"),
            ]
        } else {
            // Searchカードにフォーカス中
            vec![
                ("Return", "search"),
                ("Esc", "cancel"),
            ]
        }
    } else if app.focus == Focus::Content && app.is_playlist_detail {
        // プレイリスト詳細にフォーカス中
        vec![
            ("Space", "play/pause"),
            ("Return", "play"),
            ("j/k/g/G", "nav"),
            ("h", "back"),
            ("l", "album"),
            ("a", "add"),
            ("d", "remove"),
            ("/", "search"),
            ("q", "quit"),
        ]
    } else if app.focus == Focus::Content {
        // アルバム詳細にフォーカス中
        vec![
            ("Space", "play/pause"),
            ("Return", "play"),
            ("n/p", "track"),
            ("←→", "seek"),
            ("j/k/g/G", "nav"),
            ("h/l", "column"),
            ("a", "add"),
            ("/", "search"),
            ("q", "quit"),
        ]
    } else if app.focus == Focus::Playlists {
        // Playlistsカードにフォーカス中
        vec![
            ("Space", "play/pause"),
            ("Return", "select"),
            ("j/k/g/G", "nav"),
            ("h/l", "column"),
            ("d", "delete"),
            ("Tab", "pane"),
            ("/", "search"),
            ("q", "quit"),
        ]
    } else {
        vec![
            ("Space", "play/pause"),
            ("Return", "select"),
            ("n/p", "track"),
            ("←→", "seek"),
            ("s", "shuffle"),
            ("r", "repeat"),
            ("c", "color"),
            ("R", "refresh"),
            ("j/k/g/G", "nav"),
            ("h/l", "column"),
            ("Tab", "pane"),
            ("/", "search"),
            ("q", "quit"),
        ]
    };

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in commands.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", sep_style));
        }
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::styled(format!(" {}", desc), sep_style));
    }

    let paragraph = Paragraph::new(Line::from(spans)).wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn inner_area(area: Rect, h_padding: u16, v_padding: u16) -> Rect {
    Rect {
        x: area.x + h_padding,
        y: area.y + v_padding,
        width: area.width.saturating_sub(h_padding * 2),
        height: area.height.saturating_sub(v_padding * 2),
    }
}

/// アイテムリストの合計時間を計算
fn calculate_total_time(items: &[ListItem]) -> String {
    let mut total_seconds = 0u32;
    for item in items {
        // "M:SS" or "MM:SS" or "H:MM:SS" format
        let parts: Vec<&str> = item.time.split(':').collect();
        if parts.len() == 2 {
            // M:SS or MM:SS
            if let (Ok(m), Ok(s)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                total_seconds += m * 60 + s;
            }
        } else if parts.len() == 3 {
            // H:MM:SS
            if let (Ok(h), Ok(m), Ok(s)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                total_seconds += h * 3600 + m * 60 + s;
            }
        }
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

/// 文字列を指定幅で切り詰める（全角文字対応）
fn truncate(s: &str, max_width: usize) -> String {
    let width = s.width();
    if width <= max_width {
        return s.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;
    let target_width = max_width.saturating_sub(2); // "…" 用に2幅確保

    for c in s.chars() {
        let char_width = c.to_string().width();
        if current_width + char_width > target_width {
            break;
        }
        result.push(c);
        current_width += char_width;
    }
    result.push('…');
    result
}

/// 文字列を指定幅にパディング（全角文字対応、左寄せ）
fn pad_left(s: &str, width: usize) -> String {
    let current_width = s.width();
    if current_width >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - current_width))
    }
}

/// 文字列を指定幅にパディング（全角文字対応、右寄せ）
fn pad_right(s: &str, width: usize) -> String {
    let current_width = s.width();
    if current_width >= width {
        s.to_string()
    } else {
        format!("{}{}", " ".repeat(width - current_width), s)
    }
}
