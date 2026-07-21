# Archived Rust Port Specification

> Historical implementation planning document. It is retained for context only and does not describe the current application. Refer to the README and source code for current behavior.

This document is the single source of truth for implementing a Rust port of `clickup-tui`.

Goal: ship a Rust CLI/TUI with behavior parity to the current Go app, including command set, flags, data flow, caching semantics, and interactive workflows.

## 1. Scope and Parity

Implement these commands with matching user-facing behavior:

- `menu`
- `setup`
- `tasks`
- `browse`
- `new`
- `standup`
- `summarize`
- `team-status`
- `track [user_id]`
- `cache info`
- `cache clear`
- `clean`
- `show`

Implement these global flags on the root command:

- `--refresh`, `-r` (bypass cache reads)
- `--clear-cache` (delete cache file before command run)

Do not require the Go codebase while building the Rust version.

## 2. Runtime Requirements

- Rust stable, edition 2021.
- Terminal app only.
- ClickUp token from env var: `CLICKUP_PAT` (required for API commands).
- Gemini key from `GEMINI_API_KEY` or `GOOGLE_API_KEY` (required for AI commands).

## 3. Recommended Rust Crates

```toml
[package]
name = "clickup-tui"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
chrono = { version = "0.4", features = ["clock"] }
clap = { version = "4", features = ["derive"] }
crossterm = "0.28"
dirs = "5"
ratatui = "0.29"
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "json"] }
termimad = "0.29"

[dev-dependencies]
assert_cmd = "2"
insta = "1"
tempfile = "3"
wiremock = "0.6"
```

## 4. Project Layout

```text
src/
  main.rs
  app.rs                  # root command wiring, global flags, version string
  cmd/
    menu.rs
    setup.rs
    tasks.rs
    browse.rs
    new_task.rs
    standup.rs
    summarize.rs
    team_status.rs
    track.rs
    cache_cmd.rs
    clean.rs
    show.rs
  clickup/
    mod.rs
    api.rs                # trait/abstraction
    client.rs             # HTTP implementation
    models.rs
  cache/
    mod.rs
    store.rs
    client.rs             # caching wrapper over clickup::api
    ttl.rs
  config/
    mod.rs
    paths.rs
  ai/
    mod.rs
    summarizer.rs
  ui/
    mod.rs
    styles.rs
    spinner.rs
  util/
    env.rs
    sort.rs
    format.rs
    filter.rs
    errors.rs
tests/
```

## 5. Data Models and API Contract

Base URL:

- `https://api.clickup.com/api/v2/`

HTTP client requirements:

- Timeout: 30 seconds.
- Header: `Authorization: <CLICKUP_PAT>`.
- `Content-Type: application/json` for requests with body.
- Treat non-200 as error (`API error: status <code>` behavior parity).

### 5.1 Endpoints

- `GET /team` -> teams
- `GET /user` -> current user
- `GET /team/{team_id}/space?archived=false` -> spaces
- `GET /space/{space_id}/folder?archived=false` -> folders
- `GET /folder/{folder_id}/list?archived=false` -> lists
- `GET /list/{list_id}` -> list detail (statuses)
- `GET /list/{list_id}/task?archived=false&include_closed={bool}&subtasks=true` -> tasks
- `GET /list/{list_id}/task?archived=false&include_closed=true&subtasks=true&date_updated_gt={ms}` -> incremental tasks
- `GET /task/{task_id}` -> task detail
- `GET /task/{task_id}/comment` -> comments
- `PUT /task/{task_id}` with `{ "status": "..." }` -> update status
- `POST /task/{task_id}/comment` with `{ "comment_text": "..." }` -> create comment
- `POST /list/{list_id}/task` with:
  - required: `name`
  - optional: `description`, `status`, `assignees: [i64]`

### 5.2 Rust model requirements

Implement at least these fields:

- `Team { id: String, name: String, members: Vec<Member> }`
- `Member { user: User }`
- `User { id: i64, username: String, email: String }`
- `Space { id: String, name: String }`
- `Folder { id: String, name: String }`
- `Status { status: String, color: String, type_: String }`
- `List { id: String, name: String, statuses: Vec<Status> }`
- `Task`:
  - `id`, `name`
  - `status.status`
  - `parent` is polymorphic (`null`, string, or object with `id`) -> normalize to `parent_id: Option<String>`
  - `assignees: Vec<User>`
  - `creator: User`
  - `date_created`, `date_updated`, `date_done`, `date_closed` as Unix ms string fields
  - `text_content`
