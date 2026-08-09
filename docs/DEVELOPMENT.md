# Development

## Local commands

```bash
npm ci
npm run dev
npm run format:check
npm run lint
npm run typecheck
npm test -- --run
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
npm run tauri:build
```

The Vite browser mode uses sanitized deterministic fixtures so the dashboard and Setup surfaces are reviewable without a local Codex installation. A Tauri build calls Rust commands and shows setup/empty states until local sources are detected. The database location is platform-native and independent of the current working directory.

## Verification expectations

Discovery tests use injected temporary candidates rather than the host’s real Codex installation. They cover precedence, empty-versus-unsupported folders, plausible JSONL validation, symlink safety, redaction, executable validation, and App Server status. Collector tests cover byte checkpoints, partial final lines, path characters, truncation recovery, and unsafe links. Storage tests cover settings restart persistence, legacy migration, database path independence, and estimator rebuilds.

Keep tests deterministic and local. Do not fixture prompts, account identifiers, full paths, raw logs, credentials, generated databases, or audit snapshots. Linux packaging and Linux CI are not part of the supported verification matrix.
