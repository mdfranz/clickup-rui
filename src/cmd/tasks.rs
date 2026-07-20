use crate::ai::summarizer::GeminiSummarizer;
use crate::clickup::api::ClickUpApi;
use crate::clickup::models::Task;
use crate::config::Config;
use crate::ui::spinner::Spinner;
use crate::util::errors::Result;
use crate::util::filter::should_include_task;
use crate::util::format::{format_comment_date, format_task_date};
use crate::util::sort::{sort_comments_by_date_desc, sort_tasks_by_updated_desc};
use std::collections::HashMap;

pub async fn run_tasks<A: ClickUpApi>(
    api: &A,
    all_flag: bool,
    detailed: bool,
    summarize: bool,
    mine_only: bool,
    show_id: bool,
) -> Result<()> {
    let cfg = Config::load()?;
    let workspace_id = cfg.workspace_id.clone();

    let mut spinner = Spinner::start("Fetching current user");
    let user = api.get_current_user().await?;
    spinner.stop();

    let summarizer = GeminiSummarizer::new();

    for folder in &cfg.folders {
        println!("\x1B[1mFolder: {}\x1B[0m", folder.name);

        let mut spinner = Spinner::start("Fetching lists");
        let lists = match api.get_lists(&folder.id).await {
            Ok(l) => l,
            Err(_) => {
                spinner.stop();
                continue;
            }
        };
        spinner.stop();

        for list in &lists {
            println!("  \x1B[34mList: {}\x1B[0m", list.name);

            let mut spinner = Spinner::start("Fetching list tasks");
            let tasks = match api.get_tasks(&list.id, all_flag).await {
                Ok(t) => t,
                Err(_) => {
                    spinner.stop();
                    continue;
                }
            };
            spinner.stop();

            if tasks.is_empty() {
                println!("    No tasks.");
                continue;
            }

            // Build hierarchy maps
            let task_map: HashMap<String, Task> = tasks
                .iter()
                .map(|t| (t.id.clone(), t.clone()))
                .collect();

            let mut subtasks_by_parent: HashMap<String, Vec<Task>> = HashMap::new();
            for task in &tasks {
                if let Some(ref p_id) = task.parent_id {
                    subtasks_by_parent
                        .entry(p_id.clone())
                        .or_default()
                        .push(task.clone());
                }
            }

            // A task is top-level if it has no parent or parent is not in task_map
            let mut top_level_tasks: Vec<Task> = tasks
                .iter()
                .filter(|t| {
                    t.parent_id.is_none()
                        || !task_map.contains_key(t.parent_id.as_ref().unwrap())
                })
                .cloned()
                .collect();

            // Filter validation closure (recursive helper to see if task or any of its subtasks pass filter)
            let mut passes_filter_cache: HashMap<String, bool> = HashMap::new();
            fn check_passes_filter(
                task_id: &str,
                task_map: &HashMap<String, Task>,
                subtasks_by_parent: &HashMap<String, Vec<Task>>,
                user_id: i64,
                all_flag: bool,
                mine_only: bool,
                passes_cache: &mut HashMap<String, bool>,
            ) -> bool {
                if let Some(&res) = passes_cache.get(task_id) {
                    return res;
                }

                let task = match task_map.get(task_id) {
                    Some(t) => t,
                    None => return false,
                };

                if should_include_task(task, user_id, all_flag, mine_only) {
                    passes_cache.insert(task_id.to_string(), true);
                    return true;
                }

                if let Some(subs) = subtasks_by_parent.get(task_id) {
                    for sub in subs {
                        if check_passes_filter(
                            &sub.id,
                            task_map,
                            subtasks_by_parent,
                            user_id,
                            all_flag,
                            mine_only,
                            passes_cache,
                        ) {
                            passes_cache.insert(task_id.to_string(), true);
                            return true;
                        }
                    }
                }

                passes_cache.insert(task_id.to_string(), false);
                false
            }

            // Filter top_level_tasks
            top_level_tasks.retain(|t| {
                check_passes_filter(
                    &t.id,
                    &task_map,
                    &subtasks_by_parent,
                    user.id,
                    all_flag,
                    mine_only,
                    &mut passes_filter_cache,
                )
            });

            // Sort top-level tasks by date_updated desc
            sort_tasks_by_updated_desc(&mut top_level_tasks);

            // Render tasks
            for top_task in &top_level_tasks {
                render_task_node(
                    api,
                    top_task,
                    &subtasks_by_parent,
                    &passes_filter_cache,
                    &summarizer,
                    detailed,
                    summarize,
                    show_id,
                    &workspace_id,
                    0,
                )
                .await?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn render_task_node<A: ClickUpApi>(
    api: &A,
    task: &Task,
    subtasks_by_parent: &HashMap<String, Vec<Task>>,
    passes_filter_cache: &HashMap<String, bool>,
    summarizer: &GeminiSummarizer,
    detailed: bool,
    summarize: bool,
    show_id: bool,
    workspace_id: &str,
    indent: usize,
) -> Result<()> {
    let spaces = " ".repeat(indent * 4 + 4);
    let date_str = format_task_date(&task.date_updated);

    let assignees_str = task
        .assignees
        .iter()
        .map(|u| {
            if show_id {
                format!("{} ({}/{})", u.username, u.id, u.email.as_deref().unwrap_or("N/A"))
            } else {
                u.username.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    println!(
        "{}[{}] {} | updated: {} | assignees: {}",
        spaces, task.status.status, task.name, date_str, assignees_str
    );
    if indent == 0 {
        println!(
            "{}    \x1B[36mhttps://app.clickup.com/t/{}/{}\x1B[0m",
            spaces, workspace_id, task.id
        );
    }

    // Collect comments if detailed or summarize
    if detailed && indent == 0 {
        // Collect task and subtask comments
        let mut all_comments = Vec::new();

        // 1. Parent comments
        let mut spinner = Spinner::start("Fetching comments");
        if let Ok(comments) = api.get_task_comments(&task.id).await {
            for c in comments {
                all_comments.push((task.name.clone(), c, false));
            }
        }
        spinner.stop();

        // 2. Subtask comments
        if let Some(subs) = subtasks_by_parent.get(&task.id) {
            for sub in subs {
                let mut spinner = Spinner::start("Fetching subtask comments");
                if let Ok(comments) = api.get_task_comments(&sub.id).await {
                    for c in comments {
                        all_comments.push((sub.name.clone(), c, true));
                    }
                }
                spinner.stop();
            }
        }

        // Sort comments descending by date
        all_comments.sort_by(|a, b| {
            let a_time = a.1.date.parse::<i64>().ok().unwrap_or(0);
            let b_time = b.1.date.parse::<i64>().ok().unwrap_or(0);
            b_time.cmp(&a_time)
        });

        if !all_comments.is_empty() {
            println!("{}  Recent Comments:", spaces);
            for (sub_name, comment, is_sub) in all_comments.iter().take(3) {
                let comment_date = format_comment_date(&comment.date);
                if *is_sub {
                    println!(
                        "{}    - [{}] [Subtask: {}] {}: {}",
                        spaces, comment_date, sub_name, comment.user.username, comment.comment_text
                    );
                } else {
                    println!(
                        "{}    - [{}] {}: {}",
                        spaces, comment_date, comment.user.username, comment.comment_text
                    );
                }
            }
        }
    }

    if summarize {
        let mut spinner = Spinner::start("Summarizing task");
        let detailed_task = match api.get_task_detail(&task.id).await {
            Ok(dt) => dt,
            Err(_) => task.clone(),
        };

        let mut comments = Vec::new();
        if let Ok(c) = api.get_task_comments(&task.id).await {
            comments = c;
        }

        // Also gather subtask comments
        if let Some(subs) = subtasks_by_parent.get(&task.id) {
            for sub in subs {
                if let Ok(sc) = api.get_task_comments(&sub.id).await {
                    for mut s_comment in sc {
                        s_comment.comment_text =
                            format!("[Subtask: {}] {}", sub.name, s_comment.comment_text);
                        comments.push(s_comment);
                    }
                }
            }
        }

        sort_comments_by_date_desc(&mut comments);
        spinner.stop();

        match summarizer.summarize_task(&detailed_task, &comments).await {
            Ok(summary) => {
                println!("{}  AI Summary:", spaces);
                for line in summary.lines() {
                    println!("{}    {}", spaces, line);
                }
            }
            Err(e) => {
                println!("{}  AI Summary Error: {}", spaces, e);
            }
        }
    }

    // Render subtasks recursively if they pass filter
    if let Some(subs) = subtasks_by_parent.get(&task.id) {
        let mut filtered_subs: Vec<Task> = subs
            .iter()
            .filter(|sub| *passes_filter_cache.get(&sub.id).unwrap_or(&false))
            .cloned()
            .collect();

        sort_tasks_by_updated_desc(&mut filtered_subs);

        for sub in &filtered_subs {
            Box::pin(render_task_node(
                api,
                sub,
                subtasks_by_parent,
                passes_filter_cache,
                summarizer,
                detailed,
                summarize,
                show_id,
                workspace_id,
                indent + 1,
            ))
            .await?;
        }
    }

    Ok(())
}
