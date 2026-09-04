# Personal Growth Assistant

> Turn “I want to grow” into something you can finish today.

Personal Growth Assistant is a local-first Windows desktop app that connects tasks, calendars, reviews, industry intelligence, and evidence-based learning in one practical loop.

[Download](https://github.com/1ZYZz/personal-growth-assistant/releases/latest) · [中文说明](README.md) · [Chinese user guide](docs/user-guide.zh-CN.md) · [Contributing](CONTRIBUTING.md)

![Today view](docs/images/today.jpg)

## Highlights

- Today, calendar, recurring tasks, native reminders, snooze, trash, and capacity warnings.
- Local morning/evening/weekly reports that work without an account, network, or AI.
- Optional, isolated Codex-generated structured suggestions with budget and usage limits.
- Trusted-source ingestion, deduplication, scoring, bookmarks, and task conversion.
- Learning goals, prerequisite graphs, daily lessons, objective quizzes, mistakes, and FSRS reviews.
- Local SQLite storage, verified backups, safe restore, JSON export, no telemetry, and no automatic cloud sync.
- Complete Simplified Chinese and English interface.

## Quick start

1. Download the x64 installer from [GitHub Releases](https://github.com/1ZYZz/personal-growth-assistant/releases/latest).
2. Install and confirm language, timezone, notification, theme, and tray preferences.
3. Create your first task on **Today**.
4. Open the morning-brief card and choose **Generate local brief now**. Local briefs need no sign-in or network connection.

The installer is currently unsigned. Verify the SHA-256 published with the release before bypassing a Windows SmartScreen warning.

## Development

Prerequisites: Windows x64, Node.js 24.19, pnpm 11.19, Rust 1.98 MSVC, Microsoft C++ Build Tools, Windows SDK, and WebView2.

```powershell
pnpm install --frozen-lockfile
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm rust:fmt
pnpm rust:clippy
pnpm rust:test
pnpm sbom
pnpm desktop:build
```

Read [SECURITY.md](SECURITY.md) before reporting a vulnerability. Contributions are welcome under the [Apache License 2.0](LICENSE).
