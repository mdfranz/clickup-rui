use crate::app::Commands;
use crate::clickup::api::ClickUpApi;
use crate::util::env::set_menu_mode;
use crate::util::errors::Result;

fn needs_terminal_pause(cmd: &Commands) -> bool {
    matches!(cmd, Commands::Show | Commands::Setup)
}
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

pub async fn run_menu<A: ClickUpApi + Clone + 'static>(api: &A) -> Result<()> {
    let menu_options = vec![
        (
            "Browse Interactive View",
            Commands::Browse {
                all: false,
                team: false,
                mine: true,
            },
        ),
        ("Team Workload View", Commands::Workload),
        (
            "Track User Activity Logs",
            Commands::Track {
                user_id: None,
                summarize: false,
                raw: false,
                csv: false,
                json: false,
                markdown: false,
            },
        ),
        ("Create New Task", Commands::New),
        (
            "Log Daily Standup Updates",
            Commands::Standup {
                all: false,
                mine: true,
            },
        ),
        ("Interactive setup", Commands::Setup),
        ("Show active config", Commands::Show),
    ];

    let mut guard = crate::ui::terminal::TerminalGuard::create()?;

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    loop {
        guard.inner().draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);

            // Dynamic layout calculations based on terminal size to prevent clipping/overflows
            let show_banner = size.width >= 65 && size.height >= 20;
            let margin_val = if size.height < 14 || size.width < 65 {
                0
            } else if size.height < 23 || size.width < 80 {
                1
            } else {
                2
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(margin_val)
                .constraints(
                    if show_banner {
                        [
                            Constraint::Length(7), // ASCII Art App Banner
                            Constraint::Min(5),    // Menu List
                            Constraint::Length(2), // Footer Help (with top border)
                        ]
                    } else {
                        [
                            Constraint::Length(2), // Slim App Banner (Title + spacer)
                            Constraint::Min(3),    // Menu List
                            Constraint::Length(1), // Footer Help (plain)
                        ]
                    }
                    .as_ref(),
                )
                .split(size);

            let title_span = Span::styled(
                "⚡ CLICKUP CLI/TUI MENU ⚡",
                crate::ui::styles::style_title(),
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![title_span])).alignment(Alignment::Center),
                chunks[0],
            );

            let items: Vec<ListItem> = menu_options
                .iter()
                .map(|(label, _)| {
                    ListItem::new(format!("  •  {}", label)).style(
                        Style::default()
                            .fg(crate::ui::styles::COLOR_FG)
                            .bg(crate::ui::styles::COLOR_BG),
                    )
                })
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

            let help_line = Line::from(vec![
                Span::styled(
                    " ↑/↓ (j/k)",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(crate::ui::styles::COLOR_PRIMARY),
                ),
                Span::styled(
                    " Navigate  |  ",
                    Style::default().fg(crate::ui::styles::COLOR_FG),
                ),
                Span::styled(
                    "Enter",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(crate::ui::styles::COLOR_PRIMARY),
                ),
                Span::styled(
                    " Execute  |  ",
                    Style::default().fg(crate::ui::styles::COLOR_FG),
                ),
                Span::styled(
                    "q / Esc",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(crate::ui::styles::COLOR_PRIMARY),
                ),
                Span::styled(" Quit", Style::default().fg(crate::ui::styles::COLOR_FG)),
            ]);

            let help = if show_banner {
                Paragraph::new(help_line)
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::TOP)
                            .border_style(Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                    )
            } else {
                Paragraph::new(help_line).alignment(Alignment::Center)
            };
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
                            drop(guard);

                            // 2. Set environment menu flag
                            set_menu_mode(true);

                            // 3. Determine if this command produces terminal output requiring a pause
                            let show_pause = needs_terminal_pause(&command_to_run);

                            // 4. Execute command through router
                            let route_res =
                                Box::pin(crate::cmd::route_command(api, command_to_run)).await;
                            let had_error = if let Err(e) = route_res {
                                println!("\nError executing command: {}\n", e);
                                true
                            } else {
                                false
                            };

                            // 5. Unset menu flag
                            set_menu_mode(false);

                            // 6. Pause only when terminal output was produced or an error occurred
                            if show_pause || had_error {
                                println!(
                                    "\n[Press any key to return to menu, or 'q' / Ctrl+C to exit...]"
                                );
                                crossterm::terminal::enable_raw_mode()?;
                                let mut quit = false;
                                loop {
                                    if event::poll(std::time::Duration::from_millis(100))? {
                                        if let Event::Key(k) = event::read()? {
                                            if k.kind == KeyEventKind::Press {
                                                if k.code == KeyCode::Char('q')
                                                    || (k.code == KeyCode::Char('c')
                                                        && k.modifiers
                                                            .contains(event::KeyModifiers::CONTROL))
                                                {
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
                                // raw mode still active here; falls through to re-enter alternate screen
                            } else {
                                // TUI command with no terminal output — re-enable raw mode directly
                                crossterm::terminal::enable_raw_mode()?;
                            }

                            // 7. Re-enter alternate screen and clear terminal
                            guard = crate::ui::terminal::TerminalGuard::create()?;
                            guard.inner().clear()?;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
