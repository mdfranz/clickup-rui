use crate::app::Commands;
use crate::clickup::api::ClickUpApi;
use crate::util::env::set_menu_mode;
use crate::util::errors::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::io;

pub async fn run_menu<A: ClickUpApi>(api: &A) -> Result<()> {
    let menu_options = vec![
        ("Tasks List View", Commands::Tasks {
            all: false,
            detailed: false,
            summarize: false,
            team: false,
            mine: true,
            id: false,
        }),
        ("Browse Interactive View", Commands::Browse {
            all: false,
            team: false,
            mine: true,
        }),
        ("Create New Task", Commands::New),
        ("Log Daily Standup Updates", Commands::Standup {
            all: false,
            mine: true,
        }),
        ("Track User Activity Logs", Commands::Track {
            user_id: None,
            summarize: false,
            raw: false,
        }),
        ("View Team Status Summary", Commands::TeamStatus {
            days: 7,
            summarize: true,
            raw: false,
        }),
        ("Interactive setup", Commands::Setup),
        ("Show active config", Commands::Show),
    ];

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    loop {
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(4), // App Banner
                        Constraint::Min(5),    // Menu List
                        Constraint::Length(3), // Footer Help
                    ]
                    .as_ref(),
                )
                .split(size);
j
            let banner = "   __   __   _              _   _      _____ _   _ _____
  / _| / /  (_) ___ _ __   | | | |    |_   _| | | |_   _|
 | |  / /   | |/ __| '_ \\  | | | |______|_| | | | | |_| |
 | |_/ /____| | (__| |_) | | |_| |______| | | |_| |  | |
  \\__\\_____/|_|\\___| .__/   \\___/       |_|  \\___/   |_|
                   |_|                                    ";

            f.render_widget(
                Paragraph::new(banner)
                    .style(crate::ui::styles::style_title()),
                chunks[0],
            );

            let items: Vec<ListItem> = menu_options
                .iter()
                .map(|(label, _)| ListItem::new(format!("  •  {}", label)))
                .collect();

            let menu_list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Navigation Menu ")
                        .border_style(crate::ui::styles::style_border_active()),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(menu_list, chunks[1], &mut list_state);

            let help = Paragraph::new("Arrow Up/Down or j/k: navigate | Enter: execute subcommand | q: quit")
                .block(Block::default().borders(Borders::TOP))
                .style(ratatui::style::Style::default().fg(crate::ui::styles::COLOR_MUTED));
            f.render_widget(help, chunks[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Up | KeyCode::Char('k') => {
                            let i = list_state.selected().unwrap_or(0);
                            if i > 0 {
                                list_state.select(Some(i - 1));
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let i = list_state.selected().unwrap_or(0);
                            if i + 1 < menu_options.len() {
                                list_state.select(Some(i + 1));
                            }
                        }
                        KeyCode::Enter => {
                            let idx = list_state.selected().unwrap_or(0);
                            let command_to_run = menu_options[idx].1.clone();

                            // 1. Leave TUI screen temporarily
                            crossterm::execute!(
                                terminal.backend_mut(),
                                crossterm::terminal::LeaveAlternateScreen,
                                crossterm::event::DisableMouseCapture
                            )?;
                            crossterm::terminal::disable_raw_mode()?;

                            // 2. Set environment menu flag
                            set_menu_mode(true);

                            // 3. Execute command through router
                            let route_res = Box::pin(crate::cmd::route_command(api, command_to_run)).await;
                            if let Err(e) = route_res {
                                println!("\nError executing command: {}\n", e);
                            }

                            // 4. Unset menu flag
                            set_menu_mode(false);

                            // 5. Pause screen
                            println!("\n[Press any key to return to menu, or 'q' / Ctrl+C to exit...]");
                            crossterm::terminal::enable_raw_mode()?;
                            let mut quit = false;
                            loop {
                                if event::poll(std::time::Duration::from_millis(100))? {
                                    if let Event::Key(k) = event::read()? {
                                        if k.kind == KeyEventKind::Press {
                                            if k.code == KeyCode::Char('q') || (k.code == KeyCode::Char('c') && k.modifiers.contains(event::KeyModifiers::CONTROL)) {
                                                quit = true;
                                            }
                                            break;
                                        }
                                    }
                                }
                            }

                            if quit {
                                crossterm::terminal::disable_raw_mode()?;
                                return Ok(());
                            }

                            // 6. Re-enter alternate screen and clear terminal
                            crossterm::execute!(
                                io::stdout(),
                                crossterm::terminal::EnterAlternateScreen,
                                crossterm::event::EnableMouseCapture
                            )?;
                            terminal.clear()?;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
