mod accessibility;
mod app;
mod cache;
mod music;
mod ui;

use std::env;
use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, MouseEventKind, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Focus};

const BUILD_VERSION: &str = env!("BUILD_VERSION");
const GIT_COMMIT: &str = env!("GIT_COMMIT_HASH");

fn main() -> Result<()> {
    // Handle --version flag
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("mmt v{} build {}", BUILD_VERSION, GIT_COMMIT);
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let position_tick_rate = Duration::from_millis(200);
    let full_tick_rate = Duration::from_millis(1000);  // 1秒ごとに曲情報を更新
    let mut last_position_tick = Instant::now();
    let mut last_full_tick = Instant::now();

    app.refresh_full();

    loop {
        app.poll_responses();
        app.poll_cache_responses();
        app.poll_playlist_responses();
        app.poll_playlist_refresh();
        app.update_level_meter();
        app.update_spinner();
        app.update_visible_heights(terminal.size()?.height);
        terminal.draw(|f| ui::draw(f, &app))?;

        let timeout = Duration::from_millis(50);

        if event::poll(timeout)? {
            let terminal_height = terminal.size()?.height;
            match event::read()? {
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                            app.handle_mouse_click(mouse.column, mouse.row, terminal_height);
                        }
                        MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                            app.handle_mouse_drag(mouse.column, mouse.row, terminal_height);
                        }
                        MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                            app.handle_mouse_up();
                        }
                        _ => {}
                    }
                }
                Event::Key(key) => {
                // ウェルカム画面表示中
                if app.should_show_welcome() {
                    match key.code {
                        KeyCode::Char('c') => {
                            app.cycle_highlight_color();
                        }
                        _ => {
                            app.dismiss_welcome();
                        }
                    }
                    continue;
                }

                if !app.search_mode && !app.add_to_playlist_mode && !app.delete_confirm_mode {
                    app.message = None;
                }

                if app.delete_confirm_mode {
                    // 削除確認モード
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            app.confirm_delete();
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            app.cancel_delete();
                        }
                        _ => {}
                    }
                } else if app.new_playlist_input_mode {
                    // 新規プレイリスト名入力モード
                    match key.code {
                        KeyCode::Esc => {
                            app.cancel_add_to_playlist();
                        }
                        KeyCode::Enter => {
                            app.confirm_new_playlist();
                        }
                        KeyCode::Backspace => {
                            app.new_playlist_backspace();
                        }
                        KeyCode::Char(c) => {
                            app.new_playlist_input(c);
                        }
                        _ => {}
                    }
                } else if app.add_to_playlist_mode {
                    // プレイリスト追加モード
                    match key.code {
                        KeyCode::Esc => {
                            app.cancel_add_to_playlist();
                        }
                        KeyCode::Enter => {
                            app.confirm_add_to_playlist();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            // プレイリスト選択（+ New playlist を含む）
                            if app.playlists_selected > 0 {
                                app.playlists_selected -= 1;
                                if app.playlists_selected < app.playlists_scroll {
                                    app.playlists_scroll = app.playlists_selected;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let max_index = app.playlists_count_with_new() - 1;
                            if app.playlists_selected < max_index {
                                app.playlists_selected += 1;
                                if app.playlists_selected >= app.playlists_scroll + app.playlists_visible {
                                    app.playlists_scroll = app.playlists_selected.saturating_sub(app.playlists_visible - 1);
                                }
                            }
                        }
                        _ => {}
                    }
                } else if app.search_mode {
                    // 検索モード中のフォーカスによって動作を分岐
                    if app.focus == Focus::Content {
                        // 検索結果にフォーカス中: j/k/h でナビゲーション
                        match key.code {
                            KeyCode::Esc => {
                                app.cancel_search();
                            }
                            KeyCode::Enter => {
                                app.play_selected();
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.content_up();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.content_down();
                            }
                            KeyCode::Char('g') => {
                                app.content_top();
                            }
                            KeyCode::Char('G') => {
                                app.content_bottom();
                            }
                            KeyCode::Char('h') => {
                                // Searchカードに戻る
                                app.focus = Focus::Search;
                            }
                            KeyCode::Char('l') => {
                                // 選択中の曲のアルバム全曲を表示
                                if let Some(item) = app.search_results.get(app.content_selected) {
                                    let album_name = item.album.clone();
                                    app.show_album_tracks(&album_name);
                                    app.search_mode = false;
                                }
                            }
                            KeyCode::Char('a') => {
                                app.start_add_to_playlist();
                            }
                            _ => {}
                        }
                    } else {
                        // Searchカードにフォーカス中: 文字入力
                        match key.code {
                            KeyCode::Esc => {
                                app.cancel_search();
                            }
                            KeyCode::Enter => {
                                app.confirm_search();
                            }
                            KeyCode::Backspace => {
                                app.search_backspace();
                            }
                            KeyCode::Char(c) => {
                                app.search_input(c);
                            }
                            KeyCode::Up => {
                                app.content_up();
                            }
                            KeyCode::Down => {
                                app.content_down();
                            }
                            _ => {}
                        }
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('/') => {
                            app.start_search();
                        }
                        KeyCode::Tab => {
                            app.focus_next();
                        }
                        KeyCode::Char(' ') => {
                            app.play_pause();
                        }
                        KeyCode::Char('n') => {
                            app.next_track();
                        }
                        KeyCode::Char('p') => {
                            app.previous_track();
                        }
                        KeyCode::Char('s') => {
                            app.toggle_shuffle();
                        }
                        KeyCode::Char('r') => {
                            app.cycle_repeat();
                        }
                        KeyCode::Char('c') => {
                            app.cycle_highlight_color();
                        }
                        KeyCode::Char('R') => {
                            app.refresh_current_playlist();
                        }
                        KeyCode::Left => {
                            app.seek_backward();
                        }
                        KeyCode::Right => {
                            app.seek_forward();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            match app.focus {
                                Focus::RecentlyAdded => app.recently_added_up(),
                                Focus::Playlists => app.playlists_up(),
                                Focus::Content => app.content_up(),
                                _ => {}
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            match app.focus {
                                Focus::RecentlyAdded => app.recently_added_down(),
                                Focus::Playlists => app.playlists_down(),
                                Focus::Content => app.content_down(),
                                _ => {}
                            }
                        }
                        KeyCode::Char('g') => {
                            match app.focus {
                                Focus::RecentlyAdded => app.recently_added_top(),
                                Focus::Playlists => app.playlists_top(),
                                Focus::Content => app.content_top(),
                                _ => {}
                            }
                        }
                        KeyCode::Char('G') => {
                            match app.focus {
                                Focus::RecentlyAdded => app.recently_added_bottom(),
                                Focus::Playlists => app.playlists_bottom(),
                                Focus::Content => app.content_bottom(),
                                _ => {}
                            }
                        }
                        KeyCode::Char('h') => {
                            app.focus_left();
                        }
                        KeyCode::Char('l') => {
                            app.focus_right();
                        }
                        KeyCode::Char('a') => {
                            app.start_add_to_playlist();
                        }
                        KeyCode::Char('d') => {
                            // Playlistsカードでd: プレイリスト削除
                            // プレイリスト詳細でd: 曲を削除
                            if app.focus == Focus::Playlists {
                                app.start_delete_playlist();
                            } else if app.focus == Focus::Content && app.is_playlist_detail {
                                app.start_delete_track_from_playlist();
                            }
                        }
                        KeyCode::Enter => {
                            match app.focus {
                                Focus::RecentlyAdded => {
                                    // アルバムを再生せず、詳細paneにフォーカス移動
                                    app.focus = Focus::Content;
                                    app.content_selected = 0;
                                    app.content_scroll = 0;
                                }
                                Focus::Playlists => {
                                    // プレイリストを再生せず、詳細paneにフォーカス移動
                                    app.focus = Focus::Content;
                                    app.content_selected = 0;
                                    app.content_scroll = 0;
                                }
                                Focus::Content => app.play_selected(),
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                }
                _ => {}
            }
        }

        if last_position_tick.elapsed() >= position_tick_rate {
            app.refresh_position();
            last_position_tick = Instant::now();
        }

        if last_full_tick.elapsed() >= full_tick_rate {
            app.refresh_full();
            last_full_tick = Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
