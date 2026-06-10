use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Status, Task};
use crate::config::Config;
use crate::util::errors::{AppError, Result};
use crate::util::filter::should_include_task;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, List as RatatuiList, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::collections::HashSet;
use std::io;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StandupStep {
    SelectTasks,
    TaskReport,
    StatusPicker,
    Done,
}

struct StandupReport {
    task: Task,
    comment: String,
    new_status: Option<Status>,
    original_status: String,
    skipped: bool,
    posted_comment: bool,
    posted_status: bool,
}

pub async fn run_standup<A: ClickUpApi>(api: &A, all_flag: bool, mine_only: bool) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_standup_loop(api, &mut terminal, all_flag, mine_only).await;

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run_standup_loop<A: ClickUpApi>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    all_flag: bool,
    mine_only: bool,
) -> Result<()> {
    let cfg = Config::load()?;
    let user = api.get_current_user().await?;

    // 1. Gather all filtered tasks
    let mut tasks = Vec::new();
    for folder in &cfg.folders {
        let lists = match api.get_lists(&folder.id).await {
            Ok(l) => l,
            Err(_) => continue,
        };
        for list in &lists {
            if let Ok(t_list) = api.get_tasks(&list.id, all_flag).await {
                for task in t_list {
                    if should_include_task(&task, user.id, all_flag, mine_only) {
                        tasks.push(task);
                    }
                }
            }
        }
    }

    if tasks.is_empty() {
        return Err(AppError::Other(
            "No active tasks found to report on.".to_string(),
        ));
    }

    let mut select_state = ListState::default();
    select_state.select(Some(0));

    let mut selected_task_ids: HashSet<String> = HashSet::new();
    let mut step = StandupStep::SelectTasks;

    let mut reports: Vec<StandupReport> = Vec::new();
    let mut current_report_idx = 0;

    let mut list_statuses: Vec<Status> = Vec::new();
    let mut status_state = ListState::default();

    loop {
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3), // Header
                        Constraint::Min(5),    // Main Panel
                        Constraint::Length(3), // Help footer
                    ]
                    .as_ref(),
                )
                .split(size);

            let header_text = match step {
                StandupStep::SelectTasks => "Daily Standup: Step 1/3 - Select Tasks to Update",
                StandupStep::TaskReport => "Daily Standup: Step 2/3 - Type Update & Set Status",
                StandupStep::StatusPicker => "Daily Standup: Select Status Status Picker",
                StandupStep::Done => "Daily Standup: Step 3/3 - Done Summary",
            };
            f.render_widget(
                Paragraph::new(header_text)
                    .block(Block::default().borders(Borders::BOTTOM))
                    .style(crate::ui::styles::style_title()),
                chunks[0],
            );

            match step {
                StandupStep::SelectTasks => {
                    let items: Vec<ListItem> = tasks
                        .iter()
                        .map(|t| {
                            let checked = if selected_task_ids.contains(&t.id) {
                                "[x]"
                            } else {
                                "[ ]"
                            };
                            ListItem::new(format!("  {} [{}] {}", checked, t.status.status, t.name))
                        })
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Your Active Tasks "))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut select_state);
                }
                StandupStep::TaskReport => {
                    let rep = &reports[current_report_idx];
                    let current_status_name = rep
                        .new_status
                        .as_ref()
                        .map(|s| s.status.as_str())
                        .unwrap_or(rep.original_status.as_str());

                    let info_text = format!(
                        "Task {}/{}: {}\nCurrent Status: {}\nNew Status: {}\n\nType comment below:",
                        current_report_idx + 1,
                        reports.len(),
                        rep.task.name,
                        rep.original_status,
                        current_status_name
                    );

                    let main_layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(6), Constraint::Min(3)].as_ref())
                        .split(chunks[1]);

                    f.render_widget(Paragraph::new(info_text), main_layout[0]);

                    let p_comment = Paragraph::new(rep.comment.as_str()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(crate::ui::styles::style_border_active())
                            .title(" Comment Textarea "),
                    );
                    f.render_widget(p_comment, main_layout[1]);
                }
                StandupStep::StatusPicker => {
                    let items: Vec<ListItem> = list_statuses
                        .iter()
                        .map(|s| ListItem::new(format!("  {}", s.status)))
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Choose Status "))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut status_state);
                }
                StandupStep::Done => {
                    let mut summary = String::new();
                    for rep in &reports {
                        if rep.skipped {
                            summary.push_str(&format!("- {}: Skipped\n", rep.task.name));
                        } else {
                            summary.push_str(&format!("- {}:\n", rep.task.name));
                            if rep.posted_comment {
                                summary.push_str(&format!("    * Added comment: \"{}\"\n", rep.comment));
                            }
                            if rep.posted_status {
                                let ns = rep.new_status.as_ref().map(|s| s.status.as_str()).unwrap_or("");
                                summary.push_str(&format!("    * Changed status: {} -> {}\n", rep.original_status, ns));
                            }
                            if !rep.posted_comment && !rep.posted_status {
                                summary.push_str("    * No updates posted.\n");
                            }
                        }
                    }
                    let p = Paragraph::new(summary).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Summary Results "),
                    );
                    f.render_widget(p, chunks[1]);
                }
            }

            // Footer
            let help_text = match step {
                StandupStep::SelectTasks => {
                    "Arrow Up/Down or j/k: navigate | Space: toggle | a: toggle all | Enter: confirm selection | Esc: cancel"
                }
                StandupStep::TaskReport => {
                    "Type normally | Tab: status picker | Ctrl+s: submit report | Esc: skip task"
                }
                StandupStep::StatusPicker => {
                    "Arrow Up/Down or j/k: navigate | Enter: select status | Esc: cancel"
                }
                StandupStep::Done => "Press any key or Esc/q to exit",
            };
            f.render_widget(
                Paragraph::new(help_text)
                    .block(Block::default().borders(Borders::TOP))
                    .style(ratatui::style::Style::default().fg(crate::ui::styles::COLOR_MUTED)),
                chunks[2],
            );
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Esc && step != StandupStep::Done {
                        if let StandupStep::StatusPicker = step {
                            step = StandupStep::TaskReport;
                            continue;
                        }
                        return Ok(());
                    }

                    match step {
                        StandupStep::SelectTasks => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = select_state.selected().unwrap_or(0);
                                if i > 0 {
                                    select_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = select_state.selected().unwrap_or(0);
                                if i + 1 < tasks.len() {
                                    select_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Char(' ') => {
                                if let Some(i) = select_state.selected() {
                                    let id = tasks[i].id.clone();
                                    if selected_task_ids.contains(&id) {
                                        selected_task_ids.remove(&id);
                                    } else {
                                        selected_task_ids.insert(id);
                                    }
                                }
                            }
                            KeyCode::Char('a') | KeyCode::Char('A') => {
                                if selected_task_ids.len() == tasks.len() {
                                    selected_task_ids.clear();
                                } else {
                                    selected_task_ids =
                                        tasks.iter().map(|t| t.id.clone()).collect();
                                }
                            }
                            KeyCode::Enter => {
                                if selected_task_ids.is_empty() {
                                    return Ok(());
                                }
                                reports = tasks
                                    .iter()
                                    .filter(|t| selected_task_ids.contains(&t.id))
                                    .map(|t| StandupReport {
                                        task: t.clone(),
                                        comment: String::new(),
                                        new_status: None,
                                        original_status: t.status.status.clone(),
                                        skipped: false,
                                        posted_comment: false,
                                        posted_status: false,
                                    })
                                    .collect();
                                current_report_idx = 0;
                                step = StandupStep::TaskReport;
                            }
                            _ => {}
                        },
                        StandupStep::TaskReport => {
                            if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Submit current report and move to next
                                let rep = &mut reports[current_report_idx];
                                let status_changed = rep.new_status.is_some()
                                    && rep.new_status.as_ref().unwrap().status.to_lowercase()
                                        != rep.original_status.to_lowercase();

                                if !rep.comment.trim().is_empty() || status_changed {
                                    terminal.draw(|f| {
                                        f.render_widget(
                                            Paragraph::new("Submitting updates to ClickUp...").block(
                                                Block::default().borders(Borders::ALL).title(" Posting "),
                                            ),
                                            f.area(),
                                        );
                                    })?;

                                    if !rep.comment.trim().is_empty() {
                                        if api
                                            .create_task_comment(&rep.task.id, &rep.comment)
                                            .await
                                            .is_ok()
                                        {
                                            rep.posted_comment = true;
                                        }
                                    }

                                    if status_changed {
                                        let ns = rep.new_status.as_ref().unwrap();
                                        if api
                                            .update_task_status(&rep.task.id, &ns.status)
                                            .await
                                            .is_ok()
                                        {
                                            rep.posted_status = true;
                                        }
                                    }
                                }

                                if current_report_idx + 1 < reports.len() {
                                    current_report_idx += 1;
                                } else {
                                    step = StandupStep::Done;
                                }
                            } else if key.code == KeyCode::Tab {
                                // Fetch statuses for picker
                                let rep = &reports[current_report_idx];
                                terminal.draw(|f| {
                                    f.render_widget(
                                        Paragraph::new("Loading status list...").block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        ),
                                        f.area(),
                                    );
                                })?;

                                // Try to fetch list detailed details for statuses
                                // We need list_id. ClickUp tasks carry parent list ID inside task structure, but wait: task may not have direct list id. Actually ClickUp Task structure has lists or list ID? Wait, yes, tasks can be queried by list, or we can use task status list. RUST-PORT.md says: `GetList(list_id)` to list statuses.
                                // But if list ID isn't directly cached, we can fallback to standard status. Let's try to query list details or just use task's status options if we can, but since ClickUp lists contain statuses, we can fetch list detail! Wait, where is `list_id` in task? ClickUp Task JSON has a `list` object with `id`!
                                // Wait, let's verify if we can deserialize list.id in Task. Yes, ClickUp Task JSON carries list: { id: "..." } but we normalized Task structure without list field in 5.2. Wait! Let's check: can we query task detail? Yes, task detail has statuses, or we can just fetch the list if we know its ID, or let's assume lists statuses are available, or let's provide a list of basic statuses. Let's fetch list detail if we have it, or fallback.
                                // Actually, in models we didn't specify `list` field on Task. But wait, `statuses` from `GetList(list_id)` can be fetched or we can use status picker with default statuses: "Open", "To Do", "In Progress", "In Review", "Completed", "Closed".
                                // Let's try to parse list ID from task ID or standard ClickUp lists, but wait, since CachedClient caches list details, let's use list detail from the cached folders! Let's search configured folders for the list that contains this task:
                                let mut found_statuses = Vec::new();
                                if api.get_task_detail(&rep.task.id).await.is_ok() {
                                    // task detail might have statuses! Or let's assume we can fetch list detailed.
                                    // Wait, let's check if we can list statuses. Let's search configured folders:
                                    'outer: for folder in &cfg.folders {
                                        if let Ok(lists) = api.get_lists(&folder.id).await {
                                            for l in lists {
                                                if let Ok(tasks_in_list) = api.get_tasks(&l.id, true).await {
                                                    if tasks_in_list.iter().any(|t| t.id == rep.task.id) {
                                                        if let Ok(ld) = api.get_list_detail(&l.id).await {
                                                            found_statuses = ld.statuses;
                                                            break 'outer;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if found_statuses.is_empty() {
                                    // Fallback statuses
                                    found_statuses = vec![
                                        Status { status: "To Do".to_string(), color: String::new(), type_: "todo".to_string() },
                                        Status { status: "In Progress".to_string(), color: String::new(), type_: "custom".to_string() },
                                        Status { status: "In Review".to_string(), color: String::new(), type_: "custom".to_string() },
                                        Status { status: "Blocked".to_string(), color: String::new(), type_: "custom".to_string() },
                                        Status { status: "Complete".to_string(), color: String::new(), type_: "closed".to_string() },
                                    ];
                                }

                                list_statuses = found_statuses;
                                status_state.select(Some(0));
                                step = StandupStep::StatusPicker;
                            } else if key.code == KeyCode::Esc {
                                // Skip current task and move to next
                                reports[current_report_idx].skipped = true;
                                if current_report_idx + 1 < reports.len() {
                                    current_report_idx += 1;
                                } else {
                                    step = StandupStep::Done;
                                }
                            } else {
                                // Type normally into comments textarea
                                let rep = &mut reports[current_report_idx];
                                match key.code {
                                    KeyCode::Char(c) => {
                                        rep.comment.push(c);
                                    }
                                    KeyCode::Backspace => {
                                        rep.comment.pop();
                                    }
                                    KeyCode::Enter => {
                                        rep.comment.push('\n');
                                    }
                                    _ => {}
                                }
                            }
                        }
                        StandupStep::StatusPicker => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = status_state.selected().unwrap_or(0);
                                if i > 0 {
                                    status_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = status_state.selected().unwrap_or(0);
                                if i + 1 < list_statuses.len() {
                                    status_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Enter => {
                                let idx = status_state.selected().unwrap_or(0);
                                reports[current_report_idx].new_status = Some(list_statuses[idx].clone());
                                step = StandupStep::TaskReport;
                            }
                            _ => {}
                        },
                        StandupStep::Done => {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}
