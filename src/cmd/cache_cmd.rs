use crate::cache::store::CacheStore;
use crate::config::paths::get_cache_path;
use crate::util::errors::Result;
use std::fs;

pub async fn run_cache_clear() -> Result<()> {
    let path = get_cache_path();
    if path.exists() {
        fs::remove_file(&path)?;
        println!("Cache cleared.");
    } else {
        println!("No cache file found.");
    }
    Ok(())
}

pub async fn run_cache_info() -> Result<()> {
    let path = get_cache_path();
    println!("Cache Path: {}", path.to_string_lossy());

    if !path.exists() {
        println!("Cache file does not exist.");
        return Ok(());
    }

    let metadata = fs::metadata(&path)?;
    let size_bytes = metadata.len();
    println!("File Size: {} bytes", size_bytes);

    let store = CacheStore::load();
    println!("\nCache Store Stats:");
    println!("  Version: {}", store.version);
    println!("  Last Updated: {}", store.updated_at);
    println!(
        "  Teams Cached: {}",
        if store.teams.is_some() { "Yes" } else { "No" }
    );
    println!("  Spaces Cached: {}", store.spaces_by_team.len());
    println!("  Folders Cached: {}", store.folders_by_space.len());
    println!("  Lists Cached: {}", store.lists_by_folder.len());
    println!("  Task-lists Cached: {}", store.tasks.len());

    let total_tasks: usize = store.tasks.values().map(|entry| entry.tasks.len()).sum();
    println!("  Total Tasks in Lists: {}", total_tasks);
    println!("  Task Details Cached: {}", store.task_detail_by_task.len());
    println!("  Comment Sets Cached: {}", store.comments_by_task.len());

    Ok(())
}
