# S0-01 foundation verification

- Date: 2026-09-03
- Result: Passed within the limits listed below

## Environment

| Component     | Verified value                                 |
| ------------- | ---------------------------------------------- |
| Host OS       | Windows 10 build 19045, x64                    |
| WebView2      | 152.0.4191.53                                  |
| Node.js       | 24.19.0                                        |
| pnpm          | 11.19.0                                        |
| Rust / Cargo  | 1.98.0, x86_64-pc-windows-msvc                 |
| rustup        | 1.29.1                                         |
| C++ toolchain | Visual Studio Community 2026 with MSVC x64/x86 |
| Windows SDK   | 10.0.26100                                     |

## Reproducible checks

| Check                                      | Result                               |
| ------------------------------------------ | ------------------------------------ |
| `pnpm install --frozen-lockfile --offline` | Passed                               |
| `pnpm format:check`                        | Passed                               |
| `pnpm lint`                                | Passed                               |
| `pnpm typecheck`                           | Passed                               |
| `pnpm test`                                | 2 files, 5 tests passed              |
| `pnpm rust:fmt`                            | Passed                               |
| `pnpm rust:clippy`                         | Passed with `-D warnings`            |
| `pnpm rust:test`                           | 1 Rust unit test passed              |
| `pnpm desktop:build`                       | Release EXE and NSIS bundle produced |
| Secret and private-path scan               | No match in source files             |

MSVC writes a localized informational “creating import library” message to linker stdout for the Tauri `cdylib`. Rust 1.98 surfaces linker stdout as a warning during linked builds; strict Clippy remains clean.

## Artifacts

| Artifact                                        | SHA-256                                                            |
| ----------------------------------------------- | ------------------------------------------------------------------ |
| `Personal Growth Assistant_0.0.1_x64-setup.exe` | `EBFB27E6EA9F176220343F5786EAA1B30EE129B5CB0D11675A42AC8CBF220434` |
| `personal-growth-assistant_0.0.1_x64.exe`       | `3ECEF7BF9E489487FB1048FBBB2099021E23B7F063CD0531750693EB156403EE` |

## Installed-mode smoke test

1. Confirmed no pre-existing installation with the same display name.
2. Silent current-user installation returned exit code 0.
3. The installed binary and uninstaller existed under `%LOCALAPPDATA%\Personal Growth Assistant`.
4. The uninstall registration existed under HKCU and had no matching HKLM registration.
5. The per-user Start menu shortcut resolved to the installed binary.
6. The installed binary remained running for a five-second smoke interval and was then stopped.
7. Silent uninstall returned exit code 0 and removed the install directory, shortcut directory, and HKCU uninstall registration.

## Limits of this result

- The development artifacts are unsigned and may trigger Windows SmartScreen.
- The test host is Windows 10, not the required clean Windows 11 acceptance environment.
- Fixed AppUserModelID, Toast activator CLSID, notification actions, same-version upgrade, and data-preserving upgrade are S0-04 work and remain unverified.
- This is an engineering foundation build, not a production release.
