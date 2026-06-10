use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Comment, Status, Task};
use crate::config::Config;
use crate::util::errors::{AppError, Result};
use crate::util::filter::should_include_task;
use crate::util::format::{format_comment_date, format_task_date};
use crate::util::sort::{sort_comments_by_date_desc, sort_tasks_by_updated_desc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, Clear, List as RatatuiList, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::io;

#[derive(PartialEq, Eq)]
enum BrowseState {
    List,
    CommentEditor,
    StatusPicker,
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
        return Err(AppError::Other(
            "No active tasks found matching criteria to browse.".to_string(),
        ));
    }

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    let mut state = BrowseState::List;

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
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                    .split(size);

                // Left Pane: Tasks List (with the newly selected highlight)
                let items: Vec<ListItem> = tasks
                    .iter()
                    .map(|t| {
                        ListItem::new(format!(
                            "[{}] {}\n  Updated: {}",
                            t.status.status,
                            t.name,
                            format_task_date(&t.date_updated)
                        ))
                    })
                    .collect();

                let left_list = RatatuiList::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Tasks List ")
                            .border_style(crate::ui::styles::style_border_active()),
                    )
                    .highlight_style(crate::ui::styles::style_selected());

                f.render_stateful_widget(left_list, chunks[0], &mut list_state);

                // Right Pane: Loading Details & Comments
                let loading_msg = format!(
                    "\n\n   ⏳ Loading details & comments...\n\n   👉 \"{}\"\n\n   Please wait...",
                    current_task.name
                );
                let right_pane = Paragraph::new(loading_msg).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Loading Task ")
                        .border_style(crate::ui::styles::style_border_inactive()),
                );

                f.render_widget(right_pane, chunks[1]);
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

        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                .split(size);

            // Left Pane: Tasks List
            let items: Vec<ListItem> = tasks
                .iter()
                .map(|t| {
                    ListItem::new(format!(
                        "[{}] {}\n  Updated: {}",
                        t.status.status,
                        t.name,
                        format_task_date(&t.date_updated)
                    ))
                })
                .collect();

            let left_list = RatatuiList::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Tasks List ")
                        .border_style(crate::ui::styles::style_border_active()),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(left_list, chunks[0], &mut list_state);

            // Right Pane: Task Details & Comments
            let assignees = detailed_task
                .assignees
                .iter()
                .map(|u| u.username.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            let desc_text = detailed_task.text_content.as_deref().unwrap_or("No description");

            let mut comments_text = String::new();
            if comments.is_empty() {
                comments_text.push_str("No comments.");
            } else {
                for c in comments {
                    let dt = format_comment_date(&c.date);
                    comments_text.push_str(&format!(
                        "[{}] {}:\n  {}\n\n",
                        dt, c.user.username, c.comment_text
                    ));
                }
            }

            let detail_content = format!(
                "Status: {}\nAssignees: {}\nCreator: {}\n\nDescription:\n{}\n\n---\nComments:\n{}",
                detailed_task.status.status,
                assignees,
                detailed_task.creator.username,
                desc_text,
                comments_text
            );

            let right_pane = Paragraph::new(detail_content).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Task: {} ", detailed_task.name))
                    .border_style(crate::ui::styles::style_border_inactive()),
            );

            f.render_widget(right_pane, chunks[1]);

            // Draw popups if needed
            if state == BrowseState::CommentEditor {
                let popup_layout = get_popup_layout(size, 60, 40);
                f.render_widget(Clear, popup_layout);

                let editor_p = Paragraph::new(comment_buffer.as_str()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Add Comment (Ctrl+s to post, Esc to close) ")
                        .border_style(crate::ui::styles::style_border_active()),
                );
                f.render_widget(editor_p, popup_layout);
            } else if state == BrowseState::StatusPicker {
                let popup_layout = get_popup_layout(size, 40, 50);
                f.render_widget(Clear, popup_layout);

                let items: Vec<ListItem> = list_statuses
                    .iter()
                    .map(|s| ListItem::new(format!("  {}", s.status)))
                    .collect();

                let picker_list = RatatuiList::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Change Status (Enter to select, Esc to close) ")
                            .border_style(crate::ui::styles::style_border_active()),
                    )
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
                                if current_task_idx > 0 {
                                    list_state.select(Some(current_task_idx - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if current_task_idx + 1 < tasks.len() {
                                    list_state.select(Some(current_task_idx + 1));
                                }
                            }
                            KeyCode::Char('c') => {
                                comment_buffer.clear();
                                state = BrowseState::CommentEditor;
                            }
                            KeyCode::Char('s') => {
                                // Load list statuses for picker
                                terminal.draw(|f| {
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
                            KeyCode::Char('n') => {
                                // Section 11.4: "n opens new-task workflow overlay, preseed list and folder if selection matches"
                                // We can call run_new_task directly and then refresh the tasks!
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
                                        // invalidate comment cache
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
                                    // Invalidate cached details and comment
                                    cached_task_details.remove(&current_task.id);
                                    // Update task status locally in tasks vector
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
