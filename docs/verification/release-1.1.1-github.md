# GitHub publication checklist — 1.1.1

## Repository contents

- [x] Apache-2.0 license, notice, third-party notices, and CycloneDX SBOM are present.
- [x] Chinese and English landing documentation are present.
- [x] The Chinese user guide covers installation, tasks, local and Codex morning briefs, calendar, reviews, intelligence, learning, backup, restore, export, and FAQ.
- [x] Screenshots use synthetic development-only fixtures.
- [x] Contribution, conduct, security, Issue, and pull-request templates are present.
- [x] Generated build output, local work files, runtime databases, environment files, and TypeScript build metadata are ignored.

## Required checks before push

- [x] Formatting, linting, type checking, JavaScript tests, and production web build pass.
- [x] Rust formatting, Clippy, and tests remain green for the release commit.
- [x] Repository filenames and candidate text pass privacy and secret scans.
- [ ] The public GitHub repository is created with `main` as the default branch.
- [ ] The v1.1.1 release contains the Windows x64 installer and SHA-256 checksum.

## Screenshot provenance

The screenshots were captured on 2026-09-04 from `?demo=docs`, a development-only fixture bridge. Production builds cannot enable this mode. No screenshot contains the user's SQLite data, normal Codex home, account credentials, or private feed content.
