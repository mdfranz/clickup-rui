# Backlog

Synthesized from `reviews/` (2026-06-11). Sources: `code-review-11-june.md`, `agy-11-june.md`, `project-review-2026-06-11.md`, `LOGGING_REVIEW.md`.

## Legend

| Field | Values |
|-------|--------|
| **Priority** | P1 = bug / broken out-of-box · P2 = reliability / maintainability debt · P3 = polish / docs / perf |
| **LOE** | S < 1 hr · M 1–4 hrs · L 4+ hrs architectural · XL multi-day |
| **Risk** | Low = mechanical, no behaviour change · Med = localised, easy to test · High = shared hot path |

---

## P1 — Bugs (fix first)

All P1 items resolved on 2026-06-11.

| # | Task | LOE | Risk | Status | Notes |
|---|------|-----|------|--------|-------|
| 2 | Fix `track.rs:231` filename format `%M` (minute) → `%H%M%S`; delete two leaked data files from repo root | S | Low | ✅ Done | Format changed to `%y%m%d-%H%M%S`; files deleted from working directory |
| 3 | Fix `new_task.rs:117` off-by-one clamp: `> filtered_users.len()` → `>= filtered_users.len()` in user-picker | S | Med | ✅ Done | |
| 4 | Fix `browse.rs` recursive future on `n` press — replace `Box::pin(run_browse_loop(...))` with `'reload` loop | M | Med | ✅ Done | Wrapped `run_browse_loop` body in `'reload: loop`; `n` does `continue 'reload`, `q` does `break 'reload` |
| 5 | Fix activity attribution in `team_status.rs` — `done`/`closed`/`updated` events were credited to all assignees | M | Med | ✅ Done | Now attributed to `task.creator` (best available proxy); `track.rs` single-user heuristic unchanged |
| 6 | Add `EnvFilter` to logging init; add `env-filter` feature to `Cargo.toml` | S | Low | ✅ Done | Defaults to `info`; override with `RUST_LOG=debug` |

---

## P2 — Tech Debt & Fragility

| # | Task | LOE | Risk | Notes |
|---|------|-----|------|-------|
| 7 | Implement `TerminalGuard` RAII struct (Drop restores raw-mode + alt-screen) — replace ~6 copies of TUI setup/teardown boilerplate | L | Med | ✅ Done — safely manages TUI state, preventing terminal corruption on panics and eliminating setup/teardown boilerplate in `browse.rs`, `standup.rs`, `new_task.rs`, `menu.rs`, `setup.rs`, and `track.rs` |
| 8 | Remove dev chain-of-thought comments from `standup.rs:401-424`; port `task_list_map` from `browse.rs:65` to fix O(folders×lists×tasks) search | M | Med | Shipped AI reasoning left as source comments |
| 9 | Extract shared `collect_activities()` helper — activity pipeline duplicated between `track.rs:67-200` and `team_status.rs:12-140` | M | Med | ✅ Done — shared collector keeps team/user attribution policies explicit |
| 10 | Extract `wrap_text_by_chars` to `src/ui/` — currently duplicated in `browse.rs:673`, `standup.rs:496`, `new_task.rs:659` | S | Low | Pure function, no side effects |
| 11 | Replace `set_menu_mode()` env-var mutation with an explicit flag through `route_command` | M | Med | ✅ Done — `RouteContext` carries menu mode without mutating process environment |
| 12 | Unify `Status.color: String` and `TaskStatus.color: Option<String>` — make both `Option<String>`, remove empty-string sentinels | S | Med | Forces sentinel construction at `browse.rs:521-528`, `standup.rs:429-436` |
| 13 | Add `AppError::Cancelled` variant; stop using `Other("Setup cancelled")` for control flow | S | Low | Cancellation prints as an error in `main.rs` |
| 14 | Replace `reqwest::Client::builder().build().unwrap_or_else(|_| Client::new())` with `.expect()` | S | Low | Silently drops timeout config if builder fails; `clickup/client.rs:19-20`, `ai/summarizer.rs:78-80` |
| 15 | Resolve `#[allow(async_fn_in_trait)]` + inconsistent `impl Future` vs `async fn` in `clickup/api.rs` — pick one style | M | Low | `get_task_detail`/`get_task_comments` diverge from all other methods |
| 16 | Fix `config_cmd.rs` magic-string provider detection — compare by provider field, not model name | S | Low | Auto-switch silently no-ops when user has a custom model |
| 17 | Add `EnvFilter` stderr error layer; add command lifecycle logs in `route_command()` | M | Low | LOGGING_REVIEW recs #2 and #3; no cmd/ subcommand emits any logs today |
| 18 | Annotate `cache/store.rs` unit mismatch: `fetched_at` (seconds) vs `max_date_updated` (ms); mark clearly or migrate to `chrono::DateTime` | S | Low | Already works but will bite on maintenance |
| 19 | Widen `Spinner::start` signature from `&'static str` to `impl Into<String>` or `Cow<'static, str>` | S | Low | Forces callers to leak statics |
| 20 | Add `#[derive(Debug)]` to `SetupStep` (`setup.rs:14`) and other state-machine enums missing it | S | Low | Makes debugging state machines significantly easier |
| 21 | Replace wildcard `use crate::clickup::models::*` with explicit imports in `client.rs`, `cache/client.rs`, `ai/summarizer.rs` | S | Low | Harms readability and IDE navigation |
| 22 | Surface `AiSummarizer::new()` config-load error instead of silently using broken defaults | S | Low | Silent fallback includes the broken `gemini-3.5-flash` model |

