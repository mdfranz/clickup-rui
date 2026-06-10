use crate::ai::summarizer::GeminiSummarizer;
use crate::cache::ttl::now_ms;
use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Activity, User};
use crate::config::Config;
use crate::ui::spinner::Spinner;
use crate::util::env::is_menu_mode;
use crate::util::errors::Result;
use crate::util::format::format_comment_date;
use chrono::{DateTime, Local};
use std::collections::{HashMap, HashSet};

pub async fn run_track<A: ClickUpApi>(
    api: &A,
    user_id: Option<i64>,
    summarize: bool,
    raw_flag: bool,
    csv_flag: bool,
    json_flag: bool,
) -> Result<()> {
    let mut spinner = Spinner::start("Loading workspace users");
    let teams = match api.get_teams().await {
        Ok(t) => t,
        Err(e) => {
            spinner.stop();
            return Err(e);
        }
    };
    spinner.stop();

    let mut users = Vec::new();
    let mut seen = HashSet::new();
    for team in &teams {
        for member in &team.members {
            if seen.insert(member.user.id) {
                users.push(member.user.clone());
            }
        }
    }

    let target_user = if let Some(id) = user_id {
        users
            .iter()
            .find(|u| u.id == id)
            .cloned()
            .unwrap_or_else(|| User {
                id,
                username: format!("User #{}", id),
                email: String::new(),
            })
    } else {
        if users.is_empty() {
            println!("No workspace users found.");
            return Ok(());
        }

        match select_user_tui(&users).await? {
            Some(u) => u,
            None => return Ok(()),
        }
    };

    track_user_activities(api, target_user, summarize, raw_flag, csv_flag, json_flag).await?;
    Ok(())
}

