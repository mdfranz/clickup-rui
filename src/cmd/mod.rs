pub mod browse;
pub mod cache_cmd;
pub mod clean;
pub mod menu;
pub mod new_task;
pub mod setup;
pub mod show;
pub mod standup;
pub mod summarize;
pub mod tasks;
pub mod team_status;
pub mod track;

use crate::app::Commands;
use crate::clickup::api::ClickUpApi;
use crate::util::errors::Result;

pub async fn route_command<A: ClickUpApi>(api: &A, cmd: Commands) -> Result<()> {
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
        Commands::Summarize { all, team, mine } => {
            let mine_only = if team { false } else { mine };
            summarize::run_summarize(api, all, mine_only).await?;
        }
        Commands::TeamStatus {
            days,
            summarize,
            raw,
        } => {
            team_status::run_team_status(api, days, summarize, raw).await?;
        }
        Commands::Track {
            user_id,
            summarize,
            raw,
        } => {
            track::run_track(api, user_id, summarize, raw).await?;
        }
        Commands::Cache { cmd } => match cmd {
            crate::app::CacheSubcommands::Clear => cache_cmd::run_cache_clear().await?,
            crate::app::CacheSubcommands::Info => cache_cmd::run_cache_info().await?,
        },
        Commands::Clean => {
            clean::run_clean().await?;
        }
        Commands::Show => {
            show::run_show(api).await?;
        }
    }
    Ok(())
}
