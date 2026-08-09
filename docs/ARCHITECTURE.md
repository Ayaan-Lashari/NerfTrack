# NerfTrack architecture

NerfTrack is a local-only Tauri 2 application. React renders typed projections and invokes commands; discovery, parsing, collection, persistence, account isolation, and estimation run in Rust.

```text
Codex desktop-app or CLI JSONL
              │
              ▼
  Rust discovery → safe collector → parser
              │  typed events and redacted diagnostics
              ▼
       SQLite writer and estimator
              │
              ▼
       Tauri DTO commands → React/SVG UI
```

## Discovery and integration boundaries

The same discovery policy is used by refresh, retry, CLI mode, and desktop mode:

1. persisted user-selected Codex home;
2. valid `CODEX_HOME` (an accessible data root, including an explicitly supplied empty future folder);
3. ordered platform candidates;
4. an explicit missing or unsupported status.

Automatic candidates are accepted only when the directory is accessible, recursive traversal is safe, and at least one readable `.jsonl` file contains a plausible Codex record. Empty candidates are skipped. A manually selected empty directory is retained as a future data location and is reported as waiting for data. On macOS, known desktop-app roots precede `~/.codex`; on Windows, per-user desktop roots precede the CLI root. When multiple candidates are valid, the first candidate in that documented order wins.

The selected home determines the integration mode. A recognized desktop-app root is Desktop Mode and does not require a CLI executable or App Server. A CLI root is CLI Mode only when a valid Codex executable is also available; a GUI executable cannot relabel a CLI-only home as Desktop Mode. The CLI App Server module is retained as a tested protocol/backoff primitive, but it is not instantiated by the refresh lifecycle in this release. The status surface reports it as unavailable rather than ready.

`discovery.rs` validates executable permissions and names, Windows `PATHEXT`, dynamic PATH entries, macOS app bundles, and guarded compatibility fallback paths. Native pickers accept no JavaScript path and return only redacted status. Folder and executable selections are stored in the local settings table and can be cleared from Setup.

## Collection and parsing

Desktop and CLI records use the same collector, parser, storage, and estimator pipeline. The collector traverses only real directories, tracks canonical directories, skips recursive links, preserves byte-offset/parser-state checkpoints, and reports unreadable files or partial scans as diagnostics and failed refresh status. Spaces, Unicode, parentheses, and platform path separators are handled through `Path` APIs.

`parser.rs` reads newline-terminated JSONL records, tolerates a partial final line, converts cumulative per-turn token updates into deltas, extracts weekly quota observations, and accepts only explicit logged credits or logged charges as credit sources. It emits fingerprints and normalized model data rather than raw content.

## Persistence and continuity

`storage.rs` owns the SQLite schema, WAL, foreign keys, owner-restricted files, migrations, weekly-window rebuilding, measurements, estimates, settings, diagnostics, and DTO query projections. The production database is `nerftrack.db` under the platform-native per-user application-data directory, independent of the process working directory.

The database stores accounts, source checkpoints, parsed usage events, quota snapshots, weekly windows, measurements, estimates, annotations, settings, diagnostics, and app-run boundaries. It does not store prompts, raw account identifiers, or raw JSONL lines. `estimator.rs` keeps invalid intervals pending or rejected rather than fabricating zero values.
