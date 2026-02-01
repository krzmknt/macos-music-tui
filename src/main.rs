mod accessibility;
mod app;
mod cache;
mod music;
mod ui;

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

fn main() -> Result<()> {
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
        app.update_level_meter();
        app.update_spinner();
        app.update_visible_heights(terminal.size()?.height);
        terminal.draw(|f| ui::draw(f, &app))?;

        let timeout = Duration::from_millis(50);

        if event::poll(timeout)? {
            let terminal_height = terminal.size()?.height;
            match event::read()? {
                Event::Mouse(mouse) => {
                    if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
                        app.handle_mouse_click(mouse.column, mouse.row, terminal_height);
                    }
                }
                Event::Key(key) => {
                if !app.search_mode {
                    app.message = None;
                }

                if app.search_mode {
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
