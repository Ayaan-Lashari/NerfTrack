# Third-party notices

## ccusage

NerfTrack adapts the minimum Codex parsing and pricing behavior from the pinned `ccusage` submodule at commit `31e084afbca3981af97ab6b55abe4f38f451bad4`. The relevant upstream project is MIT licensed. NerfTrack does not depend on the ccusage CLI, terminal reporting, or whole-directory polling implementation, and it does not modify the submodule.

The adapted behavior is limited to Codex path conventions, JSONL token normalization, cumulative-delta recovery, model/turn attribution, replay/fork deduplication, aliases, cached/uncached accounting, long-context and fast-mode multipliers, and missing-pricing classification. Source headers mark the Rust modules that contain the derived behavior.

## Direct dependencies

NerfTrack uses the Rust crates and npm packages declared in `src-tauri/Cargo.toml` and `package.json`. Their licenses are resolved through their respective package metadata at build time; no signing credentials, private package registries, or generated secrets are committed.
