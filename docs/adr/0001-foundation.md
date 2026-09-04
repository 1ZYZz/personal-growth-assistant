# ADR-0001: Stage 0 foundation

- Status: Accepted
- Date: 2026-09-03

## Context

PGA is a Windows-first, local-first desktop application. Stage 0 must prove packaging, interactive notifications, scheduling and recurrence integrity, and an isolated AI bridge before product features are built.

## Decision

- Use a pnpm workspace with a Tauri 2 desktop application under `apps/desktop`.
- Use React 19, TypeScript 6, Vite 8, Fluent UI React v9, and i18next.
- Use a Rust application core. React does not receive database or arbitrary process authority.
- Package Windows releases as current-user NSIS installers using the fixed identifier `org.personal-growth-assistant.desktop`.
- Keep unimplemented product routes absent. The Stage 0 UI exposes readiness information only.
- Pin JavaScript dependencies exactly in manifests and `pnpm-lock.yaml`.
- Pin Rust dependency resolution in `Cargo.lock` once the Rust MSVC toolchain is installed.

## Version baseline

| Component               | Selected version | License           | Reason                                                 |
| ----------------------- | ---------------: | ----------------- | ------------------------------------------------------ |
| Node.js                 |          24.19.0 | MIT               | Fixed CI baseline meeting frontend engine requirements |
| Rust                    |           1.98.0 | Apache-2.0 OR MIT | Fixed stable MSVC toolchain baseline                   |
| pnpm                    |          11.19.0 | MIT               | Available workspace package manager                    |
| Tauri CLI / API         |  2.11.4 / 2.11.1 | Apache-2.0 OR MIT | Current stable Tauri 2 line                            |
| Tauri Rust crate        |           2.11.5 | Apache-2.0 OR MIT | Cargo-resolved desktop runtime                         |
| Tokio                   |           1.53.1 | MIT               | Async runtime baseline                                 |
| React / React DOM       |           19.2.8 | MIT               | Current stable React line; supported by Fluent UI      |
| TypeScript              |            6.0.3 | Apache-2.0        | Current version compatible with the lint toolchain     |
| Vite / React plugin     |    8.2.2 / 6.1.1 | MIT               | Current stable frontend toolchain                      |
| Fluent UI React         |           9.74.7 | MIT               | Accessible Windows-aligned controls                    |
| i18next / react-i18next | 26.4.1 / 17.0.13 | MIT               | Deterministic local bilingual resources                |
| Vitest / jsdom          |  4.1.11 / 30.0.1 | MIT               | Unit and component test baseline                       |

## Consequences

The web application and Tauri shell can be verified on a prepared Windows host. Installed-mode notification activation and upgrade/uninstall behavior remain separate Stage 0 gates and cannot be inferred from a successful bundle build.