---

## P3 — Polish, Docs & Performance

| # | Task | LOE | Risk | Notes |
|---|------|-----|------|-------|
| 23 | Update README: fix "8 options" → 6, add Ollama + `config` subcommand section, add `--clear-cache`, document env vars | M | Low | ✅ Done — documents cache flags, AI configuration, and logging controls |
| 24 | Update or retire `RUST-PORT.md`: fix binary name `clickup-tui`→`clickup-rui`; update `BrowseState` variants; add `config` command | S | Low | ✅ Done — retained only as archived implementation history |
| 25 | Move `RUST-PORT.md` and `LOGGING_REVIEW.md` to `docs/`; clean repo root of stale spec files | S | Low | ✅ Done for `RUST-PORT.md`; `LOGGING_REVIEW.md` remains under `reviews/` |
| 26 | Run `cargo clippy --fix` to resolve 15 of 16 mechanical warnings | S | Low | ✅ Done — strict `cargo clippy -- -D warnings` passes |
| 27 | Harden hardcoded status filter in `util/filter.rs` — expose as config field or named constant | M | Med | `"in progress" | "in review" | "blocked" | "scoping"` breaks non-default ClickUp workflows |
| 28 | Add `--days` flag to `track` command consistent with `team-status`; remove hardcoded 10-day window at `track.rs:79` | M | Low | ✅ Done — defaults to 10 days and accepts `--days` / `-d` |
| 29 | Parallelize folder→list→task fetches with `futures::stream::iter().buffer_unordered(K)` in browse/standup/track/team_status | L | Med | Currently O(N×M) serial round-trips |
| 30 | Convert `tasks.rs:296-318` `render_task_node` `Box::pin` recursion to iterative DFS | M | Med | Deep subtask graphs will stack-overflow |
| 31 | Fix scrollable TUI scroll to be viewport-aware instead of hardcoded `scroll + 5 < total_lines` | M | Low | Breaks on short terminals |
| 32 | Fix logging init order — initialise after CLI parse so `--help` doesn't emit log startup noise | S | Low | Minor UX issue |
| 33 | Fix `config` subcommand returning `Ok(())` on validation failure — should exit non-zero | S | Low | Breaks scripting / CI usage |
