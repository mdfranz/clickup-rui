use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Folder, List as ClickUpList, Status, User};
use crate::config::Config;
use crate::util::errors::{AppError, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, List as RatatuiList, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::io;

#[derive(PartialEq, Eq)]
enum NewTaskStep {
    FolderSelect,
    ListSelect,
    StatusSelect,
    NameInput,
    DescriptionInput,
    AssigneePrompt,
    AssigneeSelect,
    Confirm,
    Creating,
    Done,
}

pub async fn run_new_task<A: ClickUpApi>(api: &A) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_new_task_loop(api, &mut terminal).await;

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run_new_task_loop<A: ClickUpApi>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    let cfg = Config::load()?;
    let current_user = api.get_current_user().await?;

    let mut folders: Vec<Folder> = cfg
        .folders
        .iter()
        .map(|fc| Folder {
            id: fc.id.clone(),
            name: fc.name.clone(),
        })
        .collect();

    if folders.is_empty() {
        return Err(AppError::Other(
            "No folders configured. Run 'clickup-rui setup' first.".to_string(),
        ));
    }

    let mut folders_state = ListState::default();
    folders_state.select(Some(0));

    let mut step = NewTaskStep::FolderSelect;

    let mut lists: Vec<ClickUpList> = Vec::new();
    let mut lists_state = ListState::default();

    let mut statuses: Vec<Status> = Vec::new();
    let mut statuses_state = ListState::default();

    let mut name = String::new();
    let mut description = String::new();

    let mut assignee_choice: Option<User> = None;
    let mut workspace_users: Vec<User> = Vec::new();
    let mut users_state = ListState::default();

    let mut selected_folder: Option<Folder> = None;
    let mut selected_list: Option<ClickUpList> = None;
    let mut selected_status: Option<Status> = None;

    loop {
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3), // Progress/Title
                        Constraint::Min(5),    // Center Box
                        Constraint::Length(3), // Help / Instructions
                    ]
                    .as_ref(),
                )
                .split(size);

            let header_text = match step {
                NewTaskStep::FolderSelect => "Create Task: Step 1/7 - Select Folder",
                NewTaskStep::ListSelect => "Create Task: Step 2/7 - Select List",
                NewTaskStep::StatusSelect => "Create Task: Step 3/7 - Select Status",
                NewTaskStep::NameInput => "Create Task: Step 4/7 - Type Task Name",
                NewTaskStep::DescriptionInput => "Create Task: Step 5/7 - Type Description",
                NewTaskStep::AssigneePrompt => "Create Task: Step 6/7 - Assign to Yourself?",
                NewTaskStep::AssigneeSelect => "Create Task: Step 6/7 - Select Assignee",
                NewTaskStep::Confirm => "Create Task: Step 7/7 - Confirm Details",
                NewTaskStep::Creating => "Creating Task...",
                NewTaskStep::Done => "Task Successfully Created!",
            };
            f.render_widget(
                Paragraph::new(header_text)
                    .block(Block::default().borders(Borders::BOTTOM))
                    .style(crate::ui::styles::style_title()),
                chunks[0],
            );

            // Center Panel based on state
            match step {
                NewTaskStep::FolderSelect => {
                    let items: Vec<ListItem> = folders
                        .iter()
                        .map(|fol| ListItem::new(format!("  {}", fol.name)))
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Folders "))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut folders_state);
                }
                NewTaskStep::ListSelect => {
                    let items: Vec<ListItem> = lists
                        .iter()
                        .map(|l| ListItem::new(format!("  {}", l.name)))
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Lists "))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut lists_state);
                }
                NewTaskStep::StatusSelect => {
                    let items: Vec<ListItem> = statuses
                        .iter()
                        .map(|s| ListItem::new(format!("  {}", s.status)))
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Statuses "))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut statuses_state);
                }
                NewTaskStep::NameInput => {
                    let p = Paragraph::new(name.as_str()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(crate::ui::styles::style_border_active())
                            .title(" Enter Task Name "),
                    );
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::DescriptionInput => {
                    let p = Paragraph::new(description.as_str()).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(crate::ui::styles::style_border_active())
                            .title(" Enter Description "),
                    );
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::AssigneePrompt => {
                    let text = format!(
                        "Would you like to assign this task to yourself ({})?\n\nPress 'y' for Yes, 'n' to select another workspace user.",
                        current_user.username
                    );
                    let p = Paragraph::new(text).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Assignee Prompt "),
                    );
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::AssigneeSelect => {
                    let mut items = vec![ListItem::new("  Unassigned")];
                    for u in &workspace_users {
                        items.push(ListItem::new(format!("  {} ({})", u.username, u.email)));
                    }
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Users "))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut users_state);
                }
                NewTaskStep::Confirm => {
                    let f_name = selected_folder.as_ref().map(|f| f.name.as_str()).unwrap_or("");
                    let l_name = selected_list.as_ref().map(|l| l.name.as_str()).unwrap_or("");
                    let s_name = selected_status.as_ref().map(|s| s.status.as_str()).unwrap_or("");
                    let a_name = assignee_choice.as_ref().map(|u| u.username.as_str()).unwrap_or("Unassigned");

                    let summary_text = format!(
                        "Folder: {}\n\
                         List: {}\n\
                         Status: {}\n\
                         Name: {}\n\
                         Description: {}\n\
                         Assignee: {}\n\n\
                         Ready to create? [Press Enter to Confirm / 'n' edit name / 'd' edit description / 'a' edit assignee]",
                        f_name, l_name, s_name, name, description, a_name
                    );
                    let p = Paragraph::new(summary_text).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Confirmation "),
                    );
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::Creating => {
                    let p = Paragraph::new("Contacting ClickUp API... Please wait.")
                        .block(Block::default().borders(Borders::ALL).title(" Creating "));
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::Done => {
                    let text = "Task successfully created!\n\n\
                                Create another task in SAME list? [y]\n\
                                Restart from Folder selection? [s]\n\
                                Exit workflow? [n / Enter / Esc / q]";
                    let p = Paragraph::new(text).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Complete "),
                    );
                    f.render_widget(p, chunks[1]);
                }
            }

            // Help Block
            let help_text = match step {
                NewTaskStep::FolderSelect
                | NewTaskStep::ListSelect
                | NewTaskStep::StatusSelect
                | NewTaskStep::AssigneeSelect => {
                    "Arrow Up/Down or j/k: navigate | Enter: confirm selection | Esc: cancel"
                }
                NewTaskStep::NameInput | NewTaskStep::DescriptionInput => {
                    "Type normally | Enter: submit value | Esc: cancel"
                }
                NewTaskStep::AssigneePrompt => "Press 'y' or 'n' | Esc: cancel",
                NewTaskStep::Confirm => "Enter: Create Task | n/d/a: edit | Esc: cancel",
                NewTaskStep::Creating => "Processing...",
                NewTaskStep::Done => "y/s/n/q: choose action",
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
                    // Universal cancel key
                    if key.code == KeyCode::Esc && step != NewTaskStep::Done && step != NewTaskStep::Creating {
                        return Ok(());
                    }

                    match step {
                        NewTaskStep::FolderSelect => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = folders_state.selected().unwrap_or(0);
                                if i > 0 {
                                    folders_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = folders_state.selected().unwrap_or(0);
                                if i + 1 < folders.len() {
                                    folders_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Enter => {
                                let idx = folders_state.selected().unwrap_or(0);
                                let f_selected = folders[idx].clone();
                                selected_folder = Some(f_selected.clone());

                                lists = api.get_lists(&f_selected.id).await?;
                                if lists.is_empty() {
                                    continue;
                                }

                                // List auto-selection
                                if lists.len() == 1 {
                                    let l_selected = lists[0].clone();
                                    selected_list = Some(l_selected.clone());

                                    let l_detail = api.get_list_detail(&l_selected.id).await?;
                                    statuses = l_detail.statuses;
                                    statuses_state.select(Some(0));
                                    step = NewTaskStep::StatusSelect;
                                } else if let Some(found_list) = lists
                                    .iter()
                                    .find(|l| l.name.to_lowercase() == "list")
                                {
                                    let l_selected = found_list.clone();
                                    selected_list = Some(l_selected.clone());

                                    let l_detail = api.get_list_detail(&l_selected.id).await?;
                                    statuses = l_detail.statuses;
                                    statuses_state.select(Some(0));
                                    step = NewTaskStep::StatusSelect;
                                } else {
                                    lists_state.select(Some(0));
                                    step = NewTaskStep::ListSelect;
                                }
                            }
                            _ => {}
                        },
                        NewTaskStep::ListSelect => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = lists_state.selected().unwrap_or(0);
                                if i > 0 {
                                    lists_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = lists_state.selected().unwrap_or(0);
                                if i + 1 < lists.len() {
                                    lists_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Enter => {
                                let idx = lists_state.selected().unwrap_or(0);
                                let l_selected = lists[idx].clone();
                                selected_list = Some(l_selected.clone());

                                let l_detail = api.get_list_detail(&l_selected.id).await?;
                                statuses = l_detail.statuses;
                                statuses_state.select(Some(0));
                                step = NewTaskStep::StatusSelect;
                            }
                            _ => {}
                        },
                        NewTaskStep::StatusSelect => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = statuses_state.selected().unwrap_or(0);
                                if i > 0 {
                                    statuses_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = statuses_state.selected().unwrap_or(0);
                                if i + 1 < statuses.len() {
                                    statuses_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Enter => {
                                let idx = statuses_state.selected().unwrap_or(0);
                                selected_status = Some(statuses[idx].clone());
                                step = NewTaskStep::NameInput;
                            }
                            _ => {}
                        },
                        NewTaskStep::NameInput => match key.code {
                            KeyCode::Enter => {
                                if !name.trim().is_empty() {
                                    step = NewTaskStep::DescriptionInput;
                                }
                            }
                            KeyCode::Backspace => {
                                name.pop();
                            }
                            KeyCode::Char(c) => {
                                name.push(c);
                            }
                            _ => {}
                        },
                        NewTaskStep::DescriptionInput => match key.code {
                            KeyCode::Enter => {
                                step = NewTaskStep::AssigneePrompt;
                            }
                            KeyCode::Backspace => {
                                description.pop();
                            }
                            KeyCode::Char(c) => {
                                description.push(c);
                            }
                            _ => {}
                        },
                        NewTaskStep::AssigneePrompt => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                assignee_choice = Some(current_user.clone());
                                step = NewTaskStep::Confirm;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                if workspace_users.is_empty() {
                                    let teams = api.get_teams().await?;
                                    let mut uniq = std::collections::HashSet::new();
                                    for t in &teams {
                                        for m in &t.members {
                                            if uniq.insert(m.user.id) {
                                                workspace_users.push(m.user.clone());
                                            }
                                        }
                                    }
                                }
                                users_state.select(Some(0));
                                step = NewTaskStep::AssigneeSelect;
                            }
                            _ => {}
                        },
                        NewTaskStep::AssigneeSelect => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = users_state.selected().unwrap_or(0);
                                if i > 0 {
                                    users_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = users_state.selected().unwrap_or(0);
                                if i + 1 <= workspace_users.len() {
                                    users_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Enter => {
                                let idx = users_state.selected().unwrap_or(0);
                                if idx == 0 {
                                    assignee_choice = None;
                                } else {
                                    assignee_choice = Some(workspace_users[idx - 1].clone());
                                }
                                step = NewTaskStep::Confirm;
                            }
                            _ => {}
                        },
                        NewTaskStep::Confirm => match key.code {
                            KeyCode::Enter => {
                                step = NewTaskStep::Creating;
                                // Perform creation
                                terminal.draw(|f| {
                                    f.render_widget(
                                        Paragraph::new("Creating task...").block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        ),
                                        f.area(),
                                    );
                                })?;

                                let list_id = &selected_list.as_ref().unwrap().id;
                                let status_name = selected_status.as_ref().map(|s| s.status.as_str());
                                let assignees: Option<Vec<i64>> =
                                    assignee_choice.as_ref().map(|u| vec![u.id]);

                                let desc_opt = if description.is_empty() {
                                    None
                                } else {
                                    Some(description.as_str())
                                };

                                let create_res = api
                                    .create_task(
                                        list_id,
                                        &name,
                                        desc_opt,
                                        status_name,
                                        assignees.as_deref(),
                                    )
                                    .await;

                                match create_res {
                                    Ok(_) => {
                                        step = NewTaskStep::Done;
                                    }
                                    Err(e) => {
                                        return Err(e);
                                    }
                                }
                            }
                            KeyCode::Char('n') => {
                                step = NewTaskStep::NameInput;
                            }
                            KeyCode::Char('d') => {
                                step = NewTaskStep::DescriptionInput;
                            }
                            KeyCode::Char('a') => {
                                step = NewTaskStep::AssigneePrompt;
                            }
                            _ => {}
                        },
                        NewTaskStep::Creating => {}
                        NewTaskStep::Done => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                name.clear();
                                description.clear();
                                step = NewTaskStep::StatusSelect;
                            }
                            KeyCode::Char('s') | KeyCode::Char('S') => {
                                name.clear();
                                description.clear();
                                step = NewTaskStep::FolderSelect;
                            }
                            _ => return Ok(()),
                        },
                    }
                }
            }
        }
    }
}
