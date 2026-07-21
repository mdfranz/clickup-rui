# External Dependencies & Library Classifications (`PKG.md`)

This document enumerates and classifies the external libraries (crates) used across the **`clickup-rui`** project, explaining their responsibilities and target usages.

---

## 1. Categorized Library Matrix

| Category | Library | Used In | Primary Purpose |
| :--- | :--- | :--- | :--- |
| **Terminal User Interface (TUI)** | `ratatui` | `clickup-rui` | Constructing terminal layouts, dual-pane browser views, lists, and form wizards. |
| | `crossterm` | `clickup-rui` | Handling low-level terminal control, raw mode toggles, and keyboard/mouse events. |
| | `termimad` | `clickup-rui` | Rendering formatted Markdown text inside terminal panels (e.g. task descriptions/comments). |
| **Command Line Interface** | `clap` | `clickup-rui` | Declarative command line argument parsing and subcommand routing. |
| **HTTP Client & Integration** | `reqwest` | `clickup-rui` | Asynchronous HTTP network client for ClickUp and Gemini/Ollama APIs. |
| **Serialization & Formats** | `serde` & `serde_json` | `clickup-rui` | Serializing/deserializing structured API payloads, offline cache, and state records. |
| | `toml` | `clickup-rui` | Parsing and writing the primary configuration files (`config.toml`). |
| **Configuration & Paths** | `dirs` | `clickup-rui` | Resolving platform-agnostic OS directories for config, cache, and logs. |
| **Async & Utilities** | `tokio` | `clickup-rui` | Multithreaded asynchronous execution runtime. |
| | `chrono` | `clickup-rui` | Managing timestamps, date formatting, delta synchronization, and TTL bounds. |
| **Diagnostics & Logging** | `tracing` & `tracing-subscriber` | `clickup-rui` | Structured logging and diagnostics, incorporating `EnvFilter` for dynamic level tuning. |
| **Error Handling** | `thiserror` | `clickup-rui` | Deriving domain-specific structured error types. |
| | `anyhow` | `clickup-rui` | Propagating high-level execution context and dynamic errors. |
| **Testing Utilities** | `assert_cmd` | `clickup-rui` (dev-dep) | Validating CLI command execution, exit codes, and flag arguments. |
| | `insta` | `clickup-rui` (dev-dep) | Snapshot-asserting output format blocks and deserialization states. |
| | `tempfile` | `clickup-rui` (dev-dep) | Generating temporary directory roots for configuration and cache tests. |
| | `wiremock` | `clickup-rui` (dev-dep) | Mocking external HTTP server responses during unit and integration testing. |

---

## 2. Detailed Dependency Roles

### TUI Rendering Engine (`ratatui` & `crossterm`)
The main interface runs in terminal raw mode using `crossterm` as the backend and `ratatui` as the drawing library. `ratatui` handles layouts, block definitions, and custom widget states (e.g., active/inactive border highlighting, list index state). `crossterm` intercepts key events and ensures clean exit states.

### Document Rendering (`termimad`)
Because task descriptions and comments in ClickUp support rich markdown formatting, `termimad` is utilized to dynamically compile and skin the markdown content inside the `ratatui` description blocks, wrapping lines according to the active terminal dimensions.

### Network & Data Pipelines (`reqwest` & `serde`)
The ClickUp Client and AI Summarizer modules interact asynchronously over HTTPS with external API endpoints. `reqwest` provides the underlying connection pools and handles SSL certificates. `serde` and `serde_json` perform compile-time code-generation of model definitions, verifying API payloads statically.
