use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Comment, Status, Task};
use crate::config::Config;
use crate::util::errors::Result;
use crate::util::filter::should_include_task;
use crate::util::format::{format_comment_date, format_task_date};
use crate::util::sort::{sort_comments_by_date_desc, sort_tasks_by_updated_desc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
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

pub async fn run_browse<A: ClickUpApi + Clone + 'static>(api: &A, all_flag: bool, mine_only: bool) -> Result<()> {
    let mut guard = crate::ui::terminal::TerminalGuard::create()?;
    run_browse_loop(api, guard.inner(), all_flag, mine_only).await
}


async fn run_browse_loop<A: ClickUpApi + Clone + 'static>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    all_flag: bool,
    mine_only: bool,
) -> Result<()> {
    'reload: loop {
    let cfg = Config::load()?;

    draw_loader(terminal, "Connecting to ClickUp", "Fetching current user profile...")?;
    let user = api.get_current_user().await?;

    // Load active tasks across folders
    let mut tasks = Vec::new();
    let mut task_list_map = std::collections::HashMap::new(); // task_id -> list_id

    let total_folders = cfg.folders.len();
    for (f_idx, folder) in cfg.folders.iter().enumerate() {
        draw_loader(
            terminal,
            &format!("Fetching lists (Folder {}/{})", f_idx + 1, total_folders),
            &format!("Folder: {}", folder.name),
        )?;
        if let Ok(lists) = api.get_lists(&folder.id).await {
            let total_lists = lists.len();
            for (l_idx, list) in lists.iter().enumerate() {
                draw_loader(
                    terminal,
                    &format!(
                        "Fetching tasks (Folder {}/{}, List {}/{})",
                        f_idx + 1,
                        total_folders,
                        l_idx + 1,
                        total_lists
                    ),
                    &format!("List: {}", list.name),
                )?;
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

    let cached_comments = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashMap::<String, Vec<Comment>>::new(),
    ));
    let cached_task_details = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashMap::<String, Task>::new(),
    ));
    let mut loading_tasks = std::collections::HashSet::<String>::new();

    let mut comment_buffer = String::new();

    let mut list_statuses: Vec<Status> = Vec::new();
    let mut statuses_state = ListState::default();

    loop {
        let current_task_idx = list_state.selected().unwrap_or(0);
        let current_task = tasks[current_task_idx].clone();

        // Check if background fetch is needed
        let (has_detail, has_comments) = {
            let details = cached_task_details.lock().unwrap();
            let comments = cached_comments.lock().unwrap();
            (details.contains_key(&current_task.id), comments.contains_key(&current_task.id))
        };

        if (!has_detail || !has_comments) && !loading_tasks.contains(&current_task.id) {
            loading_tasks.insert(current_task.id.clone());

            let api_clone = api.clone();
            let details_clone = cached_task_details.clone();
            let comments_clone = cached_comments.clone();
            let task_id = current_task.id.clone();
            let task_fallback = current_task.clone();

            tokio::spawn(async move {
                // Fetch details
                let detailed = match api_clone.get_task_detail(&task_id).await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::error!("Failed to fetch task details for {}: {:?}", task_id, e);
                        task_fallback
                    }
                };
                details_clone.lock().unwrap().insert(task_id.clone(), detailed);

                // Fetch comments
                let comments = match api_clone.get_task_comments(&task_id).await {
                    Ok(mut c) => {
                        sort_comments_by_date_desc(&mut c);
                        c
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch task comments for {}: {:?}", task_id, e);
                        Vec::new()
                    }
                };
                comments_clone.lock().unwrap().insert(task_id, comments);
            });
        }

        // Get detailed task and comments if they are cached
        let detailed_task = cached_task_details.lock().unwrap().get(&current_task.id).cloned();
        let comments = cached_comments.lock().unwrap().get(&current_task.id).cloned();

        let mut detail_lines = Vec::new();
        let max_right_scroll;
        let right_title: String;

        let right_border_style = if active_pane == ActivePane::Right {
            crate::ui::styles::style_border_active()
        } else {
            crate::ui::styles::style_border_inactive()
        };

        let right_pane_widget = match (&detailed_task, &comments) {
            (Some(detailed_task), Some(comments)) => {
                let assignees = detailed_task
                    .assignees
                    .iter()
                    .map(|u| u.username.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");

                let desc_text = detailed_task.text_content.as_deref().unwrap_or("No description");

                let status_color = crate::ui::styles::get_status_color(&detailed_task.status.status);
                detail_lines.push(Line::from(vec![
                    Span::styled("Status:    ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                    Span::styled(detailed_task.status.status.to_uppercase(), Style::default().add_modifier(Modifier::BOLD).fg(status_color)),
                ]));

                detail_lines.push(Line::from(vec![
                    Span::styled("Assignees: ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                    Span::styled(assignees, Style::default().fg(crate::ui::styles::COLOR_FG)),
                ]));

                detail_lines.push(Line::from(vec![
                    Span::styled("Creator:   ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                    Span::styled(&detailed_task.creator.username, Style::default().fg(crate::ui::styles::COLOR_FG)),
                ]));

                detail_lines.push(Line::from(""));

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

                // Calculate maximum vertical scroll
                let total_detail_lines = detail_lines.len();
                let size = terminal.size()?;
                let main_layout_height = size.height.saturating_sub(2);
                let right_pane_height = main_layout_height.saturating_sub(2) as usize;
                max_right_scroll = total_detail_lines.saturating_sub(right_pane_height) as u16;

                right_title = if active_pane == ActivePane::Right {
                    format!(" Task: {} (Focused) ", detailed_task.name)
                } else {
                    format!(" Task: {} ", detailed_task.name)
                };

                Paragraph::new(detail_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(right_title)
                            .border_style(right_border_style),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    .wrap(Wrap { trim: true })
                    .scroll((right_scroll, 0))
            }
            _ => {
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
                max_right_scroll = 0;
                Paragraph::new(loading_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Loading Task ")
                            .border_style(right_border_style),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    .wrap(Wrap { trim: true })
            }
        };

        // Render Frame
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

            // Right Pane
            f.render_widget(right_pane_widget, chunks[1]);

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
                crate::ui::render_comment_editor(f, size, &comment_buffer);
            } else if state == BrowseState::StatusPicker {
                let popup_layout = crate::ui::get_popup_layout(size, 40, 50);
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
                            KeyCode::Char('q') => break 'reload,
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                break 'reload;
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
                                cached_task_details.lock().unwrap().remove(&current_task.id);
                                cached_comments.lock().unwrap().remove(&current_task.id);
                                loading_tasks.remove(&current_task.id);
                            }
                            KeyCode::Char('n') => {
                                crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
                                crossterm::terminal::disable_raw_mode()?;

                                let _ = crate::cmd::new_task::run_new_task(api).await;

                                crossterm::terminal::enable_raw_mode()?;
                                crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
                                terminal.clear()?;

                                continue 'reload;
                            }
                            _ => {}
                        },
                        BrowseState::CommentEditor => {
                            if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
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
                                        cached_comments.lock().unwrap().remove(&current_task.id);
                                        loading_tasks.remove(&current_task.id);
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
                                    cached_task_details.lock().unwrap().remove(&current_task.id);
                                    loading_tasks.remove(&current_task.id);
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

    } // 'reload

    Ok(())
}

fn draw_loader(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    message: &str,
    sub_message: &str,
) -> Result<()> {
    terminal.draw(|f| {
        let size = f.area();
        crate::ui::styles::render_background(f);

        // Center popup layout for loading
        let percent_x = 60;
        let percent_y = 35;
        let popup_layout = crate::ui::get_popup_layout(size, percent_x, percent_y);
        f.render_widget(Clear, popup_layout);

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ⚡ CLICKUP INTERACTIVE BROWSE", Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Status: ", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                Span::styled(message, Style::default().fg(crate::ui::styles::COLOR_FG).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Details: ", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                Span::styled(sub_message, Style::default().fg(crate::ui::styles::COLOR_WARN)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Please wait while we sync with ClickUp...", Style::default().fg(crate::ui::styles::COLOR_MUTED).add_modifier(Modifier::ITALIC)),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Syncing ClickUp Data ")
            .border_style(crate::ui::styles::style_border_active())
            .padding(Padding::new(2, 2, 1, 1));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));

        f.render_widget(paragraph, popup_layout);
    })?;
    Ok(())
}

