# Release 1.1.0 verification

- Build date: 2026-09-04
- Product status: usable local Windows release
- Host: Windows 10 22H2 build 19045, x64

## Automated evidence

The final release gate completed with 5 desktop UI/locale tests, 4 bridge contract tests, and 35 Rust tests. Formatting, lint, strict TypeScript, strict Clippy, production bundling, and the CycloneDX SBOM gate also passed.

| Area          | Evidence                                                                                                                                                                |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frontend      | TypeScript strict check, ESLint, Prettier, production Vite build                                                                                                        |
| UI and locale | Vitest interaction tests and exact zh-CN/en-US key parity                                                                                                               |
| Rust core     | strict Clippy and all unit/integration tests                                                                                                                            |
| Calendar      | day/week/month views, navigation, copy, drag with reminder preservation, multi-select batch move                                                                        |
| Supervision   | morning/evening/weekly/industry schedules, idempotent enqueue, local reports, structured logs                                                                           |
| Intelligence  | RSS/Atom, bounded JSON API, authorized static-page selector, public-HTTPS and private-IP rejection, ingestion-stage persistence, cross-source exact-event deduplication |
| Learning      | adaptive daily plan, prerequisite and FSRS inputs, P0/P1 workload reduction, exact-diff proposal, expiring single-use approval token                                    |
| Agent         | isolated Codex home, per-run directory, read-only sandbox, single scoped MCP server, daily/monthly ceilings, identical-input cache                                      |
| MCP           | initialization negotiation, exact six-tool read-only registry, bounded snapshot, per-job scope rejection                                                                |
| Supply chain  | exact lockfiles, license gate, CycloneDX 1.6 SBOM                                                                                                                       |

## Installed-mode checklist

The final package was installed over the existing 1.0.0 data on the development host.

| Check                     | Result                                                                                                                                              |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Installer SHA-256         | `5281CFFA6B3BC6B54F058754383F6E21A2A85F6B54D116A6E2E885183DBDD418`                                                                                  |
| Installed desktop SHA-256 | `C3C51CD15A9286602949CF74FDEEB6BC75A7294297F85C6EEA06C6DC69B8DCD5`                                                                                  |
| Installed sidecar SHA-256 | `FDEB14BCBEC6A93DF583B53C78C30F083C6EC76469BEBC666503838BEF79F6F4`                                                                                  |
| Current-user install      | Silent upgrade exited 0 and left the app, sidecar, uninstaller, Start menu shortcut, and desktop shortcut in place                                  |
| Install location          | `%LOCALAPPDATA%\Personal Growth Assistant`                                                                                                          |
| Deep link                 | Installer registered `pga:` under HKCU with the installed executable; an invalid-nonce, no-side-effect URL launched the registered app successfully |
| Runtime smoke             | Direct launch remained healthy for 10 seconds at about 68.5 MiB; protocol launch remained healthy for 8 seconds at about 71.8 MiB                   |
| Upgrade and database      | Schema versions 1–6; `integrity_check=ok`; zero foreign-key violations; all pre-upgrade domain-row counts preserved                                 |
| Sidecar                   | Installed probe reports bridge 1.1.0, embedded Node 24.19.0, no database access, and the exact six registered read-only tools                       |
| Backup                    | A byte-identical pre-upgrade database copy was retained in the verification workspace; application daily backups remained available                 |

## Explicit external gates

- The installer and binaries are not Authenticode-signed because no project code-signing certificate was supplied.
- Automatic update publishing requires a public release endpoint and signing key; no silent or unsigned updater is enabled.
- Windows 11 hardware-matrix testing, a 48-hour reminder soak, and seven-day calibration require additional devices and elapsed time. They are not fabricated in this record and are not blockers for this personal local release.
