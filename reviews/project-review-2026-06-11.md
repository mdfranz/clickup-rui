# Project Review - 2026-06-11

## Findings

1. High: Activity attribution is often incorrect in `track` and `team-status`.
`updated`, `done`, and `closed` events are attributed to assignees, not the actual actor, so reports can mis-credit work. See [src/cmd/track.rs:120](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:120), [src/cmd/track.rs:155](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:155), [src/cmd/team_status.rs:63](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/team_status.rs:63), [src/cmd/team_status.rs:97](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/team_status.rs:97).

2. Medium: Terminal cleanup is not error-safe in standalone TUI helpers.
If `draw/poll/read` returns an error, `disable_raw_mode`/`LeaveAlternateScreen` is skipped, leaving the terminal in raw/alt mode. See [src/cmd/track.rs:368](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:368), [src/cmd/track.rs:404](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:404), [src/cmd/team_status.rs:254](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/team_status.rs:254), [src/cmd/team_status.rs:290](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/team_status.rs:290), [src/cmd/track.rs:425](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:425).

3. Medium: Recursive re-entry in browse flow can grow stack over repeated `n -> new task -> return` cycles.
See [src/cmd/browse.rs:549](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/browse.rs:549).

4. Medium: Task filtering is hardcoded to a small status set, so active custom workflows can disappear from default views.
See [src/util/filter.rs:12](/Users/matthew/Code/personal-github/clickup-rui/src/util/filter.rs:12).

5. Medium: Scroll behavior is hardcoded with `scroll + 5 < total_lines`, not viewport-aware.
This causes overscroll/underscroll depending on terminal height. See [src/cmd/track.rs:393](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:393), [src/cmd/team_status.rs:279](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/team_status.rs:279).

6. Low: Logging initializes before CLI parsing, so even `--help` can print startup errors (noise in normal UX).
See [src/main.rs:47](/Users/matthew/Code/personal-github/clickup-rui/src/main.rs:47), [src/main.rs:53](/Users/matthew/Code/personal-github/clickup-rui/src/main.rs:53).

7. Low: `config` validation errors return success (`Ok(())`) instead of non-zero failure, which hurts scripting/automation.
See [src/cmd/config_cmd.rs:29](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/config_cmd.rs:29), [src/cmd/config_cmd.rs:31](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/config_cmd.rs:31).

## Refactoring Opportunities

1. Extract shared activity-collection logic from `track` and `team-status` into a common service/module (same folder/list/task/comment scan pattern). See [src/cmd/track.rs:83](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/track.rs:83), [src/cmd/team_status.rs:26](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/team_status.rs:26).

2. Add a reusable TUI session guard (RAII) for raw-mode/alternate-screen lifecycle; this code is repeated in many commands. See [src/cmd/browse.rs:31](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/browse.rs:31), [src/cmd/new_task.rs:29](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/new_task.rs:29), [src/cmd/setup.rs:21](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/setup.rs:21), [src/cmd/standup.rs:35](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/standup.rs:35).

3. De-duplicate cache read/fetch/stale-fallback pattern in `CachedClient` with a generic helper to reduce surface area and bug risk. See [src/cache/client.rs:46](/Users/matthew/Code/personal-github/clickup-rui/src/cache/client.rs:46), [src/cache/client.rs:88](/Users/matthew/Code/personal-github/clickup-rui/src/cache/client.rs:88), [src/cache/client.rs:119](/Users/matthew/Code/personal-github/clickup-rui/src/cache/client.rs:119).

4. Extract shared “load folders/lists/tasks + apply `should_include_task`” pipeline from `tasks`, `summarize`, `standup`, and `browse`. See [src/cmd/tasks.rs:28](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/tasks.rs:28), [src/cmd/summarize.rs:20](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/summarize.rs:20), [src/cmd/standup.rs:66](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/standup.rs:66), [src/cmd/browse.rs:65](/Users/matthew/Code/personal-github/clickup-rui/src/cmd/browse.rs:65).

## Testing Gaps

- Tests exist only in cache/client, clickup/client, and config modules; command behavior (TUI flows, filtering, reporting semantics) is largely untested. See [src/cache/client.rs:487](/Users/matthew/Code/personal-github/clickup-rui/src/cache/client.rs:487), [src/clickup/client.rs:271](/Users/matthew/Code/personal-github/clickup-rui/src/clickup/client.rs:271), [src/config/mod.rs:103](/Users/matthew/Code/personal-github/clickup-rui/src/config/mod.rs:103).

- `cargo check` passes. `cargo test` was blocked in this environment for wiremock port binding, so runtime test confidence is partial here.
