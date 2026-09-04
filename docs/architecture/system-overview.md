# System overview

```text
React webview -> typed Tauri commands -> Rust application services -> SQLite
                                           |-> leased scheduler
                                           |-> native Windows toast/tray adapter
                                           |-> trusted-source ingestion
                                           |-> learning + FSRS
                                           |-> isolated Codex CLI
                                           `-> packaged, scoped MCP bridge
```

## Authority and data flow

- SQLite in the per-user application-data directory is the only canonical business store. WAL, foreign keys, a 10-second busy timeout, append-only schema versions, and pre-restore backups are enabled.
- React submits typed intent and renders returned state. Its Tauri capability contains core IPC and the URL opener only; it has no database, shell, or arbitrary filesystem permission.
- Rust validates domain transitions, expected versions, recurrence identity, URLs, quiz submissions, and backup paths. Scheduler leases and idempotency keys serialize side effects after restarts or sleep.
- Recurrence series remain compact. Future occurrences are expanded virtually and materialized only when an occurrence receives a reminder, edit, status, or progress event. Single-occurrence edits persist an override; future edits split the series with a stable cutoff; entire-series edits increment the optimistic series version.
- Industry feed bodies are bounded and normalized before storage. The display URL is always the canonical URL associated with the source and item record.
- Objective quizzes are graded locally. FSRS determines review timing; mastery is computed from separate evidence records. Knowledge edges, stage outcomes, and time estimates form the local learning roadmap.

## AI boundary

The Codex adapter launches only from Rust with an explicit environment allowlist, a dedicated `CODEX_HOME` under `%LOCALAPPDATA%\\PersonalGrowthAssistant`, a fresh working directory for each job, a timeout, user cancellation with Windows process-tree termination, a JSON Schema, ignored project rules, disabled Shell and web search, and read-only sandbox selection. The normal user Codex directory is never copied into the isolated home.

`pga-agent-bridge.exe` is produced as a standalone Node Single Executable Application and bundled as a Tauri external binary. Its `mcp` subcommand requires a random one-job session token, an explicit allowed-tool scope, and a typed snapshot no larger than 1 MiB. The registry supports bounded reads for profile preferences, tasks, task events, enabled watch fields, verified news history, and learning state, but each job advertises only its allowlist. It cannot open SQLite, fetch arbitrary URLs, or invoke operating-system commands.

The morning-brief job injects only `get_tasks`; the short-lived token, snapshot path, job ID, and scope are forwarded through environment variables and never written to `config.toml`. Lesson-to-task changes use an exact-diff proposal created by Core, a five-minute one-use approval token, and optimistic version validation. Canonical task writes are not exposed through the current read-only Agent job.

## Packaging

The Windows release is an x64, current-user NSIS installer with a stable package identifier and `pga:` deep-link scheme. An optional current-user Run entry is managed with fixed Windows registry arguments and removed by the uninstaller. The application, sidecar, database migrations, third-party notice text, and native notification implementation are compiled into the release. End users do not need Node.js, pnpm, Rust, or administrator rights.
