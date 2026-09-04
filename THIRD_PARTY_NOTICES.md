# Third-party notices

Personal Growth Assistant is licensed under Apache-2.0. It incorporates package-manager dependencies under their respective licenses. The exact release inventory, versions, package URLs, and license expressions are recorded in `sbom.cdx.json`.

| Component                  |          Version | License                 | Purpose                                  |
| -------------------------- | ---------------: | ----------------------- | ---------------------------------------- |
| Tauri / Tauri CLI          |  2.11.5 / 2.11.4 | Apache-2.0 OR MIT       | Desktop shell and build tooling          |
| React / React DOM          |           19.2.8 | MIT                     | User interface                           |
| Fluent UI React Components |           9.74.7 | MIT                     | Accessible UI primitives                 |
| i18next / react-i18next    | 26.4.1 / 17.0.13 | MIT                     | Internationalization                     |
| SQLx                       |            0.9.0 | Apache-2.0 OR MIT       | SQLite access and transactions           |
| Tokio                      |           1.53.1 | MIT                     | Async runtime and scheduler              |
| RRule.rs                   |           0.14.0 | Apache-2.0 OR MIT       | RFC 5545 recurrence                      |
| chrono / chrono-tz         |  0.4.45 / 0.10.4 | Apache-2.0 OR MIT / MIT | Date, time, and IANA timezone handling   |
| fsrs                       |            6.6.2 | MIT                     | Spaced-repetition scheduling             |
| feed-rs                    |            2.4.0 | MIT                     | RSS and Atom parsing                     |
| reqwest                    |           0.13.4 | Apache-2.0 OR MIT       | HTTPS feed retrieval                     |
| windows-rs                 |           0.61.3 | Apache-2.0 OR MIT       | Native Windows toast APIs                |
| MCP TypeScript SDK server  |            2.0.0 | MIT                     | Standard MCP server and stdio transport  |
| Zod                        |            4.5.4 | MIT                     | Sidecar input/output validation          |
| esbuild                    |           0.28.1 | MIT                     | Sidecar bundling                         |
| postject                   |    1.0.0-alpha.6 | MIT                     | Node SEA payload injection at build time |
| Vite / Vitest              |      8.2.2 / 4.x | MIT                     | Frontend build and tests                 |
| TypeScript                 |            6.0.3 | Apache-2.0              | Type checking                            |

No OpenLoomi, OpenTutor, Super Productivity, changedetection.io, or other upstream source file, logo, screenshot, or branded asset is copied into this release. Node.js is used as the basis of the standalone sidecar executable and remains subject to its own license and bundled third-party notices; the release SBOM records the JavaScript components injected into that executable.

Copyright notices and full license texts for transitive packages are available from the package URLs in `sbom.cdx.json`. This notice is informational and does not replace those license texts.
