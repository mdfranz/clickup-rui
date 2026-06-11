# clickup-rui ⚡

A high-performance, premium Terminal User Interface (TUI) and Command-Line Interface (CLI) client for **ClickUp**, engineered from the ground up in Rust. Ported from my [original golang tool](https://github.com/mdfranz/clickup-tui)

`clickup-rui` provides an extremely polished, high-density dashboard, interactive wizards, real-time searchable user pickers, offline caching semantics, and advanced AI-powered task/team summarization powered by Gemini.

---

## ✨ Features

### 1. Interactive Dual-Pane Task Browser
* **Dual-Pane Border Highlighting**: Focus toggles dynamically between the left task list and the right details panel.
  * `Tab` / `BackTab` acts as a quick focus switcher.
  * Directional arrows (`Left` / `Right` or `h` / `l`) act as explicit focus navigation keys.
  * Active pane glows with ClickUp Purple, while inactive elements remain elegantly muted.
* **High-Density Design**:
  * Perfectly balanced `50/50` layout split.
  * High-density brackets with formatted date prefixes (`[MM/DD]`), left-aligned fixed-width status labels (`[IN PROGRESS]`), and task names. All elements align perfectly horizontally.
  * Vertical list space compressed to **2 lines per item** (a 33% footprint reduction).
* **Word-Wrapped Details & Scrolling**: Long descriptions and comments automatically wrap (`Wrap { trim: true }`) to perfectly fit the viewport. Focus the right pane to scroll using standard direction keys.

### 2. Stateful, Searchable User Pickers
* Integrated into both the `track` command and the **New Task Assignee Selection** wizard.
* **Real-time Query Filtering**: Instantly filters your ClickUp workspace users by name or email as you type.
* **Dynamic Typing Cursor**: Displays a natural visible cursor in the Filter box that advances fluidly with every typed character.
* **Clamped Safe State**: Automatically keeps selection bounds safe when list sizes shrink dynamically.
* **Retained "Unassigned" Option**: Kept securely at index 0 so users can leave tasks unassigned at any point.

### 3. Adaptive Main Menu
* Launches via the `menu` command with centered ClickUp ASCII-art banners.
* Dynamic geometry adjustments: adapts margins and switches to a high-density flat layout on small terminals to eliminate clipping and ensure all 8 navigation options remain fully visible.

### 4. Advanced Caching & Offline Support
* Core database backed by `${user_cache_dir}/clickup-tui/cache.json` with robust TTL validation.
* **Incremental Fetching & Merging**: Minimizes API requests by performing delta queries using updated-at high-water marks and merging incoming payloads seamlessly.
* **Resilient Fallbacks**: If the ClickUp API is unreachable, `clickup-rui` automatically drops back to stale cache entries instead of crashing, ensuring uninterrupted reading.

### 5. AI Summarization (Gemini-Powered)
* Seamlessly connects to Google/Gemini API to perform conciseness-targeted, factual summarizations for:
  * Full tasks (including description & comments).
  * Folder task sets.
  * User activity logs (grouped daily).
  * Team activity summaries.

### 6. Interactive Wizards & Commands
* **Setup wizard (`setup`)**: Configures your workspace, space, and folder multi-selections with live preview checklists.
* **Daily Standup wizard (`standup`)**: Multi-select your active tasks, type updates, change statuses, and submit them in one centralized, rapid workflow.
* **Team Status dashboard (`team-status`)**: Generates an overview of who has worked on what, with optional AI team-activity summaries.
* **Track logs (`track`)**: Track user activities over the last 10 days with optional `--csv` and `--json` pretty-print exporters.

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

# Gemini API Key (Required for AI Summaries)
export GEMINI_API_KEY="your_gemini_api_key_here" # or GOOGLE_API_KEY
```

Then, run the setup wizard to generate your local configurations:

```bash
cargo run --release -- setup
```

---

## 🛠 Command Reference

| Command | Description |
|---|---|
| `menu` | Launches the interactive, full-screen main command picker |
| `setup` | Runs the 3-step setup wizard (Workspace -> Space -> Folder selection) |
| `tasks` | Lists current tasks from configured folders |
| `browse` | Launches the premium interactive dual-pane task dashboard |
| `new` | Runs the wizard to create a new task with a searchable assignee picker |
| `standup` | Launches the rapid daily standup wizard |
| `summarize` | AI-summarizes tasks in configured folders |
| `team-status` | Compiles team activities with optional AI highlights |
| `track [user]` | Tracks activity logs (supports `--csv` / `--json` export and `--summarize`) |
| `cache info` | Details local cache store statistics and file metrics |
| `cache clear` | Safely purges local cache file |
| `clean` | Interactively prompts to delete configs and cache files |
| `show` | Outputs active space, workspace, and currently authenticated user |

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
  * Default: `${user_cache_dir}/clickup-tui/app.log`
  * Local project folder logging: Set `LOG_LOCAL=1` to write to `./app.log`

---

## 🧪 Testing

To run the robust test suite (covering cache behavior, model deserialization, filtering, sorting, and API integrations):

```bash
cargo test
```

## 📝 License

This project is licensed under the MIT License.
