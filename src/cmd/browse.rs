use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Comment, Status, Tag, Task};
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
    TagPicker,
    FilterEditor,
    FilterTagPicker,
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

    let mut space_tags: Vec<Tag> = Vec::new();
    let mut tags_state = ListState::default();
    let mut task_tag_selection: Vec<bool> = Vec::new();

    let mut filter_query = String::new();
    let mut filter_query_buffer = String::new();
    let mut active_filter_tags: Vec<String> = Vec::new();
    let mut filter_tags_state = ListState::default();
    let mut filter_tag_selection: Vec<bool> = Vec::new();
    let mut filtered_indices: Vec<usize> = (0..tasks.len()).collect();

    loop {
        let selected_filtered_pos = list_state.selected().unwrap_or(0);
        let current_real_idx = filtered_indices.get(selected_filtered_pos).copied().unwrap_or(0);
        let current_task = tasks[current_real_idx].clone();

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

        let terminal_size = terminal.size()?;
        let size = ratatui::layout::Rect::new(0, 0, terminal_size.width, terminal_size.height);
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)].as_ref())
            .split(size);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(main_layout[0]);

        let right_pane_width = (chunks[1].width as usize).saturating_sub(2);

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

                let tags_display = if detailed_task.tags.is_empty() {
                    "None".to_string()
                } else {
                    detailed_task.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
                };
                detail_lines.push(Line::from(vec![
                    Span::styled("Tags:      ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                    Span::styled(tags_display, Style::default().fg(crate::ui::styles::COLOR_FG)),
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

                let wrapped_desc = crate::util::format::wrap_text_by_words(desc_text, right_pane_width);
                for line in wrapped_desc {
                    let segments = crate::util::format::parse_links(&line);
                    let mut spans = Vec::new();
                    for seg in segments {
                        match seg {
                            crate::util::format::TextSegment::Plain(t) => {
                                spans.push(Span::styled(t, Style::default().fg(crate::ui::styles::COLOR_FG)));
                            }
                            crate::util::format::TextSegment::Link { url, text: _ } => {
                                spans.push(Span::styled(
                                    url,
                                    Style::default()
                                        .fg(ratatui::style::Color::Cyan)
                                        .add_modifier(Modifier::UNDERLINED),
                                ));
                            }
                        }
                    }
                    detail_lines.push(Line::from(spans));
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
                        let wrapped_comment = crate::util::format::wrap_text_by_words(&c.comment_text, right_pane_width.saturating_sub(2));
                        for line in wrapped_comment {
                            let segments = crate::util::format::parse_links(&line);
                            let mut spans = vec![Span::styled("  ", Style::default().fg(crate::ui::styles::COLOR_FG))];
                            for seg in segments {
                                match seg {
                                    crate::util::format::TextSegment::Plain(t) => {
                                        spans.push(Span::styled(t, Style::default().fg(crate::ui::styles::COLOR_FG)));
                                    }
                                    crate::util::format::TextSegment::Link { url, text: _ } => {
                                        spans.push(Span::styled(
                                            url,
                                            Style::default()
                                                .fg(ratatui::style::Color::Cyan)
                                                .add_modifier(Modifier::UNDERLINED),
                                        ));
                                    }
                                }
                            }
                            detail_lines.push(Line::from(spans));
                        }
                        detail_lines.push(Line::from(""));
                    }
                }

                // Calculate maximum vertical scroll
                let total_detail_lines = detail_lines.len();
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

        // Build dynamic left pane title
        let shown = filtered_indices.len();
        let total = tasks.len();
        let count_part = if shown == total {
            format!(" Tasks [{}] ", total)
        } else {
            format!(" Tasks [{}/{}] ", shown, total)
        };
        let keyword_suffix = if !filter_query.is_empty() {
            format!("  /{}", filter_query)
        } else {
            String::new()
        };
        let tag_suffix = if !active_filter_tags.is_empty() {
            format!("  {}", active_filter_tags.iter().map(|n| format!("#{}", n)).collect::<Vec<_>>().join(","))
        } else {
            String::new()
        };
        let left_pane_title = format!("{}{}{}", count_part, keyword_suffix, tag_suffix);

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

            // Split left pane vertically when the search bar is active
            let (search_area_opt, task_list_area) = if state == BrowseState::FilterEditor {
                let sub = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                    .split(chunks[0]);
                (Some(sub[0]), sub[1])
            } else {
                (None, chunks[0])
            };

            // Render search bar when active
            if let Some(search_area) = search_area_opt {
                let search_widget = Paragraph::new(format!(" {}", filter_query_buffer))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Filter (Enter: apply, Esc: clear) ")
                            .border_style(crate::ui::styles::style_border_active()),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                f.render_widget(search_widget, search_area);
                f.set_cursor_position(ratatui::layout::Position::new(
                    search_area.x + 1 + filter_query_buffer.chars().count() as u16,
                    search_area.y + 1,
                ));
            }

            // Left Pane: Tasks List
            let items: Vec<ListItem> = filtered_indices
                .iter()
                .map(|&real_idx| {
                    let t = &tasks[real_idx];
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
                        .title(left_pane_title.clone())
                        .border_style(left_border_style),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(left_list, task_list_area, &mut list_state);

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
                Span::styled(" t", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Tags |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" n", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" New Task |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" r", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Reload |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" /", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Search |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" T", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Tag Filter |", Style::default().fg(crate::ui::styles::COLOR_FG)),
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
            } else if state == BrowseState::TagPicker {
                let popup_layout = crate::ui::get_popup_layout(size, 50, 60);
                f.render_widget(Clear, popup_layout);

                let items: Vec<ListItem> = space_tags
                    .iter()
                    .enumerate()
                    .map(|(i, tag)| {
                        let checked = if task_tag_selection.get(i).copied().unwrap_or(false) { "[x]" } else { "[ ]" };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("  {} ", checked),
                                Style::default().fg(crate::ui::styles::COLOR_MUTED),
                            ),
                            Span::styled(
                                tag.name.clone(),
                                if task_tag_selection.get(i).copied().unwrap_or(false) {
                                    Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(crate::ui::styles::COLOR_FG)
                                },
                            ),
                        ]))
                        .style(Style::default().bg(crate::ui::styles::COLOR_BG))
                    })
                    .collect();

                let picker_list = RatatuiList::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Tags (Space: toggle, Enter: apply, Esc: cancel) ")
                            .border_style(crate::ui::styles::style_border_active()),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    .highlight_style(crate::ui::styles::style_selected());

                f.render_stateful_widget(picker_list, popup_layout, &mut tags_state);
            } else if state == BrowseState::FilterTagPicker {
                let popup_layout = crate::ui::get_popup_layout(size, 50, 60);
                f.render_widget(Clear, popup_layout);

                let items: Vec<ListItem> = space_tags
                    .iter()
                    .enumerate()
                    .map(|(i, tag)| {
                        let checked = if filter_tag_selection.get(i).copied().unwrap_or(false) { "[x]" } else { "[ ]" };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("  {} ", checked),
                                Style::default().fg(crate::ui::styles::COLOR_MUTED),
                            ),
                            Span::styled(
                                tag.name.clone(),
                                if filter_tag_selection.get(i).copied().unwrap_or(false) {
                                    Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(crate::ui::styles::COLOR_FG)
                                },
                            ),
                        ]))
                        .style(Style::default().bg(crate::ui::styles::COLOR_BG))
                    })
                    .collect();

                let picker_list = RatatuiList::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Filter by Tag (Space: toggle, Enter: apply, Esc: cancel) ")
                            .border_style(crate::ui::styles::style_border_active()),
                    )
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    .highlight_style(crate::ui::styles::style_selected());

                f.render_stateful_widget(picker_list, popup_layout, &mut filter_tags_state);
            }
        })?;

        // Snapshot task_id -> tag names from the detail cache for use in filtering
        let detail_tags_snapshot: std::collections::HashMap<String, Vec<String>> = {
            let details = cached_task_details.lock().unwrap();
            details.iter().map(|(id, t)| (id.clone(), t.tags.iter().map(|tag| tag.name.clone()).collect())).collect()
        };

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
                                        if selected_filtered_pos > 0 {
                                            list_state.select(Some(selected_filtered_pos - 1));
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
                                        if selected_filtered_pos + 1 < filtered_indices.len() {
                                            list_state.select(Some(selected_filtered_pos + 1));
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
                                api.invalidate_task(&current_task.id).await;
                            }
                            KeyCode::Char('t') => {
                                if space_tags.is_empty() {
                                    let cfg = Config::load()?;
                                    space_tags = api.get_space_tags(&cfg.space_id).await.unwrap_or_default();
                                }
                                let current_task_tags: Vec<String> = {
                                    let details = cached_task_details.lock().unwrap();
                                    details.get(&current_task.id)
                                        .map(|t| t.tags.iter().map(|tag| tag.name.clone()).collect())
                                        .unwrap_or_default()
                                };
                                task_tag_selection = space_tags.iter()
                                    .map(|tag| current_task_tags.contains(&tag.name))
                                    .collect();
                                tags_state.select(Some(0));
                                state = BrowseState::TagPicker;
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
                            KeyCode::Char('/') => {
                                filter_query_buffer = filter_query.clone();
                                state = BrowseState::FilterEditor;
                            }
                            KeyCode::Char('T') => {
                                if space_tags.is_empty() {
                                    let cfg = Config::load()?;
                                    space_tags = api.get_space_tags(&cfg.space_id).await.unwrap_or_default();
                                }
                                filter_tag_selection = space_tags.iter()
                                    .map(|tag| active_filter_tags.contains(&tag.name))
                                    .collect();
                                filter_tags_state.select(Some(0));
                                state = BrowseState::FilterTagPicker;
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
                        BrowseState::TagPicker => match key.code {
                            KeyCode::Esc => {
                                state = BrowseState::List;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = tags_state.selected().unwrap_or(0);
                                if i > 0 {
                                    tags_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = tags_state.selected().unwrap_or(0);
                                if i + 1 < space_tags.len() {
                                    tags_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Char(' ') => {
                                if let Some(idx) = tags_state.selected() {
                                    if let Some(sel) = task_tag_selection.get_mut(idx) {
                                        *sel = !*sel;
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                let task_id = current_task.id.clone();
                                let current_task_tags: Vec<String> = {
                                    let details = cached_task_details.lock().unwrap();
                                    details.get(&task_id)
                                        .map(|t| t.tags.iter().map(|tag| tag.name.clone()).collect())
                                        .unwrap_or_default()
                                };

                                terminal.draw(|f| {
                                    crate::ui::styles::render_background(f);
                                    f.render_widget(
                                        Paragraph::new("Updating tags...").block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        ),
                                        f.area(),
                                    );
                                })?;

                                let mut any_err = false;
                                for (i, tag) in space_tags.iter().enumerate() {
                                    let wanted = task_tag_selection.get(i).copied().unwrap_or(false);
                                    let had = current_task_tags.contains(&tag.name);
                                    if wanted && !had {
                                        if api.add_tag_to_task(&task_id, &tag.name).await.is_err() {
                                            any_err = true;
                                        }
                                    } else if !wanted && had {
                                        if api.remove_tag_from_task(&task_id, &tag.name).await.is_err() {
                                            any_err = true;
                                        }
                                    }
                                }

                                if !any_err {
                                    cached_task_details.lock().unwrap().remove(&task_id);
                                    loading_tasks.remove(&task_id);
                                    api.invalidate_task(&task_id).await;
                                }
                                state = BrowseState::List;
                            }
                            _ => {}
                        },
                        BrowseState::FilterEditor => match key.code {
                            KeyCode::Esc => {
                                filter_query_buffer.clear();
                                filter_query.clear();
                                filtered_indices = compute_filtered_indices(&tasks, "", &active_filter_tags, &detail_tags_snapshot);
                                if filtered_indices.is_empty() {
                                    list_state.select(None);
                                } else {
                                    list_state.select(Some(0));
                                    right_scroll = 0;
                                }
                                state = BrowseState::List;
                            }
                            KeyCode::Enter => {
                                filter_query = filter_query_buffer.clone();
                                state = BrowseState::List;
                            }
                            KeyCode::Char(c) => {
                                filter_query_buffer.push(c);
                                filtered_indices = compute_filtered_indices(&tasks, &filter_query_buffer, &active_filter_tags, &detail_tags_snapshot);
                                if filtered_indices.is_empty() {
                                    list_state.select(None);
                                } else {
                                    list_state.select(Some(0));
                                    right_scroll = 0;
                                }
                            }
                            KeyCode::Backspace => {
                                filter_query_buffer.pop();
                                filtered_indices = compute_filtered_indices(&tasks, &filter_query_buffer, &active_filter_tags, &detail_tags_snapshot);
                                if filtered_indices.is_empty() {
                                    list_state.select(None);
                                } else {
                                    list_state.select(Some(0));
                                    right_scroll = 0;
                                }
                            }
                            _ => {}
                        },
                        BrowseState::FilterTagPicker => match key.code {
                            KeyCode::Esc => {
                                state = BrowseState::List;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = filter_tags_state.selected().unwrap_or(0);
                                if i > 0 {
                                    filter_tags_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = filter_tags_state.selected().unwrap_or(0);
                                if i + 1 < space_tags.len() {
                                    filter_tags_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Char(' ') => {
                                if let Some(idx) = filter_tags_state.selected() {
                                    if let Some(sel) = filter_tag_selection.get_mut(idx) {
                                        *sel = !*sel;
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                active_filter_tags = space_tags.iter().enumerate()
                                    .filter(|(i, _)| filter_tag_selection.get(*i).copied().unwrap_or(false))
                                    .map(|(_, tag)| tag.name.clone())
                                    .collect();
                                filtered_indices = compute_filtered_indices(&tasks, &filter_query, &active_filter_tags, &detail_tags_snapshot);
                                if filtered_indices.is_empty() {
                                    list_state.select(None);
                                } else {
                                    list_state.select(Some(0));
                                    right_scroll = 0;
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

fn compute_filtered_indices(
    tasks: &[Task],
    query: &str,
    active_tags: &[String],
    detail_tags: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<usize> {
    tasks.iter().enumerate().filter_map(|(i, t)| {
        let keyword_match = query.is_empty() || t.name.to_lowercase().contains(&query.to_lowercase());
        let tag_match = if active_tags.is_empty() {
            true
        } else {
            // prefer detail cache tags (more complete); fall back to list-endpoint tags
            if let Some(cached) = detail_tags.get(&t.id) {
                cached.iter().any(|name| active_tags.contains(name))
            } else {
                t.tags.iter().any(|tag| active_tags.contains(&tag.name))
            }
        };
        if keyword_match && tag_match { Some(i) } else { None }
    }).collect()
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

