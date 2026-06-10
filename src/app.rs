use clap::{Parser, Subcommand};
use std::sync::OnceLock;

pub fn get_version_string() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let version = env!("CARGO_PKG_VERSION");
        let commit = option_env!("GIT_COMMIT").unwrap_or("unknown");
        let build_date = option_env!("BUILD_DATE").unwrap_or("unknown");
        format!("{} (commit: {}, built: {})", version, commit, build_date)
    })
}

#[derive(Parser, Debug)]
#[command(
    name = "clickup-rui",
    author,
    version = get_version_string(),
    about = "ClickUp CLI & TUI"
)]
pub struct Cli {
    #[arg(long, short = 'r', global = true, help = "Bypass cache reads")]
    pub refresh: bool,

    #[arg(long, global = true, help = "Delete cache file before command run")]
    pub clear_cache: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    #[command(about = "Open full-screen command menu picker")]
    Menu,
    #[command(about = "Run interactive 3-step setup selector")]
    Setup,
    #[command(about = "Display configured tasks and subtasks hierarchy")]
    Tasks {
        #[arg(long, short = 'a', help = "Include all open except completed/closed")]
        all: bool,

        #[arg(long, short = 'd', help = "Show last 3 comments")]
        detailed: bool,

        #[arg(long, short = 's', help = "AI summary per task")]
        summarize: bool,

        #[arg(long, help = "Set mine=false")]
        team: bool,

        #[arg(long, default_value = "true", action = clap::ArgAction::Set, help = "Only show my tasks")]
        mine: bool,

        #[arg(long, help = "Show user IDs/emails for assignees")]
        id: bool,
    },
    #[command(about = "Interactively browse tasks, update statuses, and add comments")]
    Browse {
        #[arg(long, short = 'a', help = "Include all open except completed/closed")]
        all: bool,

        #[arg(long, help = "Forces mine=false")]
        team: bool,

        #[arg(long, default_value = "true", action = clap::ArgAction::Set, help = "Only show my tasks")]
        mine: bool,
    },
    #[command(about = "Create a new task via step-by-step TUI")]
    New,
    #[command(about = "Interactively log standup updates, change statuses, and add comments")]
    Standup {
        #[arg(long, short = 'a', help = "Include all open except completed/closed")]
        all: bool,

        #[arg(long, default_value = "true", action = clap::ArgAction::Set, help = "Only show my tasks")]
        mine: bool,
    },
    #[command(about = "Generate AI summarizing folder task sets")]
    Summarize {
        #[arg(long, short = 'a', help = "Include all open except completed/closed")]
        all: bool,

        #[arg(long, help = "Forces mine=false")]
        team: bool,

        #[arg(long, default_value = "true", action = clap::ArgAction::Set, help = "Only show my tasks")]
        mine: bool,
    },
    #[command(about = "Summarize recent activity logs across configured team folders")]
    TeamStatus {
        #[arg(long, short = 'd', default_value = "7", help = "Days window")]
        days: u32,

        #[arg(long, short = 's', default_value = "true", help = "Enable AI summarizer")]
        summarize: bool,

        #[arg(long, short = 'c', default_value = "false", help = "Show raw events log")]
        raw: bool,
    },
    #[command(about = "Track specific user activity in configured folders")]
    Track {
        #[arg(help = "ClickUp User ID to track")]
        user_id: Option<i64>,

        #[arg(long, short = 's', default_value = "false", help = "Enable AI summarizer")]
        summarize: bool,

        #[arg(long, short = 'c', default_value = "false", help = "Show raw events log")]
        raw: bool,

        #[arg(long, help = "Output activity log to CSV file in current directory")]
        csv: bool,

        #[arg(long, help = "Output activity log to JSON file in current directory")]
        json: bool,
    },
    #[command(about = "Manage the local database cache store")]
    Cache {
        #[command(subcommand)]
        cmd: CacheSubcommands,
    },
    #[command(about = "Configure AI provider, model, and other settings")]
    Config {
        #[arg(long, help = "AI provider (gemini or ollama)")]
        provider: Option<String>,

        #[arg(long, help = "AI model (e.g., gemini-3.5-flash, granite4.1:8b)")]
        model: Option<String>,

        #[arg(long, help = "Ollama server URL (defaults to http://localhost:11434)")]
        ollama_url: Option<String>,
    },
    #[command(about = "Interactive delete prompt for configuration and cache files")]
    Clean,
    #[command(about = "Print configured workspace, space, folder information")]
    Show,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CacheSubcommands {
    #[command(about = "Clear and delete cache database file")]
    Clear,
    #[command(about = "Display cache database file size, stats, counts")]
    Info,
}