- `Comment { id, comment_text, user, date }`
- `Activity { id, user, type, date, task_id, source, detail? }`

Use custom serde for `Task.parent` normalization.

## 6. Cache Layer (Must Match Semantics)

Cache file path:

- default: `${user_cache_dir}/clickup-tui/cache.json`
- fallback: `${temp_dir}/clickup-tui/cache.json`

Cache schema:

- `version: 1`
- `updated_at`
- TTL-backed entries:
  - user
  - teams
  - spaces by team
  - folders by space
  - lists by folder
  - list detail by list
  - workspace users by workspace
  - task detail by task
  - comments by task
- Task list cache (`tasks[list_id]`) with:
  - full/partial list payload
  - `fetched_at`
  - `max_date_updated` high-water mark
  - `includes_closed` boolean

TTL constants:

- `TTL_USER = 24h`
- `TTL_TEAMS = 4h`
- `TTL_SPACES = 4h`
- `TTL_FOLDERS = 1h`
- `TTL_LISTS = 1h`
- `TTL_LIST_DETAIL = 1h`
- `TTL_WS_USERS = 4h`
- `TTL_TASK_DETAIL = 10m`
- `TTL_COMMENTS = 5m`
- `TTL_TASKS_FULL = 30m`

### 6.1 Read behavior

- If `--refresh` is false and entry is fresh -> cache hit.
- On API failure, if stale cached entry exists -> return stale entry.
- If no cache and API fails -> return error.

### 6.2 Task list behavior

For `GetTasks(list_id, include_closed)`:

- If cached exists and caller asks `include_closed=true` but cache `includes_closed=false`, force full fetch with closed included.
- If cached exists and age `< TTL_TASKS_FULL`, do incremental fetch with `date_updated_gt=max_date_updated`.
- Merge incremental tasks by ID (replace existing, append new).
- If incremental returns no updates, still refresh `fetched_at`.
- If caller requested `include_closed=false`, return list with status `closed` filtered out.
- On full fetch success, recompute `max_date_updated` from task `date_updated`.

### 6.3 Write behavior and invalidation

- Update task status:
  - call API
  - invalidate task detail cache
  - remove that task from first cached list containing it
- Create task comment:
  - call API
  - delete comments cache for task
- Create task:
  - call API
  - delete cached task list entry for target list

### 6.4 Persistence rules

- Cache file must be loaded at startup; on missing/corrupt/wrong-version -> start empty store.
- Flush only when dirty.
- Flush algorithm:
  - serialize JSON
  - ensure directory exists
  - write `${cache}.tmp`
  - atomic rename tmp -> real

## 7. Config and Paths

Config TOML schema:

```toml
workspace_id = "..."
workspace_name = "..."
space_id = "..."
space_name = "..."

[[folders]]
id = "..."
name = "..."
```

Path resolution:

- config path priority:
  - `$XDG_CONFIG_HOME/clickup-tui/config.toml` if env set
  - `~/.config/clickup-tui/config.toml`
- legacy read fallback only: `~/.local/clickup-tui.toml`

Save behavior:

- create parent dirs with mode `0755`
- write file mode `0644`

Missing config behavior in commands:

- print: `No configuration found. Run 'clickup-tui setup' first.`
- return without panic.

## 8. Environment Variables

- `CLICKUP_PAT` (required for ClickUp API)
- `GEMINI_API_KEY` or `GOOGLE_API_KEY` (required for AI commands)
- `LOG_LOCAL=1` -> log file in `./app.log`, else cache-dir log
- `LOG_RESPONSE_BODIES=1` -> log response body instead of redacted marker
- `LOG_SENSITIVE_DATA=1` -> disable sensitive redaction placeholder
- `CLICKUP_TUI_MENU=1` (internal) -> run TUI commands with alt-screen integration from menu

## 9. Logging Requirements

- JSON logs to file only (not stdout/stderr).
- Default level: debug.
- File path:
  - if `LOG_LOCAL=1`: `app.log`
  - else `${user_cache_dir}/clickup-tui/app.log` (or temp fallback)
- Log request metadata and response status/latency.
- Non-body logging must redact response bodies unless `LOG_RESPONSE_BODIES=1`.

## 10. Shared Utility Behavior

### 10.1 Task filter logic

`should_include_task(task, user_id, show_all, mine_only)`:

- if `mine_only=true`: require user ID in assignees
- if `show_all=true`: include every status except `completed` and `closed`
- else include only statuses:
  - `in progress`
  - `in review`
  - `blocked`
  - `scoping`

Status comparisons are case-insensitive.

### 10.2 Sorting

