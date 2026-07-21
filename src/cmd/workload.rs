use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Comment, Status, Tag, Task};
use crate::config::Config;
use crate::util::errors::Result;
use crate::util::format::{format_comment_date, format_task_date};
use crate::util::sort::{sort_comments_by_date_desc, sort_tasks_by_updated_desc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List as RatatuiList, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Terminal;
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::{Arc, Mutex};


#[derive(PartialEq, Eq)]
enum WorkloadPane {
    Members,
    Tasks,
    Detail,
}

struct MemberWorkload {
    username: String,
    tasks: Vec<Task>,
}

pub async fn run_workload<A: ClickUpApi + Clone + 'static>(api: &A) -> Result<()> {
    let mut guard = crate::ui::terminal::TerminalGuard::create()?;
    run_workload_loop(api, guard.inner()).await
}

async fn run_workload_loop<A: ClickUpApi + Clone + 'static>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    'reload: loop {
    let cfg = Config::load()?;

    draw_loader(terminal, "Connecting to ClickUp", "Fetching workspace tasks...")?;

    let mut all_tasks: Vec<Task> = Vec::new();
    let mut task_list_map = std::collections::HashMap::new();

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
                if let Ok(t_list) = api.get_tasks(&list.id, true).await {
                    for task in &t_list {
                        task_list_map.insert(task.id.clone(), list.id.clone());
                    }
                    all_tasks.extend(t_list);
                }
            }
        }
    }

    // Group tasks by assignee
    let mut tasks_by_user: HashMap<String, Vec<Task>> = HashMap::new();
    for task in all_tasks {
        if task.assignees.is_empty() {
            tasks_by_user
                .entry("(unassigned)".to_string())
                .or_default()
                .push(task);
        } else {
            for assignee in &task.assignees {
                tasks_by_user
                    .entry(assignee.username.clone())
                    .or_default()
                    .push(task.clone());
            }
        }
    }

    let mut members: Vec<MemberWorkload> = tasks_by_user
        .into_iter()
        .map(|(username, mut tasks)| {
            sort_tasks_by_updated_desc(&mut tasks);
            MemberWorkload { username, tasks }
        })
        .collect();

    // Sort alphabetically, but "(unassigned)" always last
    members.sort_by(|a, b| {
        let a_unassigned = a.username == "(unassigned)";
        let b_unassigned = b.username == "(unassigned)";
        match (a_unassigned, b_unassigned) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.username.cmp(&b.username),
        }
    });

    if members.is_empty() {
        terminal.draw(|f| {
            crate::ui::styles::render_background(f);
            f.render_widget(
                Paragraph::new(
                    "\n  No tasks found in configured folders.\n\n  Press any key to return.",
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Team Workload ")
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

    // Collect unique statuses across all tasks for the filter picker
    let filter_picker_items: Vec<String> = {
        let mut seen = std::collections::BTreeSet::new();
        for m in &members {
            for t in &m.tasks {
                seen.insert(t.status.status.to_lowercase());
            }
        }
        seen.into_iter().collect()
    };

    // Default: hide completed, closed, backlog, done
    let mut excluded_statuses: HashSet<String> = ["completed", "closed", "backlog", "done", "cancelled"]
        .iter()
        .filter(|s| filter_picker_items.contains(&s.to_string()))
        .map(|s| s.to_string())
        .collect();

    let mut show_filter_picker = false;
    let mut filter_picker_state = ListState::default();
    filter_picker_state.select(Some(0));

    let mut show_comment_editor = false;
    let mut comment_buffer = String::new();

    let mut show_status_picker = false;
    let mut list_statuses: Vec<Status> = Vec::new();
    let mut statuses_state = ListState::default();

    let mut show_tag_picker = false;
    let mut space_tags: Vec<Tag> = Vec::new();
    let mut tags_state = ListState::default();
    let mut task_tag_selection: Vec<bool> = Vec::new();

    let mut member_state = ListState::default();
    member_state.select(Some(0));

    let mut task_state = ListState::default();
    task_state.select(Some(0));

    let mut active_pane = WorkloadPane::Members;
    let mut right_scroll: u16 = 0;

    let cached_comments = Arc::new(Mutex::new(HashMap::<String, Vec<Comment>>::new()));
    let cached_task_details = Arc::new(Mutex::new(HashMap::<String, Task>::new()));
    let mut loading_tasks = HashSet::<String>::new();

    loop {
        let member_idx = member_state.selected().unwrap_or(0);
        let member_username = members[member_idx].username.clone();

        let filtered_tasks: Vec<Task> = members[member_idx].tasks
            .iter()
            .filter(|t| !excluded_statuses.contains(&t.status.status.to_lowercase()))
            .cloned()
            .collect();

        let task_idx = task_state.selected().unwrap_or(0).min(filtered_tasks.len().saturating_sub(1));
        if !filtered_tasks.is_empty() {
            task_state.select(Some(task_idx));
        }

        // Trigger background fetch for current task
        if !filtered_tasks.is_empty() {
            let current_task = &filtered_tasks[task_idx];
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
                    let detailed = match api_clone.get_task_detail(&task_id).await {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::error!("Failed to fetch task details for {}: {:?}", task_id, e);
                            task_fallback
                        }
                    };
                    details_clone.lock().unwrap().insert(task_id.clone(), detailed);

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
        }

        // Layout calculation for precise width of Detail Pane
        let terminal_size = terminal.size()?;
        let size = ratatui::layout::Rect::new(0, 0, terminal_size.width, terminal_size.height);

        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)].as_ref())
            .split(size);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(17),
                Constraint::Percentage(38),
                Constraint::Percentage(45),
            ].as_ref())
            .split(main_layout[0]);

        let detail_pane_width = (chunks[2].width as usize).saturating_sub(2);
        let detail_pane_height = (chunks[2].height as usize).saturating_sub(2);

        // Build detail pane content
        let detail_widget;
        let max_right_scroll: u16;

        if filtered_tasks.is_empty() {
            max_right_scroll = 0;
            let detail_border = if active_pane == WorkloadPane::Detail {
                crate::ui::styles::style_border_active()
            } else {
                crate::ui::styles::style_border_inactive()
            };
            let empty_msg = if members[member_idx].tasks.is_empty() {
                "   No tasks for this member."
            } else {
                "   No tasks match the current filter."
            };
            detail_widget = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    empty_msg,
                    Style::default().fg(crate::ui::styles::COLOR_MUTED),
                )]),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Task Detail ")
                    .border_style(detail_border),
            )
            .style(
                Style::default()
                    .fg(crate::ui::styles::COLOR_FG)
                    .bg(crate::ui::styles::COLOR_BG),
            );
        } else {
            let current_task = &filtered_tasks[task_idx];
            let detailed_task = cached_task_details.lock().unwrap().get(&current_task.id).cloned();
            let comments = cached_comments.lock().unwrap().get(&current_task.id).cloned();
            let detail_border = if active_pane == WorkloadPane::Detail {
                crate::ui::styles::style_border_active()
            } else {
                crate::ui::styles::style_border_inactive()
            };

            match (&detailed_task, &comments) {
                (Some(det), Some(coms)) => {
                    let mut detail_lines: Vec<Line> = Vec::new();
                    let assignees = det
                        .assignees
                        .iter()
                        .map(|u| u.username.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let desc_text = det.text_content.as_deref().unwrap_or("No description");
                    let status_color = crate::ui::styles::get_status_color(&det.status.status);

                    detail_lines.push(Line::from(vec![
                        Span::styled("Status:    ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                        Span::styled(det.status.status.to_uppercase(), Style::default().add_modifier(Modifier::BOLD).fg(status_color)),
                    ]));
                    detail_lines.push(Line::from(vec![
                        Span::styled("Assignees: ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                        Span::styled(assignees, Style::default().fg(crate::ui::styles::COLOR_FG)),
                    ]));

                    let tags_display = if det.tags.is_empty() {
                        "None".to_string()
                    } else {
                        det.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
                    };
                    detail_lines.push(Line::from(vec![
                        Span::styled("Tags:      ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                        Span::styled(tags_display, Style::default().fg(crate::ui::styles::COLOR_FG)),
                    ]));

                    detail_lines.push(Line::from(vec![
                        Span::styled("Creator:   ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                        Span::styled(det.creator.username.clone(), Style::default().fg(crate::ui::styles::COLOR_FG)),
                    ]));
                    detail_lines.push(Line::from(vec![
                        Span::styled("Updated:   ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                        Span::styled(format_task_date(&det.date_updated), Style::default().fg(crate::ui::styles::COLOR_FG)),
                    ]));

                    let task_url = format!("https://app.clickup.com/t/{}/{}", cfg.workspace_id, det.id);
                    let mut link_spans = vec![
                        Span::styled("Link:      ", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_MUTED)),
                    ];
                    for seg in crate::util::format::parse_links(&task_url) {
                        match seg {
                            crate::util::format::TextSegment::Plain(t) => {
                                link_spans.push(Span::styled(t, Style::default().fg(crate::ui::styles::COLOR_FG)));
                            }
                            crate::util::format::TextSegment::Link { url, text: _ } => {
                                link_spans.push(Span::styled(
                                    url,
                                    Style::default().fg(ratatui::style::Color::Cyan).add_modifier(Modifier::UNDERLINED),
                                ));
                            }
                        }
                    }
                    detail_lines.push(Line::from(link_spans));

                    detail_lines.push(Line::from(""));
                    detail_lines.push(Line::from(vec![
                        Span::styled("Description", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                    ]));
                    detail_lines.push(Line::from(vec![
                        Span::styled("───────────", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                    ]));

                    let wrapped_desc = crate::util::format::wrap_text_by_words(desc_text, detail_pane_width);
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
                    if coms.is_empty() {
                        detail_lines.push(Line::from(vec![
                            Span::styled("No comments.", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                        ]));
                    } else {
                        for c in coms {
                            let dt = format_comment_date(&c.date);
                            detail_lines.push(Line::from(vec![
                                Span::styled(format!("{} ", c.user.username), Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_FG)),
                                Span::styled(format!("({})", dt), Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                            ]));
                            let wrapped_comment = crate::util::format::wrap_text_by_words(&c.comment_text, detail_pane_width.saturating_sub(2));
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

                    max_right_scroll = detail_lines.len().saturating_sub(detail_pane_height) as u16;

                    let title = if active_pane == WorkloadPane::Detail {
                        format!(" Task: {} (Focused) ", det.name)
                    } else {
                        format!(" Task: {} ", det.name)
                    };

                    detail_widget = Paragraph::new(detail_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(title)
                                .border_style(detail_border),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .scroll((right_scroll, 0));
                }
                _ => {
                    max_right_scroll = 0;
                    let loading_lines = vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("   Loading details & comments...", Style::default().fg(crate::ui::styles::COLOR_WARN).add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("   ", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                            Span::styled(format!("\"{}\"", current_task.name), Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("   Please wait...", Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                        ]),
                    ];
                    detail_widget = Paragraph::new(loading_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Loading Task ")
                                .border_style(detail_border),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .wrap(Wrap { trim: true });
                }
            }
        }

        // Render
        terminal.draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);

            let main_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(2)].as_ref())
                .split(size);

            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(17),
                    Constraint::Percentage(38),
                    Constraint::Percentage(45),
                ].as_ref())
                .split(main_layout[0]);

            let members_border = if active_pane == WorkloadPane::Members {
                crate::ui::styles::style_border_active()
            } else {
                crate::ui::styles::style_border_inactive()
            };
            let tasks_border = if active_pane == WorkloadPane::Tasks {
                crate::ui::styles::style_border_active()
            } else {
                crate::ui::styles::style_border_inactive()
            };

            // Members pane
            let member_items: Vec<ListItem> = members
                .iter()
                .map(|m| {
                    let shown = m.tasks.iter()
                        .filter(|t| !excluded_statuses.contains(&t.status.status.to_lowercase()))
                        .count();
                    let total = m.tasks.len();
                    let count_str = if shown == total {
                        format!("({})", total)
                    } else {
                        format!("({}/{})", shown, total)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!(" {} ", m.username),
                            Style::default().fg(crate::ui::styles::COLOR_FG),
                        ),
                        Span::styled(count_str, Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                    ]))
                })
                .collect();

            let member_list = RatatuiList::new(member_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Team Members ")
                        .border_style(members_border),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(member_list, chunks[0], &mut member_state);

            // Tasks pane for selected member
            let current_member = &members[member_state.selected().unwrap_or(0)];
            let task_items: Vec<ListItem> = filtered_tasks
                .iter()
                .map(|t| {
                    let status_color = crate::ui::styles::get_status_color(&t.status.status);
                    let date_str = format_task_date(&t.date_updated);
                    let date_display = if date_str.is_empty() {
                        "        ".to_string()
                    } else {
                        format!("[{}] ", date_str)
                    };
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(date_display, Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                            Span::styled(
                                format!("[{:<11}]", t.status.status.to_uppercase()),
                                Style::default().fg(status_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" {}", t.name),
                                Style::default().fg(crate::ui::styles::COLOR_FG),
                            ),
                        ]),
                        Line::from(""),
                    ])
                })
                .collect();

            let shown = filtered_tasks.len();
            let total = current_member.tasks.len();
            let tasks_title = if shown == total {
                format!(" {} Tasks ({}) ", current_member.username, total)
            } else {
                format!(" {} Tasks ({}/{}) ", current_member.username, shown, total)
            };
            let task_list = RatatuiList::new(task_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(tasks_title)
                        .border_style(tasks_border),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(task_list, chunks[1], &mut task_state);

            // Detail pane
            f.render_widget(detail_widget, chunks[2]);

            // Help bar
            let help_line = Line::from(vec![
                Span::styled(" Tab", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Switch Pane |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" c", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Comment |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" s", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Status |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" t", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Tags |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" f", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Filter |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" n", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" New Task |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" r", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Reload |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" ↑/↓ (j/k)", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Navigate |", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled(" q", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Quit", Style::default().fg(crate::ui::styles::COLOR_FG)),
            ]);

            let help_bar = Paragraph::new(help_line).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(crate::ui::styles::COLOR_MUTED)),
            );
            f.render_widget(help_bar, main_layout[1]);

            // Filter picker popup
            if show_filter_picker {
                let popup_layout = crate::ui::get_popup_layout(size, 40, 60);
                f.render_widget(Clear, popup_layout);

                let picker_items: Vec<ListItem> = filter_picker_items
                    .iter()
                    .map(|s| {
                        let included = !excluded_statuses.contains(s);
                        let mark = if included { "[x]" } else { "[ ]" };
                        let color = if included {
                            crate::ui::styles::COLOR_SUCCESS
                        } else {
                            crate::ui::styles::COLOR_MUTED
                        };
                        ListItem::new(format!(" {} {}", mark, s.to_uppercase()))
                            .style(Style::default().fg(color).bg(crate::ui::styles::COLOR_BG))
                    })
                    .collect();

                let picker = RatatuiList::new(picker_items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Filter by Status (Space toggle, Esc/Enter close) ")
                            .border_style(crate::ui::styles::style_border_active())
                            .style(Style::default().bg(crate::ui::styles::COLOR_BG)),
                    )
                    .style(Style::default().bg(crate::ui::styles::COLOR_BG))
                    .highlight_style(crate::ui::styles::style_selected());

                f.render_stateful_widget(picker, popup_layout, &mut filter_picker_state);
            }

            // Status Picker popup
            if show_status_picker {
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

            // Tag Picker popup
            if show_tag_picker {
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
            }

            // Comment editor popup
            if show_comment_editor {
                crate::ui::render_comment_editor(f, size, &comment_buffer);
            }
        })?;

        // Event handling
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Comment editor intercepts all keys when open
                    if show_comment_editor {
                        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            if !comment_buffer.trim().is_empty() && !filtered_tasks.is_empty() {
                                let task_id = filtered_tasks[task_idx].id.clone();
                                terminal.draw(|f| {
                                    crate::ui::styles::render_background(f);
                                    f.render_widget(
                                        Paragraph::new("Posting comment...").block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        ),
                                        f.area(),
                                    );
                                })?;
                                if api.create_task_comment(&task_id, &comment_buffer).await.is_ok() {
                                    cached_comments.lock().unwrap().remove(&task_id);
                                    loading_tasks.remove(&task_id);
                                }
                            }
                            show_comment_editor = false;
                            comment_buffer.clear();
                        } else if key.code == KeyCode::Esc {
                            show_comment_editor = false;
                            comment_buffer.clear();
                        } else {
                            match key.code {
                                KeyCode::Char(c) => comment_buffer.push(c),
                                KeyCode::Backspace => { comment_buffer.pop(); }
                                KeyCode::Enter => comment_buffer.push('\n'),
                                _ => {}
                            }
                        }
                        continue;
                    }

                    // Status picker intercepts all keys when open
                    if show_status_picker {
                        match key.code {
                            KeyCode::Esc => {
                                show_status_picker = false;
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
                                if idx < list_statuses.len() && !filtered_tasks.is_empty() {
                                    let selected_stat = &list_statuses[idx];
                                    let current_task = &filtered_tasks[task_idx];
                                    let task_id = current_task.id.clone();

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
                                        .update_task_status(&task_id, &selected_stat.status)
                                        .await
                                        .is_ok()
                                    {
                                        cached_task_details.lock().unwrap().remove(&task_id);
                                        loading_tasks.remove(&task_id);
                                        if let Some(m) = members.iter_mut().find(|m| m.username == member_username) {
                                            if let Some(t) = m.tasks.iter_mut().find(|t| t.id == task_id) {
                                                t.status.status = selected_stat.status.clone();
                                            }
                                        }
                                    }
                                }
                                show_status_picker = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Tag picker intercepts all keys when open
                    if show_tag_picker {
                        match key.code {
                            KeyCode::Esc => {
                                show_tag_picker = false;
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
                                if !filtered_tasks.is_empty() {
                                    let current_task = &filtered_tasks[task_idx];
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
                                        } else if !wanted
                                            && had
                                            && api.remove_tag_from_task(&task_id, &tag.name).await.is_err()
                                        {
                                            any_err = true;
                                        }
                                    }

                                    if !any_err {
                                        cached_task_details.lock().unwrap().remove(&task_id);
                                        loading_tasks.remove(&task_id);
                                        api.invalidate_task(&task_id).await;
                                    }
                                }
                                show_tag_picker = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Filter picker intercepts all keys when open
                    if show_filter_picker {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('f') => {
                                show_filter_picker = false;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = filter_picker_state.selected().unwrap_or(0);
                                if i > 0 {
                                    filter_picker_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = filter_picker_state.selected().unwrap_or(0);
                                if i + 1 < filter_picker_items.len() {
                                    filter_picker_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Char(' ') => {
                                let i = filter_picker_state.selected().unwrap_or(0);
                                let status = filter_picker_items[i].clone();
                                if excluded_statuses.contains(&status) {
                                    excluded_statuses.remove(&status);
                                } else {
                                    excluded_statuses.insert(status);
                                }
                                task_state.select(Some(0));
                                right_scroll = 0;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') => break 'reload,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break 'reload;
                        }
                        KeyCode::Char('f') if active_pane == WorkloadPane::Tasks || active_pane == WorkloadPane::Members => {
                            show_filter_picker = true;
                        }
                        KeyCode::Char('c') if active_pane == WorkloadPane::Tasks && !filtered_tasks.is_empty() => {
                            comment_buffer.clear();
                            show_comment_editor = true;
                        }
                        KeyCode::Char('s') if active_pane == WorkloadPane::Tasks && !filtered_tasks.is_empty() => {
                            let current_task = &filtered_tasks[task_idx];
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
                            show_status_picker = true;
                        }
                        KeyCode::Char('t') if active_pane == WorkloadPane::Tasks && !filtered_tasks.is_empty() => {
                            let current_task = &filtered_tasks[task_idx];
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
                            show_tag_picker = true;
                        }
                        KeyCode::Char('r') if active_pane == WorkloadPane::Tasks && !filtered_tasks.is_empty() => {
                            let current_task = &filtered_tasks[task_idx];
                            cached_task_details.lock().unwrap().remove(&current_task.id);
                            cached_comments.lock().unwrap().remove(&current_task.id);
                            loading_tasks.remove(&current_task.id);
                            api.invalidate_task(&current_task.id).await;
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
                        KeyCode::Tab => {
                            active_pane = match active_pane {
                                WorkloadPane::Members => WorkloadPane::Tasks,
                                WorkloadPane::Tasks => WorkloadPane::Detail,
                                WorkloadPane::Detail => WorkloadPane::Members,
                            };
                        }
                        KeyCode::BackTab => {
                            active_pane = match active_pane {
                                WorkloadPane::Members => WorkloadPane::Detail,
                                WorkloadPane::Tasks => WorkloadPane::Members,
                                WorkloadPane::Detail => WorkloadPane::Tasks,
                            };
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            active_pane = match active_pane {
                                WorkloadPane::Tasks => WorkloadPane::Members,
                                WorkloadPane::Detail => WorkloadPane::Tasks,
                                WorkloadPane::Members => WorkloadPane::Members,
                            };
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            active_pane = match active_pane {
                                WorkloadPane::Members => WorkloadPane::Tasks,
                                WorkloadPane::Tasks => WorkloadPane::Detail,
                                WorkloadPane::Detail => WorkloadPane::Detail,
                            };
                        }
                        KeyCode::Enter => {
                            if active_pane == WorkloadPane::Tasks {
                                active_pane = WorkloadPane::Detail;
                                right_scroll = 0;
                            }
                        }
                        KeyCode::Esc => {
                            if active_pane == WorkloadPane::Detail {
                                active_pane = WorkloadPane::Tasks;
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => match active_pane {
                            WorkloadPane::Members => {
                                let idx = member_state.selected().unwrap_or(0);
                                if idx > 0 {
                                    member_state.select(Some(idx - 1));
                                    task_state.select(Some(0));
                                    right_scroll = 0;
                                }
                            }
                            WorkloadPane::Tasks => {
                                let idx = task_state.selected().unwrap_or(0);
                                if idx > 0 {
                                    task_state.select(Some(idx - 1));
                                    right_scroll = 0;
                                }
                            }
                            WorkloadPane::Detail => {
                                right_scroll = right_scroll.saturating_sub(1);
                            }
                        },
                        KeyCode::Down | KeyCode::Char('j') => match active_pane {
                            WorkloadPane::Members => {
                                let idx = member_state.selected().unwrap_or(0);
                                if idx + 1 < members.len() {
                                    member_state.select(Some(idx + 1));
                                    task_state.select(Some(0));
                                    right_scroll = 0;
                                }
                            }
                            WorkloadPane::Tasks => {
                                let idx = task_state.selected().unwrap_or(0);
                                if idx + 1 < filtered_tasks.len() {
                                    task_state.select(Some(idx + 1));
                                    right_scroll = 0;
                                }
                            }
                            WorkloadPane::Detail => {
                                if right_scroll < max_right_scroll {
                                    right_scroll += 1;
                                }
                            }
                        },
                        _ => {}
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

        let popup_layout = crate::ui::get_popup_layout(size, 60, 35);
        f.render_widget(Clear, popup_layout);

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  TEAM WORKLOAD VIEW", Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)),
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
            .title(" Loading Workload Data ")
            .border_style(crate::ui::styles::style_border_active())
            .padding(Padding::new(2, 2, 1, 1));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));

        f.render_widget(paragraph, popup_layout);
    })?;
    Ok(())
}
