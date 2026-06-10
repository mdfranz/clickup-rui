# Logging Implementation Review

## Summary
The project uses **tracing + tracing-subscriber** with **JSON file-based logging**. The implementation is minimal but functional, with logging only in 4 files covering core infrastructure (network, caching). There are no info/debug logs in business logic, limited observability in many critical paths, and no log level controls.

---

## Current Architecture

### Dependencies
- **tracing 0.1**: Modern, structured logging framework
- **tracing-subscriber 0.3** with features: `fmt` (formatting), `json` (JSON output)

### Initialization (`main.rs:23-43`)
```rust
fn init_logging() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let log_path = crate::config::paths::get_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(&log_path)?;

    let file_layer = fmt::layer()
        .with_writer(std::sync::Mutex::new(file))
        .json()
        .with_target(true);

    let subscriber = Registry::default().with(file_layer);
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}
```

**Observations:**
- ✅ Append mode prevents data loss on reruns
- ✅ JSON output enables machine parsing
- ✅ Creates parent directories automatically
- ❌ Single layer (file only) — no console output
- ❌ No log level filtering
- ❌ No error handling for initialization failures beyond eprintln

### Log Locations
- **Default**: `$XDG_CACHE_HOME/clickup-tui/app.log` or `$HOME/.cache/clickup-tui/app.log`
- **Override**: Set `LOG_LOCAL=1` to write `./app.log` in current directory
- **Fallback**: `$TMPDIR/clickup-tui/app.log`

---

## Current Logging Coverage

### Files with Logging (4 total, 19 log statements)

#### 1. **main.rs** (2 logs)
```rust
tracing::error!("Failed to flush cache on exit: {}", e);  // Error level
```
- Only logs cache flush errors at exit
- Silent success path
- **Gap**: No logging for:
  - Logging initialization success/failure
  - Command routing
  - PAT retrieval success
  - Cache store loading

#### 2. **cache/store.rs** (2 logs)
```rust
tracing::warn!("Cache version mismatch. Starting fresh.");
tracing::warn!("Cache corrupt: {}. Starting fresh.", e);
```
- Logs cache corruption/version issues
- **Gap**: No logs for:
  - Successful cache load/save
  - Cache file size/location
  - Data structure validation

#### 3. **cache/client.rs** (12 logs, all WARN level)
```rust
tracing::warn!("API get_teams failed, using stale cache: {:?}", e);
tracing::warn!("API get_current_user failed, using stale cache: {:?}", e);
// ... 10 more similar patterns
```
- Consistent pattern: logs API failures, falls back to stale cache
- **Strengths**:
  - Clear fallback path visible
  - Error context included
- **Gaps**:
  - No success logs (silent on cache hits)
  - No distinction between empty cache vs. actual hit
  - No timing information (API latency)
  - No refresh/bypass behavior logged

#### 4. **clickup/client.rs** (3 logs)
```rust
tracing::debug!("Sending request: method={} url={} body={}", ...);  // Debug
tracing::debug!("Received response: method={} url={} status={} latency={:?}", ...);  // Debug
tracing::error!("API Error: status={} body={} url={}", ...);  // Error
```
- **Strengths**:
  - Request/response bracketing with latency
  - Sensitive data redaction via `LOG_RESPONSE_BODIES` env var
  - Status code captured
- **Gaps**:
  - DEBUG level — invisible unless level set globally
  - No query parameter logging (even redacted)
  - Response bodies only logged on error

---

## Missing Observability

### By Severity

