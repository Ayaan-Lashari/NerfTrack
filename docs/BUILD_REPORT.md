# Build report

Date: 2026-08-09

## Product shape

NerfTrack is a local-only Tauri 2 desktop application with a React/TypeScript frontend and a Rust collector, parser, SQLite store, and weekly API-equivalent estimator. It reads local Codex desktop-app or CLI JSONL through one pipeline and exposes only typed aggregate DTOs.

## Portability and cleanup scope

The production cleanup establishes macOS ARM64 and Intel x86_64 plus Windows ARM64 and Intel x86_64 as the supported targets. Codex discovery validates readable plausible data, uses persisted selection → valid `CODEX_HOME` → deterministic platform candidates, safely skips recursive links, persists and clears overrides, and redacts diagnostic locations. The database uses the platform-native per-user application-data directory and the `NerfTrack/nerftrack.db` name.

The CLI App Server supervisor remains a tested protocol primitive, but no runtime supervisor is instantiated. CLI status therefore reports App Server supervision as unavailable/not integrated, while desktop mode remains independent of both the CLI executable and App Server.

## Verification policy

Run the commands in `docs/DEVELOPMENT.md` from the repository root. The GitHub Actions matrix performs the frontend gates, Rust gates, native build, and unsigned packaging smoke check on macOS ARM64, macOS Intel x86_64, Windows x64, and Windows ARM64. The tagged release workflow requires all four targets; Linux packaging and CI are intentionally excluded.

Unsigned artifacts, local databases, audit snapshots, dependency directories, and runtime output are ignored and must not be published. NerfTrack's original source code is licensed under GPL-3.0-only; third-party components retain their respective licenses.
