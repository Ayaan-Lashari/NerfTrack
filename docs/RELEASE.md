# Release and packaging

V1 packages an unsigned Apple Silicon macOS `.app` and `.dmg` locally, and a Windows x64 NSIS `.exe`. Pull-request CI runs macOS ARM64 and x64 plus Windows x64 checks. Windows ARM64 is represented as a non-blocking preview job when the current `windows-11-arm` runner is available. MSI is intentionally skipped until enterprise deployment requires it.

Signing and notarization are secret-driven release steps and are not part of pull-request CI. Certificates, API keys, signing identities, and generated local databases must never be committed or uploaded. Release jobs should add secrets only at the final packaging step and should preserve unsigned artifacts for review.
