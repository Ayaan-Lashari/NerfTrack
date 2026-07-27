# Troubleshooting

## “Needs setup”

Open Setup and use Retry detection. Nerfify checks both Codex desktop-app data roots and CLI homes using `CODEX_HOME`, platform-relative home directories, `CODEX_BINARY`, PATH, platform defaults, and known secondary locations. Desktop mode reads the same local records and does not need a CLI executable or App Server. The native picker commands do not accept paths from JavaScript and only return redacted status.

## “Pending” quote

Pending means the estimator is waiting for a positive weekly-usage change paired with an explicit logged credit or logged USD charge. Token counts are never converted into credits. Pending observations stay local and do not appear as zero-value graph points.

## “Unsupported” account

API-key, Bedrock, unauthenticated, third-party, or otherwise non-ChatGPT accounts do not receive a fabricated subscription quote. Official provider/backend evidence is preferred; the exact versioned Codex allowlist is supporting evidence.

## Gaps or resets

A monitoring gap, uncertain reset, quota correction, plan/account/limit change, incomplete data boundary, changed reset timestamp, or material usage decrease starts a new weekly window. Nerfify never calculates an estimate across that boundary. Reset reasons are preserved as `scheduled_reset`, `reported_reset_changed`, `usage_decreased`, or `uncertain_reset`.

## Redaction checks

Run the repository-sensitive-file audit before packaging. Diagnostics should contain only aggregate counts, model IDs, and reasons. If a full path or account identifier appears in a UI DTO, fix the projection rather than masking it in React.
