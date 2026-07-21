# clickup-rui ⚡

A Terminal User Interface (TUI) and Command-Line Interface (CLI) client for **ClickUp**, implemented in Rust. 

Ported from my [original golang tool](https://github.com/mdfranz/clickup-tui)

`clickup-rui` provides a high-density dashboard, interactive wizards, searchable user pickers, resilient local caching, and AI-powered task and team summaries. Gemini is the default and most-tested AI provider; experimental local Ollama support is also available.

See also:
* [External Dependencies & Library Classifications](PKG.md)
* [Project Backlog & Known Issues](ISSUES.md)

---

## ✨ Features

### 1. Interactive Dual-Pane Task Browser
* **Dual-Pane Border Highlighting**: Focus toggles dynamically between the left task list and the right details panel.
  * `Tab` / `BackTab` acts as a quick focus switcher.
  * Directional arrows (`Left` / `Right` or `h` / `l`) act as explicit focus navigation keys.
  * Active pane is styled with a purple border, while inactive elements use a dimmed color scheme.
* **High-Density Design**:
  * Balanced `50/50` layout split.
  * High-density brackets with formatted date prefixes (`[MM/DD]`), left-aligned fixed-width status labels (`[IN PROGRESS]`), and task names. All elements are horizontally aligned.
  * Vertical list space compressed to **2 lines per item** (a 33% footprint reduction).
* **Word-Wrapped Details & Scrolling**: Long descriptions and comments wrap (`Wrap { trim: true }`) to fit within the viewport width. Focus the right pane to scroll using standard direction keys.

### 2. Stateful, Searchable User Pickers
* Integrated into both the `track` command and the **New Task Assignee Selection** wizard.
* **Real-time Query Filtering**: Instantly filters your ClickUp workspace users by name or email as you type.
* **Dynamic Typing Cursor**: Displays a cursor in the Filter box that updates position as characters are typed.
* **Clamped Safe State**: Automatically keeps selection bounds safe when list sizes shrink dynamically.
* **Retained "Unassigned" Option**: Positioned at index 0 so users can leave tasks unassigned at any point.

### 3. Adaptive Main Menu
* Launches via the `menu` command with centered ClickUp ASCII-art banners.
* Dynamic geometry adjustments: adapts margins and switches to a high-density flat layout on small terminals to keep all menu options visible.

### 4. Local Caching & Offline Support
* Core database backed by `${user_cache_dir}/clickup-tui/cache.json` with TTL-based validation.
* **Incremental Fetching & Merging**: Minimizes API requests by performing delta queries using updated-at high-water marks and merging incoming payloads.
* **Resilient Fallbacks**: When cached data is available and the ClickUp API is unreachable, `clickup-rui` falls back to stale cache entries for supported reads.

### 5. AI Summarization (Gemini by Default)
* Gemini is the default and most-tested provider for concise, factual summaries. Experimental local Ollama support can be configured with `clickup-rui config --provider ollama`.
* Summarizes:
  * Full tasks (including description & comments).
  * Folder task sets.
  * User activity logs (grouped daily).
  * Team activity summaries.

### 6. Interactive Wizards & Commands
* **Setup wizard (`setup`)**: Configures your workspace, space, and folder multi-selections with live preview checklists.
* **Daily Standup wizard (`standup`)**: Multi-select your active tasks, type updates, change statuses, and submit them in one centralized, rapid workflow.
* **Team Status dashboard (`team-status`)**: Generates an overview of who has worked on what, with optional AI team-activity summaries.
* **Track logs (`track`)**: Track user activities over a configurable time window and export results to timestamped CSV or JSON files.
* **Workload dashboard (`workload`)**: Provides an interactive 3-pane dashboard grouped by assignee, featuring active pane title highlighting, live status filters (`f`), in-place commenting (`c`), status updates (`s`), and tag management (`t`).

---

## 🚀 Quick Start

### Installation

Clone the repository and build the binary:

```bash
git clone https://github.com/mdfranz/clickup-rui.git
cd clickup-rui
make release
```

The compiled binary will be available at `./target/release/clickup-rui`.

### Developer Workflow with Makefile

A fully-featured **Makefile** is provided to streamline common development tasks. Run `make help` (or just `make`) to see the complete list of available targets:

```bash
make help
```

Key targets include:
* `make build` / `make release`: Build debug or optimized release binary.
* `make run` / `make run-release`: Execute the debug or release binary directly.
* `make test`: Run all unit and integration tests.
* `make clippy` / `make fmt`: Code quality linting and automatic formatting.
* `make setup` / `make browse` / `make menu`: Launch interactive wizards and menus directly.
* `make install`: Copy the binary directly into your local `~/.local/bin/`.

### Runtime Requirements

Set up your required environment variables:

```bash
# ClickUp Personal Access Token (Required for ClickUp operations)
export CLICKUP_PAT="your_clickup_pat_here"

# Gemini API Key (Required for the default Gemini AI provider; never stored in config)
export GEMINI_API_KEY="your_gemini_api_key_here" # or GOOGLE_API_KEY
```

Then, run the setup wizard to generate your local configurations:

```bash
cargo run --release -- setup
```

To try the experimental local Ollama provider instead of Gemini:

```bash
clickup-rui config --provider ollama --model granite4.1:8b
```

Gemini API keys are read only from `GEMINI_API_KEY` or `GOOGLE_API_KEY`. They are not accepted by `config` and are never written to the configuration file.

---

## 🛠 Command Reference

| Command | Description |
|---|---|
| `menu` | Launches the interactive, full-screen main command picker |
| `setup` | Runs the 3-step setup wizard (Workspace -> Space -> Folder selection) |
| `tasks` | Lists current tasks from configured folders |
| `browse` | Launches the interactive dual-pane task dashboard |
| `new` | Runs the wizard to create a new task with a searchable assignee picker |
| `standup` | Launches the rapid daily standup wizard |
| `summarize` | AI-summarizes tasks in configured folders (supports `--markdown` / `-m`) |
| `workload` | Opens an interactive workload view grouped by assignee |
| `team-status` | Compiles team activities with optional AI highlights (supports `--markdown` / `-m`) |
| `track [user_id]` | Tracks activity logs (default 10 days; supports `--days` / `-d`, `--csv` / `--json`, `--summarize`, and `--markdown` / `-m`) |
| `cache info` | Details local cache store statistics and file metrics |
| `cache clear` | Safely purges local cache file |
| `config` | Shows or updates the AI provider, model, and Ollama URL; Gemini keys come only from environment variables |
| `clean` | Interactively prompts to delete configs and cache files |
| `show` | Outputs active space, workspace, and currently authenticated user |

### 📝 Raw Markdown Output

For commands that generate AI-powered summaries, you can pass the `--markdown` / `-m` flag to output the raw markdown text directly to stdout. This makes it extremely easy to copy-paste or redirect the output into reports, daily notes, or separate files.

This option is supported on:
* **`summarize`**: AI-summarizes configured folder task sets.
* **`team-status`**: Compiles recent team-activity summaries.
* **`track`**: Tracks individual user activity logs with daily summaries.

**Examples:**

```bash
# Save raw folder task summaries directly to a markdown file
clickup-rui summarize -m > folder_summaries.md

# Output raw team activity summary to stdout without terminal styling
clickup-rui team-status -m

# Track and save a user's recent daily summaries directly to a file
clickup-rui track 111900148 --summarize -m > user_updates.md
```

---

## ⚙️ Runtime Options

Global options:

* `--refresh`, `-r`: Bypass cached reads for the current command.
* `--clear-cache`: Delete the local cache before running the command.

AI configuration:

* `clickup-rui config`: Show the selected provider and model.
* `clickup-rui config --provider gemini --model gemini-3.5-flash`: Use Gemini.
* `clickup-rui config --provider ollama --model granite4.1:8b --ollama-url http://localhost:11434`: Use a local Ollama server.

Logging environment variables:

* `RUST_LOG`: Sets the log filter; defaults to `debug`.
* `LOG_RESPONSE_BODIES=1`: Includes API response bodies in logs.
* `LOG_SENSITIVE_DATA=1`: Includes request bodies in logs.

Every run writes JSON logs to `./app.log`. ClickUp requests and responses include their method, URL, status, and latency. Use `tail -f app.log` in another terminal to inspect them.

`LOG_RESPONSE_BODIES` and `LOG_SENSITIVE_DATA` can write ClickUp content to disk. Use them only for local debugging and remove the resulting logs when finished.

---

## 🎛 Keyboard Bindings

### Browser Pane (`browse`)
* **Focus Switch**: `Tab` / `BackTab` / `h` / `l` / `Left` / `Right`
* **Navigate List / Scroll Description**: `Up` / `Down` or `j` / `k`
* **Comment Editor**: Press `c` to open overlay -> Type -> `Ctrl+s` to save / `Esc` to cancel
* **Status Picker**: Press `s` to open status menu -> Arrow keys -> `Enter` to set / `Esc` to cancel
* **New Task Wizard**: Press `n` to launch
* **Task Reload**: Press `r` to refresh details and comments for the current task
* **Exit**: `Esc` or `q`

### Workload View (`workload`)
* **Focus Switch**: `Tab` / `BackTab` / `h` / `l` / `Left` / `Right` to move focus between Team Members, Tasks, and Task Detail panes
* **Navigate Lists / Scroll Details**: `Up` / `Down` or `j` / `k` (contextual to the active pane)
* **Status Filter**: Press `f` to select/toggle which task statuses to display
* **Comment Editor**: Press `c` to open comment overlay -> Type -> `Ctrl+s` to save / `Esc` to cancel (Tasks pane active)
* **Status Picker**: Press `s` to open status menu -> Arrow keys -> `Enter` to set / `Esc` to cancel (Tasks pane active)
* **Tag Picker**: Press `t` to open space tag menu -> Arrow keys -> `Space` to toggle -> `Enter` to set / `Esc` to cancel (Tasks pane active)
* **New Task Wizard**: Press `n` to launch
* **Task Reload**: Press `r` to refresh details and comments for the current task (Tasks pane active)
* **Exit**: `Esc` or `q`

### Stateful Picker (Assignee Selection)
* **Typing**: Normal alphanumeric input goes directly into the **Filter** box
* **Delete Character**: `Backspace`
* **Navigate List**: `Up` / `Down` keys
* **Select**: `Enter`
* **Cancel / Return**: `Esc`

---

## 📁 Paths & Configuration

* **Primary Config Path**:
  * `$XDG_CONFIG_HOME/clickup-tui/config.toml` (if env is set)
  * `~/.config/clickup-tui/config.toml` (default)
  * Legacy fallback: `~/.local/clickup-tui.toml`
* **Local Cache Path**:
  * `${user_cache_dir}/clickup-tui/cache.json`
  * Temp directory fallback: `/tmp/clickup-tui/cache.json`
* **Application Logs**:
  * `./app.log` in the directory where `clickup-rui` is run

The configuration file stores workspace, folder, and AI-provider settings only. Gemini credentials must remain in environment variables.

---

## 🧪 Testing

To run the test suite, including cache behavior, ClickUp response deserialization, formatting, and mocked API-client coverage:

```bash
cargo test
```

## 📝 License

This project is licensed under the MIT License.
