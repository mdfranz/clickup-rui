mod activity;
pub mod browse;
pub mod cache_cmd;
pub mod clean;
pub mod config_cmd;
pub mod menu;
pub mod new_task;
pub mod setup;
pub mod show;
pub mod standup;
pub mod summarize;
pub mod tasks;
pub mod team_status;
pub mod track;
pub mod workload;

use crate::app::Commands;
use crate::clickup::api::ClickUpApi;
use crate::util::errors::Result;

#[derive(Clone, Copy, Default)]
pub struct RouteContext {
    pub menu_mode: bool,
}

pub async fn route_command<A: ClickUpApi + Clone + 'static>(api: &A, cmd: Commands) -> Result<()> {
    route_command_with_context(api, cmd, RouteContext::default()).await
}

pub async fn route_command_with_context<A: ClickUpApi + Clone + 'static>(
    api: &A,
    cmd: Commands,
    context: RouteContext,
) -> Result<()> {
    match cmd {
        Commands::Menu => {
            Box::pin(menu::run_menu(api)).await?;
        }
        Commands::Setup => {
            setup::run_setup(api).await?;
        }
        Commands::Tasks {
            all,
            detailed,
            summarize,
            team,
            mine,
            id,
        } => {
            let mine_only = if team { false } else { mine };
            tasks::run_tasks(api, all, detailed, summarize, mine_only, id).await?;
        }
        Commands::Browse { all, team, mine } => {
            let mine_only = if team { false } else { mine };
            browse::run_browse(api, all, mine_only).await?;
        }
        Commands::New => {
            new_task::run_new_task(api).await?;
        }
        Commands::Standup { all, mine } => {
            standup::run_standup(api, all, mine).await?;
        }
        Commands::Summarize {
            all,
            team,
            mine,
            markdown,
        } => {
            let mine_only = if team { false } else { mine };
            summarize::run_summarize(api, all, mine_only, markdown).await?;
        }
        Commands::Workload => {
            workload::run_workload(api).await?;
        }
        Commands::TeamStatus {
            days,
            summarize,
            raw,
            markdown,
        } => {
            team_status::run_team_status(api, days, summarize, raw, markdown, context.menu_mode).await?;
        }
        Commands::Track {
            user_id,
            days,
            summarize,
            raw,
            csv,
            json,
            markdown,
        } => {
            track::run_track(
                api,
                user_id,
                track::TrackOptions {
                    days,
                    summarize,
                    raw,
                    csv,
                    json,
                    markdown,
                    menu_mode: context.menu_mode,
                },
            )
            .await?;
        }
        Commands::Cache { cmd } => match cmd {
            crate::app::CacheSubcommands::Clear => cache_cmd::run_cache_clear().await?,
            crate::app::CacheSubcommands::Info => cache_cmd::run_cache_info().await?,
        },
        Commands::Config {
            provider,
            model,
            ollama_url,
        } => {
            config_cmd::run_config(provider, model, ollama_url).await?;
        }
        Commands::Clean => {
            clean::run_clean().await?;
        }
        Commands::Show => {
            show::run_show(api).await?;
        }
    }
    Ok(())
}
