use crate::clickup::api::ClickUpApi;
use crate::config::Config;
use crate::util::env::get_clickup_pat;
use crate::util::errors::Result;

pub async fn run_show<A: ClickUpApi>(api: &A) -> Result<()> {
    match Config::load() {
        Ok(cfg) => {
            println!("Configuration:");
            println!("  Workspace: {} ({})", cfg.workspace_name, cfg.workspace_id);
            println!("  Space: {} ({})", cfg.space_name, cfg.space_id);
            println!("  Configured Folders:");
            for f in &cfg.folders {
                println!("    - {} ({})", f.name, f.id);
            }
        }
        Err(_) => {
            println!("No configuration found. Run 'clickup-rui setup' first.");
        }
    }

    if get_clickup_pat().is_ok() {
        if let Ok(user) = api.get_current_user().await {
            println!("\nAuthenticated ClickUp User:");
            println!("  Name: {}", user.username);
            println!("  ID: {}", user.id);
            println!("  Email: {}", user.email);
        }
    }

    Ok(())
}
