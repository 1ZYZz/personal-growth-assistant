# Threat model

## Protected assets

- Tasks, schedules, reviews, news metadata, learning state, and backups
- SQLite consistency, migration history, scheduler leases, and idempotency keys
- Isolated runtime credentials and short-lived MCP session tokens
- Notification action nonces and canonical source links
- Installer, sidecar, lockfiles, license inventory, and SBOM

## Trust boundaries and controls

| Boundary or threat                            | Release control                                                                                                    |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Webview requests excessive native authority   | Minimal Tauri capability; no shell or filesystem plugin; domain validation in Rust                                 |
| AI mutates authoritative records              | Release exposes read-only AI work only; SQLite is inaccessible to Codex and the sidecar                            |
| User's normal Codex credentials/config leak   | Dedicated `CODEX_HOME`, empty working directory, `env_clear`, explicit variable allowlist                          |
| Sidecar token leaks                           | Token is required through environment only; never accepted on command line, persisted, logged, or exported         |
| Duplicate reminder or replayed deep link      | Transactional scheduler lease, stable business idempotency key, expiring one-time action nonce                     |
| Feed-based SSRF or DNS rebinding              | HTTPS only, no embedded credentials, DNS resolution and public-IP validation, pinned addresses, redirects disabled |
| Feed or prompt injection changes instructions | External content remains bounded typed data; no authority-bearing fields or write tools                            |
| Forged news links                             | UI reads only canonical URLs stored during ingestion; AI cannot provide display URLs                               |
| Oversized or malformed input                  | Feed, sidecar environment, output, and schema bounds; Zod/JSON Schema/Rust validation                              |
| Unsafe backup restore or traversal            | Filename allowlist, backup-directory confinement, SQLite integrity validation, pre-restore backup                  |
| Startup setting becomes arbitrary execution   | Fixed HKCU Run key/value, current executable only, argument-array process launch, and uninstall-time removal       |
| Supply-chain substitution                     | Exact npm/Cargo lockfiles, SHA-pinned CI actions, license gate, CycloneDX SBOM, no Git dependencies                |

## Residual release risks

- The local installer and SEA sidecar are unsigned; Windows may display SmartScreen warnings. Signing requires an external code-signing certificate.
- Native toast integration depends on supported Windows notification services and cannot deliver while the computer is powered off.
- RSS endpoints can become unavailable or change format; failures are surfaced per source and do not affect local functions.
- The development-host validation is Windows 10 22H2. Windows 11 multi-device and long-duration certification are release-operations gates, not properties that can be proven by unit tests.
