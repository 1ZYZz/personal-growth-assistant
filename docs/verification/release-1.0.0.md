# Release 1.0.0 verification

- Build date: 2026-09-04
- Product status: usable local Windows release
- Host: Windows 10 22H2 build 19045, x64

## Automated evidence

The final gate completed with 5 desktop UI/locale tests, 3 bridge contract tests, and 28 Rust tests, in addition to formatting, lint, strict type checks, strict Clippy, production bundling, and the license/SBOM gate.

| Area          | Evidence                                                                                                      |
| ------------- | ------------------------------------------------------------------------------------------------------------- |
| Frontend      | TypeScript strict check, ESLint, Prettier, production Vite build                                              |
| UI and locale | Vitest interaction tests plus exact zh-CN/en-US key parity                                                    |
| Rust core     | strict Clippy and all unit/integration tests                                                                  |
| Recurrence    | all-day/floating/zoned, DST gap/fold, EXDATE/RDATE/COUNT/UNTIL, scoped edits, counted-series split            |
| Concurrency   | 20-way occurrence materialization creates one canonical task                                                  |
| Scheduler     | concurrent leases, retry, idempotent enqueue, pause/resume persistence, three-per-minute missed-item throttle |
| Database      | clean schema creation and v3-to-v5 migration preserving data, version, and foreign-key integrity              |
| Security      | URL/private-address rejection, canonicalization, HTML reduction, action parsing/nonces, environment isolation |
| Learning      | prerequisite edges, stage outcome/time fields, local grading, FSRS interval floor, separate mastery state     |
| Agent         | output schema, environment allowlist, runtime probe, daily ceiling, cancellation and process-tree termination |
| MCP           | initialization negotiation, exact read-only tool list, bounded snapshot, weak-token rejection                 |
| Supply chain  | exact lockfiles, license gate, CycloneDX 1.6 SBOM                                                             |

## Installed-mode checklist

The final packaging run passed on the development host:

| Check                | Result                                                                                          |
| -------------------- | ----------------------------------------------------------------------------------------------- |
| Installer SHA-256    | `81E69E76E23E91D72EF8F36FA565677027EAAE7D8BF27A5EFF04FB604B372CEC`                              |
| Desktop SHA-256      | `47501407DF501D370EA33031E4BDEB85122842DF222F132A81DF26D1390B52C9`                              |
| Sidecar SHA-256      | `750B1A0225819B2402E294D6046E70BEF8059F873D76F7AB43A3998BAEF03164`                              |
| Current-user install | Exit 0; version 1.0.0 registered under HKCU only                                                |
| Install location     | `%LOCALAPPDATA%\Personal Growth Assistant`                                                      |
| Deep link            | `pga:` registered under HKCU and points to the installed executable                             |
| Standalone files     | Desktop, sidecar, and uninstaller present; sidecar probe reports Node 24.19 embedded            |
| Runtime smoke        | Installed app remained healthy for eight seconds; working set 71,499,776 bytes                  |
| Upgrade and database | Schema versions 1–5; `integrity_check=ok`; zero foreign-key violations                          |
| Backup               | Startup created a consistent daily SQLite backup                                                |
| Uninstall            | Exit 0; install directory, HKCU registration, and optional startup entry removed; data retained |
| Reinstall            | Exit 0; final 1.0.0 installation left ready for use                                             |

## Explicit external gates

- This artifact is unsigned because no project code-signing certificate was supplied.
- Windows 11 25H2/26H1 hardware-matrix testing, 48-hour reminder soak, and seven-day calibration require elapsed time and additional devices. They are not fabricated in this record.
- Automatic update publishing requires a public release endpoint and signing key; no silent or unsigned updater is enabled.
