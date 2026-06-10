use crate::config::Config;
use crate::util::errors::Result;

pub async fn run_config(
    provider: Option<String>,
    model: Option<String>,
    ollama_url: Option<String>,
) -> Result<()> {
    let mut config = match Config::load() {
        Ok(cfg) => cfg,
        Err(_) => {
            println!("No configuration found. Run 'clickup-rui setup' first to configure ClickUp.");
            return Ok(());
        }
    };

    if provider.is_none() && model.is_none() && ollama_url.is_none() {
        println!("Current AI Configuration:");
        println!("  Provider:   {}", config.ai_provider);
        println!("  Model:      {}", config.ai_model);
        println!("  Ollama URL: {}", config.ollama_url);
        return Ok(());
    }

    if let Some(p) = provider {
        let p_lower = p.to_lowercase();
        if p_lower != "gemini" && p_lower != "ollama" {
            println!("Error: Provider must be 'gemini' or 'ollama'.");
            return Ok(());
        }
        
        // Auto-switch default models if using the defaults
        if p_lower == "ollama" && config.ai_model == "gemini-3.5-flash" {
            config.ai_model = "granite4.1:8b".to_string();
        } else if p_lower == "gemini" && config.ai_model == "granite4.1:8b" {
            config.ai_model = "gemini-3.5-flash".to_string();
        }
        
        config.ai_provider = p_lower;
    }

    if let Some(m) = model {
        config.ai_model = m;
    }

    if let Some(u) = ollama_url {
        config.ollama_url = u;
    }

    config.save()?;
    println!("AI configuration updated and saved successfully.");
    println!("  Provider:   {}", config.ai_provider);
    println!("  Model:      {}", config.ai_model);
    println!("  Ollama URL: {}", config.ollama_url);

    Ok(())
}
