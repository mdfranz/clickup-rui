use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Folder, Space, Team};
use crate::config::{Config, FolderConfig};
use crate::util::errors::{AppError, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::collections::HashSet;
use std::io;

enum SetupStep {
    SelectWorkspace,
    SelectSpace,
    SelectFolders,
}

pub async fn run_setup<A: ClickUpApi>(api: &A) -> Result<()> {
    let mut guard = crate::ui::terminal::TerminalGuard::create()?;
    let res = run_setup_loop(api, guard.inner()).await;
    drop(guard);

    match res {
        Ok(summary) => {
            if !summary.is_empty() {
                println!("{}", summary);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}


async fn run_setup_loop<A: ClickUpApi>(
    api: &A,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<String> {
    let mut step = SetupStep::SelectWorkspace;

    // Data lists
    let teams = api.get_teams().await?;
    if teams.is_empty() {
        return Err(AppError::Other("No workspaces (teams) found in your ClickUp account.".to_string()));
    }

    let mut workspaces_state = ListState::default();
    workspaces_state.select(Some(0));

    let mut spaces: Vec<Space> = Vec::new();
    let mut spaces_state = ListState::default();

    let mut folders: Vec<Folder> = Vec::new();
    let mut folders_state = ListState::default();
    let mut selected_folder_ids: HashSet<String> = HashSet::new();

    // Selections
    let mut selected_workspace: Option<Team> = None;
    let mut selected_space: Option<Space> = None;

    loop {
        terminal.draw(|f| {
            let size = f.area();
            crate::ui::styles::render_background(f);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3), // Header
                        Constraint::Min(5),    // Main List
                        Constraint::Length(3), // Help block
                    ]
                    .as_ref(),
                )
                .split(size);

            // 1. Header
            let header_text = match step {
                SetupStep::SelectWorkspace => "Step 1/3: Select ClickUp Workspace (Team)",
                SetupStep::SelectSpace => "Step 2/3: Select ClickUp Space",
                SetupStep::SelectFolders => "Step 3/3: Multi-select Folders to Track",
            };
            let header = Paragraph::new(header_text)
                .block(Block::default().borders(Borders::BOTTOM))
                .style(crate::ui::styles::style_title());
            f.render_widget(header, chunks[0]);

            // 2. Main content list
            match step {
                SetupStep::SelectWorkspace => {
                    let items: Vec<ListItem> = teams
                        .iter()
                        .map(|t| {
                            ListItem::new(format!("  {} (ID: {})", t.name, t.id))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        })
                        .collect();
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(crate::ui::styles::style_border_active())
                                .title(" Workspaces "),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut workspaces_state);
                }
                SetupStep::SelectSpace => {
                    let items: Vec<ListItem> = spaces
                        .iter()
                        .map(|s| {
                            ListItem::new(format!("  {} (ID: {})", s.name, s.id))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        })
                        .collect();
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(crate::ui::styles::style_border_active())
                                .title(" Spaces "),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut spaces_state);
                }
                SetupStep::SelectFolders => {
                    let items: Vec<ListItem> = folders
                        .iter()
                        .map(|fol| {
                            let checked = if selected_folder_ids.contains(&fol.id) {
                                "[x]"
                            } else {
                                "[ ]"
                            };
                            ListItem::new(format!("  {} {} (ID: {})", checked, fol.name, fol.id))
                                .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        })
                        .collect();
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(crate::ui::styles::style_border_active())
                                .title(" Folders "),
                        )
                        .style(Style::default().fg(crate::ui::styles::COLOR_FG).bg(crate::ui::styles::COLOR_BG))
                        .highlight_style(crate::ui::styles::style_selected());
                    f.render_stateful_widget(list, chunks[1], &mut folders_state);
                }
            }

            // 3. Help block
            let help_text = match step {
                SetupStep::SelectWorkspace | SetupStep::SelectSpace => {
                    "Arrow Up/Down or j/k: navigate | Enter: confirm | q/ctrl+c: quit"
                }
                SetupStep::SelectFolders => {
                    "Arrow Up/Down or j/k: navigate | Space: toggle folder | Enter: confirm & save | q/ctrl+c: quit"
                }
            };
            let help = Paragraph::new(help_text)
                .block(Block::default().borders(Borders::TOP))
                .style(Style::default().fg(crate::ui::styles::COLOR_MUTED).bg(crate::ui::styles::COLOR_BG));
            f.render_widget(help, chunks[2]);
        })?;

        // Event handling
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            return Ok(String::new());
                        }
                        KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            return Ok(String::new());
                        }
                        KeyCode::Up | KeyCode::Char('k') => match step {
                            SetupStep::SelectWorkspace => {
                                let i = workspaces_state.selected().unwrap_or(0);
                                if i > 0 {
                                    workspaces_state.select(Some(i - 1));
                                }
                            }
                            SetupStep::SelectSpace => {
                                let i = spaces_state.selected().unwrap_or(0);
                                if i > 0 {
                                    spaces_state.select(Some(i - 1));
                                }
                            }
                            SetupStep::SelectFolders => {
                                let i = folders_state.selected().unwrap_or(0);
                                if i > 0 {
                                    folders_state.select(Some(i - 1));
                                }
                            }
                        },
                        KeyCode::Down | KeyCode::Char('j') => match step {
                            SetupStep::SelectWorkspace => {
                                let i = workspaces_state.selected().unwrap_or(0);
                                if i + 1 < teams.len() {
                                    workspaces_state.select(Some(i + 1));
                                }
                            }
                            SetupStep::SelectSpace => {
                                let i = spaces_state.selected().unwrap_or(0);
                                if i + 1 < spaces.len() {
                                    spaces_state.select(Some(i + 1));
                                }
                            }
                            SetupStep::SelectFolders => {
                                let i = folders_state.selected().unwrap_or(0);
                                if i + 1 < folders.len() {
                                    folders_state.select(Some(i + 1));
                                }
                            }
                        },
                        KeyCode::Char(' ') => {
                            if let SetupStep::SelectFolders = step {
                                if let Some(i) = folders_state.selected() {
                                    let id = folders[i].id.clone();
                                    if selected_folder_ids.contains(&id) {
                                        selected_folder_ids.remove(&id);
                                    } else {
                                        selected_folder_ids.insert(id);
                                    }
                                }
                            }
                        }
                        KeyCode::Enter => match step {
                            SetupStep::SelectWorkspace => {
                                let idx = workspaces_state.selected().unwrap_or(0);
                                let ws = teams[idx].clone();
                                selected_workspace = Some(ws.clone());

                                spaces = api.get_spaces(&ws.id).await?;
                                if spaces.is_empty() {
                                    return Err(AppError::Other(format!(
                                        "No spaces found in workspace {}.",
                                        ws.name
                                    )));
                                }
                                spaces_state.select(Some(0));
                                step = SetupStep::SelectSpace;
                            }
                            SetupStep::SelectSpace => {
                                let idx = spaces_state.selected().unwrap_or(0);
                                let sp = spaces[idx].clone();
                                selected_space = Some(sp.clone());

                                folders = api.get_folders(&sp.id).await?;
                                folders_state.select(Some(0));
                                step = SetupStep::SelectFolders;
                            }
                            SetupStep::SelectFolders => {
                                let ws = selected_workspace.as_ref().unwrap();
                                let sp = selected_space.as_ref().unwrap();

                                let config_folders: Vec<FolderConfig> = folders
                                    .iter()
                                    .filter(|f| selected_folder_ids.contains(&f.id))
                                    .map(|f| FolderConfig {
                                        id: f.id.clone(),
                                        name: f.name.clone(),
                                    })
                                    .collect();

                                if config_folders.is_empty() {
                                    // At least one folder should be selected ideally, or allow empty but warn
                                }

                                let (ai_provider, ai_model, mut ollama_url) = if let Ok(existing) = Config::load() {
                                     (existing.ai_provider, existing.ai_model, existing.ollama_url)
                                 } else {
                                     ("gemini".to_string(), "gemini-3.5-flash".to_string(), None)
                                 };

                                 if ai_provider != "ollama" {
                                     ollama_url = None;
                                 }

                                 let cfg = Config {
                                     workspace_id: ws.id.clone(),
                                     workspace_name: ws.name.clone(),
                                     space_id: sp.id.clone(),
                                     space_name: sp.name.clone(),
                                     folders: config_folders.clone(),
                                     ai_provider,
                                     ai_model,
                                     ollama_url,
                                 };
                                 cfg.save()?;

                                return Ok(format!(
                                    "\nSetup successfully completed!\n\
                                     Saved workspace: {} ({})\n\
                                     Saved space: {} ({})\n\
                                     Saved {} folders.",
                                    ws.name,
                                    ws.id,
                                    sp.name,
                                    sp.id,
                                    config_folders.len()
                                ));
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}
