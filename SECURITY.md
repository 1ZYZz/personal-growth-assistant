# Security policy

## Supported versions

The current 1.x release line and the default branch receive security fixes.

## Reporting a vulnerability

Do not open a public issue containing exploit details, credentials, tokens, personal task data, or private source content. Contact the maintainers privately and include:

- the affected version or commit;
- a minimal reproduction using synthetic data;
- expected and observed behavior;
- potential impact and suggested mitigation, if known.

The project will acknowledge a complete report within five business days. No severity or remediation deadline is promised before the v1.0 security process is finalized.

## Security baseline

- Local business data is canonical only in SQLite and is never written directly by the webview or an AI sidecar.
- Product telemetry and crash upload are disabled by default.
- Secrets, full prompts, authorization headers, cookies, and private page bodies must not enter logs.
- Tauri capabilities and child-process environments use explicit allowlists; Codex runs with a dedicated home, per-job directory, ignored project rules, no Shell/web search, and a read-only sandbox.
- The MCP bridge accepts only a bounded snapshot, a one-job environment token, and recognized environment-only tool scopes. It dynamically omits every tool outside the current allowlist and never opens SQLite.
- Lesson-to-task conversion stores only an approval-token hash and requires the exact proposal, an unexpired one-use token, and the expected entity version.
- External content is untrusted data and cannot grant tools or change product rules.
- Feed retrieval rejects non-HTTPS URLs, private/reserved addresses, credentials, redirects, and oversized bodies.
