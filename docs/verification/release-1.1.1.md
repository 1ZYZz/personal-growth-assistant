# Release 1.1.1 verification

- Build date: 2026-09-04
- Product status: installed UI-fix release
- Host: Windows 10 22H2 build 19045, x64

## Changes verified

- Calendar cards reserve a separate control area and clamp long titles to two readable lines instead of rendering one Chinese character per line.
- The copy action is a compact icon with an accessible Chinese/English label and tooltip.
- Local and Codex-enhanced morning brief buttons use distinct labels.
- A UI regression test covers the long-title and copy-action markup.

## Automated and installed evidence

| Check                     | Result                                                                                     |
| ------------------------- | ------------------------------------------------------------------------------------------ |
| Frontend                  | Prettier, ESLint, strict TypeScript, and production Vite build passed                      |
| UI and locale             | 6 desktop tests passed, including exact zh-CN/en-US key parity and the calendar regression |
| Sidecar                   | 4 tests passed; installed probe reports bridge 1.1.1 and the exact six read-only tools     |
| Rust                      | strict Clippy and all 35 tests passed                                                      |
| Installer SHA-256         | `B856A33ECE2D3DD55A819398D6F6DAA052444A57F78A027C5ABA60AFE6519D75`                         |
| Installed desktop SHA-256 | `54318FE1D8EA599A53BA598902A7F9EB58A57F73F261944C42C90CE9A5E8D4CA`                         |
| Installed sidecar SHA-256 | `DB59C7488C0075A013A9A304A4D71118946C60564B40F5FE8DED60726CF5E04F`                         |
| Upgrade                   | Silent current-user upgrade exited 0 and retained the existing task and settings           |
| Runtime smoke             | Installed app remained healthy for 8 seconds at about 72.2 MiB                             |
| Database                  | Schema versions 1–6; `integrity_check=ok`; zero foreign-key violations                     |
| Supply chain              | CycloneDX 1.6 SBOM regenerated with 709 components                                         |

The artifact remains unsigned because no Authenticode certificate was supplied.
