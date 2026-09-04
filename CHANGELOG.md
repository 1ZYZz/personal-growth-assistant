# Changelog

## [1.1.1] - 2026-09-04

### Fixed

- Week and month calendar cards now reserve space for controls, clamp long titles to two readable lines, and expose copying as a compact accessible icon instead of squeezing Chinese text into a vertical column.
- Local and Codex-enhanced morning brief actions now use distinct labels so the zero-AI path and optional Runtime path are immediately distinguishable.

## [1.1.0] - 2026-09-04

### Added

- Day, week, and month calendar modes with drag rescheduling, independent task copying, multi-select batch moves, and reminder-preserving edits.
- Configurable completion criteria for one-off and recurring tasks, plus five-outcome evening reviews and local morning/evening/weekly reports.
- Rule-based automatic digests, scheduled intelligence refreshes, daily lesson plans that adapt to P0/P1 workload, and exact-diff lesson-to-task proposals with five-minute one-use approval tokens.
- JSON API and basic static-page monitoring, six-stage ingestion telemetry, cross-source event deduplication, region/language controls, reading budgets, and configurable ranking weights.
- Learning language/resource/budget preferences, prerequisite-aware daily plans, and first-run guides for the Supervisor and Tutor modules.
- Daily/monthly Codex limits, saving/standard/deep reasoning preferences, ten-minute result caching, structured local failure logs, and 14-day/100-MiB log retention.
- A scoped MCP read registry for profile, tasks, task events, watch fields, verified news history, and learning state; each job advertises only its explicit allowlist.
- Schema version 6, expanded privacy-safe export, and automatic expiry of 30-day recycle-bin snapshots.

### Security

- Codex jobs now use `%LOCALAPPDATA%\\PersonalGrowthAssistant\\codex-home`, ephemeral per-job work directories, no Shell or web-search tools, ignored project rules, a required local MCP bridge, and temporary snapshot/token/scope environment variables.
- Old recycle-bin snapshots remain backward compatible after the task completion-criteria schema change.

## [1.0.0] - 2026-09-04

### Added

- Local-first Windows task workspace, calendar, review, trash, backup, restore, export, preferences, and bilingual first-run experience.
- Deterministic RFC 5545 recurrence, scoped series editing, and a three-mode time model with system-timezone reconciliation and transactional virtual-occurrence materialization.
- Leased scheduler, reconciliation, Windows action toasts, fallback notification-center actions, pause/quiet controls, full tray menu, deep links, nonce replay protection, and current-user startup.
- Trusted-source intelligence ingestion with SSRF defenses, canonical links, deduplication, ranking, and feedback.
- Learning goals, prerequisite relationships, stage outcomes, knowledge-node time estimates, objective quizzes, mistake book, mastery evidence, and local FSRS scheduling.
- Optional isolated Codex runtime, cancellable structured morning briefs, bounded automatic-use policy, and a standalone read-only MCP SDK v2 bridge.
- Append-only migrations through schema version 5, upgrade validation, CycloneDX SBOM, release documentation, and Windows NSIS packaging.

### Security

- SQLite remains inaccessible to the webview and sidecar.
- Network content cannot grant tools or supply displayed URLs outside canonical source records.
- AI subprocess environments are allowlisted and use a dedicated `CODEX_HOME`.

### Packaging note

- This locally built installer is unsigned. Code signing and multi-device Windows 11 certification require external release credentials and hardware and are not represented as completed by this build.
