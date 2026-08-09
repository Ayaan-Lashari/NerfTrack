# Security Policy

## Supported Versions

The current release line is **0.5.0**. Security triage and fixes target the latest tagged release and the current development branch. Development builds and unsupported platforms are not covered by a support commitment.

NerfTrack currently targets macOS (ARM64 and Intel x86_64) and Windows (x64 and ARM64). Linux packaging is out of scope for this release. Windows ARM64 builds depend on the public `windows-11-arm` GitHub-hosted runner and are not published unless that target completes successfully.

## Reporting a Vulnerability

Please report suspected vulnerabilities privately through this repository’s [GitHub Security Advisories page](https://github.com/Ayaan-Lashari/NerfTrack/security/advisories/new) by selecting **Report a vulnerability**. Do not open a public issue for an undisclosed vulnerability.

If private vulnerability reporting is unavailable, contact the maintainer through the [project maintainer’s GitHub profile](https://github.com/Ayaan-Lashari) without posting exploit details publicly. Reports are acknowledged and triaged as maintainer capacity allows; disclosure timing is coordinated with the reporter when practical.

Include the affected NerfTrack version, platform and architecture, a concise description of the impact, reproduction steps or a minimal proof of concept, and any relevant logs after removing sensitive content. Maintainers will assess reports and coordinate disclosure as appropriate.

NerfTrack reads local Codex data and stores local application data. Do not include prompts, raw JSONL records, credentials, account identifiers, complete local paths, database files, or other private Codex data in a report. Redact or replace such values with placeholders.

## Scope and Limitations

Reports are most useful when they demonstrate a security impact in NerfTrack, such as unauthorized disclosure, modification, or destruction of local Codex or NerfTrack data; unsafe handling of untrusted local input; privilege escalation; arbitrary code execution; or an unintended network or external-process boundary.

NerfTrack is a local-only desktop application. Its security boundary includes the frontend/WebView, Rust commands, local Codex files, the NerfTrack database, selected executables, and same-user operating-system resources. The app may invoke platform discovery tooling such as macOS `mdfind`; its App Server supervisor is currently not integrated. It does not provide isolation from a compromised operating system, user account, filesystem, or other software already running with the same user’s privileges. Malformed local files and symlink behavior remain meaningful security inputs. Issues in Codex, the operating system, Tauri, or other dependencies are in scope when NerfTrack makes them reachable or materially worsens their impact; otherwise, report them to the relevant upstream project. General bugs, unsupported Linux packaging, and issues requiring a fully compromised host are normally out of scope.

NerfTrack’s original source is licensed under the GNU General Public License, version 3.0 only. Security fixes and contributions must be submitted under terms compatible with GPLv3; third-party components remain subject to their respective licenses.
