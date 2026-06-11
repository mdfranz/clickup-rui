# Code Review — 2026-06-11

Scope: tech debt, scalability, fragility, Rust antipatterns, and documentation drift across the entire `clickup-rui` codebase. Findings ranked by impact within each category. File:line references throughout.

---

## 🔴 Bugs — fix first

1. **`gemini-3.5-flash` is not a real Google model.** Default in `src/config/mod.rs:19`, `src/ai/summarizer.rs:72`, `src/cmd/setup.rs:293`, and `src/cmd/config_cmd.rs:35-38`. Real models are `gemini-1.5-flash` / `gemini-2.5-flash`. Out-of-the-box AI calls hit a 404. README/help even displays this as the example.
2. **`track.rs:231` filename uses `%M` (minute) not month or hour** — `format!("{}-{}", user.id, Local::now().format("%y%m%d%M"))`. Two evidence files in the repo root (`111900148-26061034.csv`, `118039156-26061040.json`) show two exports differ only by minute (`34` vs `40`). Output collides whenever you export twice the same minute, and the date is unreadable. Should be `"%y%m%d-%H%M%S"` or similar.
3. **Stale data files committed** at repo root (`111900148-26061034.csv`, `118039156-26061040.json`). `.gitignore` lists `*.json`/`*.csv` but these were already tracked. Will leak ClickUp data on next push.
4. **`new_task.rs:117-119` off-by-one clamp** — `if current_selected > filtered_users.len()` should be `>=` (selectable indices are `0..=filtered_users.len()` because index 0 is "Unassigned" and last index is `filtered_users.len()`, so a value `== len()` is valid; that's fine — but the condition `>` lets the cursor sit at `len()+1` which then causes the down-arrow `if i < filtered_users.len()` to silently swallow inputs). Whole user-picker boundary logic is brittle; reread `:551-579`.
5. **`standup.rs:401-424` is unreviewed AI chain-of-thought** left in source as comments ("Wait, let's verify...", "Actually, in models we didn't specify..."). Worse, the implementation it ended up shipping is **O(folders × lists × tasks)** to find one task's `list_id` for the status picker — re-fetches every list of every folder and scans every task. `browse.rs` solved this correctly with a `task_list_map` (line 65) — port that pattern back into standup.
6. **`browse.rs:539-549` recursive futures.** Pressing `n` spawns `run_new_task`, then recursively calls `Box::pin(run_browse_loop(...)).await`. Each `n` press grows the future stack permanently for the rest of the session. Should be a loop with re-fetched tasks, not recursion.
7. **Logging defaults to TRACE.** `src/main.rs:35-41` builds a JSON file layer with no `EnvFilter` — every `tracing::debug!`/`trace!` from any crate is written. The log file grows quickly. Add `EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))`.
8. **`Cargo.toml` declares `tracing-subscriber` with `fmt, json` features but no `env-filter`** — needed for the fix above.

---

## 🟠 Tech debt / duplication

- **`wrap_text_by_chars` duplicated 3×** (`browse.rs:673`, `standup.rs:496`, `new_task.rs:659`). Move to `src/ui/`.
- **TUI setup/teardown boilerplate is copy-pasted ~6 times** (`browse.rs:30-52`, `standup.rs:34-56`, `new_task.rs:28-50`, `menu.rs:46-54`+255-261, `setup.rs:20-48`, `track.rs:335-414`, `track.rs:425-606`). Extract a `with_terminal(|term| ...)` helper that handles raw mode + alt screen + mouse capture + cleanup on panic.
- **Identical fallback statuses array** in `browse.rs:521-528` and `standup.rs:429-436`. Should be a single constant in `clickup::models` or `ui`.
- **`AiSummarizer` aliased as `GeminiSummarizer`** (`src/ai/summarizer.rs:61`) — every caller still imports the alias even though Ollama is supported. Misleading; rename callers.
- **Activity-extraction pipeline is duplicated** between `track.rs:67-200` and `team_status.rs:12-140`. The differences are (a) filtering by user, (b) per-assignee fan-out. Both could share a `collect_activities(api, cfg, since_ms, filter)` helper.
- **Hard-coded 10-day window** in `track.rs:79`. `team-status` accepts `--days`; `track` should too.
- **`route_command` requires `Clone + 'static`** in `cmd/mod.rs:19` but only `menu` actually clones. Other paths can take `&A: ClickUpApi`. Either narrow the bound per-command or document why it's pinned.
- **`new_task.rs:52` `#[allow(unused_assignments)]`** masks a real flow issue: `selected_folder` and `selected_status` get assigned redundantly. Restructure rather than suppress.
- **`tasks.rs:87-129` defines `check_passes_filter` as an inner fn taking 7 args** plus a manual memo cache. This is a free function — extract it; consider `fn(&Task) -> bool` with a memo wrapped in a closure.

---

## 🟡 Scalability / performance

- **All bulk commands fetch sequentially.** `browse.rs:67-80`, `tasks.rs:28-52`, `summarize.rs:20-49`, `standup.rs:69-83`, `track.rs:83-193`, `team_status.rs:26-133` walk folders → lists → tasks (and sometimes comments) one at a time. With N folders × M lists, latency is N×M serial round trips. Use `futures::stream::iter(...).buffer_unordered(K)` (or hand-rolled `tokio::join!`/`JoinSet`). `tracing::debug` already records latency per request — easy to verify the win.
- **Cache is one big JSON, fully rewritten on every save.** `cache/store.rs:103-129`. Fine at hundreds of tasks; painful at thousands. Long-term: split per-list to per-file, or move to sled/sqlite. Short-term: compact with `to_string` (not pretty), and skip save when no writes happened (already gated by `dirty`, ✓).
- **`get_tasks` returns a `Vec<Task>` clone every call** (`cache/client.rs:299, 312, 348, 352`). For "give me my open tasks" callers, the cache hands out an owned vector that callers then iterate and re-clone. Returning `Arc<Vec<Task>>` (or yielding the items) would save allocations on hot paths.
- **`browse.rs:123-153` spawns a `tokio::spawn` on every selection change** when uncached. Background fetcher uses `std::sync::Mutex` (not `tokio::sync::Mutex`) on `cached_task_details`/`cached_comments` (lines 100-102). Locks are short, but the same fetch will fire repeatedly while loading because `loading_tasks` is on the parent task only and not visible to the spawn. Once cache lookups happen via the `CachedClient` *anyway*, this whole hand-rolled cache layer in `browse` can probably be deleted.
- **`tasks.rs:296-318` recurses `render_task_node` for subtasks** with `Box::pin`. Subtask-of-subtask graphs deeper than ~thousand will overflow. Convert to iterative DFS with an explicit stack.

---

## ⚪ Fragility

- **Unit drift in `TaskListCacheEntry`** (`cache/store.rs:22-27`): `fetched_at` is seconds, `max_date_updated` is milliseconds. They're compared against constants of different units in `cache/client.rs:265`. It works today but is exactly the kind of mismatch that bites later. Add a comment, or prefer `chrono::DateTime` everywhere.
- **`set_menu_mode` mutates env vars** at runtime (`util/env.rs:34-40`). `std::env::set_var` is unsound on Unix when other threads can read env (Rust 1.85 made it `unsafe`). Plumb a flag through arguments instead.
- **No bound on cache staleness fallback.** On API error, `cache/client.rs:80-84` etc. return *expired* cache silently. With a 30-day stale value plus a flaky API, the user sees yesterday's tasks with no signal that the data is old. Consider warning the user when stale-fallback fires (currently only goes to `tracing::warn!` in a file).
- **`reqwest::Client::builder().build().unwrap_or_else(|_| reqwest::Client::new())`** in `clickup/client.rs:19-20` and `ai/summarizer.rs:78-80` silently drops timeout config if the builder fails. The builder won't fail in practice, but the safer construct is `expect("reqwest client")`.
- **`Vec::iter().any(|u| u.id == user_id)` in tight loops** without indices on assignees: fine at ClickUp scale, just noting it.
- **`Esc` semantics in `new_task.rs`** quit the whole wizard from inside `NameInput`/`DescriptionInput` (line 402-404), surprising users who just want to clear/cancel the text field.
- **`config_cmd.rs:35-38` magic-string detection** — auto-switching between providers compares against literal `"gemini-3.5-flash"` / `"granite4.1:8b"`. As soon as a user customizes their model, the auto-switch silently no-ops. Better: keep separate `gemini_model` and `ollama_model` fields, or detect by provider at use-time.

---

## 🧱 Rust antipatterns

- **`#[allow(async_fn_in_trait)]`** on `ClickUpApi` (`clickup/api.rs:4`) plus inconsistent declaration style: `get_task_detail`/`get_task_comments` use `impl Future<Output = ...> + Send` while every other method uses `async fn`. Pick one. Long-term, `async-trait` or RPITIT (Rust 1.75+) for cleaner trait shape.
- **`enum SetupStep`** (`setup.rs:14`) and several others **lack `#[derive(Debug)]`** — debugging state machines is harder than it needs to be.
- **`Status` vs `TaskStatus` asymmetry** (`clickup/models.rs:34-56`): `Status.color: String`, `TaskStatus.color: Option<String>`. This forces empty-string sentinels (`color: String::new()`) when constructing fallback statuses (`browse.rs:521-528`, `standup.rs:429-436`). Make both `Option<String>`.
- **Wildcard imports** `use crate::clickup::models::*;` in `clickup/client.rs:2`, `cache/client.rs:4`, `ai/summarizer.rs:1`. Explicit imports help readability and IDE jump-to.
- **`Arc<Mutex<CacheStore>>` is an `async` Mutex**, but `browse.rs` keeps a parallel `std::sync::Mutex` cache. Pick one mutex flavor per layer; mixing them is a footgun.
- **`Result<T, AppError>`** carries a generic `Other(String)` variant used for control flow ("Setup cancelled by user.", `setup.rs:191`). Cancellation isn't an error — split it out (e.g., `Cancelled`) so `main.rs` can suppress the prefix and exit code without string matching.
- **`Spinner::start(message: &'static str)`** forces literal strings. `Cow<'static, str>` or `String` would let callers parameterize messages without leaking statics.
- **`as u16`/`as i64` casts everywhere** (e.g., `cache/ttl.rs:18, 25`): lossy on wraparound. Prefer `try_into` at boundaries you don't control.
- **Pattern `rep.new_status.is_some() && rep.new_status.as_ref().unwrap().status...`** in `standup.rs:346-348`. Use `if let Some(ns) = &rep.new_status`.
- **`AiSummarizer::new()` panics-fallbacks via `Config::load().unwrap_or_else`** (`ai/summarizer.rs:65`). If config is missing, the summarizer silently picks defaults — including the broken `gemini-3.5-flash`. Surface the error.
- **Tests use `static ENV_MUTEX`** (`config/mod.rs:109`) to serialize env mutation across tests — symptom of `is_menu_mode()` and config path resolution depending on env. Fixing the env-as-global-state issue removes this hack.

---

## 📚 Documentation drift (README, RUST-PORT, LOGGING_REVIEW)

- **README claims "all 8 navigation options remain fully visible"** (line 31). Menu actually has 6 (`menu.rs:15-44`): Browse, Track, New, Standup, Setup, Show. `tasks`, `summarize`, `team-status`, `cache`, `config`, `clean` are not in the menu.
- **README markets AI as "Gemini-Powered"** (lines 5, 38-43, 91). Code supports Ollama too — `Config::ai_provider`, `clickup-rui config --provider ollama --model granite4.1:8b`. README never mentions the `config` subcommand or Ollama.
- **README Command Reference (lines 105-119) is missing `config`.** Listed in CLI help.
- **README does not document `--clear-cache` global flag** (only mentions `--refresh`-equivalent indirectly).
- **Setup wizard envs:** README only mentions `CLICKUP_PAT` and `GEMINI_API_KEY`. `LOG_LOCAL`, `LOG_RESPONSE_BODIES`, `LOG_SENSITIVE_DATA`, `CLICKUP_TUI_MENU` are user-relevant (debugging) and undocumented in README. (RUST-PORT.md does document them, but that file isn't user-facing.)
- **README "Gemini-Powered" lists default model as implicit gemini-3.5-flash** (in installation flow). With the bug above, the docs imply something that doesn't work out of the box.
- **`RUST-PORT.md:24` says binary name is `clickup-tui`** — actual binary is `clickup-rui`. Also `RUST-PORT.md:160` specifies `Status { color: String, type_: String }` (non-optional), but `TaskStatus` (the variant used for tasks) actually uses `Option<String>`. Spec hasn't been updated for what shipped.
- **`RUST-PORT.md:411-414` lists `BrowseState` variants** including `NewTaskOverlay`. Actual code has only `List | CommentEditor | StatusPicker`; the `n` key shells out and recurses. Spec drifted.
- **`RUST-PORT.md:11-13` and `:580-585` describe commands `cache info` / `cache clear`**, but does not list `config` (added later).
- **`LOGGING_REVIEW.md` is a 350-line audit doc with action items dated implicitly**. None of the gaps it identifies (no env filter, no startup log, no command-router log) have been addressed since it was written. Either implement the recommendations or move it to `docs/historical/` so it stops looking like a TODO.
- **Repo-root noise:** `RUST-PORT.md`, `LOGGING_REVIEW.md`, two stray `.csv`/`.json` data exports — none are gitignored or filed. Move docs into `docs/`, delete or untrack the data files.

---

## ✅ What's solid (worth preserving)

- Cache layer's stale-fallback semantics are clean and consistent (every read method follows the same pattern).
- Atomic cache writes via temp+rename (`cache/store.rs:121-125`).
- Test coverage on cache merge, stale fallback, and status update is genuinely useful.
- Wiremock-based client tests verify auth header and error handling.
- `ClickUpApi` trait abstraction lets tests substitute mocks cleanly.

---

## Recommended next steps (concrete, ordered)

1. Fix the broken default model and the `track.rs` filename format (10-minute change).
2. Untrack and delete the two data files at repo root; verify `.gitignore` actually catches them on regen.
3. Add `EnvFilter` to logging init and the `env-filter` feature to `Cargo.toml`.
4. Extract `tui_session` and `wrap_text_by_chars` helpers; remove the AI-ramble comments in `standup.rs:401-406`; port `task_list_map` into standup.
5. Update README: drop the "8 options" line, add Ollama + `config` subcommand docs, add `--clear-cache`, list relevant env vars.
6. Replace `set_menu_mode` env mutation with an explicit parameter passed through `route_command`.
7. Move `RUST-PORT.md` and `LOGGING_REVIEW.md` to `docs/`; treat `RUST-PORT.md` as a now-stale spec or update it to match shipped behavior.
8. Address clippy's 16 warnings (`cargo clippy --fix` resolves 15 mechanically).