- Tasks sorted by `date_updated` descending (numeric parse of ms string).
- Comments sorted by `date` descending.

### 10.3 Date formatting

- Task date format: `MM/DD`
- Comment/activity date format: `MM/DD HH:MM`
- On parse failure: return empty string.

## 11. Command Specifications

## 11.0 Root command

- Command name: `clickup-tui`
- `--version` should include version, commit, and build date.
- Before any subcommand runs:
  - if `--clear-cache` is set, delete cache file and ignore not-found errors.
- All commands that call ClickUp should use the cached client wrapper and flush on command exit.

## 11.1 `menu`

- Full-screen command picker.
- Items: tasks, browse, new, standup, track, team-status, setup, show.
- On selection:
  - set `CLICKUP_TUI_MENU=1`
  - execute subcommand through root router
  - unset env var
- After command returns, pause screen:
  - any key -> back to menu
  - `q`/`ctrl+c` -> quit menu loop

## 11.2 `setup`

Interactive 3-step selector:

- Step 1 workspace (teams)
- Step 2 space
- Step 3 folder multi-select

Controls:

- `enter` confirm step
- `space` toggle folder checkbox (folder step)
- `q`/`ctrl+c` quit

Completion:

- save config with selected workspace/space/folders
- print success summary with selected names and folder count

## 11.3 `tasks`

Flags:

- `--all`, `-a` (include all open except completed/closed)
- `--detailed`, `-d` (show last 3 comments)
- `--summarize`, `-s` (AI summary per task)
- `--team` (set mine=false)
- `--mine` (default true)
- `--id` (show user IDs/emails for assignees)

Behavior:

- load config and current user
- iterate configured folders -> lists -> tasks
- build hierarchy:
  - map task by ID
  - group subtasks by parent_id
  - consider top-level if no parent or parent not found
- include top-level task if:
  - it passes filter, or
  - any subtask under it passes filter
- sort top-level by `date_updated` desc
- render subtasks indented under parent
- if `--detailed`: fetch task comments and subtask comments, prefix subtask comments with `[SubtaskName]`, sort desc, show top 3
- if `--summarize`: fetch full task, collect task+subtask comments, summarize

## 11.4 `browse`

Flags:

- `--all`, `-a`
- `--team` (forces `mine=false`)
- `--mine` (default true)

State machine:

- `List`
- `CommentEditor`
- `StatusPicker`
- `NewTaskOverlay`

Behavior:

- left pane: filtered top-level tasks sorted by updated desc
- right pane: selected task detail + comments
- auto-load comments when selection changes
- modal interactions:
  - `c` -> open comment editor
  - `ctrl+s` in comment editor -> post comment
  - `s` -> open status picker (list statuses from `GetList(list_id)`)
  - `enter` in status picker -> update status
  - `esc` closes modal
- `n` opens new-task workflow overlay
  - if a task is selected, preseed folder and list from selection
- `q` or `ctrl+c` exits from list state

## 11.5 `new`

State machine:

- folder select
- list select
- status select
- name input
- description input
- assignee yes/no prompt
- assignee select (if "no")
- confirm
- creating
- done

Important behavior:

- list auto-selection:
  - auto-pick if folder has one list
  - else auto-pick list named `list` (case-insensitive)
- assignee prompt:
  - `y` assigns current user
  - `n` loads workspace users and allows selecting any user or `Unassigned`
- confirm keys:
  - `enter` create
  - `n` edit name
  - `d` edit description
  - `a` edit assignee
- done keys:
  - `y` create another in same list
  - `s` restart from folder selection
  - `n`/`q`/`enter`/`esc` exit

## 11.6 `standup`

Flags:

- `--all`, `-a`
- `--mine` (default true)

State machine:

- loading
- multi-select task list
- per-task update screen
- status picker overlay
- posting
- done summary

Selection controls:

- `up/down` or `j/k`: move cursor
- `space`: toggle selected task
- `a`: toggle all
- `enter`: start updates (if none selected -> quit)

Per-task controls:

- textarea for optional comment
- `tab`: open status picker
- `ctrl+s`: submit comment/status
- `esc`: skip current task

Submission rules:

- if no comment and no real status change, skip posting and move on
- else post comment (if provided) then update status (if changed)
- store summary result per posted task

Done screen:

- show task-level results:
  - comment added
  - status old -> new

## 11.7 `summarize`

Flags:

- `--all`, `-a`
- `--team` (forces mine=false)
- `--mine` (default true)

Behavior:

- for each configured folder:
  - fetch lists, tasks, apply filter
  - attempt to fetch full task details per task
  - summarize folder task set with AI
