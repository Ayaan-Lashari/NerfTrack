# Build report

Date: 2026-07-23

## Scope delivered

This pass delivers a Tauri 2 desktop scaffold with React, TypeScript, Vite,
Rust, SQLite, the Nerfify visual shell, reference assets, typed UI DTOs,
native command boundaries, shared desktop-app/CLI discovery, Rust
parser/pricing/storage/estimator foundations, documentation, and CI/package
configuration.

The UI is a browser-reviewable vertical slice with Home, Setup, Diagnostics,
History, and Settings views. The Rust side includes startup Codex JSONL
reconciliation with byte-offset checkpoints, provider/pricing classification,
SQLite migrations, account-key hashing, estimator primitives, and App Server
supervision primitives. Desktop-app and CLI records use the same local
pipeline; desktop mode does not require a CLI executable or App Server.

## Local gates

All of the following passed locally from the repository root:

| Gate                  | Command                                                                                         | Result          |
| --------------------- | ----------------------------------------------------------------------------------------------- | --------------- |
| Frontend formatting   | `npm run format:check`                                                                          | PASS            |
| ESLint                | `npm run lint`                                                                                  | PASS            |
| TypeScript            | `npm run typecheck`                                                                             | PASS            |
| Frontend tests        | `npm test -- --run`                                                                             | PASS — 8 tests  |
| Vite production build | `npm run build`                                                                                 | PASS            |
| Rust formatting       | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`                                     | PASS            |
| Rust Clippy           | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | PASS            |
| Rust tests            | `cargo test --manifest-path src-tauri/Cargo.toml`                                               | PASS — 33 tests |
| Tauri bundle          | `npm run tauri:build`                                                                           | PASS            |

The Tauri build produced:

- `src-tauri/target/release/bundle/macos/Nerfify.app`
- `src-tauri/target/release/bundle/dmg/Nerfify_0.1.0_aarch64.dmg`

The unsigned application was launched locally from the generated `.app` and
exited cleanly after verification. Windows builds, GitHub-hosted CI, signing,
notarization, and release publication were not run on this Mac.

## UI and privacy review

The supplied dashboard and first-run concept were copied into
`design-reference/` and reviewed against the implementation. Browser QA
covered Home, Setup, Diagnostics, History, and Settings; range changes and
keyboard chart scrubbing were exercised. The browser review viewport was
1260×900, so the screenshot shows the responsive two-column metric layout
rather than the four-column wide-reference layout.

A final repository scan excludes `.git`, generated dependencies, build output,
the vendored ccusage fixtures, and binary reference images. It found only
intentional privacy vocabulary, parser token-field names, provider labels, and
documentation statements. No user-home paths, usernames, account values,
prompts, databases, logs, or generated secrets are included in the new project
content.

## CI and packaging

`.github/workflows/ci.yml` runs frontend, Rust, Tauri build, and artifact-upload
checks on macOS ARM64, macOS x64, and Windows x64. Windows ARM64 uses the
documented preview runner as an experimental, non-blocking matrix entry. Action
revisions are pinned; npm and Cargo caches use the supported setup actions.
macOS artifacts are unsigned `.app`/`.dmg` outputs. Windows packaging is NSIS;
MSI is intentionally deferred. Signing and notarization remain secret-driven
release steps documented in `docs/RELEASE.md`.

## Known implementation boundaries

The browser-only review build still uses deterministic fixtures. Native Tauri
builds periodically reconcile discovered or manually selected Codex JSONL
sources, price official GPT-5-family usage locally, persist embedded weekly
quota observations, rebuild settled cost/quota-delta intervals, freeze
saturated epochs, enforce configured estimator thresholds, and expose
median-stabilized current/history DTOs.

The CLI App Server supervisor remains available as a primitive but is not
required when JSONL records already contain the Codex weekly quota observation.
Remote pricing refresh, account switching boundaries, and trend classification
remain future work; missing inputs stay pending rather than becoming fabricated
zeroes.
