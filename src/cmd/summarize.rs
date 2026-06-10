use crate::ai::summarizer::GeminiSummarizer;
use crate::clickup::api::ClickUpApi;
use crate::config::Config;
use crate::ui::spinner::Spinner;
use crate::util::errors::Result;
use crate::util::filter::should_include_task;

pub async fn run_summarize<A: ClickUpApi>(
    api: &A,
    all_flag: bool,
    mine_only: bool,
) -> Result<()> {
    let cfg = Config::load()?;
    let summarizer = GeminiSummarizer::new();

    let mut spinner = Spinner::start("Fetching current user");
    let user = api.get_current_user().await?;
    spinner.stop();

    for folder in &cfg.folders {
        let mut spinner = Spinner::start("Fetching tasks for folder");
        let lists = match api.get_lists(&folder.id).await {
            Ok(l) => l,
            Err(e) => {
                spinner.stop();
                println!(
                    "Error fetching lists for folder {}: {}. Continuing.",
                    folder.name, e
                );
                continue;
            }
        };

        let mut folder_tasks = Vec::new();
        for list in &lists {
            let tasks = match api.get_tasks(&list.id, all_flag).await {
                Ok(t) => t,
                Err(_) => continue,
            };
            for mut task in tasks {
                if should_include_task(&task, user.id, all_flag, mine_only) {
                    if let Ok(detailed) = api.get_task_detail(&task.id).await {
                        task = detailed;
                    }
                    folder_tasks.push(task);
                }
            }
        }
        spinner.stop();

        if folder_tasks.is_empty() {
            println!("No active tasks found for folder: {}\n", folder.name);
            continue;
        }

        println!("Generating summary for folder: {}...", folder.name);
        match summarizer.summarize_tasks(&folder.name, &folder_tasks).await {
            Ok(summary) => {
                termimad::print_text(&summary);
                println!("\n---\n");
            }
            Err(e) => {
                println!(
                    "Error generating summary for folder {}: {}",
                    folder.name, e
                );
            }
        }
    }

    Ok(())
}
