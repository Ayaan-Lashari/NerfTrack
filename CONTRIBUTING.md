# Contributing to NerfTrack

Thanks for helping improve NerfTrack. It is a local-only Tauri desktop app that reads Codex usage records and stores aggregate usage, quota, and diagnostic data on the same machine. Contributions should preserve that privacy model, keep behavior deterministic, and work on the supported desktop targets.

For security vulnerabilities, follow [SECURITY.md](SECURITY.md). For conduct concerns, follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md); do not put sensitive incident details in a public issue.

## Before you start

Read the project documentation relevant to your change, especially [the development guide](docs/DEVELOPMENT.md), [the privacy model](docs/PRIVACY.md), and [the troubleshooting guide](docs/TROUBLESHOOTING.md). For parser, discovery, storage, or estimator changes, explain the data and state transition you are changing rather than relying on a real local Codex installation.

## License and attribution

NerfTrack's original source code is licensed under the GNU General Public License v3.0 only. See [LICENSE](LICENSE). New or modified project code must be compatible with GPL-3.0-only, and its provenance must be clear. Do not copy code with an incompatible or unknown license.

Preserve existing copyright and license notices. If you adapt code or behavior from a third-party project, retain the required attribution and update [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) when the dependency or attribution changes. In particular, the Rust Codex parser adapter contains behavior derived from the pinned MIT-licensed `ccusage` submodule; do not remove or obscure its source-level notice. The estimator is NerfTrack’s own local token-accounting code.

Do not add dependencies casually. Check their license and whether they introduce network access, telemetry, cloud sync, auto-update behavior, or a new privacy obligation. Keep dependency manifests and lockfiles consistent when a dependency change is actually needed.

## Issues

Search existing issues first, then open one issue for one problem or proposal. Include:

- a short, specific title;
- the expected and actual behavior;
- exact reproduction steps using synthetic or redacted data;
- the operating system, architecture, app mode, and relevant Node/Rust versions;
- sanitized diagnostics, logs, or screenshots; and
- a minimal fixture or test case when the issue concerns parsing, discovery, estimation, or persistence.

For discovery problems, report only the kind of source involved (for example, a CLI root or desktop data root), whether it was empty or contained plausible JSONL, and the redacted status shown by the app. Do not attach personal Codex records just to make a report reproducible. If a report would expose a secret or private data, do not put it in a public issue; contact a maintainer privately with a safe summary. For suspected vulnerabilities, follow the private reporting instructions in [SECURITY.md](SECURITY.md) instead of opening a public issue.

## Pull requests

Keep pull requests focused and explain the user-visible or maintenance benefit in the description. Link the relevant issue when one exists, call out platform-specific behavior, and include tests or a clear reason tests are not applicable. Update documentation, fixtures, or third-party notices when the behavior or attribution requires it.

Pull requests should:

- use deterministic, local fixtures rather than a contributor's Codex installation;
- preserve redaction of prompts, raw JSONL, credentials, account identifiers, usernames, and complete local paths;
- avoid committing generated databases, audit snapshots, build output, signing material, or private Codex data;
- include UI screenshots only when they contain sanitized fixture data; and
- leave the working tree free of unrelated formatting or generated changes.

CI must pass before merge. Pull-request CI may keep the Windows ARM64 preview job non-blocking while GitHub runner availability varies, but the tagged release workflow requires successful macOS ARM64, macOS Intel x86_64, Windows x64, and Windows ARM64 builds before publishing any release.

## Local quality gates

Run these commands from `NerfTrack/` after installing the lockfile-resolved dependencies. CI uses Node 22, stable Rust, Rustfmt, Clippy, and a recursive checkout of the `ccusage` submodule:

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

For changes that affect native packaging, also run `npm run tauri:build` on the target platform when possible. Use `npm run dev` for sanitized browser fixtures and `npm run tauri:dev` to exercise the native commands and local discovery path. Do not make tests depend on the host's real Codex home, credentials, account, or database.

## Cross-platform development

The blocking release targets are macOS ARM64 and Intel x86_64, and Windows x64 and ARM64. Linux packaging and Linux CI are intentionally out of scope for this release.

When changing native or filesystem behavior:

- use platform-aware path and application-data APIs; never assume `/`, `~`, or the current working directory;
- account for Windows path case, separators, executable extensions and `PATHEXT`, and symlink behavior;
- account for macOS app bundles and the macOS-only helper at `script/build_and_run.sh`;
- preserve the discovery order: persisted selection, valid `CODEX_HOME`, ordered platform candidates, then an explicit status;
- keep diagnostics redacted and test with injected temporary candidates; and
- remember that the database belongs in the platform-native per-user application-data directory, not in the repository.

Unsigned bundles are suitable for development and CI smoke checks. Signing, notarization, and installer publication require maintainer-controlled credentials and are not contributor tasks.

## Review checklist

Before requesting review, confirm that:

- the local quality gates pass, or the PR explains any platform limitation;
- the change has regression coverage where behavior changed;
- no private Codex data, secrets, credentials, full paths, generated databases, or artifacts are included;
- licensing and third-party attribution remain correct; and
- the PR description states what changed, how it was tested, and which platforms were verified.
