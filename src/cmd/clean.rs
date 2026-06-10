use crate::config::paths::{get_cache_path, get_config_path, get_legacy_config_path};
use crate::util::errors::Result;
use std::fs;
use std::io::{self, Write};

pub async fn run_clean() -> Result<()> {
    let mut files_to_prompt = Vec::new();

    files_to_prompt.push(("primary config", get_config_path()));
    if let Some(legacy) = get_legacy_config_path() {
        files_to_prompt.push(("legacy config", legacy));
    }
    files_to_prompt.push(("cache", get_cache_path()));

    for (label, path) in files_to_prompt {
        if path.exists() {
            print!(
                "Remove {} file at {}? [y/N]: ",
                label,
                path.to_string_lossy()
            );
            let _ = io::stdout().flush();
            let mut response = String::new();
            if io::stdin().read_line(&mut response).is_ok() {
                let r = response.trim().to_lowercase();
                if r == "y" {
                    fs::remove_file(&path)?;
                    println!("Removed {} file.", label);
                } else {
                    println!("Skipped.");
                }
            }
        }
    }

    Ok(())
}