- render markdown summary in terminal-friendly style

## 11.8 `team-status`

Flags:

- `--days`, `-d` (default 7)
- `--summarize`, `-s` (default true)
- `--raw`, `-c` (default false)

Activity generation algorithm:

- `date_from = now - days` (Unix ms)
- for each configured folder/list:
  - fetch recent tasks (`date_updated_gt=date_from`)
  - for each task:
    - if created in window: add `created task [status]` for creator
    - for each assignee:
      - if `date_done` in window -> `completed task [status]`
      - else if `date_closed` in window -> `closed task [status]`
      - else if updated in window and updated > created -> `updated task [status]`
    - if task updated in window, fetch comments and add `commented on task` entries for comments in window
- sort activities descending by date
- group by username for summarizer input

Display behavior:

- If not in menu: non-TUI output with spinner while loading
- If in menu: viewport-based scrollable TUI
- raw log shown when:
  - summary empty, or
  - `--raw=true`

## 11.9 `track [user_id]`

Flags:

- `--summarize`, `-s` (default false)
- `--raw`, `-c` (default false)

Behavior window is fixed to last 10 days.

Flow:

- if `user_id` provided: load activity directly
- else: show user picker from workspace users
- activity events for selected user:
  - created task in window
  - completed/closed/updated tasks where user is assignee
  - comments authored by user in window
- sort descending
- if summarization enabled:
  - group activities by day (`YYYY-MM-DD`)
  - generate one summary per day

Display:

- non-menu mode: load then print and exit
- menu mode: scrollable display; `esc` returns to user picker
- raw log shown when no summaries or `--raw=true`

## 11.10 `cache`

- `cache clear`: delete cache file; print `Cache cleared.` or `No cache file found.`
- `cache info`:
  - print path and file size
  - print cache store stats:
    - version
    - last updated
    - teams cached present?
    - counts for spaces/folders/lists/task-lists/total tasks/task details/comment sets

## 11.11 `clean`

Interactive delete prompts for each existing file:

- primary config
- legacy config
- cache

Prompt format:

- `Remove <label> file at <path>? [y/N]:`

Only `y` performs deletion.

## 11.12 `show`

- print workspace, space, folder config.
- if `CLICKUP_PAT` available and API call works, also print current user name and numeric ID.

## 12. AI Summarization Requirements

Implement a summarizer service with four functions:

- `summarize_task(task, comments)`
- `summarize_tasks(folder_name, tasks)`
- `summarize_user_activity(user_name, day, activities, task_details, task_comments)`
- `summarize_team_activity(days, user_activities, task_details)`

Model behavior targets:

- concise and factual outputs
- markdown output for folder/team/user summaries
- avoid hyperbolic language in team summary prompt

Provider behavior:

- use Gemini API key from env
- fail with clear error if key missing
- if model returns no choices: return `No summary generated.`

## 13. Error Handling Rules

- Keep command-level UX consistent:
  - config missing -> friendly message and return
  - auth/env missing -> print error and exit non-zero
  - API failures in list loops -> continue to next folder/list when safe
- Cache wrapper must prefer stale data over hard failure when stale exists.
- TUI models should surface error screens/messages and allow quit.

## 14. Testing Requirements

Minimum tests to consider parity complete:

- Config path and save/load tests (including XDG and legacy fallback).
- Env token tests (`CLICKUP_PAT` set/unset).
- Filter status + mine-only tests.
- Sort utility tests for tasks/comments.
- Date parse/format tests.
- Task parent deserialization tests (`null`, string, object).
- Cache tests:
  - corrupt cache -> fresh store
  - version mismatch -> fresh store
  - incremental merge behavior
  - stale fallback on API error
- API client tests with mock server:
  - auth header set
  - non-200 handling
- Snapshot tests for key TUI views (optional but recommended).

## 15. Build and Release

Development commands:

- `cargo fmt`
- `cargo clippy --all-targets --all-features`
- `cargo test`
- `cargo build`

Release build:

- `cargo build --release`

Version metadata parity with Go makefile:

- expose version, commit, and build date in root command `--version` output.

## 16. Definition of Done

The port is complete when:

- all commands and flags in Section 1 work as specified.
- interactive workflows in `setup`, `browse`, `new`, `standup`, `track`, `team-status`, and `menu` are implemented.
- cache behavior matches Section 6, including incremental task refresh and stale fallback.
- config/env/logging behavior matches Sections 7-9.
- AI commands function with Gemini key and degrade cleanly on missing key/errors.
- tests in Section 14 pass in CI.
