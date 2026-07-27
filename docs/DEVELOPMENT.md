# Development

## Local commands

```bash
npm install
npm run dev
npm run format:check
npm run lint
npm run typecheck
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri:build
```

The Vite browser mode uses sanitized deterministic fixtures to make the dashboard and setup surfaces reviewable without requiring a local Codex installation. A Tauri build calls the Rust commands and will show empty/setup states until local sources are detected.

## Verification expectations

Parser tests cover token normalization, turn-context model attribution, cumulative-delta recovery, explicit credit/charge discovery, request/turn IDs, and partial final lines. Estimator and storage tests cover positive paired deltas, decimal percentages, no credit floor, pending data, decrease rejection, scheduled and manual reset boundaries, late-data isolation, median smoothing, and restart rebuilds. Add storage, App Server, and UI tests alongside every new state or DTO field. Keep tests deterministic and use isolated temporary databases; do not fixture prompts, account identifiers, full paths, or real local logs.
