# Release and packaging

The supported packaging targets are unsigned macOS ARM64 and Intel x86_64 `.app`/`.dmg` artifacts, plus Windows ARM64 and Intel x86_64 current-user NSIS installers. Linux packaging is intentionally out of scope. MSI is deferred until enterprise deployment requires it.

Pull-request CI runs the frontend, Rust, and Tauri packaging smoke checks on `macos-14` (ARM64), `macos-15-intel` (Intel x86_64), `windows-latest` (Intel x86_64), and `windows-11-arm` (ARM64). The release workflow makes all four architecture builds blocking: if the Windows ARM64 hosted runner or toolchain is unavailable, no release is created and no x64 artifact is relabeled as ARM64. GitHub documents `windows-11-arm` as an ARM64 hosted runner for public repositories, and Tauri documents the native `aarch64-pc-windows-msvc` target for ARM builds.

Signing and notarization are secret-driven release steps and are not part of pull-request CI. Certificates, API keys, signing identities, generated local databases, and audit snapshots must never be committed or uploaded. NerfTrack's original source code is licensed under GPL-3.0-only; third-party components retain their respective licenses.

## Release workflow

Push a semver tag such as `v0.5.1`. The workflow validates that the tag matches the application manifests, runs the existing frontend and Rust gates, builds with Tauri's native target, normalizes the output names, and creates one GitHub Release only after all four jobs succeed.

The exact asset names are:

- `NerfTrack-0.5.1-macos-arm64.dmg`
- `NerfTrack-0.5.1-macos-x86_64.dmg`
- `NerfTrack-0.5.1-windows-x64-setup.exe`
- `NerfTrack-0.5.1-windows-arm64-setup.exe`

The workflow uploads unsigned artifacts. Code signing, macOS notarization, and any signing credentials remain outside the repository and are not required for the public build workflow.

## In-app GitHub Releases updates

The desktop app checks only GitHub's `releases/latest` API endpoint for the configured public repository. The release build points `GITHUB_REPOSITORY_URL` in `src/lib/config.ts` at `https://github.com/Ayaan-Lashari/NerfTrack`; forks should change that value before distributing their own builds. The same configured repository is used by the first-run GitHub star page.

The updater also recognizes Tauri-style `x86_64`, `aarch64`, and `x64-setup` names. It compares
the release tag with the installed version, validates the selected asset and download size/hash,
then launches Windows MSI/NSIS or macOS package handling from a private temporary directory. A
missing repository, missing release, invalid tag, unsupported asset, failed download, or unsupported
platform is shown in the Update control rather than interrupting the main UI.
