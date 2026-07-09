use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Folder, List as ClickUpList, Status, Tag, User};
use crate::config::Config;
use crate::util::errors::{AppError, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List as RatatuiList, ListItem, ListState, Padding, Paragraph};
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
    TagSelect,
    Confirm,
    Creating,
    Done,
}

pub async fn run_new_task<A: ClickUpApi>(api: &A) -> Result<()> {
    let mut guard = crate::ui::terminal::TerminalGuard::create()?;
    run_new_task_loop(api, guard.inner()).await
}


#[allow(unused_assignments)]
async fn run_new_task_loop<A: ClickUpApi>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    let cfg = Config::load()?;
    let current_user = api.get_current_user().await?;

    let folders: Vec<Folder> = cfg
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

    let mut space_tags: Vec<Tag> = Vec::new();
    let mut tags_state = ListState::default();
    let mut selected_tags: Vec<String> = Vec::new();

    let mut selected_folder: Option<Folder> = None;
    let mut selected_list: Option<ClickUpList> = None;
    let mut selected_status: Option<Status> = None;
    let mut assignee_filter = String::new();

    loop {
        let filtered_users: Vec<&User> = if step == NewTaskStep::AssigneeSelect {
            workspace_users
                .iter()
                .filter(|u| {
                    if assignee_filter.is_empty() {
                        true
                    } else {
                        u.username.to_lowercase().contains(&assignee_filter.to_lowercase())
                            || u.email.to_lowercase().contains(&assignee_filter.to_lowercase())
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        if step == NewTaskStep::AssigneeSelect {
            let current_selected = users_state.selected().unwrap_or(0);
            if current_selected >= filtered_users.len() {
                users_state.select(Some(filtered_users.len()));
            }
        }

        terminal.draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);
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
                NewTaskStep::FolderSelect => "Create Task: Step 1/8 - Select Folder",
                NewTaskStep::ListSelect => "Create Task: Step 2/8 - Select List",
                NewTaskStep::StatusSelect => "Create Task: Step 3/8 - Select Status",
                NewTaskStep::NameInput => "Create Task: Step 4/8 - Type Task Name",
                NewTaskStep::DescriptionInput => "Create Task: Step 5/8 - Type Description",
                NewTaskStep::AssigneePrompt => "Create Task: Step 6/8 - Assign to Yourself?",
                NewTaskStep::AssigneeSelect => "Create Task: Step 6/8 - Select Assignee",
                NewTaskStep::TagSelect => "Create Task: Step 7/8 - Select Tags",
                NewTaskStep::Confirm => "Create Task: Step 8/8 - Confirm Details",
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
                        .map(|fol| {
                            ListItem::new(format!("  {}", fol.name))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        })
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Folders "))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut folders_state);
                }
                NewTaskStep::ListSelect => {
                    let items: Vec<ListItem> = lists
                        .iter()
                        .map(|l| {
                            ListItem::new(format!("  {}", l.name))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        })
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Lists "))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut lists_state);
                }
                NewTaskStep::StatusSelect => {
                    let items: Vec<ListItem> = statuses
                        .iter()
                        .map(|s| {
                            ListItem::new(format!("  {}", s.status))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        })
                        .collect();
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(" Statuses "))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut statuses_state);
                }
                NewTaskStep::NameInput => {
                    let inner_width = (chunks[1].width as usize).saturating_sub(6);
                    let wrapped_lines = wrap_text_by_chars(name.as_str(), inner_width);

                    let paragraph_lines: Vec<Line> = wrapped_lines
                        .iter()
                        .map(|l| Line::from(l.as_str()))
                        .collect();

                    let p = Paragraph::new(paragraph_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(crate::ui::styles::style_border_active())
                                .title(" Enter Task Name ")
                                .padding(Padding::new(2, 2, 1, 1)),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                    f.render_widget(p, chunks[1]);

                    // Dynamic cursor placement tracking current length and wrapped lines
                    let cursor_row = wrapped_lines.len().saturating_sub(1) as u16;
                    let cursor_col = wrapped_lines.last().map(|l| l.chars().count()).unwrap_or(0) as u16;

                    let cursor_y = chunks[1].y + 2 + cursor_row;
                    let cursor_x = chunks[1].x + 3 + cursor_col;

                    let safe_cursor_x = cursor_x.min(chunks[1].x + chunks[1].width.saturating_sub(2));
                    let safe_cursor_y = cursor_y.min(chunks[1].y + chunks[1].height.saturating_sub(2));

                    f.set_cursor_position(ratatui::layout::Position::new(safe_cursor_x, safe_cursor_y));
                }
                NewTaskStep::DescriptionInput => {
                    let inner_width = (chunks[1].width as usize).saturating_sub(6);
                    let wrapped_lines = wrap_text_by_chars(description.as_str(), inner_width);

                    let paragraph_lines: Vec<Line> = wrapped_lines
                        .iter()
                        .map(|l| Line::from(l.as_str()))
                        .collect();

                    let p = Paragraph::new(paragraph_lines)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(crate::ui::styles::style_border_active())
                                .title(" Enter Description ")
                                .padding(Padding::new(2, 2, 1, 1)),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                    f.render_widget(p, chunks[1]);

                    // Dynamic cursor placement tracking current length and wrapped lines
                    let cursor_row = wrapped_lines.len().saturating_sub(1) as u16;
                    let cursor_col = wrapped_lines.last().map(|l| l.chars().count()).unwrap_or(0) as u16;

                    let cursor_y = chunks[1].y + 2 + cursor_row;
                    let cursor_x = chunks[1].x + 3 + cursor_col;

                    let safe_cursor_x = cursor_x.min(chunks[1].x + chunks[1].width.saturating_sub(2));
                    let safe_cursor_y = cursor_y.min(chunks[1].y + chunks[1].height.saturating_sub(2));

                    f.set_cursor_position(ratatui::layout::Position::new(safe_cursor_x, safe_cursor_y));
                }
                NewTaskStep::AssigneePrompt => {
                    let text = format!(
                        "Would you like to assign this task to yourself ({})?\n\nPress 'y' for Yes, 'n' to select another workspace user.",
                        current_user.username
                    );
                    let p = Paragraph::new(text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Assignee Prompt "),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::AssigneeSelect => {
                    let assignee_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(3), // Filter input box
                            Constraint::Min(3),    // Users list
                        ])
                        .split(chunks[1]);

                    let search_block = Block::default()
                        .borders(Borders::ALL)
                        .title(" Filter (Type to search, Backspace to delete) ")
                        .border_style(crate::ui::styles::style_border_active())
                        .padding(Padding::new(2, 2, 0, 0));

                    f.render_widget(
                        Paragraph::new(assignee_filter.as_str())
                            .block(search_block)
                            .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG)),
                        assignee_chunks[0],
                    );

                    // Dynamic cursor placement matching user's typing
                    let cursor_x = assignee_chunks[0].x + 3 + assignee_filter.chars().count() as u16;
                    let cursor_y = assignee_chunks[0].y + 1;
                    let safe_cursor_x = cursor_x.min(assignee_chunks[0].x + assignee_chunks[0].width.saturating_sub(3));
                    let safe_cursor_y = cursor_y.min(assignee_chunks[0].y + assignee_chunks[0].height.saturating_sub(2));
                    f.set_cursor_position(ratatui::layout::Position::new(safe_cursor_x, safe_cursor_y));

                    let mut items = vec![
                        ListItem::new("  Unassigned")
                            .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                    ];
                    for u in &filtered_users {
                        items.push(
                            ListItem::new(format!("  {} ({})", u.username, u.email))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        );
                    }
                    let list_title = format!(" Users ({}) ", filtered_users.len());
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(list_title))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, assignee_chunks[1], &mut users_state);
                }
                NewTaskStep::TagSelect => {
                    let items: Vec<ListItem> = space_tags
                        .iter()
                        .map(|tag| {
                            let checked = if selected_tags.contains(&tag.name) { "[x]" } else { "[ ]" };
                            ListItem::new(Line::from(vec![
                                Span::styled(
                                    format!("  {} ", checked),
                                    Style::default().fg(crate::ui::styles::COLOR_MUTED),
                                ),
                                Span::styled(
                                    tag.name.clone(),
                                    if selected_tags.contains(&tag.name) {
                                        Style::default().fg(crate::ui::styles::COLOR_PRIMARY).add_modifier(Modifier::BOLD)
                                    } else {
                                        Style::default().fg(crate::ui::styles::COLOR_FG)
                                    },
                                ),
                            ]))
                        })
                        .collect();
                    let list_title = if selected_tags.is_empty() {
                        " Tags (none selected) ".to_string()
                    } else {
                        format!(" Tags ({} selected) ", selected_tags.len())
                    };
                    let list = RatatuiList::new(items)
                        .block(Block::default().borders(Borders::ALL).title(list_title))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut tags_state);
                }
                NewTaskStep::Confirm => {
                    let f_name = selected_folder.as_ref().map(|f| f.name.as_str()).unwrap_or("");
                    let l_name = selected_list.as_ref().map(|l| l.name.as_str()).unwrap_or("");
                    let s_name = selected_status.as_ref().map(|s| s.status.as_str()).unwrap_or("");
                    let a_name = assignee_choice.as_ref().map(|u| u.username.as_str()).unwrap_or("Unassigned");
                    let tags_display = if selected_tags.is_empty() {
                        "None".to_string()
                    } else {
                        selected_tags.join(", ")
                    };

                    let summary_text = format!(
                        "Folder: {}\n\
                         List: {}\n\
                         Status: {}\n\
                         Name: {}\n\
                         Description: {}\n\
                         Assignee: {}\n\
                         Tags: {}\n\n\
                         Ready to create? [Press Enter to Confirm / 'n' edit name / 'd' edit description / 'a' edit assignee / 't' edit tags]",
                        f_name, l_name, s_name, name, description, a_name, tags_display
                    );
                    let p = Paragraph::new(summary_text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Confirmation "),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::Creating => {
                    let p = Paragraph::new("Contacting ClickUp API... Please wait.")
                        .block(Block::default().borders(Borders::ALL).title(" Creating "))
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                    f.render_widget(p, chunks[1]);
                }
                NewTaskStep::Done => {
                    let text = "Task successfully created!\n\n\
                                Create another task in SAME list? [y]\n\
                                Restart from Folder selection? [s]\n\
                                Exit workflow? [n / Enter / Esc / q]";
                    let p = Paragraph::new(text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Complete "),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG));
                    f.render_widget(p, chunks[1]);
                }
            }

            // Help Block
            let help_text = match step {
                NewTaskStep::FolderSelect
                | NewTaskStep::ListSelect
                | NewTaskStep::StatusSelect => {
                    "Arrow Up/Down or j/k: navigate | Enter: confirm selection | Esc: cancel"
                }
                NewTaskStep::AssigneeSelect => {
                    "Arrow Up/Down: navigate | Type to filter | Enter: confirm selection | Esc: cancel"
                }
                NewTaskStep::TagSelect => {
                    "Arrow Up/Down or j/k: navigate | Space: toggle tag | Enter: confirm | Esc: cancel"
                }
                NewTaskStep::NameInput | NewTaskStep::DescriptionInput => {
                    "Type normally | Enter: submit value | Esc: cancel"
                }
                NewTaskStep::AssigneePrompt => "Press 'y' or 'n' | Esc: cancel",
                NewTaskStep::Confirm => "Enter: Create Task | n/d/a/t: edit | Esc: cancel",
                NewTaskStep::Creating => "Processing...",
                NewTaskStep::Done => "y/s/n/q: choose action",
            };
            f.render_widget(
                Paragraph::new(help_text)
                    .block(Block::default().borders(Borders::TOP))
                    .style(Style::default().fg(crate::ui::styles::COLOR_MUTED).bg(crate::ui::styles::COLOR_BG)),
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
                                if space_tags.is_empty() {
                                    let cfg = Config::load()?;
                                    space_tags = api.get_space_tags(&cfg.space_id).await.unwrap_or_default();
                                }
                                tags_state.select(Some(0));
                                step = NewTaskStep::TagSelect;
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
                                assignee_filter.clear();
                                users_state.select(Some(0));
                                step = NewTaskStep::AssigneeSelect;
                            }
                            _ => {}
                        },
                        NewTaskStep::AssigneeSelect => match key.code {
                            KeyCode::Up => {
                                let i = users_state.selected().unwrap_or(0);
                                if i > 0 {
                                    users_state.select(Some(i - 1));
                                }
                            }
                            KeyCode::Down => {
                                let i = users_state.selected().unwrap_or(0);
                                if i < filtered_users.len() {
                                    users_state.select(Some(i + 1));
                                }
                            }
                            KeyCode::Backspace => {
                                assignee_filter.pop();
                            }
                            KeyCode::Char(c) => {
                                assignee_filter.push(c);
                            }
                            KeyCode::Enter => {
                                let idx = users_state.selected().unwrap_or(0);
                                if idx == 0 {
                                    assignee_choice = None;
                                } else if idx <= filtered_users.len() {
                                    assignee_choice = Some((*filtered_users[idx - 1]).clone());
                                }
                                if space_tags.is_empty() {
                                    let cfg = Config::load()?;
                                    space_tags = api.get_space_tags(&cfg.space_id).await.unwrap_or_default();
                                }
                                tags_state.select(Some(0));
                                step = NewTaskStep::TagSelect;
                            }
                            _ => {}
                        },
                        NewTaskStep::TagSelect => match key.code {
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
                                    if let Some(tag) = space_tags.get(idx) {
                                        let name = tag.name.clone();
                                        if let Some(pos) = selected_tags.iter().position(|t| t == &name) {
                                            selected_tags.remove(pos);
                                        } else {
                                            selected_tags.push(name);
                                        }
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                step = NewTaskStep::Confirm;
                            }
                            _ => {}
                        },
                        NewTaskStep::Confirm => match key.code {
                            KeyCode::Enter => {
                                step = NewTaskStep::Creating;
                                // Perform creation
                                terminal.draw(|f| {
                                    crate::ui::styles::render_background(f);
                                f.render_widget(
                                    Paragraph::new("Creating task...")
                                        .block(
                                            Block::default().borders(Borders::ALL).title(" Please Wait "),
                                        )
                                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG)),
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

                                let tags_opt: Option<Vec<String>> = if selected_tags.is_empty() {
                                    None
                                } else {
                                    Some(selected_tags.clone())
                                };

                                let create_res = api
                                    .create_task(
                                        list_id,
                                        &name,
                                        desc_opt,
                                        status_name,
                                        assignees.as_deref(),
                                        tags_opt.as_deref(),
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
                            KeyCode::Char('t') => {
                                step = NewTaskStep::TagSelect;
                            }
                            _ => {}
                        },
                        NewTaskStep::Creating => {}
                        NewTaskStep::Done => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                name.clear();
                                description.clear();
                                selected_tags.clear();
                                step = NewTaskStep::StatusSelect;
                            }
                            KeyCode::Char('s') | KeyCode::Char('S') => {
                                name.clear();
                                description.clear();
                                selected_tags.clear();
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

fn wrap_text_by_chars(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for line in text.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            result.push(String::new());
        } else {
            for chunk in chars.chunks(width) {
                result.push(chunk.iter().collect::<String>());
            }
        }
    }
    result
}
