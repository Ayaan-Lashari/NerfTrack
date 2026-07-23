# Troubleshooting

## “Needs setup”

Open Setup and use Retry detection. Nerfify checks both Codex desktop-app data roots and CLI homes using `CODEX_HOME`, platform-relative home directories, `CODEX_BINARY`, PATH, platform defaults, and known secondary locations. Desktop mode reads the same local records and does not need a CLI executable or App Server. The native picker commands do not accept paths from JavaScript and only return redacted status.

## “Pending” quote

Pending means the estimator is waiting for a later pricing snapshot, a settled quota window, enough eligible cost/events, or a continuous account observation. Pending measurements are retained and may be repriced only when a later snapshot is valid; priced events stay frozen.

## “Unsupported” account

API-key, Bedrock, unauthenticated, third-party, or otherwise non-ChatGPT accounts do not receive a fabricated subscription quote. Official provider/backend evidence is preferred; the exact versioned Codex allowlist is supporting evidence.

## Gaps or resets

A monitoring gap, uncertain reset, quota correction, plan/account/limit change, incomplete data boundary, or changed percentage direction starts a new estimator epoch. Nerfify never calculates a quote across that boundary.

## Redaction checks

Run the repository-sensitive-file audit before packaging. Diagnostics should contain only aggregate counts, model IDs, and reasons. If a full path or account identifier appears in a UI DTO, fix the projection rather than masking it in React.
