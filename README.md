# NerfTrack

NerfTrack is a local-only Tauri desktop app that reads Codex usage records and estimates their API-equivalent weekly value. It stores aggregate usage, quota, and diagnostic data on the same machine; prompts, raw JSONL records, credentials, account identifiers, and complete local paths are not returned through the app UI.

## Supported platforms

The supported release targets are:

- macOS ARM64 and Intel x86_64
- Windows ARM64 and Intel x86_64

Linux packaging and Linux CI are intentionally out of scope for this release. Windows ARM64 remains an experimental CI target until the hosted runner is stable and consistently available.

## Build and test

Install the pinned frontend dependencies and run the local quality gates:

```bash
npm ci
npm run format:check
npm run lint
npm run typecheck
npm test -- --run
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Use `npm run tauri:build` for the native unsigned bundle. The macOS helper in `script/build_and_run.sh` is macOS-only.

## Codex discovery

Discovery uses one deterministic policy in CLI and desktop-app modes:

1. a persisted user-selected Codex data folder;
2. `CODEX_HOME` when it points to a readable supported root (including an intentionally empty future data folder);
3. ordered platform candidates;
4. an explicit not-found or unsupported status.

Automatic candidates are selected only when their directory can be traversed safely and contains readable JSONL with a plausible Codex record. Empty directories are skipped automatically. A manually selected empty directory is retained as a valid future data location and is shown as waiting for data. Saved folder and executable selections can be cleared from Setup; they are stored in the local application database and reloaded on restart.

On macOS, known desktop-app data roots are considered before the CLI `~/.codex` root. On Windows, the per-user Codex application-data roots are considered before the CLI root. If a CLI root is selected, NerfTrack requires a valid Codex executable for CLI integration. A desktop data root does not require a CLI executable or App Server.

Executable discovery checks `CODEX_BINARY`, then the platform PATH (including Windows `PATHEXT`), then platform compatibility fallbacks. macOS app bundles are inspected for their internal Codex executable. Arbitrary regular files are rejected.

## Privacy and local storage

The application database is stored at the platform-native per-user application-data location under `NerfTrack`:

- macOS: `~/Library/Application Support/NerfTrack/nerftrack.db`
- Windows: `%LOCALAPPDATA%\\NerfTrack\\nerftrack.db`

The database does not depend on the process working directory. No network service or telemetry is required.

## License

NerfTrack's original source code is licensed under the GNU General Public License v3.0 only. See [LICENSE](LICENSE). Third-party components retain their respective licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Current limitations

- The CLI App Server supervisor module is retained as an unintegrated primitive. The status surface explicitly reports that supervision is unavailable in this release; desktop mode does not depend on it.
- Native Windows builds and Windows ARM64 packaging are verified by the corresponding CI jobs rather than this macOS development machine.
- Unsigned release artifacts are build outputs only. Signing, notarization, and installer publication require maintainer-controlled release credentials.
