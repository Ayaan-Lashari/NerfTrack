# Nerfify architecture

Nerfify is a local-only Tauri 2 application. React renders stable DTOs and invokes commands; discovery, parsing, persistence, account isolation, credit estimation, and process supervision live in Rust.

```text
Codex desktop app JSONL ─┐
Codex CLI JSONL + optional stdio App Server
          │
          ▼
  Rust discovery / parser / supervisor
          │  typed events, no raw prompts
          ▼
  serialized credit-estimator state machine
          │
          ▼
 SQLite writer + read queries ──► Tauri DTO commands ──► React/SVG UI
```

## Boundaries

- `src-tauri/src/discovery.rs` resolves either a desktop-app data root or a CLI home using local override, `CODEX_HOME`, platform-relative home directories, PATH, `CODEX_BINARY`, macOS ChatGPT/Codex app bundles, and secondary documented locations. Empty arbitrary folders are not auto-selected. Native pickers accept no JavaScript path and return only redacted status. The CLI executable and stdio App Server are optional in desktop-app mode.
- Desktop and CLI records enter the same collector, parser, storage, and credit-estimator pipeline. The integration mode changes only source discovery and whether the CLI-only App Server check is required; it does not create a separate calculation path.
- `src-tauri/src/parser.rs` reads newline-terminated JSONL records, tolerates partial final lines, converts cumulative per-turn token updates into deltas, extracts weekly quota observations, and accepts only explicit logged credits or logged charges as credit sources. It emits fingerprints rather than raw content.
- `src-tauri/src/app_server.rs` owns App Server method names, rate-limit window selection, sparse update merging, and bounded restart backoff.
- `src-tauri/src/storage.rs` owns transactional schema creation, WAL, foreign keys, owner-restricted files, migrations, weekly-window rebuilding, measurements, estimates, and DTO query projections.
- `src-tauri/src/estimator.rs` owns the `$0.04` conversion, positive paired-delta validation, median smoothing, reset-safe confidence, and pending/rejected decisions. Invalid intervals become pending/rejected diagnostics, never fabricated `$0` values.
- `src/App.tsx` is composition glue. `src/lib/commands.ts` is the typed frontend command boundary, and the browser fallback is only a visual development fixture; a Tauri build uses Rust results.

## Persistence and continuity

The database stores accounts, source checkpoints, parsed usage events, quota snapshots, weekly windows, raw credit measurements, estimates, annotations, settings, diagnostics, and app-run boundaries. Status refreshes reconcile new JSONL tails through the database’s serialized writer lock and rebuild the same credit estimates from timestamped observations. Startup and settings changes rebuild the same derived estimates, so existing and future databases share one estimator path. No table stores prompts, raw account identifiers, or raw JSONL lines.
