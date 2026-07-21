use crate::ai::summarizer::GeminiSummarizer;
use crate::cache::ttl::now_ms;
use crate::clickup::api::ClickUpApi;
use crate::clickup::models::Activity;
use crate::config::Config;
use crate::ui::spinner::Spinner;
use crate::util::env::is_menu_mode;
use crate::util::errors::Result;
use crate::util::format::format_comment_date;
use std::collections::HashMap;

pub async fn run_team_status<A: ClickUpApi>(
    api: &A,
    days: u32,
    summarize: bool,
    raw_flag: bool,
    markdown_flag: bool,
) -> Result<()> {
    let cfg = Config::load()?;
    let mut spinner = Spinner::start("Gathering team activity logs");

    let now = now_ms();
    let date_from = now - (days as i64 * 24 * 3600 * 1000);

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

                if created_ms >= date_from {
                    activities.push(Activity {
                        id: format!("{}-created", task.id),
                        user: task.creator.clone(),
                        type_: "created task".to_string(),
                        date: created_ms.to_string(),
                        task_id: task.id.clone(),
                        source: "api".to_string(),
                        detail: Some(task.status.status.clone()),
                        task_name: Some(task.name.clone()),
                    });
                }

                // ClickUp v2 doesn't expose who performed a transition, so attribute
                // done/closed/updated to the creator as the best available proxy.
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
                        user: task.creator.clone(),
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
                        user: task.creator.clone(),
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
                        user: task.creator.clone(),
                        type_: "updated task".to_string(),
                        date: updated_ms.to_string(),
                        task_id: task.id.clone(),
                        source: "api".to_string(),
                        detail: Some(task.status.status.clone()),
                        task_name: Some(task.name.clone()),
                    });
                }

                if updated_ms >= date_from {
                    if let Ok(comments) = api.get_task_comments(&task.id).await {
                        for comment in comments {
                            if let Ok(comment_ms) = comment.date.parse::<i64>() {
                                if comment_ms >= date_from {
                                    activities.push(Activity {
                                        id: format!("{}-comment-{}", task.id, comment.id),
                                        user: comment.user.clone(),
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

    activities.sort_by(|a, b| {
        let a_time = a.date.parse::<i64>().ok().unwrap_or(0);
        let b_time = b.date.parse::<i64>().ok().unwrap_or(0);
        b_time.cmp(&a_time)
    });

    spinner.stop();

    if activities.is_empty() {
        println!("No team activities found in the last {} days.", days);
        return Ok(());
    }

    let mut grouped: HashMap<String, Vec<Activity>> = HashMap::new();
    for act in &activities {
        grouped
            .entry(act.user.username.clone())
            .or_default()
            .push(act.clone());
    }

    let mut formatted_summary = String::new();
    let mut show_raw = raw_flag;

    if summarize {
        let mut spinner = Spinner::start("Generating AI team summary");
        match GeminiSummarizer::new() {
            Ok(summarizer) => {
                let user_activities: Vec<(String, Vec<Activity>)> = grouped.into_iter().collect();

                match summarizer
                    .summarize_team_activity(days, &user_activities, &[])
                    .await
                {
                    Ok(summary) => {
                        formatted_summary = summary;
                    }
                    Err(e) => {
                        println!("AI Summary failed: {}. Falling back to raw.", e);
                        show_raw = true;
                    }
                }
            }
            Err(e) => {
                println!("AI Summary failed: {}. Falling back to raw.", e);
                show_raw = true;
            }
        }
        spinner.stop();
    }

    if formatted_summary.is_empty() || formatted_summary == "No summary generated." {
        show_raw = true;
    }

    if is_menu_mode() {
        run_scrollable_tui(formatted_summary, activities, show_raw).await?;
    } else {
        if show_raw {
            println!("Raw Team Activity Log (Last {} Days):", days);
            for act in &activities {
                let dt = format_comment_date(&act.date);
                let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
                println!(
                    "[{}] {} {} \"{}\" [Task: {}] [Detail: {}]",
                    dt,
                    act.user.username,
                    act.type_,
                    t_name,
                    act.task_id,
                    act.detail.as_deref().unwrap_or("N/A")
                );
            }
        } else {
            if markdown_flag {
                println!("{}", formatted_summary);
            } else {
                termimad::print_text(&formatted_summary);
            }
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
    use ratatui::widgets::{Block, Borders, Paragraph};

    let mut guard = crate::ui::terminal::TerminalGuard::create()?;
    let terminal = guard.inner();

    let mut scroll: u16 = 0;
    let content = if show_raw {
        let mut raw_str = String::new();
        for act in &activities {
            let dt = format_comment_date(&act.date);
            let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
            raw_str.push_str(&format!(
                "[{}] {} {} \"{}\" [Task: {}] [Detail: {}]\n",
                dt,
                act.user.username,
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
                .title(" Team Status Summary (use Arrow Up/Down or j/k to scroll, Q to quit) ")
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

    Ok(())
}