async fn track_user_activities<A: ClickUpApi>(
    api: &A,
    user: User,
    summarize: bool,
    raw_flag: bool,
    csv_flag: bool,
    json_flag: bool,
) -> Result<()> {
    let mut spinner = Spinner::start("Fetching user activity logs");
    let cfg = Config::load()?;

    let now = now_ms();
    let date_from = now - (10 * 24 * 3600 * 1000); // 10 days window

    let mut activities = Vec::new();

    for folder in &cfg.folders {
        let lists = match api.get_lists(&folder.id).await {
            Ok(l) => l,
            Err(_) => continue,
        };

        for list in &lists {
            let tasks = match api.get_tasks_incremental(&list.id, date_from).await {
                Ok(t) => t,
                Err(_) => continue,
            };

            for task in tasks {
                let created_ms = task
                    .date_created
                    .as_deref()
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                let updated_ms = task
                    .date_updated
                    .as_deref()
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);

                if created_ms >= date_from && task.creator.id == user.id {
                    activities.push(Activity {
                        id: format!("{}-created", task.id),
                        user: user.clone(),
                        type_: "created task".to_string(),
                        date: created_ms.to_string(),
                        task_id: task.id.clone(),
                        source: "api".to_string(),
                        detail: Some(task.status.status.clone()),
                        task_name: Some(task.name.clone()),
                    });
                }

                let is_assignee = task.assignees.iter().any(|u| u.id == user.id);
                if is_assignee {
                    let done_ms = task
                        .date_done
                        .as_deref()
                        .and_then(|s| s.parse::<i64>().ok())
                        .unwrap_or(0);
                    let closed_ms = task
                        .date_closed
                        .as_deref()
                        .and_then(|s| s.parse::<i64>().ok())
                        .unwrap_or(0);

                    if done_ms >= date_from {
                        activities.push(Activity {
                            id: format!("{}-done", task.id),
                            user: user.clone(),
                            type_: "completed task".to_string(),
                            date: done_ms.to_string(),
                            task_id: task.id.clone(),
                            source: "api".to_string(),
                            detail: Some(task.status.status.clone()),
                            task_name: Some(task.name.clone()),
                        });
                    } else if closed_ms >= date_from {
                        activities.push(Activity {
                            id: format!("{}-closed", task.id),
                            user: user.clone(),
                            type_: "closed task".to_string(),
                            date: closed_ms.to_string(),
                            task_id: task.id.clone(),
                            source: "api".to_string(),
                            detail: Some(task.status.status.clone()),
                            task_name: Some(task.name.clone()),
                        });
                    } else if updated_ms >= date_from && updated_ms > created_ms {
                        activities.push(Activity {
                            id: format!("{}-updated", task.id),
                            user: user.clone(),
                            type_: "updated task".to_string(),
                            date: updated_ms.to_string(),
                            task_id: task.id.clone(),
                            source: "api".to_string(),
                            detail: Some(task.status.status.clone()),
                            task_name: Some(task.name.clone()),
                        });
                    }
                }

                if updated_ms >= date_from {
                    if let Ok(comments) = api.get_task_comments(&task.id).await {
                        for comment in comments {
                            if comment.user.id == user.id {
                                if let Ok(comment_ms) = comment.date.parse::<i64>() {
                                    if comment_ms >= date_from {
                                        activities.push(Activity {
                                            id: format!("{}-comment-{}", task.id, comment.id),
                                            user: user.clone(),
                                            type_: "commented on task".to_string(),
                                            date: comment_ms.to_string(),
                                            task_id: task.id.clone(),
                                            source: "api".to_string(),
                                            detail: Some(comment.comment_text.clone()),
                                            task_name: Some(task.name.clone()),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    activities.sort_by(|a, b| {
        let a_time = a.date.parse::<i64>().ok().unwrap_or(0);
        let b_time = b.date.parse::<i64>().ok().unwrap_or(0);
        b_time.cmp(&a_time)
    });

    spinner.stop();

    if csv_flag {
        let mut csv_content = String::new();
        // Write header
        csv_content.push_str("Date,Timestamp,User ID,User Name,Activity Type,Task ID,Task Name,Detail\n");

        for act in &activities {
            let ms = act.date.parse::<i64>().unwrap_or(0);
            let formatted_date = if let Some(dt) = DateTime::from_timestamp_millis(ms) {
                let local_dt: DateTime<Local> = dt.into();
                local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                String::new()
            };

            let row = format!(
                "{},{},{},{},{},{},{},{}\n",
                escape_csv_field(&formatted_date),
                ms,
                act.user.id,
                escape_csv_field(&act.user.username),
                escape_csv_field(&act.type_),
                escape_csv_field(&act.task_id),
                escape_csv_field(&act.task_name.as_deref().unwrap_or("")),
                escape_csv_field(&act.detail.as_deref().unwrap_or(""))
            );
            csv_content.push_str(&row);
        }

        let date_str = Local::now().format("%y%m%d%M").to_string();
        let filename = format!("{}-{}.csv", user.id, date_str);

        std::fs::write(&filename, csv_content)?;
        println!("Saved activity logs to CSV: {}", filename);
        return Ok(());
    }

    if json_flag {
        let json_content = serde_json::to_string_pretty(&activities)?;
        let date_str = Local::now().format("%y%m%d%M").to_string();
        let filename = format!("{}-{}.json", user.id, date_str);

        std::fs::write(&filename, json_content)?;
        println!("Saved activity logs to JSON: {}", filename);
        return Ok(());
    }

    if activities.is_empty() {
        println!("No activities found for {} in the last 10 days.", user.username);
        return Ok(());
    }

    let mut show_raw = raw_flag;
    let mut formatted_summary = String::new();

    if summarize {
        let mut spinner = Spinner::start("Generating AI user activity summary");
        let summarizer = GeminiSummarizer::new();

        let mut daily_groups: HashMap<String, Vec<Activity>> = HashMap::new();
        for act in &activities {
            let ms = act.date.parse::<i64>().unwrap_or(0);
            if let Some(dt) = DateTime::from_timestamp_millis(ms) {
                let local_dt: DateTime<Local> = dt.into();
                let day_str = local_dt.format("%Y-%m-%d").to_string();
                daily_groups.entry(day_str).or_default().push(act.clone());
            }
        }

        let mut days_sorted: Vec<String> = daily_groups.keys().cloned().collect();
        days_sorted.sort_by(|a, b| b.cmp(a));

        let mut daily_summaries = Vec::new();
        for day in days_sorted {
            let day_activities = daily_groups.get(&day).unwrap();
            match summarizer
                .summarize_user_activity(&user.username, &day, day_activities, &[], &[])
                .await
            {
                Ok(summary) => {
                    daily_summaries.push(format!("### {}\n\n{}", day, summary));
                }
                Err(e) => {
                    println!("Error summarizing for {}: {}", day, e);
                    show_raw = true;
                }
            }
        }

        formatted_summary = daily_summaries.join("\n\n");
        spinner.stop();
    }

    if formatted_summary.is_empty() {
        show_raw = true;
    }

    if is_menu_mode() {
        run_scrollable_tui(formatted_summary, activities, show_raw).await?;
    } else {
        if show_raw {
            println!("Raw Activity Log for {}:", user.username);
            for act in &activities {
                let dt = format_comment_date(&act.date);
                let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
                println!(
                    "[{}] {} \"{}\" [Task: {}] [Detail: {}]",
                    dt,
                    act.type_,
                    t_name,
                    act.task_id,
                    act.detail.as_deref().unwrap_or("N/A")
                );
            }
        } else {
            termimad::print_text(&formatted_summary);
        }
    }

    Ok(())
}

async fn run_scrollable_tui(
    summary: String,
    activities: Vec<Activity>,
    show_raw: bool,
) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use ratatui::backend::CrosstermBackend;
    use ratatui::widgets::{Block, Borders, Paragraph};
    use ratatui::Terminal;
    use std::io;

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut scroll: u16 = 0;
    let content = if show_raw {
        let mut raw_str = String::new();
        for act in &activities {
            let dt = format_comment_date(&act.date);
            let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
            raw_str.push_str(&format!(
                "[{}] {} \"{}\" [Task: {}] [Detail: {}]\n",
                dt,
                act.type_,
                t_name,
                act.task_id,
                act.detail.as_deref().unwrap_or("N/A")
            ));
        }
        raw_str
    } else {
        summary
    };

    let total_lines = content.lines().count() as u16;

    loop {
        terminal.draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);
            let block = Block::default()
                .title(" User Tracker Summary (use Arrow Up/Down or j/k to scroll, Q to quit) ")
                .borders(Borders::ALL)
                .border_style(crate::ui::styles::style_border_active());

            let p = Paragraph::new(content.as_str())
                .block(block)
                .scroll((scroll, 0));
            f.render_widget(p, size);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Up | KeyCode::Char('k') => {
                            if scroll > 0 {
                                scroll -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if scroll + 5 < total_lines {
                                scroll += 1;
                            }
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

async fn select_user_tui(users: &[User]) -> Result<Option<User>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::backend::CrosstermBackend;
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph};
    use ratatui::Terminal;
    use std::io;

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

    let mut filter = String::new();
    let mut selected_user = None;

    loop {
        // Filter users based on query
        let filtered_users: Vec<&User> = users
            .iter()
            .filter(|u| {
                if filter.is_empty() {
                    true
                } else {
                    u.username.to_lowercase().contains(&filter.to_lowercase())
                        || u.id.to_string().contains(&filter)
                }
            })
            .collect();

        // Adjust selected index if it exceeds filtered length
        let current_selected = list_state.selected().unwrap_or(0);
        if filtered_users.is_empty() {
            list_state.select(None);
        } else if current_selected >= filtered_users.len() {
            list_state.select(Some(filtered_users.len() - 1));
        } else if list_state.selected().is_none() {
            list_state.select(Some(0));
        }

        terminal.draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Length(5), // Search input box
                    Constraint::Min(5),    // User list
                    Constraint::Length(2), // Help footer
                ])
                .split(size);

            // 1. Title
            let title_span = Span::styled(
                "⚡ SELECT A USER TO TRACK ⚡",
                crate::ui::styles::style_title(),
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![title_span]))
                    .alignment(Alignment::Center),
                chunks[0],
            );

            // 2. Search Box
            let search_block = Block::default()
                .borders(Borders::ALL)
                .title(" Filter (Type to search, Backspace to delete) ")
                .border_style(crate::ui::styles::style_border_active())
                .padding(Padding::new(2, 2, 1, 1));

            f.render_widget(
                Paragraph::new(filter.as_str())
                    .block(search_block)
                    .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG)),
                chunks[1],
            );

            // Cursor position in search box
            let cursor_x = chunks[1].x + 3 + filter.chars().count() as u16;
            let cursor_y = chunks[1].y + 2;
            let safe_cursor_x = cursor_x.min(chunks[1].x + chunks[1].width.saturating_sub(3));
            let safe_cursor_y = cursor_y.min(chunks[1].y + chunks[1].height.saturating_sub(2));
            f.set_cursor_position(ratatui::layout::Position::new(safe_cursor_x, safe_cursor_y));

            // 3. User List
            let items: Vec<ListItem> = filtered_users
                .iter()
                .map(|u| {
                    ListItem::new(format!("  •  {} ({})", u.username, u.id))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                })
                .collect();

            let list_title = format!(" Workspace Users ({}) ", filtered_users.len());
            let menu_list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(list_title)
                        .border_style(crate::ui::styles::style_border_inactive()),
                )
                .highlight_style(crate::ui::styles::style_selected());

            f.render_stateful_widget(menu_list, chunks[2], &mut list_state);

            // 4. Help Footer
            let help_line = Line::from(vec![
                Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Navigate  |  ", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Confirm  |  ", Style::default().fg(crate::ui::styles::COLOR_FG)),
                Span::styled("Esc / Ctrl+C", Style::default().add_modifier(Modifier::BOLD).fg(crate::ui::styles::COLOR_PRIMARY)),
                Span::styled(" Cancel", Style::default().fg(crate::ui::styles::COLOR_FG)),
            ]);

            f.render_widget(
                Paragraph::new(help_line)
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::TOP)
                            .border_style(Style::default().fg(crate::ui::styles::COLOR_MUTED))
                    ),
                chunks[3],
            );
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => {
                            break;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break;
                        }
                        KeyCode::Up => {
                            let i = list_state.selected().unwrap_or(0);
                            if i > 0 {
                                list_state.select(Some(i - 1));
                            }
                        }
                        KeyCode::Down => {
                            let i = list_state.selected().unwrap_or(0);
                            if !filtered_users.is_empty() && i + 1 < filtered_users.len() {
                                list_state.select(Some(i + 1));
                            }
                        }
                        KeyCode::Backspace => {
                            filter.pop();
                        }
                        KeyCode::Char(c) => {
                            filter.push(c);
                        }
                        KeyCode::Enter => {
                            if let Some(idx) = list_state.selected() {
                                if idx < filtered_users.len() {
                                    selected_user = Some((*filtered_users[idx]).clone());
                                    break;
                                }
                            }
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

    Ok(selected_user)
}

fn escape_csv_field(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        let escaped = val.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        val.to_string()
    }
}