**Critical (affects debugging production issues):**
- ❌ No logging in **cmd/** (all 10+ subcommands) — what users actually run
- ❌ No logging in **config/** (setup, validation, loading)
- ❌ No logging in **ai/** (LLM calls, prompt construction, errors)
- ❌ No logging in **ui/** (interaction, rendering, state changes)
- ❌ No log level control — all DEBUG logs invisible unless subscriber tweaked

**High (affects troubleshooting):**
- ❌ No success logs in cache operations (hits, saves, loads)
- ❌ No correlation IDs or request tracing
- ❌ No logs for command entry/exit
- ❌ No logs for configuration changes
- ❌ Error context incomplete (e.g., which API endpoint failed)

**Medium (nice-to-have):**
- ⚠️ No structured fields (tracing's key feature underutilized)
- ⚠️ No spans/events for command lifecycles
- ⚠️ No performance metrics (API counts, cache hit rates)

---

## Environment Variable Controls

| Variable | Current | Effect |
|----------|---------|--------|
| `CLICKUP_PAT` | Required | ClickUp API token (not logging-related) |
| `GEMINI_API_KEY` | Optional | AI provider key (not logging-related) |
| `LOG_LOCAL` | Optional | Write to `./app.log` instead of cache dir |
| `LOG_RESPONSE_BODIES` | Optional | Include full API response bodies in logs |
| `LOG_SENSITIVE_DATA` | Optional | Include request bodies (used for API calls) |
| `CLICKUP_TUI_MENU` | Optional | Menu mode flag (not logging-related) |

**Assessment:**
- ✅ `LOG_RESPONSE_BODIES` and `LOG_SENSITIVE_DATA` provide fine-grained control
- ❌ No `LOG_LEVEL` or `RUST_LOG` equivalent (tracing doesn't support env override in current setup)
- ❌ No option to write to both file and console
- ❌ No option to disable logging

---

## Issues & Recommendations

### 1. **No Log Level Control** (Priority: HIGH)
**Problem:** All DEBUG logs are invisible; can't dynamically adjust verbosity.

**Fix:**
```rust
use tracing_subscriber::filter::EnvFilter;

let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new("info"));

let subscriber = Registry::default()
    .with(env_filter)
    .with(file_layer);
```
**Benefit:** Users can set `RUST_LOG=debug` to troubleshoot, docs can recommend it.

---

### 2. **Console Output Only During Errors** (Priority: HIGH)
**Problem:** Users don't see logs unless they check the file; critical for TUI app.

**Fix:**
```rust
let console_layer = fmt::layer()
    .with_writer(io::stderr)
    .with_filter(tracing_subscriber::filter::LevelFilter::ERROR);

let subscriber = Registry::default()
    .with(env_filter)
    .with(file_layer)
    .with(console_layer);
```
**Benefit:** Errors surface immediately in stderr; file is detailed audit trail.

---

### 3. **Add Command Lifecycle Logs** (Priority: MEDIUM)
**Problem:** No visibility into which commands succeed/fail or how long they take.

**Add to `cmd/mod.rs` `route_command()`:**
```rust
pub async fn route_command(client: &CachedClient, cmd: Commands) -> Result<()> {
    let start = std::time::Instant::now();
    tracing::info!("Command started: {:?}", cmd);
    
    let result = match cmd {
        // ... existing match arms
    };
    
    match &result {
        Ok(_) => tracing::info!("Command completed successfully: elapsed={:?}", start.elapsed()),
        Err(e) => tracing::error!("Command failed: error={} elapsed={:?}", e, start.elapsed()),
    }
    
    result
}
```
**Benefit:** Full audit trail of what users did and outcomes.

---

### 4. **Add Cache Hit/Miss Metrics** (Priority: MEDIUM)
**Problem:** Can't tell if caching is working; stale-cache fallbacks are silent.

**Add to `cache/client.rs`:**
```rust
// On cache hit:
tracing::debug!(
    "cache_hit",
    key = "teams",
    age_ms = age,
    size = data.len(),
    refresh_bypassed = refresh
);

// On cache miss + API fetch:
tracing::info!(
    "cache_miss",
    key = "teams",
    fetch_time_ms = elapsed.as_millis() as u64,
    size = data.len()
);

// On fallback:
tracing::warn!(
    "cache_fallback",
    key = "teams",
    reason = "api_failed",
    fallback_age_ms = age,
    error = format!("{:?}", e)
);
```
**Benefit:** Performance debugging; identify when cache is thrashing.

---

### 5. **Improve API Error Logging** (Priority: MEDIUM)
**Problem:** API errors don't include request context (which endpoint, which parameters).

**Current:**
```rust
tracing::error!("API Error: status={} body={} url={}", status, err_body, url);
```

**Better:**
```rust
tracing::error!(
    method = %method,
    endpoint = %url,
    status = status.as_u16(),
    body = %err_body,
    "API request failed"
);
```
**Benefit:** Structured fields let logs be parsed/filtered; easier to correlate errors.

---

### 6. **Add Config/Setup Logging** (Priority: LOW)
**Problem:** Configuration path is invisible; can't debug setup issues.

**Add to `config/mod.rs`:**
```rust
tracing::debug!("Loading config from: {}", path.display());
tracing::info!("Config loaded: workspace_id={}, spaces={}", workspace_id, spaces.len());
```

---

### 7. **Add AI/LLM Logging** (Priority: LOW)
**Problem:** LLM calls and errors are invisible to users/debuggers.

**Add to `ai/summarizer.rs`:**
```rust
tracing::debug!("Sending prompt to LLM: provider={}, model={}, tokens~{}", provider, model, tokens);
tracing::info!("LLM response: provider={}, latency={:?}, output_tokens={}", provider, elapsed, tokens);
```

---

## Testing & Validation

### Current State
- ✅ Logging doesn't break on initialization error (silently degraded)
- ❌ No tests verify log output
- ❌ No integration tests check that logs contain expected messages

### Recommendations
```rust
#[cfg(test)]
mod tests {
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_logs_api_errors() {
        let (writer, handle) = tracing_test::traced_test::init();
        
        // Trigger an API error
        
        // Assert log was written
        assert!(handle.iter().any(|e| e.contains("API Error")));
    }
}
```

---

## Summary Table

| Aspect | Current | Status | Priority |
|--------|---------|--------|----------|
| Log framework | tracing/tracing-subscriber | ✅ Good choice | — |
| Output format | JSON file | ✅ Good for audit | — |
| Log levels | Error/Warn/Debug only | ⚠️ No Info | MEDIUM |
| Filtering | None (all visible by level) | ❌ No control | HIGH |
| Console output | None | ❌ Silent by default | HIGH |
| Coverage | 4 files, 19 statements | ❌ Sparse | MEDIUM |
| Structured fields | Minimal use | ⚠️ Using strings instead | LOW |
| Correlation | None | ❌ No tracing spans | LOW |
| Tests | None | ❌ No log validation | LOW |

---

## Quick Wins (Low effort, high value)

1. **Add `EnvFilter` for log level control** (5 min)
2. **Add ERROR-level console layer** (5 min)
3. **Add logs to `route_command`** (10 min)
4. **Document `RUST_LOG` in README** (2 min)

These four changes give users visibility into errors immediately while keeping the detailed audit trail in the file.
