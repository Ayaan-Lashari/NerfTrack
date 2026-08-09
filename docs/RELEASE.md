# Release and packaging

The supported packaging targets are unsigned macOS ARM64 and Intel x86_64 `.app`/`.dmg` artifacts, plus Windows ARM64 and Intel x86_64 current-user NSIS installers. Linux packaging is intentionally out of scope. MSI is deferred until enterprise deployment requires it.

Pull-request CI runs the frontend, Rust, and Tauri packaging smoke checks on `macos-14` (ARM64), `macos-15-intel` (Intel x86_64), `windows-latest` (Intel x86_64), and the `windows-11-arm` preview runner (ARM64). The Windows ARM64 job is non-blocking while that hosted runner remains experimental or unavailable; the workflow records the exact limitation rather than claiming verification. It should become blocking after the runner is stable and consistently provisioned.

Signing and notarization are secret-driven release steps and are not part of pull-request CI. Certificates, API keys, signing identities, generated local databases, and audit snapshots must never be committed or uploaded. A maintainer must select a project license before publication; this repository intentionally does not invent one.
