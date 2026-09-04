# Contributing

Work from a small issue with explicit acceptance criteria. Before implementation, identify the milestone and the acceptance criteria it satisfies.

## Local checks

Run these commands from the repository root:

```powershell
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm rust:fmt
pnpm rust:clippy
pnpm rust:test
pnpm run sbom
```

An NSIS release candidate additionally requires `pnpm desktop:build` and the installed-mode checklist in `docs/verification/release-1.1.1.md`.

## Dependencies and upstream code

Record the license, selected version, maintenance status, Windows support, and reason for every new dependency. Add copied or adapted code to `docs/upstream-map.md` before merging it. Do not copy logos, screenshots, credentials, private data, or code with an unclear license.

## Pull requests

Include what changed, relevant upstream source and version, files changed, tests run, limitations, and the next bounded issue.
