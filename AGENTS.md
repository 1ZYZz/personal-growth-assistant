# Personal Growth Assistant contributor rules

These rules apply to the entire repository.

1. Implement one acceptance-testable issue at a time. Do not build post-v0.1 product areas before Stage 0 exits.
2. SQLite is the canonical business-data store. React components and sidecars never write it directly.
3. Keep AI optional. Core tasks, recurrence, scheduling, notifications, and backup must work offline.
4. New dependencies require a license, maintenance, platform-support, and duplication check recorded in the relevant ADR or pull request.
5. Any copied or adapted upstream source must be entered in `docs/upstream-map.md` before merge.
6. Add tests, explicit error handling, structured logs, and documentation with functional changes.
7. Never commit secrets, personal data, credentials, production prompts, or user-specific paths.
8. Run formatting, linting, type checks, tests, and relevant builds before reporting an issue complete.
9. Keep Simplified Chinese and English translation key sets identical.
10. Do not broaden Tauri capabilities or child-process permissions without a threat-model update.
