pub mod ai;
pub mod app;
pub mod cache;
pub mod clickup;
pub mod cmd;
pub mod config;
pub mod ui;
pub mod util;

use crate::app::{Cli, Commands};
use crate::cache::client::CachedClient;
use crate::cache::store::CacheStore;
use crate::clickup::client::ClickUpClient;
use crate::config::paths::get_cache_path;
use crate::util::env::get_clickup_pat;
use crate::util::errors::AppError;
use clap::Parser;
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*, Registry};

fn init_logging() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let log_path = crate::config::paths::get_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(&log_path)?;

    let file_layer = fmt::layer()
        .with_writer(std::sync::Mutex::new(file))
        .json()
        .with_target(true);

    let subscriber = Registry::default().with(file_layer);
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    // 1. Initialize logging
    if let Err(e) = init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
    }

    // 2. Parse command-line args
    let cli = Cli::parse();

    // 3. Handle clear-cache flag
    if cli.clear_cache {
        let path = get_cache_path();
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }

    // 4. Default to Commands::Menu if no subcommand provided
    let command = cli.command.unwrap_or(Commands::Menu);

    // 5. Initialize cache store
    let cache_store = Arc::new(Mutex::new(CacheStore::load()));

    // 6. Check if PAT is required and retrieve it
    let pat_required = match &command {
        Commands::Cache { .. } | Commands::Clean | Commands::Config { .. } => false,
        Commands::Show => false, // Show will try to fetch if PAT available but won't hard fail
        _ => true,
    };

    let pat = if pat_required {
        match get_clickup_pat() {
            Ok(token) => token,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    } else {
        get_clickup_pat().unwrap_or_default()
    };

    // 7. Instantiate Client and Cached Wrapper
    let raw_client = ClickUpClient::new(pat);
    let cached_client = CachedClient::new(raw_client, cache_store.clone(), cli.refresh);

    // 8. Run command through router
    let run_res = crate::cmd::route_command(&cached_client, command).await;

    // 9. Flush cache on command exit
    let mut store = cache_store.lock().await;
    if let Err(e) = store.save() {
        tracing::error!("Failed to flush cache on exit: {}", e);
    }

    if let Err(e) = run_res {
        match e {
            AppError::ConfigMissing => {
                eprintln!("{}", e);
            }
            _ => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
