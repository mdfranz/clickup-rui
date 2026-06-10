use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Comment, Status, Task};
use crate::config::Config;
use crate::util::errors::Result;
use crate::util::filter::should_include_task;
use crate::util::format::{format_comment_date, format_task_date};
use crate::util::sort::{sort_comments_by_date_desc, sort_tasks_by_updated_desc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List as RatatuiList, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Terminal;
use std::io;

#[derive(PartialEq, Eq)]
enum BrowseState {
    List,
    CommentEditor,
    StatusPicker,
}

#[derive(PartialEq, Eq)]
enum ActivePane {
    Left,
    Right,
}

pub async fn run_browse<A: ClickUpApi>(api: &A, all_flag: bool, mine_only: bool) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_browse_loop(api, &mut terminal, all_flag, mine_only).await;

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run_browse_loop<A: ClickUpApi>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    all_flag: bool,
    mine_only: bool,
) -> Result<()> {
    let cfg = Config::load()?;
    let user = api.get_current_user().await?;

    // Load active tasks across folders
    let mut tasks = Vec::new();
    let mut task_list_map = std::collections::HashMap::new(); // task_id -> list_id

    for folder in &cfg.folders {
        if let Ok(lists) = api.get_lists(&folder.id).await {
            for list in lists {
                if let Ok(t_list) = api.get_tasks(&list.id, all_flag).await {
                    for task in t_list {
                        if should_include_task(&task, user.id, all_flag, mine_only) {
                            tasks.push(task.clone());
                            task_list_map.insert(task.id.clone(), list.id.clone());
                        }
                    }
                }
            }
        }
    }

    sort_tasks_by_updated_desc(&mut tasks);

    if tasks.is_empty() {
        terminal.draw(|f| {
            crate::ui::styles::render_background(f);
            f.render_widget(
                Paragraph::new(
                    "\n  No active tasks found matching the current criteria.\n\n  Press any key to return.",
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Browse Tasks ")
                        .border_style(crate::ui::styles::style_border_active()),
                )
                .style(
                    Style::default()
                        .fg(crate::ui::styles::COLOR_FG)
                        .bg(crate::ui::styles::COLOR_BG),
                ),
                f.area(),
            );
        })?;
        loop {
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        break;
                    }
                }
            }
        }
        return Ok(());
    }

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let mut state = BrowseState::List;
    let mut active_pane = ActivePane::Left;
    let mut right_scroll: u16 = 0;

    let mut cached_comments: std::collections::HashMap<String, Vec<Comment>> =
        std::collections::HashMap::new();
    let mut cached_task_details: std::collections::HashMap<String, Task> =
        std::collections::HashMap::new();

    let mut comment_buffer = String::new();

    let mut list_statuses: Vec<Status> = Vec::new();
    let mut statuses_state = ListState::default();

    loop {
        let current_task_idx = list_state.selected().unwrap_or(0);
        let current_task = tasks[current_task_idx].clone();

        let needs_detail = !cached_task_details.contains_key(&current_task.id);
        let needs_comments = !cached_comments.contains_key(&current_task.id);

        if needs_detail || needs_comments {
            terminal.draw(|f| {
                let size = f.area();
                crate::ui::styles::render_background(f);
                let main_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(3), Constraint::Length(2)].as_ref())
                    .split(size);

                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                    .split(main_layout[0]);

                let left_border_style = if active_pane == ActivePane::Left {
                    crate::ui::styles::style_border_active()
                } else {
                    crate::ui::styles::style_border_inactive()
                };

                let right_border_style = if active_pane == ActivePane::Right {
                    crate::ui::styles::style_border_active()
                } else {
                    crate::ui::styles::style_border_inactive()
                };

                // Left Pane: Tasks List (with the newly selected highlight)
                let items: Vec<ListItem> = tasks
                    .iter()
                    .map(|t| {
                        let status_color = crate::ui::styles::get_status_color(&t.status.status);
                        let date_str = format_task_date(&t.date_updated);
                        let date_display = if date_str.is_empty() {
                            "        ".to_string()
                        } else {
                            format!("[{}] ", date_str)
                        };
                        let date_span = Span::styled(
                            date_display,
                            Style::default().fg(crate::ui::styles::COLOR_MUTED)
                        );
                        let status_upper = t.status.status.to_uppercase();
                        let status_span = Span::styled(
                            format!("[{:<11}]", status_upper),
                            Style::default().fg(status_color).add_modifier(Modifier::BOLD)
                        );
                        let name_span = Span::styled(
                            format!(" {}", t.name),
                            Style::default().fg(crate::ui::styles::COLOR_FG)
                        );

                        ListItem::new(vec![
                            Line::from(vec![date_span, status_span, name_span]),
                            Line::from(""),
                        ])
                    })
                    .collect();

                let left_list = RatatuiList::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Tasks List ")
                            .border_style(left_border_style),
                    )
                    .highlight_style(crate::ui::styles::style_selected());

                f.render_stateful_widget(left_list, chunks[0], &mut list_state);

                // Right Pane: Loading Details & Comments
                let loading_lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("   ⏳ Loading details & comments...", Style::default().fg(crate::ui::styles::COLOR_WARN).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("   👉 ", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                        Span::styled(format!("\"{}\"", current_task.name), Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("   Please wait...", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                    ]),
                ];
                let right_pane = Paragraph::new(loading_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Loading Task ")
                            .border_style(right_border_style),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    .wrap(Wrap { trim: true });

                f.render_widget(right_pane, chunks[1]);

                // Help Bar
                let help_line = Line::from(vec![
                    Span::styled(" Tab", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" Switch Pane |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(" c", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" Add Comment |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(" s", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" Change Status |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(" n", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" New Task |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(" r", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" Reload |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(" ↑/↓ (j/k)", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" Scroll Focused Pane |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(" q", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    Span::styled(" Quit", Style::default().fg(crate::ui::styles::COLOR_FG)),
                ]);

                let help_bar = Paragraph::new(help_line).block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_style(Style::default().fg(crate::ui::styles::COLOR_MUTED))
                );
                f.render_widget(help_bar, main_layout[1]);
            })?;
        }

        // Ensure current task detail and comments are loaded
        if !cached_task_details.contains_key(&current_task.id) {
            if let Ok(detailed) = api.get_task_detail(&current_task.id).await {
                cached_task_details.insert(current_task.id.clone(), detailed);
            } else {
                cached_task_details.insert(current_task.id.clone(), current_task.clone());
            }
        }

        if !cached_comments.contains_key(&current_task.id) {
            if let Ok(mut comments) = api.get_task_comments(&current_task.id).await {
                sort_comments_by_date_desc(&mut comments);
                cached_comments.insert(current_task.id.clone(), comments);
            } else {
                cached_comments.insert(current_task.id.clone(), Vec::new());
            }
        }

        let detailed_task = cached_task_details.get(&current_task.id).unwrap();
        let comments = cached_comments.get(&current_task.id).unwrap();

        // Build Right Pane details vector of Line
        let assignees = detailed_task
            .assignees
            .iter()
            .map(|u| u.username.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let desc_text = detailed_task.text_content.as_deref().unwrap_or("No description");

        let mut detail_lines = Vec::new();

        // Metadata Fields
        let status_color = crate::ui::styles::get_status_color(&detailed_task.status.status);
        detail_lines.push(Line::from(vec![
            Span::styled("Status:    ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
            Span::styled(detailed_task.status.status.to_uppercase(), Style::default().add_modifier(Modifier::BOLD).fg(status_color)),
        ]));

        detail_lines.push(Line::from(vec![
            Span::styled("Assignees: ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
            Span::styled(assignees.clone(), Style::default().fg(crate::ui::styles::COLOR_FG)),
        ]));

        detail_lines.push(Line::from(vec![
            Span::styled("Creator:   ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
            Span::styled(&detailed_task.creator.username, Style::default().fg(crate::ui::styles::COLOR_FG)),
        ]));

        detail_lines.push(Line::from(""));

        // Description Section
        detail_lines.push(Line::from(vec![
            Span::styled("Description", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
        ]));
        detail_lines.push(Line::from(vec![
            Span::styled("───────────", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
        ]));

        for line in desc_text.lines() {
            detail_lines.push(Line::from(vec![
                Span::styled(line, Style::default().fg(crate::ui::styles::COLOR_FG)),
            ]));
        }

        detail_lines.push(Line::from(""));

        // Comments Section
        detail_lines.push(Line::from(vec![
            Span::styled("Comments", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
        ]));
        detail_lines.push(Line::from(vec![
            Span::styled("────────", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
        ]));

        if comments.is_empty() {
            detail_lines.push(Line::from(vec![
                Span::styled("No comments.", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
            ]));
        } else {
            for c in comments {
                let dt = format_comment_date(&c.date);
                detail_lines.push(Line::from(vec![
                    Span::styled(format!("{} ", c.user.username), Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_FG)),
                    Span::styled(format!("({})", dt), Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                ]));
                for line in c.comment_text.lines() {
                    detail_lines.push(Line::from(vec![
                        Span::styled(format!("  {}", line), Style::default().fg(crate::ui::styles::COLOR_FG)),
                    ]));
                }
                detail_lines.push(Line::from(""));
            }
        }

        let total_detail_lines = detail_lines.len();

        // Calculate maximum vertical scroll based on current terminal height
        let size = terminal.size()?;
        let main_layout_height = size.height.saturating_sub(2); // subtracting help bar height
        let right_pane_height = main_layout_height.saturating_sub(2) as usize; // subtracting borders
        let max_right_scroll = total_detail_lines.saturating_sub(right_pane_height) as u16;

        terminal.draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);
            let main_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(2)].as_ref())
                .split(size);

            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(main_layout[0]);

            let left_border_style = if active_pane == ActivePane::Left {
                crate::ui::styles::style_border_active()
            } else {
                crate::ui::styles::style_border_inactive()
            };

            let right_border_style = if active_pane == ActivePane::Right {
                crate::ui::styles::style_border_active()
            } else {
                crate::ui::styles::style_border_inactive()
            };

            // Left Pane: Tasks List
            let items: Vec<ListItem> = tasks
                .iter()
                .map(|t| {
                    let status_color = crate::ui::styles::get_status_color(&t.status.status);
                    let date_str = format_task_date(&t.date_updated);
                    let date_display = if date_str.is_empty() {
                        "        ".to_string()
                    } else {
                        format!("[{}] ", date_str)
                    };
                    let date_span = Span::styled(
                        date_display,
                        Style::default().fg(crate::ui::styles::COLOR_MUTED)
                    );
                    let status_upper = t.status.status.to_uppercase();
                    let status_span = Span::styled(
                        format!("[{:<11}]", status_upper),
                        Style::default().fg(status_color).add_modifier(Modifier::BOLD)
                    );
                    let name_span = Span::styled(
                        format!(" {}", t.name),
                        Style::default().fg(crate::ui::styles::COLOR_FG)
                    );

                    ListItem::new(vec![
                        Line::from(vec![date_span, status_span, name_span]),
                        Line::from(""),
                    ])
                })
                .collect();

            let left_list = RatatuiList::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Tasks List ")
                        .border_style(left_border_style),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(left_list, chunks[0], &mut list_state);

            // Right Pane: Task Details & Comments
            let right_title = if active_pane == ActivePane::Right {
                format!(" Task: {} (Focused) ", detailed_task.name)
            } else {
                format!(" Task: {} ", detailed_task.name)
            };

            let right_pane = Paragraph::new(detail_lines.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(right_title)
                        .border_style(right_border_style),
                )
                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                .wrap(Wrap { trim: true })
                .scroll((right_scroll, 0));

            f.render_widget(right_pane, chunks[1]);

            // Help Bar
            let help_line = Line::from(vec![
                Span::styled(" Tab", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Switch Pane |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" c", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Add Comment |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" s", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Change Status |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" n", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" New Task |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" r", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Reload |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" ↑/↓ (j/k)", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Scroll Focused Pane |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" q", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Quit", Style::default().fg(crate::ui::styles::COLOR_FG)),
            ]);

            let help_bar = Paragraph::new(help_line).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(crate::ui::styles::COLOR_MUTED))
            );
            f.render_widget(help_bar, main_layout[1]);

            // Draw popups if needed
            if state == BrowseState::CommentEditor {
                let popup_layout = get_popup_layout(size, 50, 30);
                f.render_widget(Clear, popup_layout);

                let editor_p = Paragraph::new(comment_buffer.as_str())
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Add Comment (Ctrl+s to post, Esc to close) ")
                            .border_style(crate::ui::styles::style_border_active())
                            .padding(Padding::new(2, 2, 1, 1)),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                f.render_widget(editor_p, popup_layout);

                // Place cursor at the end of the input text
                let lines: Vec<&str> = comment_buffer.split('\n').collect();
                let last_line = lines.last().copied().unwrap_or("");
                let last_line_len = last_line.chars().count();

                // Border takes 1, Padding::new(2, 2, 1, 1) takes 2 left/right and 1 top/bottom
                let inner_width = (popup_layout.width as usize).saturating_sub(6);
                let extra_y = if inner_width > 0 { last_line_len / inner_width } else { 0 };
                let extra_x = if inner_width > 0 { last_line_len % inner_width } else { 0 };

                let cursor_y = popup_layout.y + 2 + (lines.len() - 1) as u16 + extra_y as u16;
                let cursor_x = popup_layout.x + 3 + extra_x as u16;

                // Keep cursor within bounds of the popup
                let safe_cursor_x = cursor_x.min(popup_layout.x + popup_layout.width.saturating_sub(2));
                let safe_cursor_y = cursor_y.min(popup_layout.y + popup_layout.height.saturating_sub(2));

                f.set_cursor_position(ratatui::layout::Position::new(safe_cursor_x, safe_cursor_y));
            } else if state == BrowseState::StatusPicker {
                let popup_layout = get_popup_layout(size, 40, 50);
                f.render_widget(Clear, popup_layout);

                let items: Vec<ListItem> = list_statuses
                    .iter()
                    .map(|s| {
                        ListItem::new(format!("  {}", s.status))
                            .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    })
                    .collect();

                let picker_list = RatatuiList::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Change Status (Enter to select, Esc to close) ")
                            .border_style(crate::ui::styles::style_border_active()),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    .highlight_style(crate::ui::styles::style_selected());

                f.render_stateful_widget(picker_list, popup_layout, &mut statuses_state);
            }
        })?;

        // Handle events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match state {
                        BrowseState::List => match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                match active_pane {
                                    ActivePane::Left => {
                                        if current_task_idx > 0 {
                                            list_state.select(Some(current_task_idx - 1));
                                            right_scroll = 0;
                                        }
                                    }
                                    ActivePane::Right => {
                                        if right_scroll > 0 {
                                            right_scroll -= 1;
                                        }
                                    }
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                match active_pane {
                                    ActivePane::Left => {
                                        if current_task_idx + 1 < tasks.len() {
                                            list_state.select(Some(current_task_idx + 1));
                                            right_scroll = 0;
                                        }
                                    }
                                    ActivePane::Right => {
                                        if right_scroll < max_right_scroll {
                                            right_scroll += 1;
                                        }
                                    }
                                }
                            }
                            KeyCode::Left | KeyCode::Char('h') => {
                                active_pane = ActivePane::Left;
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                active_pane = ActivePane::Right;
                            }
                            KeyCode::Tab | KeyCode::BackTab => {
                                active_pane = match active_pane {
                                    ActivePane::Left => ActivePane::Right,
                                    ActivePane::Right => ActivePane::Left,
                                };
                            }
                            KeyCode::Char('c') => {
                                comment_buffer.clear();
                                state = BrowseState::CommentEditor;
                            }
                            KeyCode::Char('s') => {
                                // Load list statuses for picker
                                terminal.draw(|f| {
                                    crate::ui::styles::render_background(f);
                                    f.render_widget(
                                        Paragraph::new("Loading status list...").block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        ),
                                        f.area(),
                                    );
                                })?;

                                let mut found_statuses = Vec::new();
                                if let Some(list_id) = task_list_map.get(&current_task.id) {
                                    if let Ok(ld) = api.get_list_detail(list_id).await {
                                        found_statuses = ld.statuses;
                                    }
                                }

                                if found_statuses.is_empty() {
                                    found_statuses = vec![
                                        Status { status: "To Do".to_string(), color: String::new(), type_: "todo".to_string() },
                                        Status { status: "In Progress".to_string(), color: String::new(), type_: "custom".to_string() },
                                        Status { status: "In Review".to_string(), color: String::new(), type_: "custom".to_string() },
                                        Status { status: "Blocked".to_string(), color: String::new(), type_: "custom".to_string() },
                                        Status { status: "Complete".to_string(), color: String::new(), type_: "closed".to_string() },
                                    ];
                                }

                                list_statuses = found_statuses;
                                statuses_state.select(Some(0));
                                state = BrowseState::StatusPicker;
                            }
                            KeyCode::Char('r') => {
                                cached_task_details.remove(&current_task.id);
                                cached_comments.remove(&current_task.id);
                            }
                            KeyCode::Char('n') => {
                                crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
                                crossterm::terminal::disable_raw_mode()?;

                                let _ = crate::cmd::new_task::run_new_task(api).await;

                                crossterm::terminal::enable_raw_mode()?;
                                crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
                                terminal.clear()?;

                                // Invalidate and reload task list
                                return Box::pin(run_browse_loop(api, terminal, all_flag, mine_only)).await;
                            }
                            _ => {}
                        },
                        BrowseState::CommentEditor => {
                            if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Submit comment
                                if !comment_buffer.trim().is_empty() {
                                    terminal.draw(|f| {
                                        crate::ui::styles::render_background(f);
                                        f.render_widget(
                                            Paragraph::new("Posting comment...").block(
                                                Block::default().borders(Borders::ALL).title(" Please Wait "),
                                            ),
                                            f.area(),
                                        );
                                    })?;

                                    if api
                                        .create_task_comment(&current_task.id, &comment_buffer)
                                        .await
                                        .is_ok()
                                    {
                                        cached_comments.remove(&current_task.id);
                                    }
                                }
                                state = BrowseState::List;
                            } else if key.code == KeyCode::Esc {
                                state = BrowseState::List;
                            } else {
                                match key.code {
                                    KeyCode::Char(c) => {
                                        comment_buffer.push(c);
                                    }
                                    KeyCode::Backspace => {
                                        comment_buffer.pop();
                                    }
                                    KeyCode::Enter => {
                                        comment_buffer.push('\n');
                                    }
                                    _ => {}
                                }
                            }
                        }
                        BrowseState::StatusPicker => match key.code {
                            KeyCode::Esc => {
                                state = BrowseState::List;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = statuses_state.selected().unwrap_or(0);
                                if i > 0 {
                                    statuses_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = statuses_state.selected().unwrap_or(0);
                                if i + 1 < list_statuses.len() {
                                    statuses_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Enter => {
                                let idx = statuses_state.selected().unwrap_or(0);
                                let selected_stat = &list_statuses[idx];

                                terminal.draw(|f| {
                                    crate::ui::styles::render_background(f);
                                    f.render_widget(
                                        Paragraph::new("Updating status...").block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        ),
                                        f.area(),
                                    );
                                })?;

                                if api
                                    .update_task_status(&current_task.id, &selected_stat.status)
                                    .await
                                    .is_ok()
                                {
                                    cached_task_details.remove(&current_task.id);
                                    if let Some(t) = tasks.iter_mut().find(|t| t.id == current_task.id) {
                                        t.status.status = selected_stat.status.clone();
                                    }
                                }
                                state = BrowseState::List;
                            }
                            _ => {}
                        },
                    }
                }
            }
        }
    }

    Ok(())
}

fn get_popup_layout(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
