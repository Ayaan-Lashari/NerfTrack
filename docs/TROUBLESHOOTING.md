# Troubleshooting

## “Needs setup”

Open Setup and use Retry detection. NerfTrack applies the same precedence in desktop and CLI modes: persisted folder selection, valid `CODEX_HOME`, ordered platform candidates, then a clear not-found status. An explicitly supplied empty `CODEX_HOME` is retained as a future location; automatic candidates need readable plausible Codex JSONL, so an empty arbitrary directory is not selected automatically. A manually selected empty directory is accepted as a future location and is shown as waiting for Codex JSONL.

If the selected folder is inaccessible or contains unsupported JSONL, choose the actual Codex data root. CLI Mode also needs a valid Codex executable. Desktop Mode uses the desktop data root and does not need a CLI executable or App Server. Use “Reset saved selections” to clear persisted folder and executable overrides and return to automatic discovery.

## App Server status

The CLI App Server supervisor is not integrated into this release. A CLI status of “Unavailable: App Server supervision is not integrated” is intentional; usage collection continues from JSONL. Desktop Mode marks App Server as not required.

## “Pending” quote

Pending means the estimator is waiting for a positive weekly-usage change paired with an explicit logged credit or logged USD charge. Token counts are never converted into credits. Pending observations stay local and do not appear as zero-value graph points.

## “Unsupported” account

API-key, Bedrock, unauthenticated, third-party, or otherwise non-ChatGPT accounts do not receive a fabricated subscription quote. Official provider/backend evidence is preferred; the exact versioned Codex allowlist is supporting evidence.

## Gaps or resets

A monitoring gap, uncertain reset, quota correction, plan/account/limit change, incomplete data boundary, changed reset timestamp, or material usage decrease starts a new weekly window. NerfTrack never calculates an estimate across that boundary. Reset reasons are preserved as `scheduled_reset`, `reported_reset_changed`, `usage_decreased`, or `uncertain_reset`.

## Redaction checks

Diagnostics contain aggregate counts, model IDs, and reasons only. If a full path, prompt, username, or account identifier appears in a UI DTO, fix the Rust projection rather than masking it in React.
