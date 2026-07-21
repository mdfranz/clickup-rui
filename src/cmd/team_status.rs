use crate::ai::summarizer::GeminiSummarizer;
use crate::cmd::activity::{collect_activities, ActivityScope};
use crate::clickup::api::ClickUpApi;
use crate::clickup::models::Activity;
use crate::config::Config;
use crate::ui::spinner::Spinner;
use crate::util::errors::Result;
use crate::util::format::format_comment_date;
use std::collections::HashMap;

pub async fn run_team_status<A: ClickUpApi>(
    api: &A,
    days: u32,
    summarize: bool,
    raw_flag: bool,
    markdown_flag: bool,
    menu_mode: bool,
) -> Result<()> {
    let cfg = Config::load()?;
    let mut spinner = Spinner::start("Gathering team activity logs");

    let date_from = crate::cache::ttl::now_ms() - (days as i64 * 24 * 3600 * 1000);
    let activities = collect_activities(api, &cfg.folders, date_from, ActivityScope::Team).await;

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

    if menu_mode {
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
                            scroll = scroll.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') if scroll + 5 < total_lines => {
                            scroll += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
