# ADR-0002: Release integration and standalone sidecar

- Status: Accepted
- Date: 2026-09-03

## Context

The baseline requires an independently packaged TypeScript Agent Bridge that works without a user-installed Node.js. The selected `@yao-pkg/pkg` packaging path could not be made reproducible on the development host because its prebuilt runtime download path was unavailable. Node.js 24 provides a supported Single Executable Application mechanism from the already pinned build runtime.

## Decision

- Bundle the bridge with esbuild, create a Node SEA blob, inject it with `postject`, and name the target-triple executable for Tauri `externalBin` packaging.
- Keep `runtime probe` and `mcp` as fixed subcommands. React receives no process-launch capability.
- Require a session token of at least 32 characters and a typed snapshot bounded to 1 MiB for `mcp`.
- Keep six bounded read schemas in one registry and publish only the explicit allowlist for each job. The sidecar never opens SQLite and inherits no product authority.
- Keep the native Rust Codex adapter as the owner of process isolation, timeout, daily/monthly run limits, reasoning mode, schema validation, per-job cleanup, and `CODEX_HOME` separation.

## Consequences

End users need no Node.js installation. The local bridge binary is independently testable and appears in the SBOM. Injecting a payload into the signed upstream Node binary invalidates its original signature, so the locally produced executable and installer are documented as unsigned until a project code-signing certificate is supplied.
